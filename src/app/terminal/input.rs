//! Legacy xterm keyboard encoding from the terminal's current modes.

use crate::app::keys::Keystroke;
use knot_terminal::{Mode, Terminal};

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

/// How mouse reports are encoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MouseFormat {
    #[default]
    Legacy,
    Utf8,
    Sgr,
}

/// The terminal modes that change what keyboard and mouse input encode to
/// or where the wheel goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InputModes {
    pub(crate) mouse: MouseTracking,
    pub(crate) mouse_format: MouseFormat,
    pub(crate) app_cursor: bool,
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
        let mouse_format = if terminal.mode(Mode::MouseFormatSgr) {
            MouseFormat::Sgr
        } else if terminal.mode(Mode::MouseFormatUtf8) {
            MouseFormat::Utf8
        } else {
            MouseFormat::Legacy
        };
        Self {
            mouse,
            mouse_format,
            app_cursor: terminal.mode(Mode::CursorKeys),
            alternate_screen: terminal.alternate_screen(),
            alternate_scroll: terminal.mode(Mode::AlternateScroll),
        }
    }

    pub(crate) fn mouse_reporting(&self) -> bool {
        self.mouse != MouseTracking::Off
    }
}

pub(crate) fn terminal_key_bytes(stroke: &Keystroke, mode: InputModes) -> Vec<u8> {
    if stroke.modifiers.platform {
        return Vec::new();
    }
    let modifiers = stroke.modifiers;
    let parameter = 1
        + u8::from(modifiers.shift)
        + 2 * u8::from(modifiers.alt)
        + 4 * u8::from(modifiers.control);
    let key = stroke.key.to_lowercase();
    let final_byte = match key.as_str() {
        "up" => Some('A'),
        "down" => Some('B'),
        "right" => Some('C'),
        "left" => Some('D'),
        "home" => Some('H'),
        "end" => Some('F'),
        "f1" => Some('P'),
        "f2" => Some('Q'),
        "f3" => Some('R'),
        "f4" => Some('S'),
        _ => None,
    };
    if let Some(final_byte) = final_byte {
        return if parameter != 1 {
            format!("\x1b[1;{parameter}{final_byte}").into_bytes()
        } else if key.starts_with('f') || mode.app_cursor {
            format!("\x1bO{final_byte}").into_bytes()
        } else {
            format!("\x1b[{final_byte}").into_bytes()
        };
    }
    let number = match key.as_str() {
        "insert" => Some(2),
        "delete" => Some(3),
        "pageup" => Some(5),
        "pagedown" => Some(6),
        "f5" => Some(15),
        "f6" => Some(17),
        "f7" => Some(18),
        "f8" => Some(19),
        "f9" => Some(20),
        "f10" => Some(21),
        "f11" => Some(23),
        "f12" => Some(24),
        _ => None,
    };
    if let Some(number) = number {
        return if parameter == 1 {
            format!("\x1b[{number}~")
        } else {
            format!("\x1b[{number};{parameter}~")
        }
        .into_bytes();
    }
    if key == "tab" && modifiers.shift {
        return if parameter == 2 {
            b"\x1b[Z".to_vec()
        } else {
            format!("\x1b[1;{parameter}Z").into_bytes()
        };
    }
    let mut bytes = match key.as_str() {
        "enter" => b"\r".to_vec(),
        "backspace" => vec![if modifiers.control { 0x08 } else { 0x7f }],
        "tab" => b"\t".to_vec(),
        "escape" => vec![0x1b],
        _ if modifiers.control => control_byte(&key).into_iter().collect(),
        "space" => vec![b' '],
        _ if modifiers.alt && key.len() == 1 && key.is_ascii() => if modifiers.shift {
            key.to_uppercase()
        } else {
            key.clone()
        }
        .into_bytes(),
        _ => stroke
            .key_char
            .as_deref()
            .unwrap_or_default()
            .as_bytes()
            .to_vec(),
    };
    if modifiers.alt && !bytes.is_empty() {
        bytes.insert(0, 0x1b);
    }
    bytes
}

