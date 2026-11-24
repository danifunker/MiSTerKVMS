//! A JetKVM client: the device's video (WebRTC, H.264, decoded here to
//! BGRA) and its USB keyboard and mouse (the `hidrpc` data channel).
//!
//! Protocol (JetKVM firmware 0.5.x, github.com/jetkvm/kvm):
//! - `GET /device/status` (public): `{isSetup, sku}`.
//! - `POST /auth/login-local {password}` → `authToken` cookie (skipped in
//!   no-password mode).
//! - WebSocket `/webrtc/signaling/client`: we send
//!   `{"type":"offer","data":{"sd": base64(JSON {type, sdp})}}`, it answers
//!   `{"type":"answer","data": base64(JSON {type, sdp})}` and trickles
//!   `{"type":"new-ice-candidate","data":{candidate,…}}`.
//! - Data channel `hidrpc`: binary, first byte is the message type.
//!
//! JetKVM allows one session: connecting takes over from whoever had it.
//!
//! One thread per connection owns the WebRTC state (str0m, sans-I/O) and
//! its UDP socket; a second reads the signaling socket.

use std::io::{ErrorKind, Read};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use base64::Engine;
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;
use serde::Deserialize;
use str0m::change::{SdpAnswer, SdpPendingOffer};
use str0m::channel::ChannelId;
use str0m::media::{Direction, KeyframeRequestKind, MediaKind, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// hidrpc message types (internal/hidrpc/hidrpc.go).
mod hid {
    pub const HANDSHAKE: u8 = 0x01;
    pub const KEYBOARD_REPORT: u8 = 0x02;
    pub const POINTER_REPORT: u8 = 0x03;
    pub const WHEEL_REPORT: u8 = 0x04;
    pub const KEYPRESS_REPORT: u8 = 0x05;
    pub const MOUSE_REPORT: u8 = 0x06;
    pub const VERSION: u8 = 0x01;
}

/// The absolute pointer's range on both axes (the gadget's logical max).
pub const POINTER_MAX: u32 = 32767;

/// `GET /device/status`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub is_setup: bool,
    #[serde(default)]
    pub sku: String,
}

