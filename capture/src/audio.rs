//! Audio from a capture card's own audio device, as 16-bit interleaved PCM.
//! Through `cpal`, except on Windows, where USB inputs are read straight
//! from their driver (ds.rs, "direct"), and Linux, where only inputs that go
//! straight to the kernel's driver are offered (`hw:`): neither then
//! depends on the system's sound server.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};

use crate::clock::now_100ns;
use crate::CaptureError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    /// cpal's id ("wasapi:{...}"), stable across restarts.
    pub id: String,
    pub name: String,
}

/// Audio ids starting with this come from a capture card's own audio pin,
/// opened together with its video (`VideoCapture::open_with_audio`).
pub const CARD_AUDIO: &str = "card:";

/// Whether an audio id (or name, as the setup page stores it) is a card's
/// own audio rather than a system input.
pub fn is_card_audio(id_or_name: &str) -> bool {
    id_or_name.starts_with(CARD_AUDIO) || id_or_name.ends_with(" (card audio)")
}

/// Input devices (microphones, capture card audio, ...), then audio that
/// comes from a card's own pin. On Windows, inputs read straight from their
/// driver come first (`... (direct)`), so a card's audio is matched to them
/// rather than to the same input through the Windows audio engine.
pub fn audio_devices() -> Vec<AudioDevice> {
    #[cfg(windows)]
    let mut list = crate::ds::direct_audio_devices();
    #[cfg(not(windows))]
    let mut list = Vec::new();
    let inputs: Vec<cpal::Device> = cpal::default_host().input_devices().map(|d| d.collect()).unwrap_or_default();
    let direct: Vec<&cpal::Device> = inputs.iter().filter(|d| is_direct(d)).collect();
    // None of them (an unusual ALSA setup): offer what there is.
    let shown = if direct.is_empty() { inputs.iter().collect() } else { direct };
    list.extend(shown.into_iter().filter_map(|d| {
        let name = d.description().ok()?.name().to_string();
        let id = d.id().ok()?.to_string();
        Some(AudioDevice { id, name })
    }));
    #[cfg(windows)]
    list.extend(crate::ds::audio_devices());
    if crate::pattern::listed() {
        list.push(crate::pattern::audio_device());
    }
    list
}

pub struct AudioChunk {
    pub channels: u16,
    pub rate: u32,
    /// Interleaved.
    pub samples: Vec<i16>,
    /// Capture time of the first sample, 100 ns since the Unix epoch.
    pub timestamp: i64,
}

/// A running audio capture. Chunks arrive every few milliseconds; if the
/// reader falls behind by more than about a second, chunks are dropped.
pub struct AudioCapture {
    rx: Receiver<Result<AudioChunk, CaptureError>>,
    stop: Arc<AtomicBool>,
    pub channels: u16,
    pub rate: u32,
}

impl AudioCapture {
    /// Audio fed by a video backend (a card's own audio pin).
    pub(crate) fn from_channel(rx: Receiver<Result<AudioChunk, CaptureError>>, stop: Arc<AtomicBool>, channels: u16, rate: u32) -> Self {
        Self { rx, stop, channels, rate }
    }

