//! A connection to one stream on a `misterkvms-capture` server (mkvm
//! protocol). One link carries a host's video, audio and input for as long
//! as the host is open; it reconnects by itself. The video session and the
//! input session both use it (docs/CAPTURE-SERVER.md §4, §7).
//!
//! Connections are always encrypted (mkvm::secure), against the capture PC's
//! identity, which the host remembers; a paired device proves its key
//! inside. A paired device connects at home or, when home doesn't answer,
//! at the server's internet address; from the internet it asks for fewer,
//! lighter pictures (docs/IPHONE.md §3–4).

use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use mkvm::secure::{self, DeviceKey, SecureReader, SecureWriter};
use mkvm::{
    read_message, write_message, Audio, Codec, ControlState, Device, Hello, InputCmd, Message, MisterState, Pair, Paired, StreamAddress,
    Subscribe, Video, Welcome,
};

use crate::log;
use crate::session::{ConnectionState, SessionObserver};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
const PING_EVERY: Duration = Duration::from_secs(2);
/// No message for this long: the server is gone even if TCP hasn't noticed.
const SILENT_LIMIT: Duration = Duration::from_secs(8);
/// With an internet address to fall back on, the other route gets this long.
const FIRST_TRY_TIMEOUT: Duration = Duration::from_millis(1500);
/// From the internet a viewer asks for at most this, unless it asked for less.
const INTERNET_FPS: f64 = 30.0;
const INTERNET_QUALITY: u8 = 70;

/// The capture PC identity a host answered with before, checked on every
/// connection (another computer answering in its place is refused).
#[derive(Clone, Default)]
pub struct Security {
    pub pinned: Option<[u8; secure::PUBLIC_KEY_LEN]>,
    /// Told the identity the first time, to remember for the host.
    pub learned: Option<Arc<dyn Fn([u8; secure::PUBLIC_KEY_LEN]) + Send + Sync>>,
}

impl std::fmt::Debug for Security {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Security(pinned: {})", self.pinned.is_some())
    }
}

/// A paired device's key, and where the server is reached from the internet.
#[derive(Debug, Clone)]
pub struct Remote {
    pub key: DeviceKey,
    pub internet: Option<StreamAddress>,
}

/// Receives a stream's pictures and sound (the video session). Called on
/// the link's reader thread; timestamps are already on this machine's clock.
pub(crate) trait LinkMedia: Send + Sync {
    fn video(&self, video: &Video, timestamp: i64);
    fn audio(&self, audio: &Audio, timestamp: i64);
    /// "connected", "no signal", "connecting", or an error.
    fn state(&self, state: &str);
}

/// What the UI shows about a server stream.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, uniffi::Record)]
#[serde(rename_all = "camelCase")]
pub struct StreamState {
    /// Connected to the capture server.
    pub connected: bool,
    /// Why not, when not connected.
    pub error: String,
    /// The server is connected to the stream's MiSTer.
    pub mister_connected: bool,
    pub mister: String,
    /// This viewer controls the MiSTer.
    pub have_control: bool,
    /// Who does, when someone else does (empty if nobody).
    pub holder: String,
    pub round_trip_ms: f64,
    /// Connected from the internet (encrypted), not at home.
    pub internet: bool,
    /// The capture PC's permanent id (empty: an older server).
    pub server_id: String,
}

#[derive(Default)]
struct State {
    connected: bool,
    /// The connection goes through the internet address.
    internet: bool,
    /// The route that worked last, tried first next time.
    prefer_internet: bool,
    error: String,
    welcome: Option<Welcome>,
    control: Option<ControlState>,
    mister: Option<MisterState>,
    status: HashMap<String, String>,
    /// Server clock minus ours, 100 ns units.
    clock_offset: i64,
    round_trip_ms: f64,
}

pub struct MkvmLink {
    address: StreamAddress,
    viewer: String,
    stop: AtomicBool,
    state: Mutex<State>,
    remote: Option<Remote>,
    security: Security,
    /// The open connection: where messages are written, and its socket.
    conn: Mutex<Option<Conn>>,
    subscription: Mutex<Subscribe>,
    media: Mutex<Option<Arc<dyn LinkMedia>>>,
    observer: Mutex<Option<Arc<dyn SessionObserver>>>,
}

