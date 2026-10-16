//! Demo hosts: the test pattern instead of a MiSTer's picture, and the input
//! a MiSTer would have received written out as short lines instead of sent
//! (docs/APP-STORE.md S1). Lets App Review and new users try the apps
//! without a MiSTer or a capture PC.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::keymap::linux_to_hid;

/// A line disappears this long after it was written (or, for mouse
/// movement, last added to).
pub const LINE_LIFETIME: Duration = Duration::from_secs(5);
/// At most this many lines are shown.
pub const MAX_LINES: usize = 8;
/// Movement this soon after the last joins its line.
const MOVE_JOIN: Duration = Duration::from_millis(600);
/// A button released this soon after it was pressed is a click.
const CLICK_TIME: Duration = Duration::from_millis(400);
/// A click this soon after another of the same button is a double click.
const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(500);

/// MiSTer-Kun, the MiSTer project's mascot (original design by HeWhoisRed;
/// remastered artwork by baxysquare: apple/Icon/CREDITS.md), drawn on the
/// demo's test pattern. Rendered from apple/Icon/misterkun.svg:
/// `rsvg-convert -h 220 apple/Icon/misterkun.svg -o core/assets/misterkun.png`.
const LOGO_PNG: &[u8] = include_bytes!("../assets/misterkun.png");

/// The logo as premultiplied BGRA rows: (width, height, pixels).
fn logo() -> &'static (usize, usize, Vec<[u8; 4]>) {
    static LOGO: OnceLock<(usize, usize, Vec<[u8; 4]>)> = OnceLock::new();
    LOGO.get_or_init(|| {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(LOGO_PNG));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().expect("logo png");
        let mut buf = vec![0; reader.output_buffer_size().expect("logo size")];
        let info = reader.next_frame(&mut buf).expect("logo png");
        let (w, h) = (info.width as usize, info.height as usize);
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
        (w, h, pixels)
    })
}