fn control_byte(key: &str) -> Option<u8> {
    match key {
        "space" | "@" | "2" => Some(0),
        "[" | "3" => Some(0x1b),
        "\\" | "4" => Some(0x1c),
        "]" | "5" => Some(0x1d),
        "^" | "6" => Some(0x1e),
        "_" | "-" | "7" => Some(0x1f),
        "?" | "8" => Some(0x7f),
        _ if key.len() == 1 && key.as_bytes()[0].is_ascii_lowercase() => {
            Some(key.as_bytes()[0] - b'a' + 1)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keys::Modifiers;

    #[test]
    fn cursor_function_and_navigation_modes_and_every_modifier() {
        for app_cursor in [false, true] {
            let mode = InputModes {
                app_cursor,
                ..InputModes::default()
            };
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
                    let stroke = Keystroke {
                        key: key.into(),
                        modifiers: Modifiers {
                            shift: bits & 1 != 0,
                            alt: bits & 2 != 0,
                            control: bits & 4 != 0,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    let expected = if bits != 0 {
                        format!("\x1b[1;{}{suffix}", bits + 1)
                    } else if app_cursor || key.starts_with('f') {
                        format!("\x1bO{suffix}")
                    } else {
                        format!("\x1b[{suffix}")
                    };
                    assert_eq!(
                        terminal_key_bytes(&stroke, mode),
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
        ] {
            for bits in 0..8 {
                let stroke = Keystroke {
                    key: key.into(),
                    modifiers: Modifiers {
                        shift: bits & 1 != 0,
                        alt: bits & 2 != 0,
                        control: bits & 4 != 0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let expected = if bits == 0 {
                    format!("\x1b[{number}~")
                } else {
                    format!("\x1b[{number};{}~", bits + 1)
                };
                assert_eq!(
                    terminal_key_bytes(
                        &stroke,
                        InputModes {
                            app_cursor: true,
                            ..InputModes::default()
                        }
                    ),
                    expected.as_bytes()
                );
            }
        }
    }

    #[test]
    fn text_controls_and_unrecognized_keys() {
        for (key, byte) in [
            ("space", 0),
            ("2", 0),
            ("a", 1),
            ("z", 26),
            ("[", 27),
            ("\\", 28),
            ("]", 29),
            ("^", 30),
            ("_", 31),
            ("?", 127),
            ("backspace", 8),
        ] {
            let stroke = Keystroke::parse(&format!("ctrl-{key}")).unwrap();
            assert_eq!(terminal_key_bytes(&stroke, InputModes::default()), [byte]);
        }
        for (key, expected) in [
            ("shift-tab", "\x1b[Z"),
            ("ctrl-alt-up", "\x1b[1;7A"),
            ("alt-enter", "\x1b\r"),
            ("alt-a", "\x1ba"),
            ("cmd-a", ""),
            ("ctrl-f13", ""),
        ] {
            assert_eq!(
                terminal_key_bytes(&Keystroke::parse(key).unwrap(), InputModes::default()),
                expected.as_bytes()
            );
        }
        let stroke = Keystroke {
            key: "é".into(),
            key_char: Some("é界".into()),
            ..Default::default()
        };
        assert_eq!(
            terminal_key_bytes(&stroke, InputModes::default()),
            "é界".as_bytes()
        );
        let stroke = Keystroke {
            key: "b".into(),
            key_char: Some("∫".into()),
            modifiers: Modifiers {
                alt: true,
                ..Default::default()
            },
        };
        assert_eq!(terminal_key_bytes(&stroke, InputModes::default()), b"\x1bb");
        let stroke = Keystroke {
            key: "2".into(),
            key_char: Some("2".into()),
            ..Default::default()
        };
        assert_eq!(terminal_key_bytes(&stroke, InputModes::default()), b"2");
    }
}
