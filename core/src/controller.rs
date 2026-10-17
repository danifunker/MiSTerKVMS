//! The app's session logic, shared by every shell: saved hosts, the one live
//! MiSTer (input session + video), stats and the overlay rule, saving what
//! the predictive cursor learned, display settings per host, recording.
//!
//! A shell renders `state()` and forwards input. It's told about changes
//! through `ControllerObserver`, called from background threads.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::input::{KeyResult, MouseButton};
use crate::keymap::hid;
use crate::link::{MkvmLink, StreamState};
use crate::log;
use crate::previews::{self, HostPreview, Previews};
use crate::profiles::{HostProfile, ProfileStore, SourceKind};
use crate::screenshot::{self, Screenshot};
use crate::session::{ConnectionState, InputStats, MisterSession, SessionObserver};
use crate::video::ffmpeg::FfmpegRecorder;
use crate::video::{CursorModel, DisplaySettings, FrameOutput, MediaSink, NativeSurface, VideoSession, VideoSource, VideoStats};

const TICK: Duration = Duration::from_millis(500);
/// How long the stats overlay stays up after something changes.
const STATS_FLASH: Duration = Duration::from_secs(5);
/// How often what the predictive cursor learned is saved.
const MODEL_SAVE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, uniffi::Record)]
#[serde(rename_all = "camelCase")]
pub struct ControllerState {
    /// The open host (None = nothing selected).
    pub profile_id: Option<String>,
    pub connection: ConnectionState,
    /// Reported by mrext, e.g. "HP9K715" ("" = MiSTer menu).
    pub core_running: String,
    pub game_running: String,
    /// The shell is capturing keyboard and mouse.
    pub capturing: bool,
    /// Show the stats overlay now (flashed on changes, or pinned).
    pub stats_visible: bool,
    pub stats_pinned: bool,
    pub input_stats: Option<InputStats>,
    pub video_stats: Option<VideoStats>,
    /// Why there's no video (e.g. a GPU error).
    pub video_error: Option<String>,
    pub display: DisplaySettings,
    pub muted: bool,
    /// Seconds since recording started (None = not recording).
    pub recording_seconds: Option<f64>,
    /// Path of the last finished recording.
    pub last_recording: Option<String>,
    pub recording_error: Option<String>,
    /// Capture server hosts: the stream's connection and who has control.
    pub stream: Option<StreamState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum ControllerEvent {
    /// Something in `state()` changed.
    Changed,
    /// The saved hosts changed.
    ProfilesChanged,
    /// The connection just came up: start capturing input (auto-capture),
    /// if the window is active. Only with `capture_automatically`.
    CaptureRequested,
    /// The connection went down: stop capturing.
    ReleaseRequested,
    /// A host's picture or caption in `host_previews()` changed.
    PreviewsChanged,
}

/// Implemented by the shell. Called from background threads; hop to the UI
/// thread before touching UI.
#[uniffi::export(with_foreign)]
pub trait ControllerObserver: Send + Sync {
    fn on_event(&self, event: ControllerEvent);
}

#[derive(Debug, uniffi::Error)]
pub enum ControllerError {
    Failed { message: String },
}

impl std::fmt::Display for ControllerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ControllerError::Failed { message } = self;
        f.write_str(message)
    }
}

impl std::error::Error for ControllerError {}

fn failed(message: impl Into<String>) -> ControllerError {
    ControllerError::Failed { message: message.into() }
}

enum Recorder {
    /// The shell's own encoder; it reports the result itself.
    Shell,
    Ffmpeg(Arc<FfmpegRecorder>),
}

struct Recording {
    since: Instant,
    recorder: Recorder,
}

/// One line of the Connection Info panel: "Round trip", "4.2 ms".
#[derive(Debug, Clone, PartialEq, Serialize, uniffi::Record)]
pub struct InfoLine {
    pub label: String,
    pub value: String,
}

/// "42 s", "3 min", "1 h 5 min".
fn duration_text(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..60 => format!("{s} s"),
        60..3600 => format!("{} min", s / 60),
        _ => format!("{} h {} min", s / 3600, s / 60 % 60),
    }
}

/// What a viewer asks a capture server stream for besides the host's
/// Quality: its own screen's size, and for a slow connection fewer pictures
/// and a lower JPEG quality. None: no limit of its own.
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct StreamLimits {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub max_fps: Option<f64>,
    /// JPEG quality, 1–100.
    pub quality: Option<u8>,
}

impl StreamLimits {
    /// Combined with the host's Quality: the smaller size of the two.
    fn with_profile(&self, profile: &HostProfile) -> (Option<u32>, Option<u32>) {
        let min = |a: Option<u32>, b: Option<u32>| match (a, b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let (w, h) = profile.max_size();
        (min(w, self.max_width), min(h, self.max_height))
    }

    fn apply(&self, link: &MkvmLink, profile: &HostProfile) {
        let (w, h) = self.with_profile(profile);
        link.set_limits(w, h, self.max_fps, self.quality);
    }
}

struct State {
    profile: Option<HostProfile>,
    session: Option<Arc<MisterSession>>,
    /// Capture server hosts: the stream connection (video, audio, input).
    link: Option<Arc<MkvmLink>>,
    stream: Option<StreamState>,
    video: Option<Arc<VideoSession>>,
    surface: Option<(NativeSurface, u32, u32, f32)>,
    /// Where pictures go for an `Offscreen` surface.
    frames: Option<Arc<FrameOutput>>,
    /// Bumped per session so late callbacks from a closed one are ignored.
    generation: u64,
    capture_on_connect: bool,
    connection: ConnectionState,
    /// When the connection last came up (None: not connected).
    connected_since: Option<Instant>,
    core_running: String,
    game_running: String,
    capturing: bool,
    stats_visible: bool,
    stats_hide_at: Instant,
    stats_signature: String,
    /// AppData::stats_on_change, kept here for the tick.
    stats_on_change: bool,
    input_stats: Option<InputStats>,
    video_stats: Option<VideoStats>,
    video_error: Option<String>,
    display: DisplaySettings,
    muted: bool,
    recording: Option<Recording>,
    last_recording: Option<String>,
    recording_error: Option<String>,
    last_model_save: Instant,
    limits: StreamLimits,
    /// The open stream is connected from the internet: the predictive cursor
    /// uses (and learns) the internet switch and model.
    route_internet: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            profile: None,
            session: None,
            link: None,
            stream: None,
            video: None,
            surface: None,
            frames: None,
            generation: 0,
            capture_on_connect: false,
            connection: not_connected(),
            connected_since: None,
            core_running: String::new(),
            game_running: String::new(),
            capturing: false,
            stats_visible: false,
            stats_hide_at: Instant::now(),
            stats_signature: String::new(),
            stats_on_change: false,
            input_stats: None,
            video_stats: None,
            video_error: None,
            display: DisplaySettings::default(),
            muted: false,
            recording: None,
            last_recording: None,
            recording_error: None,
            last_model_save: Instant::now(),
            limits: StreamLimits::default(),
            route_internet: false,
        }
    }
}

fn not_connected() -> ConnectionState {
    ConnectionState::Disconnected { reason: "not connected".into() }
}

struct Inner {
    /// Ourselves, for background work and session observers.
    me: Weak<Inner>,
    observer: Arc<dyn ControllerObserver>,
    store: ProfileStore,
    state: Mutex<State>,
    /// Paired hosts' device keys, by host id: handed over by the shell (the
    /// iPhone Keychain), never written to the settings file.
    device_keys: Mutex<HashMap<String, mkvm::secure::DeviceKey>>,
    /// Capture automatically this run whatever the setting (the shells'
    /// --auto-capture launch switch).
    auto_capture_forced: AtomicBool,
    /// The shell shows host previews (desktop sidebars); off, no streams
    /// are looked at.
    previews_on: AtomicBool,
    previews: Mutex<Previews>,
    #[cfg(windows)]
    win_capture: Mutex<Option<crate::win::capture::WinCapture>>,
}

/// What pairing returns: the key for the shell to keep safe (Keychain), and
/// where the capture server is reached from the internet.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PairResult {
    pub key_id: String,
    pub key: String,
    pub internet_address: String,
    /// Under which name to keep the key (the capture PC's id).
    pub server_id: String,
    /// The capture PC's existing key was reused; nothing new to save.
    pub reused: bool,
}

/// A device key as the shell keeps it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct StoredKey {
    pub key_id: String,
    pub key: String,
}

/// Keys of capture PCs older than server ids are kept under this prefix and
/// the PC's home address.
const ADDRESS_KEY: &str = "addr:";

/// Under which name a host's device key is kept (`Controller::device_key_server`).
fn key_server(profile: &HostProfile) -> String {
    if !profile.server_id.is_empty() {
        return profile.server_id.clone();
    }
    match mkvm::StreamAddress::parse(&profile.stream) {
        Ok(a) => format!("{ADDRESS_KEY}{}:{}", a.host.to_lowercase(), a.port),
        Err(_) => format!("{ADDRESS_KEY}{}", profile.id),
    }
}

#[derive(uniffi::Object)]
pub struct Controller {
    inner: Arc<Inner>,
}

#[uniffi::export]
impl Controller {
    /// Uses the settings file in the platform config folder.
    #[uniffi::constructor]
    pub fn new(observer: Arc<dyn ControllerObserver>) -> Arc<Self> {
        Self::with_store(ProfileStore::open_default(), observer)
    }