impl MkvmLink {
    /// Start connecting to `address` (`mkvm://host/stream`). `viewer` is the
    /// name other viewers see.
    pub fn connect(address: StreamAddress, viewer: String) -> Arc<Self> {
        Self::connect_with(address, viewer, None, Security::default())
    }

    /// Like `connect`, as a paired device: encrypted, and from the internet
    /// when home doesn't answer.
    pub fn connect_with(address: StreamAddress, viewer: String, remote: Option<Remote>, security: Security) -> Arc<Self> {
        let link = Arc::new(Self {
            address,
            viewer,
            stop: AtomicBool::new(false),
            state: Mutex::new(State::default()),
            remote,
            security,
            conn: Mutex::new(None),
            subscription: Mutex::new(Subscribe { video: false, audio: false, ..Default::default() }),
            media: Mutex::new(None),
            observer: Mutex::new(None),
        });
        let weak = Arc::downgrade(&link);
        let name = format!("mkvm-{}", link.address.stream);
        thread::Builder::new()
            .name(name)
            .spawn(move || run(weak))
            .expect("spawn mkvm link");
        link
    }

    pub fn address(&self) -> &StreamAddress {
        &self.address
    }

    pub fn close(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(c) = self.conn.lock().unwrap().take() {
            let _ = c.sock.shutdown(Shutdown::Both);
        }
    }

    pub fn stream_state(&self) -> StreamState {
        let s = self.state.lock().unwrap();
        let control = s.control.clone();
        StreamState {
            connected: s.connected,
            error: s.error.clone(),
            mister_connected: s.mister.as_ref().is_some_and(|m| m.connected),
            mister: s.welcome.as_ref().and_then(|w| w.stream.mister.clone()).unwrap_or_default(),
            have_control: control.as_ref().is_some_and(|c| c.you),
            holder: control.and_then(|c| if c.you { None } else { c.holder.map(|h| h.name) }).unwrap_or_default(),
            round_trip_ms: s.round_trip_ms,
            internet: s.connected && s.internet,
            server_id: s.welcome.as_ref().map(|w| w.server_id.clone()).unwrap_or_default(),
        }
    }

    /// Where pictures and sound go (None: stop receiving them).
    pub(crate) fn set_media(&self, media: Option<Arc<dyn LinkMedia>>) {
        let on = media.is_some();
        *self.media.lock().unwrap() = media;
        let sub = {
            let mut sub = self.subscription.lock().unwrap();
            sub.video = on;
            sub.audio = on;
            sub.clone()
        };
        self.send(&Message::Subscribe(self.effective(sub)));
    }

    /// Ask for at most this picture size (None: native), this many pictures
    /// a second and this JPEG quality (None: the server's).
    pub fn set_limits(&self, max_width: Option<u32>, max_height: Option<u32>, max_fps: Option<f64>, quality: Option<u8>) {
        let sub = {
            let mut sub = self.subscription.lock().unwrap();
            sub.max_width = max_width;
            sub.max_height = max_height;
            sub.max_fps = max_fps;
            sub.quality = quality;
            sub.clone()
        };
        self.send(&Message::Subscribe(self.effective(sub)));
    }

    /// What to ask for on this connection: from the internet, at most
    /// INTERNET_FPS pictures at INTERNET_QUALITY.
    fn effective(&self, mut sub: Subscribe) -> Subscribe {
        if self.state.lock().unwrap().internet {
            sub.max_fps = Some(sub.max_fps.map_or(INTERNET_FPS, |f| f.min(INTERNET_FPS)));
            sub.quality = Some(sub.quality.map_or(INTERNET_QUALITY, |q| q.min(INTERNET_QUALITY)));
        }
        sub
    }

