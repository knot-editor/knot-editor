//! Keyboard input for the program, and the modes that route the mouse.
//!
//! Keystrokes become libghostty key events, which the terminal encodes in
//! the keyboard protocol the program asked for.

use crate::app::keys::Keystroke;
use knot_terminal::{Key, KeyEvent, Mode, Modifiers, Terminal};

/// Which mouse events the program asked to be told about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MouseTracking {
    #[default]
    Off,
    /// Presses and releases.
    Click,
    /// Presses, releases, and motion while a button is held.
    Drag,
    /// Presses, releases, and all motion.
    Motion,
}

/// The terminal modes that decide where mouse and wheel input go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InputModes {
    pub(crate) mouse: MouseTracking,
    pub(crate) alternate_screen: bool,
    pub(crate) alternate_scroll: bool,
}

impl InputModes {
    pub(crate) fn read(terminal: &Terminal) -> Self {
        let mouse = if terminal.mode(Mode::MouseAny) {
            MouseTracking::Motion
        } else if terminal.mode(Mode::MouseButton) {
            MouseTracking::Drag
        } else if terminal.mode(Mode::MouseNormal) || terminal.mode(Mode::MouseX10) {
            MouseTracking::Click
        } else {
            MouseTracking::Off
        };
        Self {
            mouse,
            alternate_screen: terminal.alternate_screen(),
            alternate_scroll: terminal.mode(Mode::AlternateScroll),
        }
    }

    pub(crate) fn mouse_reporting(&self) -> bool {
        self.mouse != MouseTracking::Off
    }
}

/// The bytes `stroke` sends to the program; empty for Command or Super
/// chords, which stay with the editor, and for keys with no encoding.
pub(crate) fn terminal_key_bytes(stroke: &Keystroke, terminal: &Terminal) -> Vec<u8> {
    if stroke.modifiers.platform {
        return Vec::new();
    }
    let name = stroke.key.to_lowercase();
    let key = named_key(&name).or_else(|| single_char(&name).map(Key::Character));
    let modifiers = Modifiers {
        shift: stroke.modifiers.shift,
        control: stroke.modifiers.control,
        alt: stroke.modifiers.alt,
        super_key: false,
    };
    // As in Ghostty on macOS, Option-Left and Option-Right move by word:
    // shells bind Escape b and Escape f, not the modified arrows.
    if cfg!(target_os = "macos")
        && modifiers
            == (Modifiers {
                alt: true,
                ..Modifiers::default()
            })
    {
        match key {
            Some(Key::Left) => return b"\x1bb".to_vec(),
            Some(Key::Right) => return b"\x1bf".to_vec(),
            _ => {}
        }
    }
    let unshifted = if key == Some(Key::Space) {
        Some(' ')
    } else {
        single_char(&name)
    };
    // The encoder wants what the key types without Control or Alt, which
    // act as modifiers here (Option is Alt on macOS).
    let typed = stroke
        .key_char
        .as_deref()
        .filter(|text| !modifiers.control && !modifiers.alt && !text.contains(char::is_control));
    let text = match (key, typed) {
        (Some(Key::Space), _) => " ".to_owned(),
        (Some(Key::Character(_)) | None, Some(text)) => text.to_owned(),
        (Some(Key::Character(character)), None) if modifiers.shift => {
            character.to_uppercase().collect()
        }
        (Some(Key::Character(character)), None) => character.to_string(),
        _ => String::new(),
    };
    terminal.encode_key(&KeyEvent {
        key,
        modifiers,
        consumed: Modifiers {
            shift: modifiers.shift && !text.is_empty(),
            ..Modifiers::default()
        },
        text: &text,
        unshifted,
        ..KeyEvent::default()
    })
}

fn single_char(name: &str) -> Option<char> {
    let mut chars = name.chars();
    chars.next().filter(|_| chars.next().is_none())
}

