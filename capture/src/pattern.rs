//! A built-in test pattern, `test:pattern`: moving rainbow bars at 1280x720,
//! 60 fps, with a white box that steps once a second, and its own audio (a
//! short beep each time the box steps, to check sound and sync). For trying
//! out streams, viewers and soak runs without a capture card. Listed by
//! `devices()` when switched on (`set_listed`, the setup page's "test
//! pattern") or `MISTERKVMS_TEST_PATTERN` is set; it can always be opened by id.
//!
//! `test:demo` is the same with MiSTer-Kun stepping instead of the box: the
//! apps' demo hosts (docs/APP-STORE.md S1). Never listed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::audio::{AudioCapture, AudioChunk, AudioDevice, CARD_AUDIO};
use crate::clock::now_100ns;
use crate::{Output, PixelFormat, VideoDevice, VideoFormat, VideoFrame};

pub const ID: &str = "test:pattern";
pub const DEMO_ID: &str = "test:demo";
const FORMAT: VideoFormat = VideoFormat { pixel: PixelFormat::Bgra, width: 1280, height: 720, fps: 60.0 };
const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

static LISTED: AtomicBool = AtomicBool::new(false);

pub fn set_listed(on: bool) {
    LISTED.store(on, Ordering::Relaxed);
}

pub fn listed() -> bool {
    LISTED.load(Ordering::Relaxed) || std::env::var_os("MISTERKVMS_TEST_PATTERN").is_some()
}

pub fn audio_device() -> AudioDevice {
    AudioDevice { id: format!("{CARD_AUDIO}{ID}"), name: "Test pattern (card audio)".into() }
}

/// The pattern's sound: silence, with a 1 kHz beep for the first 100 ms of
/// every second since `start` (when the box steps). Chunks of 10 ms, in
/// real time.
fn tone(start: Instant, start_ts: i64) -> AudioCapture {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::sync_channel(256);
    let thread_stop = stop.clone();
    let _ = std::thread::Builder::new().name("test-tone".into()).spawn(move || {
        const CHUNK: u64 = RATE as u64 / 100;
        let mut n: u64 = 0;
        while !thread_stop.load(Ordering::Relaxed) {
            let due = start + Duration::from_secs_f64((n + CHUNK) as f64 / RATE as f64);
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
            let mut samples = Vec::with_capacity(CHUNK as usize * CHANNELS as usize);
            for i in n..n + CHUNK {
                let in_second = i % RATE as u64;
                let v = if in_second < RATE as u64 / 10 {
                    ((i as f64 * 1000.0 * std::f64::consts::TAU / RATE as f64).sin() * 12_000.0) as i16
                } else {
                    0
                };
                samples.extend([v; CHANNELS as usize]);
            }
            let timestamp = start_ts + (n * 10_000_000 / RATE as u64) as i64;
            if tx.send(Ok(AudioChunk { channels: CHANNELS, rate: RATE, samples, timestamp })).is_err() {
                break;
            }
            n += CHUNK;
        }
    });
    AudioCapture::from_channel(rx, stop, CHANNELS, RATE)
}

pub fn device() -> VideoDevice {
    VideoDevice { id: ID.into(), name: "Test pattern".into(), formats: vec![FORMAT] }
}

/// MiSTer-Kun, the MiSTer project's mascot (original design by HeWhoisRed;
/// remastered artwork by baxysquare: apple/Icon/CREDITS.md). Rendered from
/// apple/Icon/misterkun.svg:
/// `rsvg-convert -h 160 apple/Icon/misterkun.svg -o capture/assets/misterkun.png`.
const KUN_PNG: &[u8] = include_bytes!("../assets/misterkun.png");

/// MiSTer-Kun as premultiplied BGRA: (width, height, pixels).
fn kun() -> &'static (usize, usize, Vec<[u8; 4]>) {
    static KUN: OnceLock<(usize, usize, Vec<[u8; 4]>)> = OnceLock::new();
    KUN.get_or_init(|| {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(KUN_PNG));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().expect("MiSTer-Kun png");
        let mut buf = vec![0; reader.output_buffer_size().expect("MiSTer-Kun size")];
        let info = reader.next_frame(&mut buf).expect("MiSTer-Kun png");
        let channels = info.color_type.samples();
        let pixels = buf[..info.buffer_size()]
            .chunks_exact(channels)
            .map(|p| {
                let (r, g, b, a) = match channels {
                    4 => (p[0], p[1], p[2], p[3]),
                    3 => (p[0], p[1], p[2], 255),
                    _ => (p[0], p[0], p[0], p.get(1).copied().unwrap_or(255)),
                };
                let pre = |c: u8| (c as u16 * a as u16 / 255) as u8;
                [pre(b), pre(g), pre(r), a]
            })
            .collect();
        (info.width as usize, info.height as usize, pixels)
    })
}