    /// Who hears about the connection and MiSTer status (the input session's
    /// observer, i.e. the controller).
    pub(crate) fn set_observer(&self, observer: Option<Arc<dyn SessionObserver>>) {
        *self.observer.lock().unwrap() = observer.clone();
        if let Some(o) = observer {
            o.on_state(self.connection_state());
            let status = self.state.lock().unwrap().status.clone();
            for (k, v) in status {
                o.on_status(k, v);
            }
        }
    }

    /// The capture server's name and where it is, once connected.
    pub(crate) fn server_name(&self) -> Option<String> {
        self.state.lock().unwrap().welcome.as_ref().map(|w| w.server.clone())
    }

    /// The server's name for the stream, once connected.
    pub(crate) fn stream_name(&self) -> Option<String> {
        self.state.lock().unwrap().welcome.as_ref().map(|w| w.stream.name.clone()).filter(|n| !n.trim().is_empty())
    }

    /// A status value mrext pushed through the server, e.g. "coreRunning".
    pub(crate) fn status_value(&self, key: &str) -> Option<String> {
        self.state.lock().unwrap().status.get(key).cloned()
    }

    pub fn take_control(&self) {
        self.send(&Message::TakeControl);
    }

    pub fn release_control(&self) {
        self.send(&Message::ReleaseControl);
    }

    /// Input for the MiSTer. Returns false if not connected.
    pub(crate) fn send_input(&self, cmds: Vec<InputCmd>) -> bool {
        !cmds.is_empty() && self.send(&Message::Input(cmds))
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.state.lock().unwrap().connected
    }

    /// As the input session reports it: connected only when the server is
    /// connected to the MiSTer too.
    fn connection_state(&self) -> ConnectionState {
        let s = self.state.lock().unwrap();
        if !s.connected {
            if s.error.is_empty() {
                ConnectionState::Connecting
            } else {
                ConnectionState::Disconnected { reason: s.error.clone() }
            }
        } else {
            match &s.mister {
                Some(m) if m.connected => ConnectionState::Connected,
                Some(m) if !m.reason.is_empty() => ConnectionState::Disconnected { reason: format!("capture server: {}", m.reason) },
                _ => ConnectionState::Connecting,
            }
        }
    }

    fn send(&self, msg: &Message) -> bool {
        let mut c = self.conn.lock().unwrap();
        let Some(conn) = c.as_mut() else { return false };
        if let Err(e) = write_message(&mut conn.write, msg, mkvm_now()) {
            log(&format!("mkvm: send failed: {e}"));
            let _ = conn.sock.shutdown(Shutdown::Both);
            *c = None;
            return false;
        }
        true
    }

    fn notify_state(&self) {
        let state = self.connection_state();
        let observer = self.observer.lock().unwrap().clone();
        if let Some(o) = observer {
            o.on_state(state);
        }
    }

    fn media_state(&self, text: &str) {
        let media = self.media.lock().unwrap().clone();
        if let Some(m) = media {
            m.state(text);
        }
    }
}

fn mkvm_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| (d.as_nanos() / 100) as i64)
}

fn run(weak: std::sync::Weak<MkvmLink>) {
    loop {
        let Some(link) = weak.upgrade() else { return };
        if link.stop.load(Ordering::Relaxed) {
            return;
        }
        link.media_state("connecting");
        let result = session(&link);
        {
            let mut s = link.state.lock().unwrap();
            s.connected = false;
            s.control = None;
            s.mister = None;
            s.error = match &result {
                Ok(()) => "closed by the capture server".into(),
                Err(e) => e.clone(),
            };
        }
        *link.conn.lock().unwrap() = None;
        if link.stop.load(Ordering::Relaxed) {
            return;
        }
        log(&format!("mkvm: {} {}", link.address, link.state.lock().unwrap().error));
        link.media_state(&link.state.lock().unwrap().error.clone());
        link.notify_state();
        drop(link);
        thread::sleep(RECONNECT_DELAY);
    }
}

struct Conn {
    write: Box<dyn Write + Send>,
    sock: TcpStream,
}