    /// Set when the capture is dropped, for a backend that runs its own thread.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    /// Open the input device with id `id`, or else the first whose name is
    /// `id`.
    pub fn open(id: &str) -> Result<Self, CaptureError> {
        if is_card_audio(id) {
            return Err(CaptureError::Unsupported("card audio comes with its video (open_with_audio)".into()));
        }
        #[cfg(windows)]
        if crate::ds::is_direct_audio(id) {
            return crate::ds::open_direct_audio(id);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(256);
        let (ready_tx, ready_rx) = mpsc::channel();
        let id = id.to_string();
        let thread_stop = stop.clone();
        thread::Builder::new()
            .name("capture-audio".into())
            .spawn(move || {
                // cpal streams aren't Send everywhere: build and hold it here.
                match start(&id, tx.clone()) {
                    Ok((stream, channels, rate)) => {
                        let _ = ready_tx.send(Ok((channels, rate)));
                        while !thread_stop.load(Ordering::Relaxed) {
                            thread::park_timeout(Duration::from_millis(200));
                        }
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| CaptureError::Other(e.to_string()))?;
        let (channels, rate) = ready_rx.recv().map_err(|e| CaptureError::Other(e.to_string()))??;
        Ok(Self { rx, stop, channels, rate })
    }

    /// The next chunk, or None after `timeout`.
    pub fn next_chunk(&self, timeout: Duration) -> Result<Option<AudioChunk>, CaptureError> {
        match self.rx.recv_timeout(timeout) {
            Ok(r) => r.map(Some),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(CaptureError::Lost("audio stopped".into())),
        }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Linux: an input that goes straight to the kernel's driver,
/// `hw:CARD=<name>,DEV=<n>`, rather than one of the plugins listed for the
/// same card (`default`, `pipewire`, `pulse`, `sysdefault`, `plughw`,
/// `dsnoop`, ...), which may route through the sound server. Cards by name:
/// their numbers change with the order they're plugged in. Elsewhere every
/// input counts.
fn is_direct(d: &cpal::Device) -> bool {
    cfg!(not(target_os = "linux")) || d.id().is_ok_and(|id| is_alsa_hw(id.id()))
}

fn is_alsa_hw(pcm: &str) -> bool {
    pcm.strip_prefix("hw:CARD=").and_then(|rest| rest.split(',').next()).is_some_and(|card| card.parse::<u32>().is_err())
}

fn find(id: &str) -> Result<cpal::Device, CaptureError> {
    let host = cpal::default_host();
    if let Ok(parsed) = cpal::DeviceId::from_str(id) {
        if let Some(d) = host.device_by_id(&parsed) {
            return Ok(d);
        }
    }
    // By name, the direct input first: on Linux one card's inputs share it.
    let named: Vec<cpal::Device> = host
        .input_devices()
        .map_err(|e| CaptureError::Other(e.to_string()))?
        .filter(|d| d.description().is_ok_and(|desc| desc.name() == id))
        .collect();
    let direct = named.iter().position(is_direct).unwrap_or(0);
    named.into_iter().nth(direct).ok_or_else(|| CaptureError::NotFound(id.to_string()))
}

/// The format to capture in. Linux's direct inputs don't convert, and their
/// default may be a rate or sample format we'd rather not: ask for 48 kHz
/// (what the protocol carries) in a format we read, 16-bit and stereo if
/// offered. Elsewhere the system's own format, which it converts to.
fn input_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, CaptureError> {
    #[cfg(target_os = "linux")]
    {
        use cpal::SampleFormat::{F32, I16, I32};
        let best = device
            .supported_input_configs()
            .into_iter()
            .flatten()
            .filter(|r| matches!(r.sample_format(), F32 | I16 | I32) && r.min_sample_rate() <= 48000 && r.max_sample_rate() >= 48000)
            .max_by_key(|r| (r.sample_format() == I16, r.channels() == 2, r.sample_format() == I32));
        if let Some(r) = best {
            return Ok(r.with_sample_rate(48000));
        }
    }
    device.default_input_config().map_err(|e| CaptureError::Other(e.to_string()))
}

type Tx = SyncSender<Result<AudioChunk, CaptureError>>;

fn start(id: &str, tx: Tx) -> Result<(cpal::Stream, u16, u32), CaptureError> {
    let device = find(id)?;
    let config = input_config(&device)?;
    let channels = config.channels();
    let rate = config.sample_rate();
    let format = config.sample_format();
    let send = move |tx: &Tx, samples: Vec<i16>| {
        let frames = samples.len() as i64 / channels.max(1) as i64;
        // The callback runs when the buffer is full: its first sample is
        // one buffer older than now.
        let timestamp = now_100ns() - frames * 10_000_000 / rate.max(1) as i64;
        if let Err(TrySendError::Disconnected(_)) = tx.try_send(Ok(AudioChunk { channels, rate, samples, timestamp })) {
            // Reader gone; the stream stops when the thread exits.
        }
    };
    let err_tx = tx.clone();
    let on_error = move |e: cpal::Error| {
        // A glitch, not a lost device: the stream keeps delivering. Stopping
        // here (and reopening) turned one glitch into a dead stream, since
        // the error arrives before the buffer it belongs to.
        if e.kind() == cpal::ErrorKind::Xrun {
            return;
        }
        let _ = err_tx.try_send(Err(CaptureError::Lost(e.to_string())));
    };
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            config.config(),
            move |data: &[f32], _: &_| {
                send(&tx, data.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect());
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            config.config(),
            move |data: &[i16], _: &_| send(&tx, data.to_vec()),
            on_error,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream(
            config.config(),
            move |data: &[i32], _: &_| send(&tx, data.iter().map(|s| (s >> 16) as i16).collect()),
            on_error,
            None,
        ),
        other => return Err(CaptureError::Format(format!("audio sample format {other:?}"))),
    }
    .map_err(|e| CaptureError::Other(e.to_string()))?;
    stream.play().map_err(|e| CaptureError::Other(e.to_string()))?;
    Ok((stream, channels, rate))
}

#[cfg(test)]
mod tests {
    #[test]
    fn alsa_hw_by_card_name_only() {
        assert!(super::is_alsa_hw("hw:CARD=U4K,DEV=0"));
        assert!(!super::is_alsa_hw("hw:CARD=1,DEV=0"), "card numbers change with plug order");
        for plugin in ["default", "pipewire", "pulse", "sysdefault:CARD=U4K", "plughw:CARD=U4K,DEV=0", "dsnoop:CARD=U4K,DEV=0"] {
            assert!(!super::is_alsa_hw(plugin), "{plugin}");
        }
    }
}