    /// Uses the settings file at `path` (tests, portable installs).
    #[uniffi::constructor]
    pub fn with_settings_file(path: String, observer: Arc<dyn ControllerObserver>) -> Arc<Self> {
        Self::with_store(ProfileStore::open(PathBuf::from(path)), observer)
    }

    // MARK: Hosts

    pub fn profiles(&self) -> Vec<HostProfile> {
        self.inner.store.profiles()
    }

    /// Save a new or edited host. If it's the open one, it reconnects with
    /// the new settings.
    pub fn save_profile(&self, profile: HostProfile) {
        let mut profile = profile;
        profile.mister_host = profile.mister_host.trim().to_string();
        if let Some(before) = self.inner.store.get(&profile.id) {
            // Keep what was learned meanwhile (the editor had an older copy).
            profile.cursor_lag_ms = before.cursor_lag_ms;
            profile.cursor_gain = before.cursor_gain;
            profile.reset_learned_if_mouse_changed(&before);
        }
        self.inner.store.upsert(profile.clone());
        self.inner.notify(ControllerEvent::ProfilesChanged);
        let is_open = self.inner.state.lock().unwrap().profile.as_ref().is_some_and(|p| p.id == profile.id);
        if is_open {
            self.close();
            self.open(profile.id);
        }
    }

    pub fn delete_profile(&self, id: String) {
        if self.open_profile_id().as_deref() == Some(id.as_str()) {
            self.close();
        }
        self.inner.store.delete(&id);
        self.inner.notify(ControllerEvent::ProfilesChanged);
    }

    /// Hosts as JSON, to back up or move to another computer.
    pub fn export_profiles(&self) -> String {
        self.inner.store.export_json()
    }

    /// Add hosts from an export (or the macOS app's old storage). Returns
    /// how many.
    pub fn import_profiles(&self, json: String) -> Result<u32, ControllerError> {
        let count = self.inner.store.import_json(&json).map_err(failed)?;
        self.inner.notify(ControllerEvent::ProfilesChanged);
        Ok(count)
    }

    /// Whether settings from older storage were already imported.
    pub fn legacy_imported(&self) -> bool {
        self.inner.store.data().imported_legacy
    }

    pub fn mark_legacy_imported(&self) {
        self.inner.store.change(|d| d.imported_legacy = true);
    }

    /// Where hosts are saved.
    pub fn settings_path(&self) -> String {
        self.inner.store.path().map(|p| p.display().to_string()).unwrap_or_default()
    }

    // MARK: Session

    /// Connect to a saved host (closing the current one).
    pub fn open(&self, id: String) {
        let Some(profile) = self.inner.store.get(&id) else { return };
        if self.open_profile_id().as_deref() == Some(id.as_str()) {
            return;
        }
        log(&format!(
            "host: open '{}' source {:?} stream '{}' local '{}' mister {}",
            profile.name,
            profile.source_kind(),
            profile.stream,
            profile.local_video,
            profile.mister_host
        ));
        self.close();
        let viewer = self.viewer_name();
        {
            let mut s = self.inner.state.lock().unwrap();
            s.display = profile.display_settings();
            s.profile = Some(profile.clone());
            s.generation += 1;
            s.route_internet = false;
            let observer = Arc::new(Observer { inner: Arc::downgrade(&self.inner), generation: s.generation });
            if profile.source_kind() == SourceKind::Capture && profile.stream.trim().is_empty() {
                if profile.mister_host.trim().is_empty() {
                    s.connection = ConnectionState::Disconnected { reason: "no capture stream chosen".into() };
                } else {
                    // A host without a stream (e.g. saved when it used NDI):
                    // use the stream that shows its MiSTer.
                    s.connection = ConnectionState::Disconnected { reason: format!("looking for {}'s capture stream", profile.mister_host) };
                    let (inner, generation, profile) = (self.inner.clone(), s.generation, profile.clone());
                    thread::spawn(move || inner.find_stream_for(profile, generation, viewer));
                }
            } else if profile.source_kind() == SourceKind::Capture {
                self.inner.connect_capture(&mut s, &profile, viewer, observer);
            } else if profile.source_kind() == SourceKind::Demo {
                let session = MisterSession::demo(observer);
                session.set_settings(profile.input_settings());
                s.session = Some(session);
                s.capture_on_connect = true;
            } else if profile.mister_host.is_empty() {
                s.connection = ConnectionState::Disconnected { reason: "no MiSTer address set".into() };
            } else {
                let session = MisterSession::new(profile.mister_host.clone(), profile.mister_port, observer);
                session.set_settings(profile.input_settings());
                s.session = Some(session);
                s.capture_on_connect = true;
            }
        }
        self.inner.start_video();
        self.inner.notify(ControllerEvent::Changed);
    }

    pub fn close(&self) {
        self.inner.stop_platform_capture();
        self.inner.stop_video();
        let mut s = self.inner.state.lock().unwrap();
        if let Some(session) = s.session.take() {
            session.disconnect();
        }
        if let Some(link) = s.link.take() {
            link.close();
        }
        s.stream = None;
        s.generation += 1;
        s.profile = None;
        s.capturing = false;
        s.capture_on_connect = false;
        s.core_running.clear();
        s.game_running.clear();
        s.input_stats = None;
        s.connection = not_connected();
        s.connected_since = None;
        drop(s);
        self.inner.notify(ControllerEvent::Changed);
    }

    pub fn state(&self) -> ControllerState {
        let pinned = self.inner.store.data().stats_pinned;
        let s = self.inner.state.lock().unwrap();
        ControllerState {
            profile_id: s.profile.as_ref().map(|p| p.id.clone()),
            connection: s.connection.clone(),
            core_running: s.core_running.clone(),
            game_running: s.game_running.clone(),
            capturing: s.capturing,
            stats_visible: s.stats_visible || pinned,
            stats_pinned: pinned,
            input_stats: s.input_stats.clone(),
            video_stats: s.video_stats.clone(),
            video_error: s.video_error.clone(),
            display: s.display.clone(),
            muted: s.muted,
            recording_seconds: s.recording.as_ref().map(|r| r.since.elapsed().as_secs_f64()),
            last_recording: s.last_recording.clone(),
            recording_error: s.recording_error.clone(),
            stream: s.stream.clone(),
        }
    }

    /// Keep host previews fresh (off to start with): a shell that shows
    /// them turns them on. Off, other hosts' streams aren't looked at.
    pub fn set_host_previews(&self, on: bool) {
        self.inner.previews_on.store(on, Ordering::Relaxed);
    }

    /// Each saved host's picture and what it's running, in `profiles()`
    /// order, for the host list (docs in previews.rs). Refreshed on its
    /// own while `set_host_previews` is on; PreviewsChanged says when.
    pub fn host_previews(&self) -> Vec<HostPreview> {
        let ids: Vec<String> = self.inner.store.profiles().into_iter().map(|p| p.id).collect();
        self.inner.previews.lock().unwrap().list(&ids)
    }

    /// The open host's connection in plain words, for a Connection Info
    /// panel: what it is, where its picture and input go, how the link is
    /// doing. Empty with no host open. Values change as it runs: refresh
    /// while the panel is open.
    pub fn connection_info(&self) -> Vec<InfoLine> {
        let (profile, connection, since, core, game, link, video, input) = {
            let s = self.inner.state.lock().unwrap();
            let Some(profile) = s.profile.clone() else { return Vec::new() };
            (
                profile,
                s.connection.clone(),
                s.connected_since,
                s.core_running.clone(),
                s.game_running.clone(),
                s.link.clone(),
                s.video.clone(),
                s.session.clone(),
            )
        };
        let mut lines = Vec::new();
        let mut add = |label: &str, value: String| {
            if !value.trim().is_empty() {
                lines.push(InfoLine { label: label.into(), value });
            }
        };
        add("Host", profile.name.clone());
        add(
            "Status",
            match (&connection, since) {
                (ConnectionState::Connected, Some(t)) => format!("Connected for {}", duration_text(t.elapsed())),
                (ConnectionState::Connected, None) => "Connected".into(),
                (ConnectionState::Connecting, _) => "Connecting…".into(),
                (ConnectionState::Disconnected { reason }, _) => format!("Not connected: {reason}"),
            },
        );
        if connection == ConnectionState::Connected {
            add("Running", previews::core_caption(&core));
            add("Game", game);
        }
        match profile.source_kind() {
            SourceKind::Demo => add("Picture", "Demo test pattern (no MiSTer)".into()),
            SourceKind::Local => {
                add("Picture", format!("Capture card on this computer: {}", profile.local_video));
                if !profile.local_audio.is_empty() {
                    add("Sound", profile.local_audio.clone());
                }
                add("MiSTer", format!("{}:{} (mrext Remote)", profile.mister_host, profile.mister_port));
            }
            SourceKind::Capture => {
                add("Stream", profile.stream.clone());
                if let Some(link) = &link {
                    let st = link.stream_state();
                    let server = match (st.connected, link.server_name()) {
                        (true, Some(name)) => name,
                        (true, None) => "Connected".into(),
                        (false, _) if st.error.is_empty() => "Connecting…".into(),
                        (false, _) => st.error.clone(),
                    };
                    add("Capture server", server);
                    if st.connected {
                        add(
                            "Route",
                            match (st.internet, link.encrypted()) {
                                (true, _) => "Internet, encrypted".into(),
                                (false, true) => "Home network, encrypted".into(),
                                (false, false) => "Home network".into(),
                            },
                        );
                        add("Round trip", format!("{:.1} ms", st.round_trip_ms));
                    }
                    add("MiSTer", if st.mister_connected { st.mister.clone() } else { format!("{} (not connected to the server)", st.mister) });
                    add(
                        "Control",
                        if st.have_control {
                            "You".into()
                        } else if st.holder.is_empty() {
                            "Nobody (yours when you capture)".into()
                        } else {
                            st.holder.clone()
                        },
                    );
                }
                add("Quality", if profile.quality.is_empty() { "auto".into() } else { profile.quality.clone() });
                add("Viewer name", self.viewer_name());
            }
        }
        if let Some(v) = video.map(|v| v.stats()) {
            if v.state == "connected" {
                let crop = if (v.crop_width, v.crop_height) != (v.source_width, v.source_height) && v.crop_width > 0 {
                    format!(" (shown {}×{})", v.crop_width, v.crop_height)
                } else {
                    String::new()
                };
                add("Picture size", format!("{}×{}{crop}", v.source_width, v.source_height));
                add("Frame rate", format!("{:.1} fps", v.received_fps));
                if v.frame_age_ms >= 0.0 && v.frame_age_ms < 1000.0 {
                    add("Delay", format!("≈{:.0} ms capture to screen", v.frame_age_ms));
                }
                if v.audio_rate > 0 {
                    add("Sound", format!("{} Hz, {:.0} ms buffered", v.audio_rate, v.audio_buffer_ms));
                }
            } else {
                add("Picture", v.state);
            }
        }
        if let Some(stats) = input.map(|s| s.stats()) {
            add("Input", format!("{} messages sent", stats.messages_sent));
        }
        lines
    }