fn tcp(address: &StreamAddress, timeout: Duration) -> Result<TcpStream, String> {
    let addr = address
        .socket()
        .to_socket_addrs()
        .map_err(|e| format!("can't find {}: {e}", address.host))?
        .next()
        .ok_or_else(|| format!("can't find {}", address.host))?;
    TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("can't reach the capture server at {}: {e}", address.host))
}

/// Connect at home or from the internet, the route that worked last first.
/// Returns the socket and whether it goes through the internet.
fn open(link: &MkvmLink) -> Result<(TcpStream, bool), String> {
    let Some(internet) = link.remote.as_ref().and_then(|r| r.internet.clone()) else {
        return tcp(&link.address, CONNECT_TIMEOUT).map(|s| (s, false));
    };
    let mut routes = [(link.address.clone(), false), (internet, true)];
    if link.state.lock().unwrap().prefer_internet {
        routes.reverse();
    }
    let mut errors = Vec::new();
    for (i, (address, internet)) in routes.into_iter().enumerate() {
        match tcp(&address, if i == 0 { FIRST_TRY_TIMEOUT } else { CONNECT_TIMEOUT }) {
            Ok(s) => return Ok((s, internet)),
            Err(e) => errors.push(format!("{}: {e}", if internet { "from the internet" } else { "at home" })),
        }
    }
    Err(errors.join("; "))
}

type Halves = (Box<dyn Read + Send>, Box<dyn Write + Send>);

/// Start a connection's encryption: check the capture PC's identity
/// against the one the host remembers, and for a paired device prove `key`
/// inside. Returns the two halves.
fn secure_open(sock: &TcpStream, security: &Security, key: Option<&DeviceKey>) -> Result<Halves, String> {
    let clone = || sock.try_clone().map_err(|e| e.to_string());
    let opened = secure::connect_nx(&mut clone()?).map_err(|e| e.to_string())?;
    match security.pinned {
        Some(pinned) if pinned == opened.server_key => {}
        Some(_) => {
            return Err("This capture PC answered with a different identity than before. If MiSTerKVMS Capture was \
                        reinstalled there, remove this MiSTer and add it again; otherwise another computer may be \
                        answering in its place."
                .into())
        }
        None => {
            if let Some(learned) = &security.learned {
                learned(opened.server_key);
            }
        }
    }
    let mut write: Box<dyn Write + Send> = Box::new(SecureWriter::new(clone()?, opened.session.clone()));
    if let Some(key) = key {
        let proof = secure::device_proof(&key.key, &opened.handshake);
        let device = Message::Device(Device { key_id: key.id_hex(), proof: secure::hex(&proof) });
        write_message(&mut write, &device, mkvm_now()).map_err(|e| e.to_string())?;
    }
    Ok((Box::new(SecureReader::new(clone()?, opened.session)), write))
}

/// One request on its own connection to `address`.
fn request(address: &StreamAddress, security: &Security, key: Option<&DeviceKey>, timeout: Duration) -> Result<(TcpStream, Halves), String> {
    let sock = tcp(address, CONNECT_TIMEOUT)?;
    sock.set_read_timeout(Some(timeout)).ok();
    let halves = secure_open(&sock, security, key)?;
    Ok((sock, halves))
}

/// Ask a capture server on the home network for a device key, to connect
/// from the internet with later (docs/IPHONE.md §2). `replaces`: the key
/// this device had for it, which the server then drops.
pub fn pair(address: &StreamAddress, device: &str, replaces: Option<String>, security: &Security) -> Result<Paired, String> {
    let (_sock, (read, mut write)) = request(address, security, None, Duration::from_secs(10))?;
    write_message(&mut write, &Message::Pair(Pair { device: device.to_string(), replaces }), mkvm_now()).map_err(|e| e.to_string())?;
    match read_message(&mut BufReader::new(read)).map_err(|e| e.to_string())?.0 {
        Message::Paired(p) => Ok(p),
        Message::Error(e) => Err(e.message),
        other => Err(format!("unexpected answer {other:?}")),
    }
}

