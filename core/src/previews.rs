//! Previews for the host list: a small picture of each MiSTer and what it's
//! running. The open host's comes from its own video every couple of
//! seconds; the others' from a short look at their capture server stream
//! (one small picture, no input, so nobody's control is touched), each
//! every 30 s and never two looks within 5 s. Hosts with a capture card on
//! this computer keep the picture from when they were last open.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use kvms_codec::{jpeg, scale, Bgra};
use mkvm::{Audio, StreamAddress, Video};

use crate::link::{LinkMedia, MkvmLink, Remote};
use crate::session::ConnectionState;
use crate::video::crop;

/// The open host's picture is taken this often.
pub(crate) const OPEN_EVERY: Duration = Duration::from_secs(2);
/// Each other host is looked at this often…
pub(crate) const PEEK_EVERY: Duration = Duration::from_secs(30);
/// …and looks start at least this far apart.
pub(crate) const PEEK_GAP: Duration = Duration::from_secs(5);
/// A look gives up after this long without a picture.
const PEEK_TIMEOUT: Duration = Duration::from_secs(8);
/// Previews are at most this big (a sidebar on a Retina screen).
const MAX_W: u32 = 480;
const MAX_H: u32 = 360;
const QUALITY: u8 = 80;

/// One host in the list.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HostPreview {
    pub id: String,
    /// The picture as a JPEG (at most 480×360, borders cropped). None until
    /// there is one.
    pub jpeg: Option<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    /// What to show under the name: the core ("No core running" in the
    /// MiSTer menu), "Offline", "Connecting…", or "" when not known.
    pub caption: String,
    /// Bumped whenever the picture changes, so a shell only decodes new ones.
    pub version: u64,
}

#[derive(Default)]
pub(crate) struct Previews {
    hosts: HashMap<String, HostPreview>,
    /// When each host is next looked at.
    due: HashMap<String, Instant>,
    last_peek: Option<Instant>,
    last_open: Option<Instant>,
    peeking: bool,
    version: u64,
}

/// "SNES", or "No core running" for the menu ("" from mrext).
pub(crate) fn core_caption(core: &str) -> String {
    let core = core.trim();
    if core.is_empty() { "No core running".into() } else { core.into() }
}

/// The open host's caption from its connection.
pub(crate) fn open_caption(connection: &ConnectionState, core: &str) -> String {
    match connection {
        ConnectionState::Connected => core_caption(core),
        ConnectionState::Connecting => "Connecting…".into(),
        ConnectionState::Disconnected { .. } => "Offline".into(),
    }
}

impl Previews {
    pub fn list(&self, ids: &[String]) -> Vec<HostPreview> {
        ids.iter()
            .map(|id| {
                self.hosts.get(id).cloned().unwrap_or_else(|| HostPreview {
                    id: id.clone(),
                    jpeg: None,
                    width: 0,
                    height: 0,
                    caption: String::new(),
                    version: 0,
                })
            })
            .collect()
    }

    /// Forget hosts that were deleted.
    pub fn retain(&mut self, ids: &[String]) {
        self.hosts.retain(|id, _| ids.contains(id));
        self.due.retain(|id, _| ids.contains(id));
    }

    /// Time to take the open host's picture again?
    pub fn open_due(&mut self, now: Instant) -> bool {
        if self.last_open.is_some_and(|t| now.duration_since(t) < OPEN_EVERY) {
            return false;
        }
        self.last_open = Some(now);
        true
    }

    /// Which of `candidates` (hosts that can be looked at) to look at now,
    /// if any: the most overdue, if no look is running and the last one
    /// started at least PEEK_GAP ago. Marks the look as started.
    pub fn next_peek(&mut self, candidates: &[String], now: Instant) -> Option<String> {
        if self.peeking || self.last_peek.is_some_and(|t| now.duration_since(t) < PEEK_GAP) {
            return None;
        }
        let id = candidates
            .iter()
            .map(|id| (id, self.due.get(id).copied()))
            .filter(|(_, due)| due.is_none_or(|d| d <= now))
            // Never looked at first, then the longest overdue.
            .min_by_key(|(_, due)| due.map(|d| (1, d)).unwrap_or((0, now)))
            .map(|(id, _)| id.clone())?;
        self.peeking = true;
        self.last_peek = Some(now);
        self.due.insert(id.clone(), now + PEEK_EVERY);
        Some(id)
    }

    pub fn peek_done(&mut self) {
        self.peeking = false;
    }

    /// The open host was just closed or refreshed: its picture is fresh,
    /// so it isn't looked at for a while.
    pub fn fresh(&mut self, id: &str, now: Instant) {
        self.due.insert(id.to_string(), now + PEEK_EVERY);
    }

