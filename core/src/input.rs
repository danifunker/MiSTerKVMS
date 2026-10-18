//! Input state machine: turns platform-neutral key/mouse events into mrext
//! WebSocket messages. Pure logic (no I/O) so it can be unit-tested.

use std::collections::BTreeSet;

use crate::keymap::{hid, hid_to_linux};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, uniffi::Enum)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    fn name(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum KeyResult {
    /// Sent (or a state change was applied) on MiSTer.
    Forwarded,
    /// Unknown key, auto-repeat, or a release for a key MiSTer never saw.
    Ignored,
    /// The release chord (`ReleaseChord`) was pressed together and let go.
    /// Everything held has been released; the shell should leave capture
    /// mode.
    ExitCapture,
}

/// The two keys that, pressed together, give the keyboard and mouse back.
/// Once both are down nothing more reaches the MiSTer; capture ends when
/// they're let go, so their releases don't reach the computer either (a
/// lone Win or Super release would open Start or Activities).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum ReleaseChord {
    /// Right ⌘/Win/Super + Right ⌥/Alt: on nearly every keyboard, and next
    /// to nothing on the MiSTer uses it.
    #[default]
    RightGuiAlt,
    /// Left ⌘/Win/Super + Left Alt, for keyboards without a right Win key.
    LeftGuiAlt,
    /// Left Ctrl + Left Alt, like QEMU. Ctrl+Alt combinations (Ctrl+Alt+Del)
    /// can't reach the MiSTer then.
    CtrlAlt,
}

impl ReleaseChord {
    fn keys(self) -> [u16; 2] {
        match self {
            ReleaseChord::RightGuiAlt => [hid::RIGHT_GUI, hid::RIGHT_ALT],
            ReleaseChord::LeftGuiAlt => [hid::LEFT_GUI, hid::LEFT_ALT],
            ReleaseChord::CtrlAlt => [hid::LEFT_CTRL, hid::LEFT_ALT],
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ReleaseChord::RightGuiAlt => "rightGuiAlt",
            ReleaseChord::LeftGuiAlt => "leftGuiAlt",
            ReleaseChord::CtrlAlt => "ctrlAlt",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "leftGuiAlt" => ReleaseChord::LeftGuiAlt,
            "ctrlAlt" => ReleaseChord::CtrlAlt,
            _ => ReleaseChord::RightGuiAlt,
        }
    }

    /// As this computer's keyboard labels the keys, e.g. "Right ⌘ + Right ⌥".
    pub fn name(self) -> String {
        let mac = cfg!(any(target_os = "macos", target_os = "ios"));
        let gui = if mac { "⌘" } else if cfg!(windows) { "Win" } else { "Super" };
        let alt = if mac { "⌥" } else { "Alt" };
        let ctrl = if mac { "⌃" } else { "Ctrl" };
        match self {
            ReleaseChord::RightGuiAlt => format!("Right {gui} + Right {alt}"),
            ReleaseChord::LeftGuiAlt => format!("Left {gui} + Left {alt}"),
            ReleaseChord::CtrlAlt => format!("Left {ctrl} + Left {alt}"),
        }
    }

    pub const ALL: [ReleaseChord; 3] = [ReleaseChord::RightGuiAlt, ReleaseChord::LeftGuiAlt, ReleaseChord::CtrlAlt];
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct InputSettings {
    /// Multiplier applied to relative mouse deltas.
    pub mouse_sensitivity: f64,
    /// Alt/Option + left click is sent as a middle click (for 3-button Unix
    /// desktops on single-button mice/trackpads).
    pub alt_click_is_middle: bool,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 1.0,
            alt_click_is_middle: true,
        }
    }
}

#[derive(Default)]
pub struct InputState {
    pub settings: InputSettings,
    held_keys: BTreeSet<u16>,
    held_buttons: BTreeSet<MouseButton>,
    left_is_middle: bool,
    pub release_chord: ReleaseChord,
    /// The release chord's keys still down after it was pressed (None: not
    /// pressed). Capture ends when the last one is let go.
    releasing: Option<BTreeSet<u16>>,
}