fn http_base(host: &str) -> String {
    if host.starts_with("http://") || host.starts_with("https://") {
        host.trim_end_matches('/').to_string()
    } else {
        format!("http://{}", host.trim_end_matches('/'))
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Is there a JetKVM at `host`, and is it set up?
pub fn status(host: &str) -> Result<DeviceStatus, String> {
    let mut resp = agent()
        .get(format!("{}/device/status", http_base(host)))
        .call()
        .map_err(|e| format!("{host}: {e}"))?;
    if resp.status() != 200 {
        return Err(format!("{host}: /device/status returned {}", resp.status()));
    }
    resp.body_mut().read_json().map_err(|e| format!("{host}: not a JetKVM ({e})"))
}

/// MiSTer's custom `video_mode` for standard 1080p at 30 Hz (CEA-861 mode
/// 34: 2200x1125 total at 74.25 MHz), the only mode the JetKVM Mini takes.
pub const MISTER_1080P30: &str = "video_mode=1920,88,44,148,1080,4,5,36,74250,+hsync,+vsync";

/// What to tell someone whose JetKVM connects but shows no picture: the
/// MiSTer side (MiSTer.ini) is the usual cause. `sku` from `status`.
pub fn no_picture_help(sku: &str) -> String {
    let common = "Also check the HDMI cable, and restart the MiSTer while the JetKVM is already on.";
    if sku.contains("mini") {
        format!(
            "The JetKVM is connected but gets no picture from the MiSTer. The JetKVM Mini only accepts 1080p at 30 Hz, \
             and MiSTer sends 60 Hz by default. In MiSTer.ini on the SD card, set {MISTER_1080P30} and vsync_adjust=0 \
             under [MiSTer] and in every core section that sets its own video_mode, then restart the MiSTer. {common}"
        )
    } else {
        format!(
            "The JetKVM is connected but gets no picture from the MiSTer. MiSTer sends the video_mode from MiSTer.ini \
             whatever the JetKVM supports: try video_mode=0 (720p60) under [MiSTer] and in any core section that sets \
             its own, then restart the MiSTer. {common}"
        )
    }
}

/// Log in (password mode); returns the `authToken` cookie value.
fn login(host: &str, password: &str) -> Result<String, String> {
    let resp = agent()
        .post(format!("{}/auth/login-local", http_base(host)))
        .header("Origin", http_base(host))
        .send_json(serde_json::json!({ "password": password }))
        .map_err(|e| format!("login: {e}"))?;
    if resp.status() != 200 {
        return Err(format!("login: wrong password ({})", resp.status()));
    }
    for v in resp.headers().get_all("set-cookie") {
        let v = v.to_str().unwrap_or_default();
        if let Some(rest) = v.strip_prefix("authToken=") {
            return Ok(rest.split(';').next().unwrap_or_default().to_string());
        }
    }
    Err("login: no authToken cookie".into())
}

/// A decoded picture.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// BGRA, `width * 4` bytes per row.
    pub bgra: Vec<u8>,
    /// Counts up from 1 with each decoded picture.
    pub number: u64,
    pub received: Instant,
}

/// How far a connection has got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Signaling,
    Connecting,
    /// ICE and DTLS done; `hid` once the hidrpc handshake came back.
    Connected { hid: bool },
    /// Gone, and why.
    Closed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mini_help_names_the_ini_line() {
        let h = no_picture_help("jetkvm-mini-ethernet");
        assert!(h.contains("MiSTer.ini") && h.contains(MISTER_1080P30) && h.contains("vsync_adjust=0"));
        assert!(!no_picture_help("jetkvm-v2").contains("30 Hz"));
    }

    #[test]
    fn mids_round_trip() {
        let sdp = "v=0\r\na=group:BUNDLE aB c9\r\nm=video 9\r\na=mid:aB\r\nm=application 9\r\na=mid:c9\r\n";
        let mids = sdp_mids(sdp);
        assert_eq!(mids, ["aB", "c9"]);
        let numbered = vec!["0".to_string(), "1".to_string()];
        let out = rename_mids(sdp, &mids, &numbered);
        assert!(out.contains("BUNDLE 0 1") && out.contains("a=mid:1"));
        assert_eq!(rename_mids(&out, &numbered, &mids), sdp);
    }

    #[test]
    fn c_line_moves_up() {
        let sdp = "v=0\r\nm=video 9\r\na=mid:0\r\nc=IN IP4 0.0.0.0\r\na=ice-ufrag:x";
        assert_eq!(tidy_answer(sdp), "v=0\r\nm=video 9\r\nc=IN IP4 0.0.0.0\r\na=mid:0\r\na=ice-ufrag:x");
    }
}

#[derive(Default)]
struct Shared {
    frame: Option<Frame>,
    state: Option<State>,
    stats: Stats,
}

/// Counters for diagnostics.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub video_packets: u64,
    pub decoded: u64,
    pub decode_errors: u64,
    pub hid_sent: u64,
    pub codec: String,
}

enum Cmd {
    Hid(Vec<u8>),
    Keyframe,
    Rpc(String),
    Signal(Signal),
    SignalClosed(String),
}

enum Signal {
    Answer(String),
    Candidate(String),
}