    /// A demo host's input, as the lines to show over the picture (oldest
    /// first; each stays 5 s). Empty for other hosts. Poll it a few times a
    /// second while a demo host is open.
    pub fn demo_input(&self) -> Vec<String> {
        let s = self.inner.state.lock().unwrap();
        s.session.as_ref().and_then(|session| session.demo_log()).map(|log| log.lines()).unwrap_or_default()
    }

    /// Capture server hosts: take control of the MiSTer from whoever has it.
    /// Always granted.
    pub fn take_control(&self) {
        let link = self.inner.state.lock().unwrap().link.clone();
        if let Some(link) = link {
            link.take_control();
        }
    }

    /// Capture server hosts: give control up, so another viewer's input
    /// takes it without asking. Input from here would take it back, so the
    /// shell stops sending any until the user takes control again. An older
    /// server ignores it.
    pub fn release_control(&self) {
        let link = self.inner.state.lock().unwrap().link.clone();
        if let Some(link) = link {
            link.release_control();
        }
    }

    /// How this computer appears to other viewers (the computer's name
    /// unless set).
    pub fn viewer_name(&self) -> String {
        let name = self.inner.store.data().viewer_name;
        if name.trim().is_empty() {
            computer_name()
        } else {
            name
        }
    }

    pub fn set_viewer_name(&self, name: String) {
        self.inner.store.change(|d| d.viewer_name = name.trim().to_string());
    }

    /// Change the open host's quality ("auto", "best", "balanced", "low")
    /// and remember it.
    pub fn set_quality(&self, quality: String) {
        let (link, id, limits) = {
            let mut s = self.inner.state.lock().unwrap();
            let link = s.link.clone();
            let limits = s.limits.clone();
            let Some(p) = s.profile.as_mut() else { return };
            p.quality = quality.clone();
            (link, p.id.clone(), limits)
        };
        self.inner.store.update(&id, |p| p.quality = quality.clone());
        if let Some(link) = link {
            limits.apply(&link, &HostProfile { quality, ..Default::default() });
        }
        self.inner.notify(ControllerEvent::Changed);
    }

    /// Use a host over the internet (docs/IPHONE.md §2): this device keeps
    /// one key per capture PC, shared by all its streams. If it already has
    /// one for this host's capture PC, it's reused at once; otherwise (or
    /// with `again`, after a revoke) it pairs on the home network, and the
    /// key comes back for the shell to keep (Keychain, filed under
    /// `server_id`) and hand back with `set_device_key` on later starts.
    /// Saves the host's "use over the internet", capture PC and internet
    /// address; an open host reconnects encrypted. Blocks up to ~15 s: call
    /// it off the main thread.
    pub fn pair_device(&self, id: String, device_name: String, again: bool) -> Result<PairResult, ControllerError> {
        let failed = |message: String| ControllerError::Failed { message };
        let profile = self.inner.store.get(&id).ok_or_else(|| failed("no such host".into()))?;
        let address = mkvm::StreamAddress::parse(&profile.stream).map_err(failed)?;
        // Which capture PC: known from connecting before, else asked now (a
        // stream added with the internet switch on has never connected).
        let mut server = key_server(&profile);
        if profile.server_id.is_empty() {
            if let Ok(found) = crate::link::server_id(&address, &device_name) {
                if !found.is_empty() {
                    server = found;
                }
            }
        }
        let existing = self.inner.device_keys.lock().unwrap().get(&server).cloned();
        let (key, server_address, reused) = match existing {
            Some(key) if !again => (key, String::new(), true),
            existing => {
                let paired = crate::link::pair(&address, &device_name, existing.map(|k| k.id_hex())).map_err(failed)?;
                let key = mkvm::secure::DeviceKey::from_hex(&paired.key_id, &paired.key)
                    .ok_or_else(|| failed("the server sent a malformed key".into()))?;
                if !paired.server_id.is_empty() {
                    server = paired.server_id;
                }
                (key, paired.internet_address.trim().to_string(), false)
            }
        };
        self.inner.device_keys.lock().unwrap().insert(server.clone(), key.clone());
        // The address from the server, else one this capture PC's other
        // streams already use.
        let profiles = self.inner.store.data().profiles;
        let sibling_address =
            profiles.iter().filter(|p| key_server(p) == server && !p.internet_address.trim().is_empty()).map(|p| p.internet_address.clone()).next();
        let internet_address = if !server_address.is_empty() { Some(server_address) } else { sibling_address };
        let server_id = if server.starts_with(ADDRESS_KEY) { String::new() } else { server.clone() };
        self.inner.store.update(&id, |p| {
            p.use_internet = true;
            if !server_id.is_empty() {
                p.server_id = server_id.clone();
            }
            if let Some(a) = &internet_address {
                if p.internet_address.trim().is_empty() || !reused {
                    p.internet_address = a.clone();
                }
            }
        });
        log(&format!(
            "host: '{}' over the internet with {} key {}",
            profile.name,
            if reused { "this capture PC's" } else { "a new" },
            key.id_hex()
        ));
        self.reopen_if_open(&id);
        self.inner.notify(ControllerEvent::ProfilesChanged);
        let saved = self.inner.store.get(&id).map(|p| p.internet_address).unwrap_or_default();
        Ok(PairResult { key_id: key.id_hex(), key: key.key_hex(), internet_address: saved, server_id: server, reused })
    }

    /// A capture PC's device key (`server_id` as `device_key_server` or
    /// `PairResult` named it), from where the shell keeps it. Ignored if
    /// malformed.
    pub fn set_device_key(&self, server_id: String, key_id: String, key: String) {
        if let Some(key) = mkvm::secure::DeviceKey::from_hex(&key_id, &key) {
            self.inner.device_keys.lock().unwrap().insert(server_id, key);
        }
    }

    /// Under which name a host's key is kept: its capture PC's id (or its
    /// home address, for servers older than ids).
    pub fn device_key_server(&self, id: String) -> String {
        self.inner.store.get(&id).map(|p| key_server(&p)).unwrap_or_default()
    }

    /// Stop using a host over the internet. Returns whether the key is still
    /// needed (another stream of the same capture PC uses it); if not, it's
    /// forgotten here and the shell should delete its copy. (The server keeps
    /// it until revoked on its setup page.)
    pub fn stop_internet(&self, id: String) -> bool {
        let Some(profile) = self.inner.store.get(&id) else { return false };
        let server = key_server(&profile);
        self.inner.store.update(&id, |p| p.use_internet = false);
        let still_needed = self.inner.store.data().profiles.iter().any(|p| p.id != id && p.use_internet && key_server(p) == server);
        if !still_needed {
            self.inner.device_keys.lock().unwrap().remove(&server);
        }
        self.reopen_if_open(&id);
        self.inner.notify(ControllerEvent::ProfilesChanged);
        still_needed
    }

    /// The key this device holds for a capture PC (as `device_key_server`
    /// names it), for the shell to keep: e.g. after a key kept by address
    /// moved to the PC's id once the id was learned.
    pub fn device_key(&self, server_id: String) -> Option<StoredKey> {
        self.inner.device_keys.lock().unwrap().get(&server_id).map(|k| StoredKey { key_id: k.id_hex(), key: k.key_hex() })
    }

    /// Whether this device holds a key for the host's capture PC.
    pub fn has_device_key(&self, id: String) -> bool {
        let Some(profile) = self.inner.store.get(&id) else { return false };
        self.inner.device_keys.lock().unwrap().contains_key(&key_server(&profile))
    }

    /// This viewer's own limits for capture server streams (a phone: its
    /// screen's size; away from home: fewer pictures, lower JPEG quality).
    /// Applies now and to every stream opened later.
    pub fn set_stream_limits(&self, limits: StreamLimits) {
        let mut s = self.inner.state.lock().unwrap();
        if s.limits == limits {
            return;
        }
        s.limits = limits;
        if let (Some(link), Some(profile)) = (s.link.clone(), s.profile.clone()) {
            s.limits.apply(&link, &profile);
        }
    }