/// The capture PC's permanent id, asked of a stream on the home network
/// (Hello, Welcome, hang up). Empty from servers older than server ids.
pub fn server_id(address: &StreamAddress, viewer: &str, security: &Security) -> Result<String, String> {
    let (sock, (read, mut write)) = request(address, security, None, Duration::from_secs(5))?;
    let hello = Message::Hello(Hello {
        protocol: mkvm::PROTOCOL_VERSION,
        stream: address.stream.clone(),
        viewer: viewer.to_string(),
        platform: std::env::consts::OS.into(),
        codecs: vec![Codec::Jpeg],
    });
    write_message(&mut write, &hello, mkvm_now()).map_err(|e| e.to_string())?;
    let answer = read_message(&mut BufReader::new(read)).map_err(|e| e.to_string())?.0;
    let _ = sock.shutdown(Shutdown::Both);
    match answer {
        Message::Welcome(w) => Ok(w.server_id),
        Message::Error(e) => Err(e.message),
        other => Err(format!("unexpected answer {other:?}")),
    }
}

/// Tell the capture PC this device is done with it: it drops the device's
/// key, as removing it on its setup page does. Tried at home, then at the
/// internet address.
pub fn forget(home: &StreamAddress, internet: Option<&StreamAddress>, security: &Security, key: &DeviceKey) -> Result<(), String> {
    let mut errors = Vec::new();
    for (address, label) in [(Some(home), "at home"), (internet, "from the internet")] {
        let Some(address) = address else { continue };
        let attempt = || -> Result<(), String> {
            let (_sock, (read, mut write)) = request(address, security, Some(key), Duration::from_secs(5))?;
            write_message(&mut write, &Message::Forget, mkvm_now()).map_err(|e| e.to_string())?;
            match read_message(&mut BufReader::new(read)).map_err(|e| e.to_string())?.0 {
                Message::Forgotten => Ok(()),
                Message::Error(e) => Err(e.message),
                other => Err(format!("unexpected answer {other:?}")),
            }
        };
        match attempt() {
            Ok(()) => return Ok(()),
            Err(e) => errors.push(format!("{label}: {e}")),
        }
    }
    Err(errors.join("; "))
}