/// An open JetKVM session. Dropping it closes the session.
pub struct Connection {
    sku: String,
    tx: Sender<Cmd>,
    shared: Arc<(Mutex<Shared>, Condvar)>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Connection {
    /// Start connecting to the JetKVM at `host` (IP or name, optional
    /// `:port`). Returns once signaling has started; follow progress with
    /// `state` / `wait_frame`.
    pub fn connect(host: &str, password: Option<&str>) -> Result<Self, String> {
        let st = status(host)?;
        if !st.is_setup {
            return Err(format!("{host}: JetKVM isn't set up yet; finish its setup in a browser first"));
        }
        let cookie = match password.filter(|p| !p.is_empty()) {
            Some(p) => Some(login(host, p)?),
            None => None,
        };

        let base = http_base(host);
        let authority = base.split("://").nth(1).unwrap_or(&base).to_string();
        let addr = if authority.contains(':') { authority.clone() } else { format!("{authority}:80") };
        let remote: SocketAddr = addr
            .to_socket_addrs()
            .map_err(|e| format!("resolve {addr}: {e}"))?
            .find(|a| a.is_ipv4())
            .ok_or_else(|| format!("no IPv4 address for {addr}"))?;

        // Signaling socket.
        let tcp = TcpStream::connect_timeout(&remote, Duration::from_secs(5)).map_err(|e| format!("connect {remote}: {e}"))?;
        let mut req = tungstenite::client::IntoClientRequest::into_client_request(format!("ws://{authority}/webrtc/signaling/client"))
            .map_err(|e| format!("signaling: {e}"))?;
        // Newer firmware (0.5.20) only accepts its own web page's origin.
        req.headers_mut().insert("Origin", base.parse().map_err(|_| "bad host")?);
        if let Some(c) = &cookie {
            req.headers_mut().insert("Cookie", format!("authToken={c}").parse().unwrap());
        }
        let (mut ws, _) = tungstenite::client(req, tcp).map_err(|e| match e {
            tungstenite::HandshakeError::Failure(tungstenite::Error::Http(r)) if r.status() == 401 => {
                "JetKVM needs a password".to_string()
            }
            e => format!("signaling handshake: {e}"),
        })?;

        // Media socket, on the interface that reaches the JetKVM.
        let probe = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        probe.connect(remote).map_err(|e| e.to_string())?;
        let local_ip = probe.local_addr().map_err(|e| e.to_string())?.ip();
        let socket = UdpSocket::bind(SocketAddr::new(local_ip, 0)).map_err(|e| format!("udp bind: {e}"))?;
        let local = socket.local_addr().map_err(|e| e.to_string())?;

        let mut rtc = Rtc::builder()
            .clear_codecs()
            .enable_h264(true)
            // str0m takes a stream without RTX for audio and reports on it
            // every 5 s; the Mini's video has no RTX. Report every second,
            // as browsers do, so its loss-based bitrate control has data.
            .set_rtcp_report_interval_audio(Duration::from_secs(1))
            .build(Instant::now());
        rtc.add_local_candidate(Candidate::host(local, "udp").map_err(|e| e.to_string())?);
        let mut api = rtc.sdp_api();
        let video = api.add_media(MediaKind::Video, Direction::RecvOnly, None, None, None);
        // "rpc" first, then "hidrpc", in the web page's order. The Mini's
        // firmware waits for the rpc channel before it counts the session
        // as connected and starts the video.
        let rpc_channel = api.add_channel("rpc".into());
        let hid_channel = api.add_channel("hidrpc".into());
        let (offer, pending) = api.apply().ok_or("no offer")?;

        // The Mini's firmware (esp-webrtc) answers with mids "0", "1", ...
        // whatever the offer said, as browsers number them: offer it that way.
        let offer_sdp = offer.to_sdp_string();
        let mids = sdp_mids(&offer_sdp);
        let numbered: Vec<String> = (0..mids.len()).map(|i| i.to_string()).collect();
        let offer_sdp = rename_mids(&offer_sdp, &mids, &numbered);
        if std::env::var_os("JETKVM_DEBUG").is_some() {
            eprintln!("jetkvm offer:\n{offer_sdp}");
        }
        let offer_json = serde_json::json!({ "type": "offer", "sdp": offer_sdp });
        let msg = serde_json::json!({ "type": "offer", "data": { "sd": B64.encode(offer_json.to_string()) } });
        ws.send(tungstenite::Message::text(msg.to_string())).map_err(|e| format!("send offer: {e}"))?;

        let (tx, rx) = mpsc::channel();
        let shared = Arc::new((Mutex::new(Shared { state: Some(State::Signaling), ..Default::default() }), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));

        // Signaling reader: answer, trickled candidates, pings.
        if let Ok(s) = ws.get_ref().try_clone() {
            let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
        }
        let sig_tx = tx.clone();
        let sig_stop = stop.clone();
        thread::Builder::new()
            .name("jetkvm-signal".into())
            .spawn(move || signal_loop(ws, sig_tx, sig_stop))
            .map_err(|e| e.to_string())?;

        let decoder = Decoder::new().map_err(|e| format!("h264 decoder: {e}"))?;
        let (video_tx, video_rx) = mpsc::channel::<Vec<u8>>();
        let decode_shared = shared.clone();
        let decode_cmd = tx.clone();
        thread::Builder::new()
            .name("jetkvm-decode".into())
            .spawn(move || decode_loop(decoder, video_rx, decode_shared, decode_cmd))
            .map_err(|e| e.to_string())?;

        let worker = Worker {
            rtc,
            socket,
            local,
            rx,
            shared: shared.clone(),
            stop: stop.clone(),
            pending: Some(pending),
            mids,
            video,
            hid_channel,
            rpc_channel,
            rpc_id: 0,
            hid_open: false,
            video_tx,
            asked_keyframe: None,
        };
        let thread = thread::Builder::new()
            .name("jetkvm-rtc".into())
            .spawn(move || worker.run())
            .map_err(|e| e.to_string())?;

        Ok(Self { sku: st.sku, tx, shared, stop, thread: Some(thread) })
    }

    pub fn state(&self) -> State {
        self.shared.0.lock().unwrap().state.clone().unwrap_or(State::Signaling)
    }

    pub fn stats(&self) -> Stats {
        self.shared.0.lock().unwrap().stats.clone()
    }

    /// The latest picture, if any.
    pub fn frame(&self) -> Option<Frame> {
        self.shared.0.lock().unwrap().frame.clone()
    }

    /// Wait for a picture newer than `after` (its `number`), up to `timeout`.
    pub fn wait_frame(&self, after: u64, timeout: Duration) -> Result<Frame, String> {
        let until = Instant::now() + timeout;
        let (lock, cv) = &*self.shared;
        let mut g = lock.lock().unwrap();
        loop {
            if let Some(f) = g.frame.as_ref().filter(|f| f.number > after) {
                return Ok(f.clone());
            }
            if let Some(State::Closed(why)) = &g.state {
                return Err(why.clone());
            }
            let now = Instant::now();
            if now >= until {
                // Connected, yet not one video packet: the JetKVM has no HDMI signal.
                if matches!(g.state, Some(State::Connected { .. })) && g.stats.video_packets == 0 {
                    return Err(no_picture_help(&self.sku));
                }
                return Err(format!("no picture after {:.1}s ({:?})", timeout.as_secs_f64(), g.state.clone().unwrap()));
            }
            g = cv.wait_timeout(g, until - now).unwrap().0;
        }
    }

    /// The model, from `/device/status` (e.g. "jetkvm-mini-ethernet").
    pub fn sku(&self) -> &str {
        &self.sku
    }

    /// Wait until the hidrpc channel is ready.
    pub fn wait_hid(&self, timeout: Duration) -> Result<(), String> {
        let until = Instant::now() + timeout;
        let (lock, cv) = &*self.shared;
        let mut g = lock.lock().unwrap();
        loop {
            match &g.state {
                Some(State::Connected { hid: true }) => return Ok(()),
                Some(State::Closed(why)) => return Err(why.clone()),
                _ => {}
            }
            let now = Instant::now();
            if now >= until {
                return Err(format!("keyboard channel not ready ({:?})", g.state.clone().unwrap()));
            }
            g = cv.wait_timeout(g, until - now).unwrap().0;
        }
    }

    /// Caution: on the Mini (firmware 0.5.20) an all-zero report did not
    /// release the keys; use `key` for taps.
    ///
    /// The whole keyboard state: modifier bits (USB HID: bit 0 left ctrl …
    /// bit 7 right GUI) and up to six pressed keys (HID usages).
    pub fn keyboard(&self, modifiers: u8, keys: &[u8]) {
        let mut m = vec![hid::KEYBOARD_REPORT, modifiers];
        let mut k = [0u8; 6];
        for (slot, key) in k.iter_mut().zip(keys) {
            *slot = *key;
        }
        m.extend_from_slice(&k);
        let _ = self.tx.send(Cmd::Hid(m));
    }

    /// One key (HID usage, modifiers included) down or up. The firmware
    /// tracks what's held; on the Mini this is the message that releases
    /// reliably (an all-zero `keyboard` report didn't).
    pub fn key(&self, usage: u8, down: bool) {
        let _ = self.tx.send(Cmd::Hid(vec![hid::KEYPRESS_REPORT, usage, down as u8]));
    }

    /// Send a JSON-RPC call with no parameters; the answer is logged with
    /// JETKVM_DEBUG (for diagnostics such as getVideoLogStatus).
    pub fn rpc(&self, method: &str) {
        let _ = self.tx.send(Cmd::Rpc(method.to_string()));
    }

    /// Ask the JetKVM for a full picture (a keyframe).
    pub fn request_keyframe(&self) {
        let _ = self.tx.send(Cmd::Keyframe);
    }

    /// Relative mouse movement and buttons (bit 0 left, 1 right, 2 middle).
    pub fn mouse_relative(&self, dx: i8, dy: i8, buttons: u8) {
        let _ = self.tx.send(Cmd::Hid(vec![hid::MOUSE_REPORT, dx as u8, dy as u8, buttons]));
    }

    /// Absolute pointer: 0..=POINTER_MAX on each axis, and buttons.
    pub fn pointer(&self, x: u32, y: u32, buttons: u8) {
        let mut m = vec![hid::POINTER_REPORT];
        m.extend_from_slice(&(x.min(POINTER_MAX) as i32).to_be_bytes());
        m.extend_from_slice(&(y.min(POINTER_MAX) as i32).to_be_bytes());
        m.push(buttons);
        let _ = self.tx.send(Cmd::Hid(m));
    }

    /// Scroll wheel: positive is up.
    pub fn wheel(&self, delta: i8) {
        let _ = self.tx.send(Cmd::Hid(vec![hid::WHEEL_REPORT, delta as u8]));
    }

    pub fn close(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The mids of an SDP's media sections, in order.
fn sdp_mids(sdp: &str) -> Vec<String> {
    sdp.lines().filter_map(|l| l.trim_end().strip_prefix("a=mid:")).map(str::to_string).collect()
}

/// Rename mids (`a=mid:` lines and the BUNDLE group), `from[i]` to `to[i]`.
fn rename_mids(sdp: &str, from: &[String], to: &[String]) -> String {
    let map = |m: &str| from.iter().position(|f| f == m).map_or(m.to_string(), |i| to[i].clone());
    sdp.split("\r\n")
        .map(|l| {
            if let Some(m) = l.strip_prefix("a=mid:") {
                format!("a=mid:{}", map(m))
            } else if let Some(g) = l.strip_prefix("a=group:BUNDLE ") {
                format!("a=group:BUNDLE {}", g.split(' ').map(map).collect::<Vec<_>>().join(" "))
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// esp-webrtc puts each section's `c=` line after its `a=` lines; SDP
/// wants it right after `m=`, and str0m stops reading at it otherwise.
fn tidy_answer(sdp: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut section_start = None;
    for line in sdp.split("\r\n") {
        if line.starts_with("m=") {
            out.push(line);
            section_start = Some(out.len());
        } else if line.starts_with("c=") {
            match section_start {
                Some(i) => out.insert(i, line),
                None => out.push(line),
            }
        } else {
            out.push(line);
        }
    }
    out.join("\r\n")
}

fn signal_loop(mut ws: tungstenite::WebSocket<TcpStream>, tx: Sender<Cmd>, stop: Arc<AtomicBool>) {
    #[derive(Deserialize)]
    struct Msg {
        #[serde(rename = "type", default)]
        kind: String,
        #[serde(default)]
        data: serde_json::Value,
        #[serde(default)]
        error: Option<serde_json::Value>,
    }
    while !stop.load(Ordering::Relaxed) {
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                let Ok(m) = serde_json::from_str::<Msg>(&t) else { continue };
                if let Some(e) = m.error {
                    let _ = tx.send(Cmd::SignalClosed(format!("JetKVM refused the session: {e}")));
                    return;
                }
                let sig = match m.kind.as_str() {
                    "answer" => m.data.as_str().map(|s| Signal::Answer(s.to_string())),
                    "new-ice-candidate" => {
                        m.data.get("candidate").and_then(|c| c.as_str()).map(|c| Signal::Candidate(c.to_string()))
                    }
                    _ => None,
                };
                if let Some(s) = sig {
                    if tx.send(Cmd::Signal(s)).is_err() {
                        return;
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => {
                let _ = tx.send(Cmd::SignalClosed("signaling closed".into()));
                return;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => {
                let _ = tx.send(Cmd::SignalClosed(format!("signaling: {e}")));
                return;
            }
        }
    }
    let _ = ws.close(None);
    let _ = ws.flush();
    // Let the close frame go out before the socket drops.
    let mut sink = [0u8; 64];
    let _ = ws.get_mut().read(&mut sink);
}

struct Worker {
    rtc: Rtc,
    socket: UdpSocket,
    local: SocketAddr,
    rx: Receiver<Cmd>,
    shared: Arc<(Mutex<Shared>, Condvar)>,
    stop: Arc<AtomicBool>,
    pending: Option<SdpPendingOffer>,
    /// Our offer's mids, in order (the device sees them as "0", "1", ...).
    mids: Vec<String>,
    video: Mid,
    hid_channel: ChannelId,
    /// JSON-RPC to the device (the web page's "rpc" channel).
    rpc_channel: ChannelId,
    rpc_id: u64,
    hid_open: bool,
    /// Encoded frames to the decoder thread, which keeps this one free for
    /// the network (decoding 1080p inline let the socket back up).
    video_tx: Sender<Vec<u8>>,
    asked_keyframe: Option<Instant>,
}

impl Worker {
    fn set_state(&self, state: State) {
        let (lock, cv) = &*self.shared;
        let mut g = lock.lock().unwrap();
        if matches!(g.state, Some(State::Closed(_))) {
            return;
        }
        g.state = Some(state);
        cv.notify_all();
    }

    fn run(mut self) {
        let why = match self.run_inner() {
            Ok(()) => "closed".to_string(),
            Err(e) => e,
        };
        self.rtc.disconnect();
        self.set_state(State::Closed(why));
    }

    fn run_inner(&mut self) -> Result<(), String> {
        let mut buf = vec![0u8; 2000];
        loop {
            let timeout = self.drain()?;
            if self.stop.load(Ordering::Relaxed) {
                return Ok(());
            }

            // One mutation per drain: a command, else a packet, else the timeout.
            match self.rx.try_recv() {
                Ok(cmd) => {
                    self.command(cmd)?;
                    continue;
                }
                Err(TryRecvError::Disconnected) => return Ok(()),
                Err(TryRecvError::Empty) => {}
            }

            // Short waits so commands (input) don't sit behind a long timeout.
            let wait = timeout.saturating_duration_since(Instant::now()).clamp(Duration::from_millis(1), Duration::from_millis(4));
            self.socket.set_read_timeout(Some(wait)).map_err(|e| e.to_string())?;
            buf.resize(2000, 0);
            let input = match self.socket.recv_from(&mut buf) {
                Ok((n, source)) => {
                    buf.truncate(n);
                    match buf.as_slice().try_into() {
                        Ok(contents) => Input::Receive(
                            Instant::now(),
                            Receive { proto: Protocol::Udp, source, destination: self.local, contents },
                        ),
                        Err(_) => continue,
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Input::Timeout(Instant::now()),
                Err(e) => return Err(format!("udp: {e}")),
            };
            self.rtc.handle_input(input).map_err(|e| format!("webrtc: {e}"))?;
        }
    }

    fn command(&mut self, cmd: Cmd) -> Result<(), String> {
        match cmd {
            Cmd::Rpc(m) => self.rpc(&m),
            Cmd::Keyframe => {
                self.asked_keyframe = None;
                self.request_keyframe();
            }
            Cmd::Hid(m) => {
                if !self.hid_open {
                    return Ok(());
                }
                if let Some(mut ch) = self.rtc.channel(self.hid_channel) {
                    ch.write(true, &m).map_err(|e| format!("hidrpc: {e}"))?;
                    self.shared.0.lock().unwrap().stats.hid_sent += 1;
                }
            }
            Cmd::Signal(Signal::Answer(b64)) => {
                let json = B64.decode(b64.trim()).map_err(|e| format!("answer: {e}"))?;
                let v: serde_json::Value = serde_json::from_slice(&json).map_err(|e| format!("answer: {e}"))?;
                let sdp = v.get("sdp").and_then(|s| s.as_str()).ok_or("answer without sdp")?;
                if std::env::var_os("JETKVM_DEBUG").is_some() {
                    eprintln!("jetkvm answer:\n{sdp}");
                }
                let numbered: Vec<String> = (0..self.mids.len()).map(|i| i.to_string()).collect();
                let sdp = tidy_answer(&rename_mids(sdp, &numbered, &self.mids));
                let answer = SdpAnswer::from_sdp_string(&sdp).map_err(|e| format!("answer: {e}"))?;
                let pending = self.pending.take().ok_or("second answer")?;
                self.rtc.sdp_api().accept_answer(pending, answer).map_err(|e| format!("answer: {e}"))?;
                self.set_state(State::Connecting);
            }
            Cmd::Signal(Signal::Candidate(c)) => {
                let c = c.strip_prefix("a=").unwrap_or(&c);
                // Only IPv4 UDP host/srflx: that's what our socket can reach.
                if let Ok(cand) = Candidate::from_sdp_string(c) {
                    if cand.addr().is_ipv4() && cand.proto() == Protocol::Udp {
                        self.rtc.add_remote_candidate(cand);
                    }
                }
            }
            Cmd::SignalClosed(why) => {
                // After connecting, the media keeps going without signaling.
                if !matches!(self.state(), State::Connected { .. }) {
                    return Err(why);
                }
            }
        }
        Ok(())
    }

    fn state(&self) -> State {
        self.shared.0.lock().unwrap().state.clone().unwrap_or(State::Signaling)
    }

    /// Poll until the engine wants to wait; returns when to wake up.
    fn drain(&mut self) -> Result<Instant, String> {
        loop {
            match self.rtc.poll_output().map_err(|e| format!("webrtc: {e}"))? {
                Output::Timeout(t) => return Ok(t),
                Output::Transmit(t) => {
                    let _ = self.socket.send_to(&t.contents, t.destination);
                }
                Output::Event(e) => self.event(e)?,
            }
        }
    }

    fn event(&mut self, e: Event) -> Result<(), String> {
        match e {
            Event::Connected => {
                self.set_state(State::Connected { hid: false });
                self.request_keyframe();
            }
            Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                return Err("connection lost (another viewer took over?)".into());
            }
            Event::ChannelData(d) if d.id == self.rpc_channel => {
                if std::env::var_os("JETKVM_DEBUG").is_some() {
                    eprintln!("rpc: {}", String::from_utf8_lossy(&d.data));
                }
            }
            Event::ChannelOpen(id, _) if id == self.hid_channel => {
                self.hid_open = true;
                if let Some(mut ch) = self.rtc.channel(id) {
                    ch.write(true, &[hid::HANDSHAKE, hid::VERSION]).map_err(|e| format!("hidrpc: {e}"))?;
                }
            }
            Event::ChannelData(d) if d.id == self.hid_channel => {
                if d.data.first() == Some(&hid::HANDSHAKE) {
                    self.set_state(State::Connected { hid: true });
                }
            }
            Event::ChannelClose(id) if id == self.hid_channel => {
                self.hid_open = false;
            }
            Event::MediaData(m) if m.mid == self.video => {
                {
                    let mut g = self.shared.0.lock().unwrap();
                    g.stats.video_packets += 1;
                    if g.stats.codec.is_empty() {
                        g.stats.codec = format!("{:?}", m.params.spec().codec);
                    }
                }
                self.decode(&m.data, m.contiguous);
            }
            _ => {}
        }
        Ok(())
    }

    /// Send a JSON-RPC call (no params) on the rpc channel.
    fn rpc(&mut self, method: &str) {
        self.rpc_id += 1;
        let msg = serde_json::json!({ "jsonrpc": "2.0", "id": self.rpc_id, "method": method, "params": {} }).to_string();
        if let Some(mut ch) = self.rtc.channel(self.rpc_channel) {
            let _ = ch.write(false, msg.as_bytes());
        }
    }

    fn request_keyframe(&mut self) {
        if self.asked_keyframe.is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
            return;
        }
        self.asked_keyframe = Some(Instant::now());
        if let Some(mut w) = self.rtc.writer(self.video) {
            let _ = w.request_keyframe(None, KeyframeRequestKind::Pli);
        }
    }

    fn decode(&mut self, data: &[u8], contiguous: bool) {
        if !contiguous {
            self.request_keyframe();
        }
        let _ = self.video_tx.send(data.to_vec());
    }
}

/// The decoder thread: H.264 in, the latest BGRA picture out (`Shared`).
/// Ends when the connection's worker drops its sender.
fn decode_loop(mut decoder: Decoder, rx: Receiver<Vec<u8>>, shared: Arc<(Mutex<Shared>, Condvar)>, cmd: Sender<Cmd>) {
    let mut frames = 0u64;
    while let Ok(data) = rx.recv() {
        let picture = match decoder.decode(&data) {
            Ok(Some(yuv)) => {
                let (w, h) = yuv.dimensions();
                let mut bgra = vec![0u8; w * h * 4];
                yuv.write_rgba8(&mut bgra);
                for px in bgra.chunks_exact_mut(4) {
                    px.swap(0, 2);
                }
                Some((w as u32, h as u32, bgra))
            }
            Ok(None) => None,
            Err(_) => {
                shared.0.lock().unwrap().stats.decode_errors += 1;
                let _ = cmd.send(Cmd::Keyframe);
                None
            }
        };
        if let Some((width, height, bgra)) = picture {
            frames += 1;
            let (lock, cv) = &*shared;
            let mut g = lock.lock().unwrap();
            g.stats.decoded += 1;
            g.frame = Some(Frame { width, height, bgra, number: frames, received: Instant::now() });
            cv.notify_all();
        }
    }
}