    /// The open host's input session (shells whose capture talks to it
    /// directly, like macOS).
    pub fn input_session(&self) -> Option<Arc<MisterSession>> {
        self.inner.state.lock().unwrap().session.clone()
    }

    // MARK: Video surface

    /// The view the video is drawn into, its size in pixels and pixels per
    /// point. Video starts whenever a host with a source is open. The
    /// view must stay valid until `detach_surface` (macOS: call on the main
    /// thread).
    pub fn attach_surface(&self, surface: NativeSurface, width_px: u32, height_px: u32, points_scale: f32) {
        let same = self.inner.state.lock().unwrap().surface.as_ref().is_some_and(|(s, ..)| *s == surface);
        if same {
            self.resize_surface(width_px, height_px, points_scale);
            return;
        }
        self.inner.stop_video();
        self.inner.state.lock().unwrap().surface = Some((surface, width_px, height_px, points_scale));
        self.inner.start_video();
        self.inner.notify(ControllerEvent::Changed);
    }

    /// Stop drawing into the view. Returns once the view can be released.
    pub fn detach_surface(&self) {
        self.inner.stop_video();
        self.inner.state.lock().unwrap().surface = None;
        self.inner.notify(ControllerEvent::Changed);
    }

    pub fn resize_surface(&self, width_px: u32, height_px: u32, points_scale: f32) {
        let video = {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(surface) = s.surface.as_mut() {
                (surface.1, surface.2, surface.3) = (width_px, height_px, points_scale);
            }
            s.video.clone()
        };
        match video {
            Some(v) => v.resize(width_px, height_px, points_scale),
            // It may not have had a usable size before.
            None => self.inner.start_video(),
        }
    }

    // MARK: Input

    pub fn is_connected(&self) -> bool {
        self.inner.state.lock().unwrap().connection == ConnectionState::Connected
    }

    /// The shell started or stopped capturing keyboard and mouse.
    pub fn set_capturing(&self, capturing: bool) {
        self.inner.set_capturing(capturing);
    }

    /// A key changed state (USB HID usage). `ExitCapture` means the release
    /// chord was pressed: the shell should stop capturing.
    pub fn key_event(&self, hid_usage: u16, down: bool) -> KeyResult {
        match self.input_session() {
            Some(s) => s.key_event(hid_usage, down),
            None => KeyResult::Ignored,
        }
    }

    pub fn mouse_move(&self, dx: f64, dy: f64) {
        if let Some(s) = self.input_session() {
            s.mouse_move(dx, dy);
        }
    }

    pub fn mouse_button(&self, button: MouseButton, down: bool) {
        if let Some(s) = self.input_session() {
            s.mouse_button(button, down);
        }
    }

    /// Press and release a button, in order with whatever is sent next (a
    /// touch screen's tap: a second tap or a drag can't overtake it).
    pub fn click(&self, button: MouseButton) {
        if let Some(s) = self.input_session() {
            s.mouse_button(button, true);
            s.delay(40);
            s.mouse_button(button, false);
        }
    }

    pub fn release_all(&self) {
        if let Some(s) = self.input_session() {
            s.release_all();
        }
    }

    /// Press the MiSTer menu (OSD) key, F12.
    pub fn open_menu(&self) {
        if let Some(s) = self.input_session() {
            s.open_menu();
        }
    }

    pub fn send_ctrl_alt_del(&self) {
        if let Some(s) = self.input_session() {
            s.press_combo(vec![hid::LEFT_CTRL, hid::LEFT_ALT, hid::DELETE]);
        }
    }

    /// Press and release one key (on-screen buttons: arrows, OK, Backspace).
    pub fn tap_key(&self, hid_usage: u16) {
        if let Some(s) = self.input_session() {
            s.tap_key(hid_usage);
        }
    }

    /// Press keys together and release them in reverse (a key with the
    /// on-screen modifiers held).
    pub fn press_combo(&self, hid_usages: Vec<u16>) {
        if let Some(s) = self.input_session() {
            s.press_combo(hid_usages);
        }
    }

    /// Type text as key presses on a US layout (a touch keyboard sends
    /// characters, not keys; the apps' Type Text box), each with `modifiers`
    /// held, a short gap after each. Characters with no US key are skipped;
    /// `text_typing` says what will happen first.
    pub fn type_text(&self, text: String, modifiers: Vec<u16>) {
        let Some(s) = self.input_session() else { return };
        for c in crate::text::normalize(&text).chars() {
            let Some((usage, shift)) = crate::text::key_for_char(c) else { continue };
            let mut keys = modifiers.clone();
            if shift && !keys.contains(&crate::text::LEFT_SHIFT) {
                keys.push(crate::text::LEFT_SHIFT);
            }
            keys.push(usage);
            s.press_combo(keys);
            s.delay(crate::text::TYPE_GAP_MS);
        }
    }

    /// The Type Text box: type `text` at `keys_per_sec` (remembered for the
    /// open host, as its `type_speed`). `text_typing` says what will happen.
    pub fn type_text_at(&self, text: String, keys_per_sec: f64) {
        let Some(s) = self.input_session() else { return };
        let (hold, gap) = crate::text::pace(keys_per_sec);
        for c in crate::text::normalize(&text).chars() {
            let Some((usage, shift)) = crate::text::key_for_char(c) else { continue };
            let mut keys = if shift { vec![crate::text::LEFT_SHIFT] } else { Vec::new() };
            keys.push(usage);
            s.press_combo_held(keys, Duration::from_millis(hold as u64));
            s.delay(gap);
        }
        let id = self.open_profile_id();
        if let Some(id) = id {
            let speed = keys_per_sec.clamp(crate::text::MIN_TYPE_SPEED, crate::text::MAX_TYPE_SPEED);
            self.inner.store.update(&id, |p| p.type_speed = speed);
            if let Some(p) = self.inner.state.lock().unwrap().profile.as_mut() {
                p.type_speed = speed;
            }
            self.inner.notify(ControllerEvent::ProfilesChanged);
        }
    }

    // MARK: Display

    /// Apply display settings; scale, smoothing, auto-crop and the
    /// predictive cursor are remembered for the open host.
    pub fn set_display(&self, settings: DisplaySettings) {
        let mut d = settings;
        d.zoom = d.zoom.clamp(0.25, 16.0);
        d.pan_x = d.pan_x.clamp(-0.5, 0.5);
        d.pan_y = d.pan_y.clamp(-0.5, 0.5);
        if d.zoom <= 1.0 {
            d.pan_x = 0.0;
            d.pan_y = 0.0;
        }
        let (video, id) = {
            let mut s = self.inner.state.lock().unwrap();
            s.display = d.clone();
            let internet = s.route_internet;
            if let Some(p) = s.profile.as_mut() {
                p.store_display(&d, internet);
            }
            (s.video.clone(), s.profile.as_ref().map(|p| (p.id.clone(), internet)))
        };
        if let Some((id, internet)) = id {
            self.inner.store.update(&id, |p| p.store_display(&d, internet));
        }
        if let Some(v) = video {
            v.set_display(d);
        }
        self.inner.notify(ControllerEvent::Changed);
    }

    pub fn zoom_by(&self, factor: f64) {
        let mut d = self.display();
        d.zoom *= factor;
        self.set_display(d);
    }

    pub fn reset_zoom(&self) {
        let mut d = self.display();
        d.zoom = 1.0;
        self.set_display(d);
    }

    /// Pan by a fraction of the picture (when zoomed in).
    pub fn pan_by(&self, dx: f64, dy: f64) {
        let mut d = self.display();
        d.pan_x += dx;
        d.pan_y += dy;
        self.set_display(d);
    }

    pub fn set_muted(&self, muted: bool) {
        let video = {
            let mut s = self.inner.state.lock().unwrap();
            s.muted = muted;
            s.video.clone()
        };
        if let Some(v) = video {
            v.set_muted(muted);
        }
        self.inner.notify(ControllerEvent::Changed);
    }

    /// Show the stats overlay for a few seconds.
    pub fn flash_stats(&self) {
        self.inner.state.lock().unwrap().flash();
        self.inner.notify(ControllerEvent::Changed);
    }

    /// Whether the shell has shown how to leave capture (once, before the
    /// first capture, so nobody thinks their computer is stuck).
    pub fn capture_intro_seen(&self) -> bool {
        self.inner.store.data().capture_intro_seen
    }

    pub fn set_capture_intro_seen(&self) {
        self.inner.store.change(|d| d.capture_intro_seen = true);
    }

    /// Capture keyboard and mouse by themselves, when a session connects
    /// and when the picture is clicked (the setting; off unless turned on).
    pub fn auto_capture(&self) -> bool {
        self.inner.store.data().auto_capture
    }

    pub fn set_auto_capture(&self, on: bool) {
        self.inner.store.change(|d| d.auto_capture = on);
        self.inner.notify(ControllerEvent::Changed);
    }

    /// The --auto-capture launch switch: capture automatically this run,
    /// whatever the setting. Not saved.
    pub fn force_auto_capture(&self) {
        self.inner.auto_capture_forced.store(true, Ordering::Relaxed);
        self.inner.notify(ControllerEvent::Changed);
    }

    /// Whether to capture by itself now: on connect (the core asks with
    /// CaptureRequested) and when the picture is clicked (the shell asks
    /// this). Otherwise only the Capture button, menu or shortcut captures.
    pub fn capture_automatically(&self) -> bool {
        self.inner.capture_automatically()
    }

    /// Show the stats for a few seconds when the connection changes.
    pub fn stats_on_change(&self) -> bool {
        self.inner.store.data().stats_on_change
    }