impl InputState {
    pub fn key_event(&mut self, usage: u16, down: bool, out: &mut Vec<String>) -> KeyResult {
        let Some(code) = hid_to_linux(usage) else {
            return KeyResult::Ignored;
        };
        let chord = self.release_chord.keys();
        // The chord was pressed: swallow everything until its keys are up.
        if let Some(waiting) = &mut self.releasing {
            if !down {
                waiting.remove(&usage);
                if waiting.is_empty() {
                    self.releasing = None;
                    return KeyResult::ExitCapture;
                }
            }
            return KeyResult::Ignored;
        }
        if down && chord.contains(&usage) && chord.iter().all(|k| *k == usage || self.held_keys.contains(k)) {
            self.release_all(out);
            self.releasing = Some(chord.into_iter().collect());
            return KeyResult::Ignored;
        }
        if down {
            if !self.held_keys.insert(usage) {
                return KeyResult::Ignored;
            }
            out.push(format!("kbdRawDown:{code}"));
        } else {
            if !self.held_keys.remove(&usage) {
                return KeyResult::Ignored;
            }
            out.push(format!("kbdRawUp:{code}"));
        }
        KeyResult::Forwarded
    }

    pub fn mouse_button(&mut self, button: MouseButton, down: bool, out: &mut Vec<String>) {
        let mut button = button;
        if button == MouseButton::Left {
            if down && self.settings.alt_click_is_middle && self.alt_held() {
                // Let go of Alt on MiSTer so the core sees a plain middle click.
                for usage in [hid::LEFT_ALT, hid::RIGHT_ALT] {
                    if self.held_keys.remove(&usage) {
                        out.push(format!("kbdRawUp:{}", hid_to_linux(usage).unwrap()));
                    }
                }
                self.left_is_middle = true;
                button = MouseButton::Middle;
            } else if !down && self.left_is_middle {
                self.left_is_middle = false;
                button = MouseButton::Middle;
            }
        }
        let changed = if down {
            self.held_buttons.insert(button)
        } else {
            self.held_buttons.remove(&button)
        };
        if changed {
            let dir = if down { "down" } else { "up" };
            out.push(format!("mouseBtn:{}_{dir}", button.name()));
        }
    }

    /// Release everything MiSTer thinks is held.
    pub fn release_all(&mut self, out: &mut Vec<String>) {
        for usage in std::mem::take(&mut self.held_keys) {
            if let Some(code) = hid_to_linux(usage) {
                out.push(format!("kbdRawUp:{code}"));
            }
        }
        for button in std::mem::take(&mut self.held_buttons) {
            out.push(format!("mouseBtn:{}_up", button.name()));
        }
        self.left_is_middle = false;
    }

    /// Capture ended or restarted: a half-pressed release chord is forgotten.
    pub fn reset_chord(&mut self) {
        self.releasing = None;
    }