fn named_key(name: &str) -> Option<Key> {
    Some(match name {
        "space" => Key::Space,
        "enter" => Key::Enter,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "escape" => Key::Escape,
        "insert" => Key::Insert,
        "delete" => Key::Delete,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        _ => {
            let number = name.strip_prefix('f')?.parse().ok()?;
            Key::Function(number)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keys::Modifiers;
    use knot_terminal::TerminalSize;

    /// The bytes of `stroke` after the program wrote `setup`.
    fn encode(stroke: &Keystroke, setup: &[u8]) -> Vec<u8> {
        let size = TerminalSize {
            columns: 80,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        };
        let mut terminal = Terminal::new(size, 0);
        terminal.write(setup);
        terminal_key_bytes(stroke, &terminal)
    }

    fn parsed(key: &str, setup: &[u8]) -> Vec<u8> {
        let mut stroke = Keystroke::parse(key).unwrap();
        if !stroke.modifiers.control && !stroke.modifiers.alt && stroke.key.chars().count() == 1 {
            stroke.key_char = Some(if stroke.modifiers.shift {
                stroke.key.to_uppercase()
            } else {
                stroke.key.clone()
            });
        }
        encode(&stroke, setup)
    }

    fn with_bits(key: &str, bits: u8) -> Keystroke {
        Keystroke {
            key: key.into(),
            modifiers: Modifiers {
                shift: bits & 1 != 0,
                alt: bits & 2 != 0,
                control: bits & 4 != 0,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn cursor_function_and_navigation_modes_and_every_modifier() {
        for (app_cursor, setup) in [(false, &b""[..]), (true, b"\x1b[?1h")] {
            for (key, suffix) in [
                ("up", 'A'),
                ("down", 'B'),
                ("right", 'C'),
                ("left", 'D'),
                ("home", 'H'),
                ("end", 'F'),
                ("f1", 'P'),
                ("f2", 'Q'),
                ("f3", 'R'),
                ("f4", 'S'),
            ] {
                for bits in 0..8 {
                    if cfg!(target_os = "macos") && bits == 2 && matches!(key, "left" | "right") {
                        // Option-arrows move by word; see below.
                        continue;
                    }
                    let expected = if bits != 0 && key == "f3" {
                        // `CSI 1;nR` would read as a cursor position report.
                        format!("\x1b[13;{}~", bits + 1)
                    } else if bits != 0 {
                        format!("\x1b[1;{}{suffix}", bits + 1)
                    } else if app_cursor || key.starts_with('f') {
                        format!("\x1bO{suffix}")
                    } else {
                        format!("\x1b[{suffix}")
                    };
                    assert_eq!(
                        encode(&with_bits(key, bits), setup),
                        expected.as_bytes(),
                        "{key}, modifiers {bits}"
                    );
                }
            }
        }
        for (key, number) in [
            ("insert", 2),
            ("delete", 3),
            ("pageup", 5),
            ("pagedown", 6),
            ("f5", 15),
            ("f6", 17),
            ("f7", 18),
            ("f8", 19),
            ("f9", 20),
            ("f10", 21),
            ("f11", 23),
            ("f12", 24),
            ("f13", 25),
        ] {
            for bits in 0..8 {
                let expected = if bits == 0 {
                    format!("\x1b[{number}~")
                } else {
                    format!("\x1b[{number};{}~", bits + 1)
                };
                assert_eq!(
                    encode(&with_bits(key, bits), b"\x1b[?1h"),
                    expected.as_bytes(),
                    "{key}, modifiers {bits}"
                );
            }
        }
    }

    #[test]
    fn legacy_text_controls_and_alt_prefix() {
        for (key, byte) in [
            ("space", 0),
            ("2", 0),
            ("a", 1),
            ("z", 26),
            ("\\", 28),
            ("]", 29),
            ("6", 30),
            ("/", 31),
            ("8", 127),
            ("backspace", 8),
        ] {
            assert_eq!(parsed(&format!("ctrl-{key}"), b""), [byte], "ctrl-{key}");
        }
        for (key, expected) in [
            ("enter", "\r"),
            ("tab", "\t"),
            ("escape", "\x1b"),
            ("backspace", "\x7f"),
            ("space", " "),
            ("a", "a"),
            ("shift-a", "A"),
            ("shift-tab", "\x1b[Z"),
            ("ctrl-alt-up", "\x1b[1;7A"),
            ("alt-enter", "\x1b\r"),
            ("alt-a", "\x1ba"),
            ("alt-shift-a", "\x1bA"),
            ("ctrl-alt-a", "\x1b\x01"),
            ("alt-up", "\x1b[1;3A"),
            ("shift-alt-left", "\x1b[1;4D"),
            ("cmd-a", ""),
            // Keys whose legacy byte is ambiguous are disambiguated, as
            // fixterms and Ghostty do.
            ("ctrl-[", "\x1b[91;5u"),
            ("ctrl-i", "\x1b[105;5u"),
            ("ctrl-shift-a", "\x1b[97;6u"),
            ("ctrl-enter", "\x1b[27;5;13~"),
            ("shift-enter", "\x1b[27;2;13~"),
        ] {
            assert_eq!(
                String::from_utf8_lossy(&parsed(key, b"")),
                expected,
                "{key}"
            );
        }
        let stroke = Keystroke {
            key: "é".into(),
            key_char: Some("é界".into()),
            ..Default::default()
        };
        assert_eq!(encode(&stroke, b""), "é界".as_bytes());
        // Option is Alt: Option-b sends Escape b, not the ∫ it types.
        let stroke = Keystroke {
            key: "b".into(),
            key_char: Some("∫".into()),
            modifiers: Modifiers {
                alt: true,
                ..Default::default()
            },
        };
        assert_eq!(encode(&stroke, b""), b"\x1bb");
        let stroke = Keystroke {
            key: "2".into(),
            key_char: Some("2".into()),
            ..Default::default()
        };
        assert_eq!(encode(&stroke, b""), b"2");
    }

    #[test]
    fn option_arrows_move_by_word_on_macos() {
        let (left, right): (&[u8], &[u8]) = if cfg!(target_os = "macos") {
            (b"\x1bb", b"\x1bf")
        } else {
            (b"\x1b[1;3D", b"\x1b[1;3C")
        };
        for setup in [&b""[..], b"\x1b[>1u"] {
            assert_eq!(parsed("alt-left", setup), left);
            assert_eq!(parsed("alt-right", setup), right);
        }
    }

    #[test]
    fn kitty_keyboard_flags_requested_by_the_program() {
        let disambiguate = b"\x1b[>1u";
        for (key, expected) in [
            ("a", "a"),
            ("ctrl-a", "\x1b[97;5u"),
            ("ctrl-i", "\x1b[105;5u"),
            ("tab", "\t"),
            ("escape", "\x1b[27u"),
            ("shift-enter", "\x1b[13;2u"),
            ("ctrl-enter", "\x1b[13;5u"),
            ("alt-a", "\x1b[97;3u"),
            ("ctrl-up", "\x1b[1;5A"),
        ] {
            assert_eq!(
                String::from_utf8_lossy(&parsed(key, disambiguate)),
                expected,
                "{key}"
            );
        }
        // Every flag: text keys report their codes and the text they type.
        let all = b"\x1b[>31u";
        assert_eq!(parsed("a", all), b"\x1b[97;;97u");
        assert_eq!(parsed("shift-a", all), b"\x1b[97:65;2;65u");
        // Popping the flags returns to legacy encoding.
        assert_eq!(parsed("ctrl-enter", b"\x1b[>1u\x1b[<u"), b"\x1b[27;5;13~");
    }

    #[test]
    fn modify_other_keys_requested_by_the_program() {
        let state_2 = b"\x1b[>4;2m";
        for (key, expected) in [
            ("a", "a"),
            ("ctrl-i", "\x1b[27;5;105~"),
            ("ctrl-a", "\x1b[27;5;97~"),
            ("ctrl-enter", "\x1b[27;5;13~"),
            ("ctrl-shift-a", "\x1b[27;6;65~"),
        ] {
            assert_eq!(
                String::from_utf8_lossy(&parsed(key, state_2)),
                expected,
                "{key}"
            );
        }
        assert_eq!(parsed("ctrl-a", b"\x1b[>4;2m\x1b[>4m"), [1]);
    }
}