    /// Set a host's picture (None: keep the one it has) and caption.
    /// Returns whether anything changed.
    pub fn set(&mut self, id: &str, picture: Option<(Vec<u8>, u32, u32)>, caption: String) -> bool {
        let e = self.hosts.entry(id.to_string()).or_insert_with(|| HostPreview {
            id: id.to_string(),
            jpeg: None,
            width: 0,
            height: 0,
            caption: String::new(),
            version: 0,
        });
        let mut changed = e.caption != caption;
        e.caption = caption;
        if let Some((jpeg, w, h)) = picture {
            self.version += 1;
            e.jpeg = Some(jpeg);
            e.width = w;
            e.height = h;
            e.version = self.version;
            changed = true;
        }
        changed
    }
}

/// `picture` cropped of MiSTer's borders (unless `cropped` already),
/// scaled to fit the preview size, as a JPEG.
pub(crate) fn encode(picture: &Bgra, cropped: bool) -> Option<(Vec<u8>, u32, u32)> {
    if picture.width == 0 || picture.height == 0 {
        return None;
    }
    let src = if cropped {
        None
    } else {
        crop::detect(&picture.data, picture.stride as usize, picture.width, picture.height)
            .filter(|c| (c.w, c.h) != (picture.width, picture.height))
            .map(|c| {
                let mut data = Vec::with_capacity((c.w * c.h * 4) as usize);
                for row in c.y..c.y + c.h {
                    let start = (row * picture.stride + c.x * 4) as usize;
                    data.extend_from_slice(&picture.data[start..start + (c.w * 4) as usize]);
                }
                Bgra { width: c.w, height: c.h, stride: c.w * 4, data }
            })
    };
    let src = src.as_ref().unwrap_or(picture);
    let (w, h) = scale::fit(src.width, src.height, MAX_W, MAX_H);
    let mut small = Bgra::default();
    scale::scale(src, w, h, &mut small);
    let mut out = Vec::new();
    jpeg::encode(&small, QUALITY, &mut out).ok()?;
    Some((out, w, h))
}

/// What a look at a stream found.
pub(crate) struct Peek {
    pub picture: Option<(Vec<u8>, u32, u32)>,
    pub caption: String,
    /// The server's name for the stream.
    pub stream_name: Option<String>,
}

/// The first picture a look gets.
#[derive(Default)]
struct FirstPicture {
    jpeg: Mutex<Option<Vec<u8>>>,
    arrived: Condvar,
}

impl LinkMedia for FirstPicture {
    fn video(&self, video: &Video, _timestamp: i64) {
        let mut jpeg = self.jpeg.lock().unwrap();
        if jpeg.is_none() {
            *jpeg = Some(video.data.clone());
            self.arrived.notify_all();
        }
    }

    fn audio(&self, _audio: &Audio, _timestamp: i64) {}

    fn state(&self, _state: &str) {}
}