    fn alt_held(&self) -> bool {
        self.held_keys.contains(&hid::LEFT_ALT) || self.held_keys.contains(&hid::RIGHT_ALT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: impl FnOnce(&mut InputState, &mut Vec<String>)) -> Vec<String> {
        let mut state = InputState::default();
        let mut out = Vec::new();
        f(&mut state, &mut out);
        out
    }

    #[test]
    fn key_down_up_and_repeat() {
        let out = run(|s, o| {
            assert_eq!(s.key_event(0x04, true, o), KeyResult::Forwarded);
            assert_eq!(s.key_event(0x04, true, o), KeyResult::Ignored);
            assert_eq!(s.key_event(0x04, false, o), KeyResult::Forwarded);
            assert_eq!(s.key_event(0x04, false, o), KeyResult::Ignored);
        });
        assert_eq!(out, ["kbdRawDown:30", "kbdRawUp:30"]);
    }

    #[test]
    fn release_chord_exits_when_let_go() {
        let out = run(|s, o| {
            assert_eq!(s.key_event(hid::RIGHT_GUI, true, o), KeyResult::Forwarded);
            // Both down: the MiSTer lets go of everything, nothing more goes.
            assert_eq!(s.key_event(hid::RIGHT_ALT, true, o), KeyResult::Ignored);
            // The old chord's Esc is swallowed too.
            assert_eq!(s.key_event(hid::ESCAPE, true, o), KeyResult::Ignored);
            assert_eq!(s.key_event(hid::ESCAPE, false, o), KeyResult::Ignored);
            assert_eq!(s.key_event(hid::RIGHT_ALT, false, o), KeyResult::Ignored);
            assert_eq!(s.key_event(hid::RIGHT_GUI, false, o), KeyResult::ExitCapture);
            // Back to normal afterwards.
            assert_eq!(s.key_event(hid::RIGHT_GUI, true, o), KeyResult::Forwarded);
        });
        assert_eq!(out, ["kbdRawDown:126", "kbdRawUp:126", "kbdRawDown:126"]);
    }

    #[test]
    fn other_release_chords() {
        for (chord, keys) in [
            (ReleaseChord::LeftGuiAlt, [hid::LEFT_ALT, hid::LEFT_GUI]),
            (ReleaseChord::CtrlAlt, [hid::LEFT_CTRL, hid::LEFT_ALT]),
        ] {
            let mut s = InputState { release_chord: chord, ..Default::default() };
            let o = &mut Vec::new();
            // The default chord's keys are plain keys now.
            assert_eq!(s.key_event(hid::RIGHT_GUI, true, o), KeyResult::Forwarded);
            assert_eq!(s.key_event(hid::RIGHT_ALT, true, o), KeyResult::Forwarded);
            assert_eq!(s.key_event(keys[0], true, o), KeyResult::Forwarded);
            assert_eq!(s.key_event(keys[1], true, o), KeyResult::Ignored);
            assert_eq!(s.key_event(keys[1], false, o), KeyResult::Ignored);
            assert_eq!(s.key_event(keys[0], false, o), KeyResult::ExitCapture);
            assert_eq!(ReleaseChord::from_id(chord.id()), chord);
        }
        assert!(ReleaseChord::CtrlAlt.name().contains("Left"));
    }

    #[test]
    fn one_chord_key_alone_is_forwarded() {
        let out = run(|s, o| {
            assert_eq!(s.key_event(hid::RIGHT_ALT, true, o), KeyResult::Forwarded);
            assert_eq!(s.key_event(0x04, true, o), KeyResult::Forwarded);
            assert_eq!(s.key_event(hid::RIGHT_ALT, false, o), KeyResult::Forwarded);
        });
        assert_eq!(out, ["kbdRawDown:100", "kbdRawDown:30", "kbdRawUp:100"]);
    }

    #[test]
    fn plain_esc_is_forwarded() {
        let out = run(|s, o| {
            s.key_event(hid::RIGHT_GUI, true, o);
            assert_eq!(s.key_event(hid::ESCAPE, true, o), KeyResult::Forwarded);
        });
        assert_eq!(out, ["kbdRawDown:126", "kbdRawDown:1"]);
    }

    #[test]
    fn alt_click_becomes_middle() {
        let out = run(|s, o| {
            s.key_event(hid::LEFT_ALT, true, o);
            s.mouse_button(MouseButton::Left, true, o);
            s.mouse_button(MouseButton::Left, false, o);
            s.key_event(hid::LEFT_ALT, false, o);
            s.mouse_button(MouseButton::Left, true, o);
        });
        assert_eq!(
            out,
            [
                "kbdRawDown:56",
                "kbdRawUp:56",
                "mouseBtn:middle_down",
                "mouseBtn:middle_up",
                "mouseBtn:left_down",
            ]
        );
    }

    #[test]
    fn release_all_covers_keys_and_buttons() {
        let out = run(|s, o| {
            s.key_event(0x04, true, o);
            s.mouse_button(MouseButton::Right, true, o);
            o.clear();
            s.release_all(o);
        });
        assert_eq!(out, ["kbdRawUp:30", "mouseBtn:right_up"]);
    }

    #[test]
    fn unpaired_button_up_is_dropped() {
        let out = run(|s, o| s.mouse_button(MouseButton::Left, false, o));
        assert!(out.is_empty());
    }
}