    pub fn set_stats_on_change(&self, on: bool) {
        self.inner.store.change(|d| d.stats_on_change = on);
        self.inner.state.lock().unwrap().stats_on_change = on;
    }

    pub fn set_stats_pinned(&self, pinned: bool) {
        self.inner.store.change(|d| d.stats_pinned = pinned);
        if !pinned {
            self.inner.state.lock().unwrap().flash();
        }
        self.inner.notify(ControllerEvent::Changed);
    }

    // MARK: Screenshots

    /// The picture now as a PNG, cropped like the view. `taken_at` is the
    /// local time as the shell writes it, e.g. "2026-10-17 14:32:05"; it
    /// names the file (with the core) and, with `screenshot_stamp`, goes in
    /// a bar under the picture.
    pub fn screenshot(&self, taken_at: String) -> Result<Screenshot, ControllerError> {
        let (video, core) = {
            let s = self.inner.state.lock().unwrap();
            (s.video.clone(), s.core_running.clone())
        };
        let picture = video.and_then(|v| v.picture()).ok_or_else(|| failed("no picture yet"))?;
        let stamp = self.screenshot_stamp();
        screenshot::make(picture, &core, &taken_at, stamp).map_err(failed)
    }

    /// Take a screenshot and save it in `folder` (made if missing) under its
    /// own name, or "… (2).png" if two land in the same second. Returns the
    /// file's path.
    pub fn save_screenshot(&self, taken_at: String, folder: String) -> Result<String, ControllerError> {
        let shot = self.screenshot(taken_at)?;
        let folder = PathBuf::from(folder);
        std::fs::create_dir_all(&folder).map_err(|e| failed(format!("{}: {e}", folder.display())))?;
        let stem = shot.file_name.trim_end_matches(".png");
        let path = (1..)
            .map(|n| folder.join(if n == 1 { shot.file_name.clone() } else { format!("{stem} ({n}).png") }))
            .find(|p| !p.exists())
            .expect("a free name");
        std::fs::write(&path, &shot.png).map_err(|e| failed(format!("{}: {e}", path.display())))?;
        log(&format!("screenshot: saved {}", path.display()));
        Ok(path.display().to_string())
    }

    /// Screenshots get a bar with the core and the time (on unless turned off).
    pub fn screenshot_stamp(&self) -> bool {
        self.inner.store.data().screenshot_stamp
    }

    pub fn set_screenshot_stamp(&self, on: bool) {
        self.inner.store.change(|d| d.screenshot_stamp = on);
    }

    // MARK: Recording

    /// Record video and audio to `path` (.mp4) through ffmpeg.
    pub fn start_recording(&self, path: String) -> Result<(), ControllerError> {
        let video = self.recordable_video()?;
        let recorder = FfmpegRecorder::start(PathBuf::from(path)).map_err(failed)?;
        video.set_media_sink(Some(recorder.clone()));
        self.inner.recording_started(Recorder::Ffmpeg(recorder));
        Ok(())
    }

    /// Record with the shell's own encoder. The shell reports the result
    /// with `recording_finished` after `stop_recording`.
    pub fn start_recording_with_sink(&self, sink: Arc<dyn MediaSink>) -> Result<(), ControllerError> {
        let video = self.recordable_video()?;
        video.set_media_sink(Some(sink));
        self.inner.recording_started(Recorder::Shell);
        Ok(())
    }

    /// Stop recording. With ffmpeg, the file is finished in the background
    /// and `last_recording` (or `recording_error`) is set when it's done.
    pub fn stop_recording(&self) {
        self.inner.stop_recording();
    }

    pub fn recording_finished(&self, path: Option<String>, error: Option<String>) {
        self.inner.recording_finished(path, error);
    }
}

#[cfg(windows)]
impl Controller {
    /// Capture keyboard and mouse with the built-in Windows capture (see
    /// `win::capture`). `window` is the app's top-level window; the rest is
    /// the video area in screen pixels. Ends on the release chord, focus
    /// loss, disconnect or `stop_capture`.
    pub fn start_capture(&self, window: isize, x: i32, y: i32, width: i32, height: i32) -> Result<(), String> {
        use crate::win::capture::{CaptureTarget, WinCapture};
        let (session, raw_mouse) = {
            let s = self.inner.state.lock().unwrap();
            if s.connection != ConnectionState::Connected {
                return Err("not connected".into());
            }
            let session = s.session.clone().ok_or("not connected")?;
            (session, s.profile.as_ref().is_some_and(|p| p.raw_mouse))
        };
        let mut slot = self.inner.win_capture.lock().unwrap();
        if slot.as_ref().is_some_and(|c| c.is_running()) {
            return Ok(());
        }
        let target = CaptureTarget { window, x, y, width, height, raw_mouse };
        let me = self.inner.me.clone();
        let capture = WinCapture::start(
            target,
            session,
            Box::new(move || {
                if let Some(inner) = me.upgrade() {
                    inner.set_capturing(false);
                }
            }),
        )?;
        *slot = Some(capture);
        drop(slot);
        self.inner.set_capturing(true);
        Ok(())
    }

    pub fn stop_capture(&self) {
        self.inner.stop_platform_capture();
        self.inner.set_capturing(false);
    }
}

impl Controller {
    /// Draw into memory instead of a view (`NativeSurface::Offscreen`): each
    /// picture goes to `frames`. Call before `attach_surface`.
    pub fn set_frame_output(&self, frames: Option<Arc<FrameOutput>>) {
        self.inner.state.lock().unwrap().frames = frames;
    }

    /// Reconnect a host that's open, e.g. after its key changed.
    fn reopen_if_open(&self, id: &str) {
        if self.open_profile_id().as_deref() == Some(id) {
            self.close();
            self.open(id.to_string());
        }
    }

    fn with_store(store: ProfileStore, observer: Arc<dyn ControllerObserver>) -> Arc<Self> {
        let state = State { stats_on_change: store.data().stats_on_change, ..Default::default() };
        let inner = Arc::new_cyclic(|me| Inner {
            me: me.clone(),
            observer,
            store,
            state: Mutex::new(state),
            device_keys: Mutex::new(HashMap::new()),
            auto_capture_forced: AtomicBool::new(false),
            previews_on: AtomicBool::new(false),
            previews: Mutex::new(Previews::default()),
            #[cfg(windows)]
            win_capture: Mutex::new(None),
        });
        let weak = Arc::downgrade(&inner);
        thread::Builder::new()
            .name("controller-tick".into())
            .spawn(move || tick_loop(weak))
            .expect("spawn controller thread");
        Arc::new(Self { inner })
    }

    fn open_profile_id(&self) -> Option<String> {
        self.inner.state.lock().unwrap().profile.as_ref().map(|p| p.id.clone())
    }

    fn display(&self) -> DisplaySettings {
        self.inner.state.lock().unwrap().display.clone()
    }

    fn recordable_video(&self) -> Result<Arc<VideoSession>, ControllerError> {
        let s = self.inner.state.lock().unwrap();
        if s.recording.is_some() {
            return Err(failed("already recording"));
        }
        s.video.clone().ok_or_else(|| failed("no video to record"))
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.close();
        self.detach_surface();
    }
}

impl State {
    fn flash(&mut self) {
        self.stats_visible = true;
        self.stats_hide_at = Instant::now() + STATS_FLASH;
    }

    /// Re-show the overlay, if wanted, when the connection changes in a way
    /// worth noticing: connected or not, the core, the video's state and
    /// size. Not dropped frames, the auto-crop or audio underruns: they
    /// change all the time while a stream settles.
    fn check_signature(&mut self) {
        let connected = match self.connection {
            ConnectionState::Connected => "up",
            ConnectionState::Connecting => "connecting",
            ConnectionState::Disconnected { .. } => "down",
        };
        let mut parts = vec![connected.to_string(), self.core_running.clone()];
        if let Some(v) = &self.video_stats {
            parts.extend([
                v.state.clone(),
                format!("{}x{}", v.source_width, v.source_height),
            ]);
        }
        let signature = parts.join("|");
        if signature != self.stats_signature {
            self.stats_signature = signature;
            if self.stats_on_change {
                self.flash();
            }
        }
    }
}

impl Inner {
    fn notify(&self, event: ControllerEvent) {
        self.observer.on_event(event);
    }

    fn capture_automatically(&self) -> bool {
        self.auto_capture_forced.load(Ordering::Relaxed) || self.store.data().auto_capture
    }

    fn set_capturing(&self, capturing: bool) {
        let (video, session) = {
            let mut s = self.state.lock().unwrap();
            if s.capturing == capturing {
                return;
            }
            s.capturing = capturing;
            if capturing {
                s.capture_on_connect = false;
            }
            (s.video.clone(), s.session.clone())
        };
        if let (false, Some(session)) = (capturing, session) {
            session.release_all();
        }
        if let Some(v) = video {
            v.set_pointer_captured(capturing);
        }
        self.notify(ControllerEvent::Changed);
    }

    /// End platform capture (Windows), if running.
    fn stop_platform_capture(&self) {
        #[cfg(windows)]
        {
            let capture = self.win_capture.lock().unwrap().take();
            if let Some(c) = capture {
                c.stop();
            }
        }
    }