/// Connect to a stream as a viewer, take one small picture and what the
/// MiSTer is running, and leave. Blocks up to PEEK_TIMEOUT.
pub(crate) fn peek(address: StreamAddress, viewer: String, remote: Option<Remote>) -> Peek {
    let link = MkvmLink::connect_with(address, viewer, remote);
    link.set_limits(Some(MAX_W), Some(MAX_H), Some(2.0), Some(QUALITY));
    let media = Arc::new(FirstPicture::default());
    link.set_media(Some(media.clone()));
    let deadline = Instant::now() + PEEK_TIMEOUT;
    let jpeg = {
        let mut jpeg = media.jpeg.lock().unwrap();
        while jpeg.is_none() && Instant::now() < deadline {
            jpeg = media.arrived.wait_timeout(jpeg, Duration::from_millis(200)).unwrap().0;
            // The server can't be reached: don't wait it out.
            let st = link.stream_state();
            if !st.connected && !st.error.is_empty() && Instant::now() + PEEK_TIMEOUT / 2 > deadline {
                break;
            }
        }
        jpeg.take()
    };
    let state = link.stream_state();
    let core = link.status_value("coreRunning");
    let stream_name = link.stream_name();
    link.close();
    let picture = jpeg.and_then(|data| {
        let mut pic = Bgra::default();
        jpeg::decode(&data, &mut pic).ok()?;
        encode(&pic, false)
    });
    let caption = if !state.connected {
        "Offline".into()
    } else if !state.mister_connected {
        "MiSTer offline".into()
    } else {
        core.map(|c| core_caption(&c)).unwrap_or_default()
    };
    Peek { picture, caption, stream_name }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("h{i}")).collect()
    }

    #[test]
    fn looks_are_spread_out_and_repeat_every_30s() {
        let mut p = Previews::default();
        let hosts = ids(3);
        let t0 = Instant::now();
        let first = p.next_peek(&hosts, t0).unwrap();
        // One at a time, and not again within the gap.
        assert_eq!(p.next_peek(&hosts, t0), None);
        p.peek_done();
        assert_eq!(p.next_peek(&hosts, t0 + Duration::from_secs(4)), None);
        let second = p.next_peek(&hosts, t0 + PEEK_GAP).unwrap();
        assert_ne!(first, second);
        p.peek_done();
        let third = p.next_peek(&hosts, t0 + PEEK_GAP * 2).unwrap();
        p.peek_done();
        assert!(![&first, &second].contains(&&third));
        // All seen: nobody's due until 30 s after their look.
        assert_eq!(p.next_peek(&hosts, t0 + PEEK_GAP * 3), None);
        assert_eq!(p.next_peek(&hosts, t0 + PEEK_EVERY), Some(first));
    }

    /// A look against a fake capture server: one picture and the core, no
    /// input sent.
    #[test]
    fn a_look_gets_a_picture_and_the_core() {
        use mkvm::*;
        use std::io::BufReader;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut read = BufReader::new(sock.try_clone().unwrap());
            let Message::Hello(hello) = read_message(&mut read).unwrap().0 else { panic!("no hello") };
            let info = StreamInfo {
                name: hello.stream,
                mister: Some("10.0.0.5".into()),
                width: 640,
                height: 480,
                fps: 60.0,
                audio_rate: 48000,
                audio_channels: 2,
                sizes: vec![],
                codecs: vec![Codec::Jpeg],
            };
            let welcome = Welcome { protocol: 1, server: "test".into(), server_id: String::new(), viewer_id: 1, stream: info };
            write_message(&mut sock, &Message::Welcome(welcome), 0).unwrap();
            write_message(&mut sock, &Message::Mister(MisterState { connected: true, reason: String::new() }), 0).unwrap();
            write_message(&mut sock, &Message::Status(Status { key: "coreRunning".into(), value: "SNES".into() }), 0).unwrap();
            let mut data = Vec::new();
            jpeg::encode(&Bgra { data: vec![200; 640 * 480 * 4], ..Bgra::new(640, 480) }, 80, &mut data).unwrap();
            let mut sent = false;
            loop {
                match read_message(&mut read) {
                    Ok((Message::Subscribe(sub), _)) => {
                        tx.send(format!("subscribe {:?}x{:?} video {}", sub.max_width, sub.max_height, sub.video)).unwrap();
                        if sub.video && !sent {
                            sent = true;
                            let video = Video { codec: Codec::Jpeg, keyframe: true, width: 640, height: 480, data: data.clone() };
                            write_message(&mut sock, &Message::Video(video), 0).unwrap();
                        }
                    }
                    Ok((Message::Input(_), _)) | Ok((Message::TakeControl, _)) => tx.send("input".into()).unwrap(),
                    Ok(_) => {}
                    Err(_) => return,
                }
            }
        });
        let address = StreamAddress::parse(&format!("mkvm://127.0.0.1:{port}/MiSTer")).unwrap();
        let found = peek(address, "Test (preview)".into(), None);
        assert_eq!(found.caption, "SNES");
        let (_, w, h) = found.picture.expect("a picture");
        assert_eq!((w, h), (MAX_W, MAX_H));
        let said: Vec<String> = rx.try_iter().collect();
        assert!(said.iter().any(|s| s == "subscribe Some(480)xSome(360) video true"), "{said:?}");
        assert!(!said.iter().any(|s| s == "input"), "{said:?}");
    }

    #[test]
    fn a_look_at_nothing_says_offline() {
        // Port 9: nothing listens.
        let address = StreamAddress::parse("mkvm://127.0.0.1:9/MiSTer").unwrap();
        let t = Instant::now();
        let found = peek(address, "Test (preview)".into(), None);
        assert_eq!(found.caption, "Offline");
        assert!(found.picture.is_none());
        assert!(t.elapsed() < PEEK_TIMEOUT);
    }

    #[test]
    fn the_open_host_refreshes_every_2s() {
        let mut p = Previews::default();
        let t0 = Instant::now();
        assert!(p.open_due(t0));
        assert!(!p.open_due(t0 + Duration::from_secs(1)));
        assert!(p.open_due(t0 + OPEN_EVERY));
    }

    #[test]
    fn pictures_are_cropped_scaled_and_versioned() {
        // 640×480 black with a 320×240 grey picture in the middle.
        let mut pic = Bgra::new(640, 480);
        for y in 120..360 {
            for x in 160..480 {
                let i = (y * 640 + x) * 4;
                pic.data[i..i + 4].copy_from_slice(&[128, 128, 128, 255]);
            }
        }
        let (jpeg, w, h) = encode(&pic, false).unwrap();
        assert_eq!((w, h), (320, 240));
        let mut p = Previews::default();
        assert!(p.set("a", Some((jpeg, w, h)), core_caption("")));
        let list = p.list(&["a".into(), "b".into()]);
        assert_eq!(list[0].caption, "No core running");
        assert_eq!(list[0].version, 1);
        assert_eq!(list[1].jpeg, None);
        assert!(!p.set("a", None, "No core running".into()));
        assert!(p.set("a", None, "SNES".into()));
        assert_eq!(p.list(&["a".into()])[0].version, 1);
    }
}
