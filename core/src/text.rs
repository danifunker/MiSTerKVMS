//! Text and key names → HID usages (US layout), for typing text into a
//! machine and for key combos like "ctrl+alt+del". Shared by the CLI and the
//! app's paste-as-keystrokes.

use crate::keymap::hid;

pub const LEFT_CTRL: u16 = 0xE0;
pub const LEFT_SHIFT: u16 = 0xE1;

/// The key (and whether Shift is needed) that types `c` on a US keyboard.
pub fn key_for_char(c: char) -> Option<(u16, bool)> {
    let plain = |u: u16| Some((u, false));
    let shifted = |u: u16| Some((u, true));
    match c {
        'a'..='z' => plain(0x04 + (c as u16 - 'a' as u16)),
        'A'..='Z' => shifted(0x04 + (c as u16 - 'A' as u16)),
        '1'..='9' => plain(0x1E + (c as u16 - '1' as u16)),
        '0' => plain(0x27),
        '\n' | '\r' => plain(0x28),
        '\t' => plain(0x2B),
        ' ' => plain(0x2C),
        '\u{8}' => plain(0x2A),
        '\u{1b}' => plain(hid::ESCAPE),
        '-' => plain(0x2D),
        '_' => shifted(0x2D),
        '=' => plain(0x2E),
        '+' => shifted(0x2E),
        '[' => plain(0x2F),
        '{' => shifted(0x2F),
        ']' => plain(0x30),
        '}' => shifted(0x30),
        '\\' => plain(0x31),
        '|' => shifted(0x31),
        ';' => plain(0x33),
        ':' => shifted(0x33),
        '\'' => plain(0x34),
        '"' => shifted(0x34),
        '`' => plain(0x35),
        '~' => shifted(0x35),
        ',' => plain(0x36),
        '<' => shifted(0x36),
        '.' => plain(0x37),
        '>' => shifted(0x37),
        '/' => plain(0x38),
        '?' => shifted(0x38),
        '!' => shifted(0x1E),
        '@' => shifted(0x1F),
        '#' => shifted(0x20),
        '$' => shifted(0x21),
        '%' => shifted(0x22),
        '^' => shifted(0x23),
        '&' => shifted(0x24),
        '*' => shifted(0x25),
        '(' => shifted(0x26),
        ')' => shifted(0x27),
        _ => None,
    }
}

/// The on-screen keyboard's typing: each key held for a tap (60 ms), then
/// this gap, so cores that scan the keyboard slowly see every key.
pub const TYPE_GAP_MS: u32 = 25;

/// The Type Text box's speed, keys per second: the range of its slider and
/// where it starts.
pub const MIN_TYPE_SPEED: f64 = 2.0;
pub const MAX_TYPE_SPEED: f64 = 40.0;
pub const DEFAULT_TYPE_SPEED: f64 = 10.0;

/// How long each key is held, and the gap after it, at `keys_per_sec`: 60%
/// of a key's time held, at least 15 ms down and 10 ms up.
pub fn pace(keys_per_sec: f64) -> (u32, u32) {
    let speed = if keys_per_sec > 0.0 { keys_per_sec.clamp(MIN_TYPE_SPEED, MAX_TYPE_SPEED) } else { DEFAULT_TYPE_SPEED };
    let per = 1000.0 / speed;
    let hold = (per * 0.6).max(15.0);
    let gap = (per - hold).max(10.0);
    (hold.round() as u32, gap.round() as u32)
}

/// The slider's range and default, for the apps.
#[uniffi::export]
pub fn type_speed_range() -> Vec<f64> {
    vec![MIN_TYPE_SPEED, MAX_TYPE_SPEED, DEFAULT_TYPE_SPEED]
}

/// Text as the characters to type: Windows line ends (CR LF) are one Enter,
/// and the typographic punctuation that phones and documents put in (curly
/// quotes, dashes, the ellipsis, non-breaking spaces) is what a US keyboard
/// types instead of being skipped.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.replace("\r\n", "\n").chars() {
        match c {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{2032}' => out.push('\''),
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' => out.push('"'),
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            '\u{00A0}' | '\u{2007}' | '\u{202F}' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

/// What typing `text` would do, for the apps' Type Text box.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TextTyping {
    /// Characters that become key presses.
    pub keys: u32,
    /// Characters with no key on a US keyboard (skipped).
    pub skipped: u32,
    /// Roughly how long it takes.
    pub seconds: f64,
}

#[uniffi::export]
pub fn text_typing(text: String, keys_per_sec: f64) -> TextTyping {
    let text = normalize(&text);
    let keys = text.chars().filter(|&c| key_for_char(c).is_some()).count() as u32;
    let skipped = text.chars().count() as u32 - keys;
    let (hold, gap) = pace(keys_per_sec);
    TextTyping { keys, skipped, seconds: keys as f64 * (hold + gap) as f64 / 1000.0 }
}