    /// Start video if a host with a source is open and there's a view.
    fn start_video(&self) {
        let mut s = self.state.lock().unwrap();
        let (Some(profile), Some((surface, w, h, scale))) = (s.profile.clone(), s.surface.clone()) else {
            return;
        };
        if s.video.is_some() || w < 2 || h < 2 {
            return;
        }
        let source = match profile.source_kind() {
            SourceKind::Capture => match &s.link {
                Some(link) => VideoSource::Link(link.clone()),
                None => return,
            },
            SourceKind::Local if !profile.local_video.is_empty() => {
                VideoSource::Local { video: profile.local_video.clone(), audio: profile.local_audio.clone() }
            }
            SourceKind::Demo => VideoSource::Demo,
            _ => return,
        };
        let frames = s.frames.clone();
        match VideoSession::start(source, surface, frames, w, h, scale) {
            Ok(video) => {
                video.set_display(s.display.clone());
                video.set_muted(s.muted);
                if profile.cursor_lag_ms > 0.0 {
                    // Gain 0 = mouse scaling changed since it was learned; start neutral.
                    let gain = if profile.cursor_gain > 0.0 { profile.cursor_gain } else { 1.0 };
                    video.set_cursor_model(CursorModel { lag_ms: profile.cursor_lag_ms, gain });
                }
                if let Some(session) = &s.session {
                    video.link_input(session.clone());
                }
                video.set_pointer_captured(s.capturing);
                s.video = Some(video);
                s.video_error = None;
                s.last_model_save = Instant::now();
            }
            Err(e) => {
                log(&format!("video: {e}"));
                s.video_error = Some(e.to_string());
            }
        }
    }

    fn stop_video(&self) {
        self.stop_recording();
        self.save_cursor_model();
        let video = {
            let mut s = self.state.lock().unwrap();
            s.video_stats = None;
            s.video.take()
        };
        if let Some(v) = video {
            v.stop();
        }
    }

    /// Remember what the predictive cursor learned for next time (for the
    /// route in use: home and the internet each have their own).
    fn save_cursor_model(&self) {
        let (video, id, internet) = {
            let mut s = self.state.lock().unwrap();
            s.last_model_save = Instant::now();
            (s.video.clone(), s.profile.as_ref().map(|p| p.id.clone()), s.route_internet)
        };
        let (Some(video), Some(id)) = (video, id) else { return };
        // Nothing measured this session: keep what was saved before.
        if video.stats().cursor_fit_samples == 0 {
            return;
        }
        let model = video.cursor_model();
        self.store.update(&id, |p| {
            if internet {
                p.cursor_lag_internet_ms = model.lag_ms;
                p.cursor_gain_internet = model.gain;
            } else {
                p.cursor_lag_ms = model.lag_ms;
                p.cursor_gain = model.gain;
            }
        });
    }

    /// The connection switched between home and the internet: keep what the
    /// cursor learned on the old route, then use the new route's switch and
    /// model.
    fn route_changed(&self, internet: bool) {
        self.save_cursor_model();
        let (video, display, profile) = {
            let mut s = self.state.lock().unwrap();
            s.route_internet = internet;
            let Some(profile) = s.profile.clone() else { return };
            let (enabled, _, _) = profile.cursor_for(internet);
            s.display.predictive_cursor = enabled;
            (s.video.clone(), s.display.clone(), profile)
        };
        let cursor_on = display.predictive_cursor;
        let Some(video) = video else { return };
        video.set_display(display);
        let (_, lag, gain) = profile.cursor_for(internet);
        if lag > 0.0 {
            video.set_cursor_model(CursorModel { lag_ms: lag, gain: if gain > 0.0 { gain } else { 1.0 } });
        } else if internet {
            // Nothing learned from the internet yet: start from a typical
            // mobile delay rather than home's.
            video.set_cursor_model(CursorModel { lag_ms: 250.0, gain: if profile.cursor_gain > 0.0 { profile.cursor_gain } else { 1.0 } });
        }
        log(&format!("host: connected {}: predictive cursor {}", if internet { "from the internet" } else { "at home" }, if cursor_on { "on" } else { "off" }));
    }

    fn recording_started(&self, recorder: Recorder) {
        let mut s = self.state.lock().unwrap();
        s.recording = Some(Recording { since: Instant::now(), recorder });
        s.recording_error = None;
        drop(s);
        self.notify(ControllerEvent::Changed);
    }

    fn stop_recording(&self) {
        let (recording, video) = {
            let mut s = self.state.lock().unwrap();
            (s.recording.take(), s.video.clone())
        };
        let Some(recording) = recording else { return };
        if let Some(v) = video {
            v.set_media_sink(None);
        }
        if let Recorder::Ffmpeg(recorder) = recording.recorder {
            // Finishing takes a moment (muxing); don't block the caller.
            let inner = self.me.clone();
            thread::spawn(move || {
                let result = recorder.finish();
                if let Some(inner) = inner.upgrade() {
                    match result {
                        Ok(path) => inner.recording_finished(Some(path.display().to_string()), None),
                        Err(e) => inner.recording_finished(None, Some(e)),
                    }
                }
            });
        }
        self.notify(ControllerEvent::Changed);
    }

    fn recording_finished(&self, path: Option<String>, error: Option<String>) {
        let mut s = self.state.lock().unwrap();
        if let Some(path) = path {
            log(&format!("record: saved {path}"));
            s.last_recording = Some(path);
        }
        if let Some(e) = &error {
            log(&format!("record: failed: {e}"));
        }
        s.recording_error = error;
        drop(s);
        self.notify(ControllerEvent::Changed);
    }

    /// Connect a capture server stream host (the state's generation is the
    /// observer's).
    /// A paired host's way in: its device key, and the internet address
    /// (with the same stream) when one is set.
    fn remote_for(&self, profile: &HostProfile, home: &mkvm::StreamAddress) -> Option<crate::link::Remote> {
        if !profile.use_internet {
            return None;
        }
        let key = self.device_keys.lock().unwrap().get(&key_server(profile)).cloned()?;
        let internet = profile.internet_address.trim();
        let internet = (!internet.is_empty())
            .then(|| mkvm::StreamAddress::parse(&format!("mkvm://{internet}/x")).ok())
            .flatten()
            .map(|a| mkvm::StreamAddress { stream: home.stream.clone(), ..a });
        Some(crate::link::Remote { key, internet })
    }

    fn connect_capture(&self, s: &mut State, profile: &HostProfile, viewer: String, observer: Arc<Observer>) {
        match mkvm::StreamAddress::parse(&profile.stream) {
            Ok(address) => {
                let remote = self.remote_for(profile, &address);
                let link = MkvmLink::connect_with(address, viewer, remote);
                s.limits.apply(&link, profile);
                let session = MisterSession::with_link(link.clone(), observer);
                session.set_settings(profile.input_settings());
                s.session = Some(session);
                s.link = Some(link);
                s.capture_on_connect = true;
            }
            Err(e) => s.connection = ConnectionState::Disconnected { reason: e },
        }
    }

    /// Find the capture stream that shows `profile`'s MiSTer, save it, and
    /// connect if the host is still the open one.
    fn find_stream_for(self: Arc<Self>, profile: HostProfile, generation: u64, viewer: String) {
        let host = profile.mister_host.trim().to_lowercase();
        let found = crate::discover::find_streams(3000).into_iter().find(|f| f.mister.trim().to_lowercase() == host);
        let mut s = self.state.lock().unwrap();
        if s.generation != generation {
            return;
        }
        let Some(found) = found else {
            s.connection = ConnectionState::Disconnected {
                reason: format!("no capture stream shows {}: choose one in the host's settings", profile.mister_host),
            };
            drop(s);
            self.notify(ControllerEvent::Changed);
            return;
        };
        log(&format!("host: '{}' uses stream {} ({})", profile.name, found.name, found.address));
        let mut profile = profile;
        profile.source = "capture".into();
        profile.stream = found.address.clone();
        self.store.update(&profile.id, |p| {
            p.source = "capture".into();
            p.stream = found.address.clone();
        });
        s.profile = Some(profile.clone());
        let observer = Arc::new(Observer { inner: Arc::downgrade(&self), generation });
        self.connect_capture(&mut s, &profile, viewer, observer);
        drop(s);
        self.start_video();
        self.notify(ControllerEvent::ProfilesChanged);
        self.notify(ControllerEvent::Changed);
    }

    /// Keep the host list's previews current: the open host's from its
    /// video, the others' by looking at their streams now and then.
    fn tick_previews(&self) {
        if !self.previews_on.load(Ordering::Relaxed) {
            return;
        }
        let now = Instant::now();
        let profiles = self.store.profiles();
        let ids: Vec<String> = profiles.iter().map(|p| p.id.clone()).collect();
        let (open, video, caption) = {
            let s = self.state.lock().unwrap();
            let caption = previews::open_caption(&s.connection, &s.core_running);
            (s.profile.as_ref().map(|p| p.id.clone()), s.video.clone(), caption)
        };
        let candidates: Vec<String> = profiles
            .iter()
            .filter(|p| p.source_kind() == SourceKind::Capture && !p.stream.trim().is_empty() && Some(&p.id) != open.as_ref())
            .map(|p| p.id.clone())
            .collect();
        let (open_due, peek) = {
            let mut p = self.previews.lock().unwrap();
            p.retain(&ids);
            let open_due = open.is_some() && p.open_due(now);
            (open_due, p.next_peek(&candidates, now))
        };
        if let (Some(id), true) = (&open, open_due) {
            let picture = video.and_then(|v| v.picture_bgra()).and_then(|b| previews::encode(&b, true));
            let changed = {
                let mut p = self.previews.lock().unwrap();
                p.fresh(id, now);
                p.set(id, picture, caption)
            };
            if changed {
                self.notify(ControllerEvent::PreviewsChanged);
            }
        }
        let Some(id) = peek else { return };
        let Some(profile) = profiles.into_iter().find(|p| p.id == id) else { return };
        let (Ok(address), Some(inner)) = (mkvm::StreamAddress::parse(&profile.stream), self.me.upgrade()) else {
            self.previews.lock().unwrap().peek_done();
            return;
        };
        let remote = self.remote_for(&profile, &address);
        let viewer = format!("{} (preview)", self.viewer_name());
        thread::spawn(move || {
            let found = previews::peek(address, viewer, remote);
            // Opened meanwhile: its own video says it better.
            let open = inner.state.lock().unwrap().profile.as_ref().is_some_and(|p| p.id == id);
            let changed = {
                let mut p = inner.previews.lock().unwrap();
                p.peek_done();
                !open && p.set(&id, found.picture, found.caption)
            };
            if changed {
                inner.notify(ControllerEvent::PreviewsChanged);
            }
        });
    }