/// Draw MiSTer-Kun in the middle of a BGRA frame.
pub(crate) fn draw_logo(frame: &mut [u8], stride: usize, width: usize, height: usize) {
    let (w, h, pixels) = logo();
    if *w > width || *h > height {
        return;
    }
    let (x0, y0) = ((width - w) / 2, (height - h) / 2);
    for y in 0..*h {
        let row = &mut frame[(y0 + y) * stride + x0 * 4..][..w * 4];
        for (dst, src) in row.chunks_exact_mut(4).zip(&pixels[y * w..(y + 1) * w]) {
            let a = src[3] as u16;
            if a == 0 {
                continue;
            }
            for c in 0..3 {
                dst[c] = (src[c] as u16 + dst[c] as u16 * (255 - a) / 255) as u8;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Key,
    Move(f64, f64),
    ButtonDown(String),
    Click(String),
    Other,
}

#[derive(Debug, Clone)]
struct Line {
    kind: Kind,
    text: String,
    at: Instant,
}

#[derive(Default)]
struct State {
    lines: VecDeque<Line>,
    /// Modifiers held down, and whether a key was pressed with them (Shift
    /// then Tab is "Shift+Tab"; Shift alone is "Shift").
    modifiers: Vec<(u16, bool)>,
}

/// The input a demo host received, as lines for the app to show.
#[derive(Default)]
pub struct DemoLog {
    state: Mutex<State>,
}

impl DemoLog {
    /// One mrext message, as the session would have sent it.
    pub fn message(&self, message: &str) {
        self.message_at(message, Instant::now());
    }

    /// Relative mouse movement.
    pub fn moved(&self, dx: f64, dy: f64) {
        self.moved_at(dx, dy, Instant::now());
    }

    /// The lines to show now, oldest first.
    pub fn lines(&self) -> Vec<String> {
        self.lines_at(Instant::now())
    }

    fn message_at(&self, message: &str, now: Instant) {
        let mut s = self.state.lock().unwrap();
        let (kind, value) = message.split_once(':').unwrap_or((message, ""));
        match kind {
            "kbdRawDown" | "kbdRawUp" => {
                let Some(usage) = value.parse().ok().and_then(linux_to_hid) else {
                    return push(&mut s, Kind::Other, message.into(), now);
                };
                key(&mut s, usage, kind == "kbdRawDown", now);
            }
            "mouseBtn" => {
                let Some((button, dir)) = value.split_once('_') else {
                    return push(&mut s, Kind::Other, message.into(), now);
                };
                mouse_button(&mut s, button, dir == "down", now);
            }
            "mousePos" => push(&mut s, Kind::Other, format!("pointer at {value} (‰)"), now),
            _ => push(&mut s, Kind::Other, message.into(), now),
        }
    }

    fn moved_at(&self, dx: f64, dy: f64, now: Instant) {
        let mut s = self.state.lock().unwrap();
        if let Some(last) = s.lines.back_mut() {
            if let Kind::Move(x, y) = last.kind {
                if now.duration_since(last.at) < MOVE_JOIN {
                    let (x, y) = (x + dx, y + dy);
                    *last = Line { kind: Kind::Move(x, y), text: move_text(x, y), at: now };
                    return;
                }
            }
        }
        push(&mut s, Kind::Move(dx, dy), move_text(dx, dy), now);
    }

    fn lines_at(&self, now: Instant) -> Vec<String> {
        let mut s = self.state.lock().unwrap();
        s.lines.retain(|l| now.duration_since(l.at) < LINE_LIFETIME);
        s.lines.iter().map(|l| l.text.clone()).collect()
    }
}

fn push(s: &mut State, kind: Kind, text: String, at: Instant) {
    s.lines.push_back(Line { kind, text, at });
    while s.lines.len() > MAX_LINES {
        s.lines.pop_front();
    }
}

fn key(s: &mut State, usage: u16, down: bool, now: Instant) {
    if is_modifier(usage) {
        if down {
            if !s.modifiers.iter().any(|(u, _)| *u == usage) {
                s.modifiers.push((usage, false));
            }
        } else if let Some(i) = s.modifiers.iter().position(|(u, _)| *u == usage) {
            let (_, used) = s.modifiers.remove(i);
            if !used {
                push(s, Kind::Key, key_name(usage), now);
            }
        }
        return;
    }
    if !down {
        return;
    }
    let mut text = String::new();
    for (modifier, used) in s.modifiers.iter_mut() {
        *used = true;
        text.push_str(&key_name(*modifier));
        text.push('+');
    }
    text.push_str(&key_name(usage));
    push(s, Kind::Key, text, now);
}

fn mouse_button(s: &mut State, button: &str, down: bool, now: Instant) {
    if down {
        return push(s, Kind::ButtonDown(button.into()), format!("{button} button down"), now);
    }
    let pressed_just_now = matches!(
        s.lines.back(),
        Some(Line { kind: Kind::ButtonDown(b), at, .. }) if b == button && now.duration_since(*at) < CLICK_TIME
    );
    if !pressed_just_now {
        return push(s, Kind::Other, format!("{button} button up"), now);
    }
    s.lines.pop_back();
    let after_click = matches!(
        s.lines.back(),
        Some(Line { kind: Kind::Click(b), at, .. }) if b == button && now.duration_since(*at) < DOUBLE_CLICK_TIME
    );
    if after_click {
        s.lines.pop_back();
        let text = if button == "left" { "double click".into() } else { format!("{button} double click") };
        // Not a Click: a third click starts a new one.
        push(s, Kind::Other, text, now);
    } else {
        push(s, Kind::Click(button.into()), format!("{button} click"), now);
    }
}

fn move_text(dx: f64, dy: f64) -> String {
    let n = |v: f64| {
        let v = v.round() as i64;
        if v < 0 {
            format!("−{}", -v)
        } else {
            format!("+{v}")
        }
    };
    format!("mouse moved {}, {}", n(dx), n(dy))
}

fn is_modifier(usage: u16) -> bool {
    (0xE0..=0xE7).contains(&usage)
}

/// A key's name on a US keyboard (HID usage page 7).
fn key_name(usage: u16) -> String {
    let name = match usage {
        0x04..=0x1D => return ((b'A' + (usage - 0x04) as u8) as char).to_string(),
        0x1E..=0x26 => return ((b'1' + (usage - 0x1E) as u8) as char).to_string(),
        0x27 => "0",
        0x28 => "Enter",
        0x29 => "Esc",
        0x2A => "⌫ Backspace",
        0x2B => "Tab",
        0x2C => "Space",
        0x2D => "-",
        0x2E => "=",
        0x2F => "[",
        0x30 => "]",
        0x31 | 0x32 => "\\",
        0x33 => ";",
        0x34 => "'",
        0x35 => "`",
        0x36 => ",",
        0x37 => ".",
        0x38 => "/",
        0x39 => "Caps Lock",
        0x3A..=0x45 => return format!("F{}", usage - 0x3A + 1),
        0x46 => "Print Screen",
        0x47 => "Scroll Lock",
        0x48 => "Pause",
        0x49 => "Insert",
        0x4A => "Home",
        0x4B => "Page Up",
        0x4C => "Delete",
        0x4D => "End",
        0x4E => "Page Down",
        0x4F => "→",
        0x50 => "←",
        0x51 => "↓",
        0x52 => "↑",
        0x53 => "Num Lock",
        0x54 => "Keypad /",
        0x55 => "Keypad *",
        0x56 => "Keypad -",
        0x57 => "Keypad +",
        0x58 => "Keypad Enter",
        0x59..=0x61 => return format!("Keypad {}", usage - 0x59 + 1),
        0x62 => "Keypad 0",
        0x63 => "Keypad .",
        0x65 => "Menu",
        0xE0 => "Ctrl",
        0xE1 => "Shift",
        0xE2 => "Alt",
        0xE3 => "Win",
        0xE4 => "Right Ctrl",
        0xE5 => "Right Shift",
        0xE6 => "Right Alt",
        0xE7 => "Right Win",
        _ => return format!("key {usage:#04x}"),
    };
    name.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputState, MouseButton};

    /// Feeds what `InputState` produces, as the session would send it.
    struct Feed {
        log: DemoLog,
        input: InputState,
        now: Instant,
    }

    impl Feed {
        fn new() -> Self {
            Self { log: DemoLog::default(), input: InputState::default(), now: Instant::now() }
        }
        fn wait(&mut self, ms: u64) {
            self.now += Duration::from_millis(ms);
        }
        fn key(&mut self, usage: u16, down: bool) {
            let mut out = Vec::new();
            self.input.key_event(usage, down, &mut out);
            out.iter().for_each(|m| self.log.message_at(m, self.now));
        }
        fn tap(&mut self, usage: u16) {
            self.key(usage, true);
            self.wait(60);
            self.key(usage, false);
        }
        fn button(&mut self, button: MouseButton, down: bool) {
            let mut out = Vec::new();
            self.input.mouse_button(button, down, &mut out);
            out.iter().for_each(|m| self.log.message_at(m, self.now));
        }
        fn click(&mut self, button: MouseButton) {
            self.button(button, true);
            self.wait(40);
            self.button(button, false);
        }
        fn moved(&mut self, dx: f64, dy: f64) {
            self.log.moved_at(dx, dy, self.now);
        }
        fn lines(&self) -> Vec<String> {
            self.log.lines_at(self.now)
        }
    }

    #[test]
    fn logo_is_drawn_in_the_middle() {
        let (w, h) = (1280, 720);
        let mut frame = vec![0u8; w * h * 4];
        draw_logo(&mut frame, w * 4, w, h);
        let at = |x: usize, y: usize| &frame[(y * w + x) * 4..][..4];
        // Corners untouched; MiSTer-Kun's light-grey face in the middle.
        assert_eq!(at(0, 0), [0, 0, 0, 0]);
        let face = at(w / 2, h / 2 - 60);
        assert!(face[0] > 150 && face[1] > 150 && face[2] > 150, "{face:?}");
        // Too small a frame: left alone.
        let mut small = vec![7u8; 100 * 100 * 4];
        draw_logo(&mut small, 400, 100, 100);
        assert!(small.iter().all(|&b| b == 7));
    }

    #[test]
    fn keys_and_combos() {
        let mut f = Feed::new();
        f.tap(0x04); // A
        f.tap(0x45); // F12
        f.tap(0x2A); // backspace
        f.key(0xE1, true); // Shift+Tab
        f.tap(0x2B);
        f.key(0xE1, false);
        f.key(0xE0, true); // Ctrl+Alt+Delete
        f.key(0xE2, true);
        f.tap(0x4C);
        f.key(0xE2, false);
        f.key(0xE0, false);
        f.key(0xE1, true); // Shift alone
        f.key(0xE1, false);
        assert_eq!(f.lines(), ["A", "F12", "⌫ Backspace", "Shift+Tab", "Ctrl+Alt+Delete", "Shift"]);
    }

    #[test]
    fn clicks_double_clicks_and_drags() {
        let mut f = Feed::new();
        f.click(MouseButton::Left);
        f.wait(1000);
        f.click(MouseButton::Right);
        f.wait(1000);
        // The iPhone's double tap: a click, then the second tap's press.
        f.click(MouseButton::Left);
        f.wait(150);
        f.click(MouseButton::Left);
        f.wait(1000);
        // Tap and drag: click, hold, move, let go.
        f.click(MouseButton::Left);
        f.button(MouseButton::Left, true);
        f.moved(10.0, 0.0);
        f.wait(500);
        f.button(MouseButton::Left, false);
        assert_eq!(
            f.lines(),
            ["left click", "right click", "double click", "left click", "left button down", "mouse moved +10, +0", "left button up"]
        );
    }

    #[test]
    fn movement_joins_into_one_line_until_it_pauses() {
        let mut f = Feed::new();
        for _ in 0..50 {
            f.moved(2.4, -0.8);
            f.wait(16);
        }
        f.wait(1000);
        f.moved(-5.0, 3.0);
        assert_eq!(f.lines(), ["mouse moved +120, −40", "mouse moved −5, +3"]);
    }

    #[test]
    fn lines_clear_after_five_seconds_and_stay_few() {
        let mut f = Feed::new();
        f.tap(0x04);
        f.wait(3000);
        f.tap(0x05);
        f.wait(2500);
        assert_eq!(f.lines(), ["B"]);
        for _ in 0..20 {
            f.tap(0x06);
        }
        assert_eq!(f.lines().len(), MAX_LINES);
        f.wait(6000);
        assert!(f.lines().is_empty());
    }
}