/// HID usage for a key name: "a", "5", "f12", "enter", "lctrl", "ralt", "/".
pub fn key_by_name(name: &str) -> Option<u16> {
    let n = name.trim().to_ascii_lowercase();
    if let Some(num) = n.strip_prefix('f').and_then(|d| d.parse::<u16>().ok()) {
        return match num {
            1..=12 => Some(0x3A + num - 1),
            13..=24 => Some(0x68 + num - 13),
            _ => None,
        };
    }
    let usage = match n.as_str() {
        "ctrl" | "control" | "lctrl" => 0xE0,
        "shift" | "lshift" => 0xE1,
        "alt" | "lalt" | "option" => 0xE2,
        "meta" | "gui" | "win" | "cmd" | "command" | "super" | "lmeta" => 0xE3,
        "rctrl" => 0xE4,
        "rshift" => 0xE5,
        "ralt" | "altgr" | "roption" => 0xE6,
        "rmeta" | "rgui" | "rwin" | "rcmd" => 0xE7,
        "esc" | "escape" => 0x29,
        "enter" | "return" => 0x28,
        "tab" => 0x2B,
        "space" => 0x2C,
        "backspace" | "bs" => 0x2A,
        "delete" | "del" => 0x4C,
        "insert" | "ins" => 0x49,
        "home" => 0x4A,
        "end" => 0x4D,
        "pageup" | "pgup" => 0x4B,
        "pagedown" | "pgdn" => 0x4E,
        "up" => 0x52,
        "down" => 0x51,
        "left" => 0x50,
        "right" => 0x4F,
        "capslock" | "caps" => 0x39,
        "numlock" => 0x53,
        "scrolllock" => 0x47,
        "printscreen" | "prtsc" | "sysrq" => 0x46,
        "pause" | "break" => 0x48,
        "menu" | "osd" => 0x45, // MiSTer menu = F12
        "app" | "compose" => 0x65,
        _ => {
            // Single characters: their unshifted key.
            let mut chars = n.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else { return None };
            return key_for_char(c).map(|(u, _)| u);
        }
    };
    Some(usage)
}

/// Parse "ctrl+alt+del" into usages, in press order.
pub fn parse_combo(combo: &str) -> Result<Vec<u16>, String> {
    // "+" alone (or at the end, as in "shift++") means the plus key.
    let parts: Vec<&str> = if combo == "+" { vec!["+"] } else { combo.split('+').collect() };
    let mut keys = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let p = if p.is_empty() && i == parts.len() - 1 { "=" } else { p };
        if p.is_empty() {
            continue;
        }
        keys.push(key_by_name(p).ok_or_else(|| format!("unknown key '{p}'"))?);
    }
    if keys.is_empty() {
        return Err("no keys given".into());
    }
    Ok(keys)
}

/// Replace C-style escapes (\n, \t, \e, \\) typed on a command line.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('e') => out.push('\u{1b}'),
            Some('b') => out.push('\u{8}'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn typing_counts_keys_skips_and_one_enter_per_line_end() {
        let t = super::text_typing("ls -l\r\ncd é\n".into(), 10.0);
        // "ls -l" 5, Enter, "cd " 3, Enter; é has no US key.
        assert_eq!((t.keys, t.skipped), (10, 1));
        assert!((t.seconds - 1.0).abs() < 1e-9);
    }

    #[test]
    fn typographic_punctuation_types_as_plain() {
        assert_eq!(super::normalize("LOAD \u{201C}*\u{201D},8,1"), "LOAD \"*\",8,1");
        assert_eq!(super::normalize("it\u{2019}s \u{2014} ok\u{2026}\u{00A0}x"), "it's - ok... x");
        assert_eq!(super::text_typing("\u{201C}a\u{201D}".into(), 10.0).skipped, 0);
    }

    #[test]
    fn pace_holds_most_of_each_key_and_keeps_real_presses() {
        assert_eq!(super::pace(10.0), (60, 40));
        assert_eq!(super::pace(2.0), (300, 200));
        assert_eq!(super::pace(40.0), (15, 10));
        assert_eq!(super::pace(1000.0), super::pace(40.0), "clamped");
        assert_eq!(super::pace(0.0), super::pace(10.0), "unset: the default");
    }

    use super::*;

    #[test]
    fn chars() {
        assert_eq!(key_for_char('a'), Some((0x04, false)));
        assert_eq!(key_for_char('Z'), Some((0x1D, true)));
        assert_eq!(key_for_char('"'), Some((0x34, true)));
        assert_eq!(key_for_char('\n'), Some((0x28, false)));
        assert_eq!(key_for_char('é'), None);
    }

    #[test]
    fn combos() {
        assert_eq!(parse_combo("ctrl+alt+del"), Ok(vec![0xE0, 0xE2, 0x4C]));
        assert_eq!(parse_combo("F12"), Ok(vec![0x45]));
        assert_eq!(parse_combo("shift+/"), Ok(vec![0xE1, 0x38]));
        assert_eq!(parse_combo("rmeta+ralt+esc"), Ok(vec![0xE7, 0xE6, 0x29]));
        assert!(parse_combo("ctrl+nope").is_err());
    }

    #[test]
    fn escapes() {
        assert_eq!(unescape(r#"LOAD "*",8,1\n"#), "LOAD \"*\",8,1\n");
        assert_eq!(unescape(r"a\\b"), r"a\b");
    }
}