/// One connection, from connect to close.
fn session(link: &Arc<MkvmLink>) -> Result<(), String> {
    let (sock, internet) = open(link)?;
    sock.set_nodelay(true).ok();
    sock.set_read_timeout(Some(SILENT_LIMIT)).ok();
    let (read, mut write) = secure_open(&sock, &link.security, link.remote.as_ref().map(|r| &r.key))?;
    let hello = Message::Hello(Hello {
        protocol: mkvm::PROTOCOL_VERSION,
        stream: link.address.stream.clone(),
        viewer: link.viewer.clone(),
        platform: std::env::consts::OS.into(),
        codecs: vec![Codec::Jpeg],
    });
    write_message(&mut write, &hello, mkvm_now()).map_err(|e| e.to_string())?;
    let mut read = BufReader::with_capacity(256 * 1024, read);
    let welcome = match read_message(&mut read).map_err(|e| e.to_string())?.0 {
        Message::Welcome(w) => w,
        Message::Error(e) => return Err(e.message),
        other => return Err(format!("unexpected {other:?}")),
    };
    log(&format!(
        "mkvm: {} connected{} ({} {}x{}, MiSTer {:?})",
        link.address,
        if internet { " from the internet" } else { "" },
        welcome.server,
        welcome.stream.width,
        welcome.stream.height,
        welcome.stream.mister
    ));
    {
        let mut s = link.state.lock().unwrap();
        s.internet = internet;
        s.prefer_internet = internet;
    }
    let sub = link.subscription.lock().unwrap().clone();
    write_message(&mut write, &Message::Subscribe(link.effective(sub)), mkvm_now()).map_err(|e| e.to_string())?;
    {
        let mut s = link.state.lock().unwrap();
        s.connected = true;
        s.error.clear();
        s.welcome = Some(welcome);
        s.status.clear();
    }
    *link.conn.lock().unwrap() = Some(Conn { write, sock });
    link.notify_state();
    link.media_state("connected");

    let mut last_ping = Instant::now() - PING_EVERY;
    loop {
        if link.stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if last_ping.elapsed() >= PING_EVERY {
            last_ping = Instant::now();
            link.send(&Message::Ping);
        }
        let (msg, header) = read_message(&mut read).map_err(|e| e.to_string())?;
        match msg {
            Message::Video(v) => {
                let ts = header.timestamp - link.state.lock().unwrap().clock_offset;
                let media = link.media.lock().unwrap().clone();
                if let Some(m) = media {
                    m.video(&v, ts);
                }
            }
            Message::Audio(a) => {
                let ts = header.timestamp - link.state.lock().unwrap().clock_offset;
                let media = link.media.lock().unwrap().clone();
                if let Some(m) = media {
                    m.audio(&a, ts);
                }
            }
            Message::Ping => {
                link.send(&Message::Pong { echo: header.timestamp });
            }
            Message::Pong { echo } => {
                let now = mkvm_now();
                let rtt = now - echo;
                if (0..10_000_000).contains(&rtt) {
                    let mut s = link.state.lock().unwrap();
                    let offset = header.timestamp - (echo + rtt / 2);
                    // Smooth: one slow round trip shouldn't move the clock much.
                    s.clock_offset = if s.round_trip_ms == 0.0 { offset } else { (s.clock_offset * 7 + offset) / 8 };
                    s.round_trip_ms = rtt as f64 / 10_000.0;
                }
            }
            Message::Format(f) => {
                if !f.signal {
                    link.media_state("no signal");
                }
            }
            Message::Control(c) => {
                link.state.lock().unwrap().control = Some(c);
                link.notify_state();
            }
            Message::Mister(m) => {
                link.state.lock().unwrap().mister = Some(m);
                link.notify_state();
            }
            Message::Status(st) => {
                link.state.lock().unwrap().status.insert(st.key.clone(), st.value.clone());
                let observer = link.observer.lock().unwrap().clone();
                if let Some(o) = observer {
                    o.on_status(st.key, st.value);
                }
            }
            Message::Error(e) => return Err(e.message),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkvm::secure::ServerIdentity;
    use mkvm::{ErrorMsg, StreamInfo, WireError};
    use std::net::TcpListener;

    fn welcome() -> Message {
        let stream = StreamInfo {
            name: "MiSTer-SS1".into(),
            mister: None,
            width: 0,
            height: 0,
            fps: 0.0,
            audio_rate: 48000,
            audio_channels: 2,
            sizes: vec![],
            codecs: vec![Codec::Jpeg],
        };
        Message::Welcome(Welcome { protocol: 1, server: "test".into(), server_id: "abc".into(), viewer_id: 1, stream })
    }

    /// A capture server from before encrypted connections: it reads the
    /// opening bytes as a message and hangs up on anything but plain mkvm.
    fn older_server() -> StreamAddress {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            for sock in listener.incoming().flatten() {
                let mut read = BufReader::new(sock.try_clone().unwrap());
                let mut sock = sock;
                match read_message(&mut read) {
                    Ok((Message::Hello(_), _)) => write_message(&mut sock, &welcome(), 0).unwrap(),
                    Err(WireError::TooLarge(_)) => {}
                    other => panic!("unexpected {other:?}"),
                }
            }
        });
        StreamAddress::parse(&format!("mkvm://127.0.0.1:{port}/MiSTer-SS1")).unwrap()
    }

    /// A current capture server: encrypted connections, a paired
    /// device's proof, and Forget.
    fn server(identity: ServerIdentity, device: Option<DeviceKey>) -> StreamAddress {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            for mut sock in listener.incoming().flatten() {
                let mut magic = [0u8; 4];
                sock.read_exact(&mut magic).unwrap();
                assert_eq!(&magic, secure::MAGIC_NX);
                secure::read_prelude_nx(&mut sock).unwrap();
                let (session, handshake) = secure::accept_nx(&mut sock, &identity).unwrap();
                let mut read = BufReader::new(SecureReader::new(sock.try_clone().unwrap(), session.clone()));
                let mut write = SecureWriter::new(sock, session);
                // A viewer that refused the identity hangs up here.
                let Ok((mut first, _)) = read_message(&mut read) else { continue };
                if let Message::Device(claim) = &first {
                    let ok = device.as_ref().is_some_and(|d| {
                        claim.key_id == d.id_hex()
                            && secure::check_device_proof(&d.key, &handshake, &secure::unhex::<32>(&claim.proof).unwrap())
                    });
                    if !ok {
                        write_message(&mut write, &Message::Error(ErrorMsg { message: "unknown device".into() }), 0).unwrap();
                        continue;
                    }
                    first = read_message(&mut read).unwrap().0;
                }
                let answer = match first {
                    Message::Hello(_) => welcome(),
                    Message::Forget => Message::Forgotten,
                    other => panic!("unexpected {other:?}"),
                };
                write_message(&mut write, &answer, 0).unwrap();
            }
        });
        StreamAddress::parse(&format!("mkvm://127.0.0.1:{port}/MiSTer-SS1")).unwrap()
    }

    fn remembering(pinned: Option<[u8; 32]>) -> (Security, Arc<Mutex<Option<[u8; 32]>>>) {
        let seen = Arc::new(Mutex::new(None));
        let keep = seen.clone();
        (Security { pinned, learned: Some(Arc::new(move |k| *keep.lock().unwrap() = Some(k))) }, seen)
    }

    #[test]
    fn an_older_server_is_refused_with_a_reason() {
        let refused = server_id(&older_server(), "Pixel", &Security::default()).unwrap_err();
        assert!(refused.contains("update MiSTerKVMS Capture"), "{refused}");
    }

    #[test]
    fn the_capture_pc_identity_is_learned_then_checked() {
        let identity = ServerIdentity::generate().unwrap();
        let public = *identity.public();
        let address = server(identity, None);
        // First time: learned.
        let (security, seen) = remembering(None);
        assert_eq!(server_id(&address, "Pixel", &security).unwrap(), "abc");
        assert_eq!(*seen.lock().unwrap(), Some(public));
        // Same identity: fine, nothing new.
        let (security, seen) = remembering(Some(public));
        server_id(&address, "Pixel", &security).unwrap();
        assert_eq!(*seen.lock().unwrap(), None);
        // Another identity: refused.
        let (security, seen) = remembering(Some([9; 32]));
        assert!(server_id(&address, "Pixel", &security).unwrap_err().contains("different identity"));
        assert_eq!(*seen.lock().unwrap(), None);
    }

    #[test]
    fn a_paired_device_proves_its_key_and_can_be_forgotten() {
        let key = DeviceKey::generate().unwrap();
        let address = server(ServerIdentity::generate().unwrap(), Some(key.clone()));
        forget(&address, None, &Security::default(), &key).unwrap();
        let stranger = DeviceKey::generate().unwrap();
        assert!(forget(&address, None, &Security::default(), &stranger).unwrap_err().contains("unknown device"));
        // An older server can't.
        assert!(forget(&older_server(), None, &Security::default(), &key).is_err());
    }
}

/// For tests' fake capture servers.
#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    /// The server side of a viewer's encrypted connection: the reading and
    /// writing halves, to speak mkvm on.
    pub(crate) fn accept(mut sock: TcpStream) -> (BufReader<SecureReader<TcpStream>>, SecureWriter<TcpStream>) {
        let mut magic = [0u8; 4];
        sock.read_exact(&mut magic).unwrap();
        assert_eq!(&magic, secure::MAGIC_NX);
        secure::read_prelude_nx(&mut sock).unwrap();
        let identity = secure::ServerIdentity::generate().unwrap();
        let (session, _) = secure::accept_nx(&mut sock, &identity).unwrap();
        (BufReader::new(SecureReader::new(sock.try_clone().unwrap(), session.clone())), SecureWriter::new(sock, session))
    }
}