    fn viewer_name(&self) -> String {
        let name = self.store.data().viewer_name;
        if name.trim().is_empty() { computer_name() } else { name }
    }

    fn tick(&self) {
        self.tick_previews();
        let (session, video, link) = {
            let s = self.state.lock().unwrap();
            (s.session.clone(), s.video.clone(), s.link.clone())
        };
        let stream = link.map(|l| l.stream_state());
        let input_stats = session.map(|s| s.stats());
        let video_stats = video.map(|v| v.stats());
        let (save_model, lost_control, route, server_id) = {
            let mut s = self.state.lock().unwrap();
            // Someone took control: stop capturing here.
            let had = s.stream.as_ref().is_some_and(|st| st.have_control);
            let has = stream.as_ref().is_some_and(|st| st.have_control);
            let lost_control = had && !has && s.capturing;
            let connected = stream.as_ref().filter(|st| st.connected);
            let route = connected.map(|st| st.internet).filter(|&internet| internet != s.route_internet);
            // The capture PC's id, first learned (or changed): kept with the host.
            let server_id = connected
                .map(|st| st.server_id.clone())
                .filter(|id| !id.is_empty() && s.profile.as_ref().is_some_and(|p| p.server_id != *id));
            if let (Some(id), Some(p)) = (&server_id, s.profile.as_mut()) {
                // A key kept by the PC's address (before its id was known)
                // now goes by the id.
                let old = key_server(p);
                p.server_id = id.clone();
                let mut keys = self.device_keys.lock().unwrap();
                if !keys.contains_key(id) {
                    if let Some(key) = keys.remove(&old) {
                        keys.insert(id.clone(), key);
                    }
                }
            }
            let server_id = server_id.zip(s.profile.as_ref().map(|p| p.id.clone()));
            s.stream = stream;
            s.input_stats = input_stats;
            s.video_stats = video_stats;
            s.check_signature();
            if s.stats_visible && Instant::now() >= s.stats_hide_at {
                s.stats_visible = false;
            }
            (s.last_model_save.elapsed() >= MODEL_SAVE, lost_control, route, server_id)
        };
        if let Some((server, id)) = server_id {
            self.store.update(&id, |p| p.server_id = server.clone());
            self.notify(ControllerEvent::ProfilesChanged);
        }
        if let Some(internet) = route {
            self.route_changed(internet);
        } else if save_model {
            self.save_cursor_model();
        }
        if lost_control {
            log("host: control taken by another viewer");
            self.stop_platform_capture();
            self.notify(ControllerEvent::ReleaseRequested);
        }
        self.notify(ControllerEvent::Changed);
    }
}

fn tick_loop(inner: Weak<Inner>) {
    loop {
        thread::sleep(TICK);
        match inner.upgrade() {
            Some(inner) => inner.tick(),
            None => return,
        }
    }
}

/// The computer's name, for other viewers to see.
fn computer_name() -> String {
    let from_env = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default();
    if !from_env.trim().is_empty() {
        return from_env.trim().to_string();
    }
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().trim_end_matches(".local").to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "MiSTerKVMS".into())
}

/// Session callbacks (worker thread) → controller state.
struct Observer {
    inner: Weak<Inner>,
    generation: u64,
}

impl SessionObserver for Observer {
    fn on_state(&self, state: ConnectionState) {
        let Some(inner) = self.inner.upgrade() else { return };
        let mut events = vec![ControllerEvent::Changed];
        let auto = inner.capture_automatically();
        {
            let mut s = inner.state.lock().unwrap();
            if s.generation != self.generation {
                return;
            }
            s.connection = state.clone();
            if state == ConnectionState::Connected {
                s.connected_since.get_or_insert_with(Instant::now);
            } else {
                s.connected_since = None;
            }
            s.check_signature();
            match state {
                ConnectionState::Connected if s.capture_on_connect => {
                    s.capture_on_connect = false;
                    if auto {
                        events.push(ControllerEvent::CaptureRequested);
                    }
                }
                ConnectionState::Disconnected { .. } if s.capturing => {
                    events.push(ControllerEvent::ReleaseRequested);
                }
                _ => {}
            }
        }
        if events.contains(&ControllerEvent::ReleaseRequested) {
            inner.stop_platform_capture();
        }
        for e in events {
            inner.notify(e);
        }
    }