/// Draw MiSTer-Kun centred on (cx, cy) of a BGRA frame `w` wide, clipped
/// to the frame.
fn draw_kun(data: &mut [u8], w: usize, h: usize, cx: usize, cy: usize) {
    let (kw, kh, pixels) = kun();
    for ky in 0..*kh {
        let Some(y) = (cy + ky).checked_sub(kh / 2).filter(|&y| y < h) else { continue };
        for kx in 0..*kw {
            let Some(x) = (cx + kx).checked_sub(kw / 2).filter(|&x| x < w) else { continue };
            let src = pixels[ky * kw + kx];
            let a = src[3] as u16;
            if a == 0 {
                continue;
            }
            let dst = &mut data[(y * w + x) * 4..][..3];
            for c in 0..3 {
                dst[c] = (src[c] as u16 + dst[c] as u16 * (255 - a) / 255) as u8;
            }
        }
    }
}

pub struct Pattern {
    started: Instant,
    started_ts: i64,
    frame: u64,
    /// MiSTer-Kun instead of the box (`DEMO_ID`).
    kun: bool,
}

impl Pattern {
    pub fn open(_output: Output, kun: bool) -> (Self, VideoFormat) {
        (Self { started: Instant::now(), started_ts: now_100ns(), frame: 0, kun }, FORMAT)
    }

    /// Its sound, in step with the picture.
    pub fn audio(&self) -> AudioCapture {
        tone(self.started, self.started_ts)
    }

    pub fn next_frame(&mut self, timeout: Duration) -> Option<VideoFrame> {
        let due = self.started + Duration::from_secs_f64(self.frame as f64 / FORMAT.fps);
        let now = Instant::now();
        if due > now + timeout {
            std::thread::sleep(timeout);
            return None;
        }
        if due > now {
            std::thread::sleep(due - now);
        }
        let (w, h) = (FORMAT.width as usize, FORMAT.height as usize);
        let mut data = vec![0u8; w * h * 4];
        let offset = (self.frame * 4) as usize;
        // Where the box is this second (or MiSTer-Kun, centred on it).
        let box_x = ((self.frame / 60) % 8) as usize * 160;
        for y in 0..h {
            let row = &mut data[y * w * 4..(y + 1) * w * 4];
            for x in 0..w {
                let bar = ((x + offset) / 160) % 8;
                let [r, g, b] = [
                    if bar & 1 != 0 { 230 } else { 20 },
                    if bar & 2 != 0 { 230 } else { 20 },
                    if bar & 4 != 0 { 230 } else { 20 },
                ];
                // A white box that steps once a second, to see motion and
                // check for frozen pictures.
                let in_box = !self.kun && (300..420).contains(&y) && (box_x..box_x + 120).contains(&x);
                let px = if in_box { [255, 255, 255, 255] } else { [b, g, r, 255] };
                row[x * 4..x * 4 + 4].copy_from_slice(&px);
            }
        }
        if self.kun {
            draw_kun(&mut data, w, h, box_x + 60, 360);
        }
        self.frame += 1;
        let timestamp = self.started_ts + ((self.frame - 1) as f64 * 10_000_000.0 / FORMAT.fps) as i64;
        Some(VideoFrame { format: PixelFormat::Bgra, width: FORMAT.width, height: FORMAT.height, stride: FORMAT.width * 4, data, timestamp })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(kun: bool) -> VideoFrame {
        let (mut p, _) = Pattern::open(Output::Bgra, kun);
        p.next_frame(Duration::from_secs(1)).unwrap()
    }

    #[test]
    fn demo_has_mister_kun_where_the_box_is() {
        let at = |f: &VideoFrame, x: usize, y: usize| f.data[(y * 1280 + x) * 4..][..3].to_vec();
        // Frame 0: the box is at x 0..120, y 300..420.
        let plain = frame(false);
        assert_eq!(at(&plain, 60, 360), [255, 255, 255]);
        let demo = frame(true);
        // No white box corner; MiSTer-Kun's light-grey face over the dark
        // first bar, centred where the box was.
        assert_ne!(at(&demo, 2, 302), [255, 255, 255]);
        let light = (280..440)
            .flat_map(|y| (0..140).map(move |x| (x, y)))
            .filter(|&(x, y)| at(&demo, x, y).iter().all(|&c| c > 150))
            .count();
        assert!(light > 5000, "{light} light pixels");
        // Outside both: the same bars.
        assert_eq!(at(&plain, 600, 100), at(&demo, 600, 100));
    }
}