    fn on_status(&self, key: String, value: String) {
        let Some(inner) = self.inner.upgrade() else { return };
        {
            let mut s = inner.state.lock().unwrap();
            if s.generation != self.generation {
                return;
            }
            match key.as_str() {
                "coreRunning" => s.core_running = value,
                "gameRunning" => s.game_running = value,
                _ => return,
            }
            s.check_signature();
        }
        inner.notify(ControllerEvent::Changed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::ScaleMode;

    struct Events(Mutex<Vec<ControllerEvent>>);

    impl ControllerObserver for Events {
        fn on_event(&self, event: ControllerEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn controller() -> (Arc<Controller>, Arc<Events>) {
        let events = Arc::new(Events(Mutex::new(Vec::new())));
        (Controller::with_store(ProfileStore::in_memory(), events.clone()), events)
    }

    #[test]
    fn display_changes_are_clamped_and_remembered_per_host() {
        let (c, _) = controller();
        let p = HostProfile { name: "A".into(), ..Default::default() };
        c.save_profile(p.clone());
        c.open(p.id.clone());
        assert_eq!(c.state().profile_id.as_deref(), Some(p.id.as_str()));
        c.set_display(DisplaySettings { scale_mode: ScaleMode::Integer, zoom: 100.0, pan_x: 3.0, ..Default::default() });
        let d = c.state().display;
        assert_eq!((d.zoom, d.pan_x), (16.0, 0.5));
        c.reset_zoom();
        assert_eq!(c.state().display.pan_x, 0.0);
        assert_eq!(c.profiles()[0].scale_mode, "integer");
        // Reopening starts from the saved settings, at zoom 1.
        c.close();
        c.open(p.id);
        assert_eq!(c.state().display.scale_mode, ScaleMode::Integer);
    }

    #[test]
    fn editing_keeps_learned_cursor_unless_mouse_changed() {
        let (c, events) = controller();
        let p = HostProfile::default();
        c.save_profile(p.clone());
        c.inner.store.update(&p.id, |p| (p.cursor_lag_ms, p.cursor_gain) = (90.0, 1.1));
        c.save_profile(HostProfile { name: "Renamed".into(), ..p.clone() });
        assert_eq!((c.profiles()[0].cursor_lag_ms, c.profiles()[0].cursor_gain), (90.0, 1.1));
        c.save_profile(HostProfile { mouse_sensitivity: 2.0, ..p });
        assert_eq!((c.profiles()[0].cursor_lag_ms, c.profiles()[0].cursor_gain), (90.0, 0.0));
        assert!(events.0.lock().unwrap().contains(&ControllerEvent::ProfilesChanged));
    }

    #[test]
    fn host_without_address_reports_why() {
        let (c, _) = controller();
        let p = HostProfile { source: "capture".into(), ..Default::default() };
        c.save_profile(p.clone());
        c.open(p.id);
        assert_eq!(c.state().connection, ConnectionState::Disconnected { reason: "no capture stream chosen".into() });
        // A blank host is a capture host.
        let p = HostProfile::default();
        c.save_profile(p.clone());
        c.open(p.id);
        assert_eq!(c.state().connection, ConnectionState::Disconnected { reason: "no capture stream chosen".into() });
        // Saved when it used NDI: looks for its MiSTer's stream.
        let p = HostProfile { source: "ndi".into(), mister_host: "192.0.2.7".into(), ..Default::default() };
        c.save_profile(p.clone());
        c.open(p.id);
        assert_eq!(
            c.state().connection,
            ConnectionState::Disconnected { reason: "looking for 192.0.2.7's capture stream".into() }
        );
        let p = HostProfile { source: "local".into(), local_video: "test:pattern".into(), ..Default::default() };
        c.save_profile(p.clone());
        c.open(p.id);
        assert_eq!(
            c.state().connection,
            ConnectionState::Disconnected { reason: "no MiSTer address set".into() }
        );
        assert_eq!(c.key_event(0x04, true), KeyResult::Ignored);
    }

    /// A screenshot of the demo picture. Needs a GPU (the renderer); with
    /// KVMS_SCREENSHOT=path it's saved there to look at.
    #[test]
    #[ignore]
    fn demo_screenshot() {
        let (c, _events) = controller();
        c.set_frame_output(Some(FrameOutput::new(|| {})));
        c.attach_surface(NativeSurface::Offscreen, 640, 480, 1.0);
        let p = crate::profiles::demo_host_profile();
        c.save_profile(p.clone());
        c.open(p.id);
        let deadline = Instant::now() + Duration::from_secs(10);
        let shot = loop {
            match c.screenshot("2026-10-17 14:32:05".into()) {
                Ok(shot) => break shot,
                Err(e) if Instant::now() > deadline => panic!("{e}"),
                Err(_) => thread::sleep(Duration::from_millis(50)),
            }
        };
        assert!(shot.file_name.ends_with(" 2026-10-17 14.32.05.png"), "{}", shot.file_name);
        c.set_screenshot_stamp(false);
        let plain = c.screenshot("2026-10-17 14:32:05".into()).unwrap();
        assert!(plain.height < shot.height);
        let dir = std::env::temp_dir().join(format!("kvms-shots-{}", std::process::id()));
        let a = c.save_screenshot("2026-10-17 14:32:05".into(), dir.display().to_string()).unwrap();
        let b = c.save_screenshot("2026-10-17 14:32:05".into(), dir.display().to_string()).unwrap();
        assert!(b.ends_with(" 14.32.05 (2).png"), "{a} then {b}");
        let _ = std::fs::remove_dir_all(dir);
        if let Ok(path) = std::env::var("KVMS_SCREENSHOT") {
            std::fs::write(&path, &shot.png).unwrap();
            std::fs::write(format!("{path}.plain.png"), &plain.png).unwrap();
        }
    }

    /// A demo host: connected at once, its input written out, not sent.
    #[test]
    fn demo_host_logs_input() {
        let (c, _events) = controller();
        let p = crate::profiles::demo_host_profile();
        c.save_profile(p.clone());
        c.open(p.id);
        let deadline = Instant::now() + Duration::from_secs(5);
        while c.state().connection != ConnectionState::Connected && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(c.state().connection, ConnectionState::Connected);
        c.tap_key(0x45);
        c.click(MouseButton::Left);
        let deadline = Instant::now() + Duration::from_secs(5);
        // The click's release comes 40 ms after its press.
        while c.demo_input() != ["F12", "left click"] && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(c.demo_input(), ["F12", "left click"]);
        // Tap and drag, as the iPhone's touchpad sends it (not so soon that
        // its click makes a double click with the one before).
        thread::sleep(Duration::from_millis(600));
        c.click(MouseButton::Left);
        c.mouse_button(MouseButton::Left, true);
        c.mouse_move(30.0, 10.0);
        c.mouse_button(MouseButton::Left, false);
        let want = ["F12", "left click", "left click", "left button down", "mouse moved +30, +10", "left button up"];
        let deadline = Instant::now() + Duration::from_secs(5);
        while c.demo_input() != want && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(c.demo_input(), want);
        // The Type Text box: key by key, at the speed asked, remembered.
        thread::sleep(Duration::from_millis(600));
        c.type_text_at("Hi \u{201C}x\u{201D}".into(), 40.0);
        let want = ["Shift+H", "I", "Space", "Shift+'", "X", "Shift+'"];
        let deadline = Instant::now() + Duration::from_secs(5);
        while !c.demo_input().ends_with(&want.map(String::from)) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(c.demo_input().ends_with(&want.map(String::from)), "{:?}", c.demo_input());
        assert_eq!(c.profiles().iter().find(|p| p.id == crate::profiles::DEMO_HOST_ID).unwrap().type_speed, 40.0);
        c.close();
        assert!(c.demo_input().is_empty());
    }

    /// A capture server stream end to end against a fake server: connect,
    /// input arrives as mkvm Input, control taken away releases capture.
    #[test]
    fn capture_stream_carries_input_and_control() {
        use mkvm::*;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut read = sock.try_clone().unwrap();
            let Message::Hello(hello) = read_message(&mut read).unwrap().0 else { panic!("no hello") };
            assert_eq!(hello.stream, "MiSTer SS1");
            let info = StreamInfo {
                name: hello.stream,
                mister: Some("10.0.0.5".into()),
                width: 0,
                height: 0,
                fps: 0.0,
                audio_rate: 48000,
                audio_channels: 2,
                sizes: vec![],
                codecs: vec![Codec::Jpeg],
            };
            write_message(&mut sock, &Message::Welcome(Welcome { protocol: 1, server: "test".into(), server_id: String::new(), viewer_id: 1, stream: info }), 0).unwrap();
            write_message(&mut sock, &Message::Mister(MisterState { connected: true, reason: String::new() }), 0).unwrap();
            write_message(&mut sock, &Message::Control(ControlState { holder: Some(Viewer { id: 1, name: hello.viewer }), you: true }), 0).unwrap();
            write_message(&mut sock, &Message::Status(Status { key: "coreRunning".into(), value: "HP9K715".into() }), 0).unwrap();
            loop {
                match read_message(&mut read) {
                    Ok((Message::Input(cmds), _)) => {
                        tx.send(cmds).unwrap();
                        // Someone else takes control.
                        let other = Viewer { id: 2, name: "Other PC".into() };
                        write_message(&mut sock, &Message::Control(ControlState { holder: Some(other), you: false }), 0).unwrap();
                    }
                    Ok(_) => {}
                    Err(_) => return,
                }
            }
        });

        let (c, events) = controller();
        c.set_auto_capture(true);
        let p = HostProfile { stream: format!("mkvm://127.0.0.1:{port}/MiSTer%20SS1"), ..Default::default() };
        c.save_profile(p.clone());
        c.open(p.id);
        let deadline = Instant::now() + Duration::from_secs(5);
        while (c.state().connection != ConnectionState::Connected || c.state().core_running.is_empty()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(c.state().connection, ConnectionState::Connected);
        assert_eq!(c.state().core_running, "HP9K715");
        assert!(events.0.lock().unwrap().contains(&ControllerEvent::CaptureRequested));

        c.set_capturing(true);
        c.inner.tick();
        assert!(c.state().stream.unwrap().have_control);
        c.key_event(0x04, true); // A
        let cmds = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(cmds, vec![InputCmd::Send { m: "kbdRawDown:30".into() }]);

        let deadline = Instant::now() + Duration::from_secs(2);
        while c.state().stream.as_ref().is_some_and(|s| s.holder.is_empty()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
            c.inner.tick();
        }
        assert_eq!(c.state().stream.unwrap().holder, "Other PC");
        assert!(events.0.lock().unwrap().contains(&ControllerEvent::ReleaseRequested));
        c.close();
    }

    #[test]
    fn connection_info_says_what_is_open() {
        let (c, _) = controller();
        assert!(c.connection_info().is_empty());
        let p = crate::profiles::demo_host_profile();
        c.save_profile(p.clone());
        c.open(p.id);
        let deadline = Instant::now() + Duration::from_secs(5);
        while c.state().connection != ConnectionState::Connected && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        let info = c.connection_info();
        let value = |label: &str| info.iter().find(|l| l.label == label).map(|l| l.value.clone());
        assert_eq!(value("Host"), Some(p.name));
        assert!(value("Status").is_some_and(|s| s.starts_with("Connected for ")), "{info:?}");
        assert_eq!(value("Picture").as_deref(), Some("Demo test pattern (no MiSTer)"));
        assert!(info.iter().all(|l| !l.value.trim().is_empty()));
        c.close();
        assert!(c.connection_info().is_empty());
        assert_eq!(duration_text(Duration::from_secs(42)), "42 s");
        assert_eq!(duration_text(Duration::from_secs(3 * 3600 + 5 * 60 + 9)), "3 h 5 min");
    }

    /// Connecting captures by itself only with the setting or the launch
    /// switch.
    #[test]
    fn auto_capture_is_off_unless_asked_for() {
        let connect = |c: &Arc<Controller>| {
            let p = crate::profiles::demo_host_profile();
            c.save_profile(p.clone());
            c.open(p.id);
            let deadline = Instant::now() + Duration::from_secs(5);
            while c.state().connection != ConnectionState::Connected && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(c.state().connection, ConnectionState::Connected);
            c.close();
        };
        let requested = |events: &Events| events.0.lock().unwrap().contains(&ControllerEvent::CaptureRequested);

        let (c, events) = controller();
        assert!(!c.auto_capture() && !c.capture_automatically());
        connect(&c);
        assert!(!requested(&events));

        let (c, events) = controller();
        c.set_auto_capture(true);
        assert!(c.capture_automatically());
        connect(&c);
        assert!(requested(&events));

        let (c, events) = controller();
        c.force_auto_capture();
        assert!(c.capture_automatically() && !c.auto_capture());
        connect(&c);
        assert!(requested(&events));
    }

    #[test]
    fn unreachable_mister_disconnects_and_capture_releases() {
        let (c, events) = controller();
        // Port 9 on localhost: nothing listens, so the connect fails fast.
        let p = HostProfile { source: "local".into(), mister_host: "127.0.0.1".into(), mister_port: 9, ..Default::default() };
        c.save_profile(p.clone());
        c.open(p.id);
        // Until the session reports in, the state still says "not connected"
        // from close(): wait for the connect itself to fail.
        let failed = || { let s = c.state().connection; matches!(s, ConnectionState::Disconnected { .. }) && s != not_connected() };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !failed() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(failed(), "{:?}", c.state().connection);
        assert!(!events.0.lock().unwrap().contains(&ControllerEvent::CaptureRequested));
        c.set_capturing(true);
        assert!(c.state().capturing);
        c.close();
        assert!(!c.state().capturing);
    }
}
