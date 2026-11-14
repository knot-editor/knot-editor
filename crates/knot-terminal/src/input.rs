//! Keyboard and mouse input encoded for the program, in the protocols and
//! formats the terminal's modes select.

use std::ptr;

use crate::{
    ffi,
    terminal::{Terminal, check},
};

/// Modifier keys held during an input event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    /// Command on macOS, the Windows or Super key elsewhere.
    pub super_key: bool,
}

impl Modifiers {
    fn raw(self) -> ffi::GhosttyMods {
        let mut mods = 0;
        for (held, bit) in [
            (self.shift, ffi::GHOSTTY_MODS_SHIFT),
            (self.control, ffi::GHOSTTY_MODS_CTRL),
            (self.alt, ffi::GHOSTTY_MODS_ALT),
            (self.super_key, ffi::GHOSTTY_MODS_SUPER),
        ] {
            if held {
                mods |= bit as ffi::GhosttyMods;
            }
        }
        mods
    }
}

/// Whether a key went down, repeats while held, or came up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyAction {
    #[default]
    Press,
    Repeat,
    Release,
}

/// A physical key, named by what it types on a US layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// The writing-system key of an ASCII letter, digit, or punctuation
    /// character, or no identified key for any other character.
    Character(char),
    Space,
    Enter,
    Tab,
    Backspace,
    Escape,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    /// A function key, F1 through F25.
    Function(u8),
}

impl Key {
    fn raw(self) -> ffi::GhosttyKey {
        match self {
            Self::Character(character) => character_key(character),
            Self::Space => ffi::GHOSTTY_KEY_SPACE,
            Self::Enter => ffi::GHOSTTY_KEY_ENTER,
            Self::Tab => ffi::GHOSTTY_KEY_TAB,
            Self::Backspace => ffi::GHOSTTY_KEY_BACKSPACE,
            Self::Escape => ffi::GHOSTTY_KEY_ESCAPE,
            Self::Insert => ffi::GHOSTTY_KEY_INSERT,
            Self::Delete => ffi::GHOSTTY_KEY_DELETE,
            Self::Home => ffi::GHOSTTY_KEY_HOME,
            Self::End => ffi::GHOSTTY_KEY_END,
            Self::PageUp => ffi::GHOSTTY_KEY_PAGE_UP,
            Self::PageDown => ffi::GHOSTTY_KEY_PAGE_DOWN,
            Self::Up => ffi::GHOSTTY_KEY_ARROW_UP,
            Self::Down => ffi::GHOSTTY_KEY_ARROW_DOWN,
            Self::Left => ffi::GHOSTTY_KEY_ARROW_LEFT,
            Self::Right => ffi::GHOSTTY_KEY_ARROW_RIGHT,
            Self::Function(number @ 1..=25) => {
                ffi::GHOSTTY_KEY_F1 + ffi::GhosttyKey::from(number - 1)
            }
            Self::Function(_) => ffi::GHOSTTY_KEY_UNIDENTIFIED,
        }
    }
}

fn character_key(character: char) -> ffi::GhosttyKey {
    match character.to_ascii_lowercase() {
        letter @ 'a'..='z' => ffi::GHOSTTY_KEY_A + (letter as u8 - b'a') as ffi::GhosttyKey,
        digit @ '0'..='9' => ffi::GHOSTTY_KEY_DIGIT_0 + (digit as u8 - b'0') as ffi::GhosttyKey,
        '`' => ffi::GHOSTTY_KEY_BACKQUOTE,
        '\\' => ffi::GHOSTTY_KEY_BACKSLASH,
        '[' => ffi::GHOSTTY_KEY_BRACKET_LEFT,
        ']' => ffi::GHOSTTY_KEY_BRACKET_RIGHT,
        ',' => ffi::GHOSTTY_KEY_COMMA,
        '=' => ffi::GHOSTTY_KEY_EQUAL,
        '-' => ffi::GHOSTTY_KEY_MINUS,
        '.' => ffi::GHOSTTY_KEY_PERIOD,
        '\'' => ffi::GHOSTTY_KEY_QUOTE,
        ';' => ffi::GHOSTTY_KEY_SEMICOLON,
        '/' => ffi::GHOSTTY_KEY_SLASH,
        _ => ffi::GHOSTTY_KEY_UNIDENTIFIED,
    }
}

/// One keyboard event, as the platform reported it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyEvent<'a> {
    pub action: KeyAction,
    /// The physical key; `None` for one the encoder does not know.
    pub key: Option<Key>,
    pub modifiers: Modifiers,
    /// The modifiers the layout used to produce `text`, such as Shift for
    /// an uppercase letter.
    pub consumed: Modifiers,
    /// What the key types with `consumed` applied and no other modifier;
    /// empty for keys that type nothing.
    pub text: &'a str,
    /// What the key types with no modifier at all.
    pub unshifted: Option<char>,
    /// Whether the key is part of an input-method composition.
    pub composing: bool,
}

/// What the mouse did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

/// A mouse button, including the wheel directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    WheelLeft,
    WheelRight,
    Back,
    Forward,
}

impl MouseButton {
    fn raw(self) -> ffi::GhosttyMouseButton {
        match self {
            Self::Left => ffi::GHOSTTY_MOUSE_BUTTON_LEFT,
            Self::Middle => ffi::GHOSTTY_MOUSE_BUTTON_MIDDLE,
            Self::Right => ffi::GHOSTTY_MOUSE_BUTTON_RIGHT,
            Self::WheelUp => ffi::GHOSTTY_MOUSE_BUTTON_FOUR,
            Self::WheelDown => ffi::GHOSTTY_MOUSE_BUTTON_FIVE,
            Self::WheelLeft => ffi::GHOSTTY_MOUSE_BUTTON_SIX,
            Self::WheelRight => ffi::GHOSTTY_MOUSE_BUTTON_SEVEN,
            Self::Back => ffi::GHOSTTY_MOUSE_BUTTON_EIGHT,
            Self::Forward => ffi::GHOSTTY_MOUSE_BUTTON_NINE,
        }
    }
}

/// One mouse event over the grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseEvent {
    pub action: MouseAction,
    /// The button pressed or released, or held during motion.
    pub button: Option<MouseButton>,
    pub modifiers: Modifiers,
    /// The position in the cell pixels of the terminal's size, from the
    /// grid's top-left corner.
    pub x: f32,
    pub y: f32,
}

impl Terminal {
    /// The bytes that tell the program about `event`, in the keyboard
    /// protocol its modes select; empty when the key sends nothing.
    ///
    /// On macOS, Option acts as Alt.
    pub fn encode_key(&self, event: &KeyEvent) -> Vec<u8> {
        let mut encoder = ptr::null_mut();
        let mut raw = ptr::null_mut();
        // SAFETY: the out pointers are valid; the default allocator is used.
        unsafe {
            check(
                ffi::ghostty_key_encoder_new(ptr::null(), &mut encoder),
                "ghostty_key_encoder_new",
            );
            check(
                ffi::ghostty_key_event_new(ptr::null(), &mut raw),
                "ghostty_key_event_new",
            );
        }
        let action = match event.action {
            KeyAction::Press => ffi::GHOSTTY_KEY_ACTION_PRESS,
            KeyAction::Repeat => ffi::GHOSTTY_KEY_ACTION_REPEAT,
            KeyAction::Release => ffi::GHOSTTY_KEY_ACTION_RELEASE,
        };
        let option_as_alt = ffi::GHOSTTY_OPTION_AS_ALT_TRUE;
        let text = ffi::GhosttyString {
            ptr: event.text.as_ptr(),
            len: event.text.len(),
        };
        // SAFETY: both handles are live; the text outlives the event, which
        // is freed below; the option value is a `GhosttyOptionAsAlt`.
        unsafe {
            check(
                ffi::ghostty_key_encoder_set_from_terminal(encoder, self.raw),
                "configuring the key encoder",
            );
            check(
                ffi::ghostty_key_encoder_set(
                    encoder,
                    ffi::GHOSTTY_KEY_ENCODER_OPT_MACOS_OPTION_AS_ALT,
                    (&option_as_alt as *const ffi::GhosttyOptionAsAlt).cast(),
                ),
                "setting option as alt",
            );
            check(
                ffi::ghostty_key_event_set_action(raw, action),
                "setting the key action",
            );
            check(
                ffi::ghostty_key_event_set_key(
                    raw,
                    event.key.map_or(ffi::GHOSTTY_KEY_UNIDENTIFIED, Key::raw),
                ),
                "setting the key",
            );
            check(
                ffi::ghostty_key_event_set_mods(raw, event.modifiers.raw()),
                "setting the key modifiers",
            );
            check(
                ffi::ghostty_key_event_set_consumed_mods(raw, event.consumed.raw()),
                "setting the consumed modifiers",
            );
            check(
                ffi::ghostty_key_event_set_utf8(raw, text),
                "setting the key text",
            );
            check(
                ffi::ghostty_key_event_set_unshifted_codepoint(
                    raw,
                    event.unshifted.map_or(0, u32::from),
                ),
                "setting the unshifted codepoint",
            );
            check(
                ffi::ghostty_key_event_set_composing(raw, event.composing),
                "setting composition",
            );
            let bytes = encode(|buffer, length, written| {
                ffi::ghostty_key_encoder_encode(encoder, raw, buffer, length, written)
            });
            ffi::ghostty_key_event_free(raw);
            ffi::ghostty_key_encoder_free(encoder);
            bytes
        }
    }

    /// The bytes that report `event` to the program in its mouse tracking
    /// mode and format; empty when the mode does not report it or the
    /// format cannot represent its position.
    ///
    /// `any_button_pressed` says whether a reported button is held, which
    /// decides whether motion is a drag.
    pub fn encode_mouse(&self, event: &MouseEvent, any_button_pressed: bool) -> Vec<u8> {
        let terminal_size = self.size();
        let width = u32::from(terminal_size.columns) * terminal_size.cell_width.max(1);
        let height = u32::from(terminal_size.rows) * terminal_size.cell_height.max(1);
        let size = ffi::GhosttyMouseEncoderSize {
            size: std::mem::size_of::<ffi::GhosttyMouseEncoderSize>(),
            screen_width: width,
            screen_height: height,
            cell_width: terminal_size.cell_width.max(1),
            cell_height: terminal_size.cell_height.max(1),
            padding_top: 0,
            padding_bottom: 0,
            padding_right: 0,
            padding_left: 0,
        };
        let action = match event.action {
            MouseAction::Press => ffi::GHOSTTY_MOUSE_ACTION_PRESS,
            MouseAction::Release => ffi::GHOSTTY_MOUSE_ACTION_RELEASE,
            MouseAction::Motion => ffi::GHOSTTY_MOUSE_ACTION_MOTION,
        };
        let mut encoder = ptr::null_mut();
        let mut raw = ptr::null_mut();
        // SAFETY: the out pointers are valid; both handles are live until
        // freed below; each option value has its documented type.
        unsafe {
            check(
                ffi::ghostty_mouse_encoder_new(ptr::null(), &mut encoder),
                "ghostty_mouse_encoder_new",
            );
            check(
                ffi::ghostty_mouse_event_new(ptr::null(), &mut raw),
                "ghostty_mouse_event_new",
            );
            check(
                ffi::ghostty_mouse_encoder_set_from_terminal(encoder, self.raw),
                "configuring the mouse encoder",
            );
            check(
                ffi::ghostty_mouse_encoder_set(
                    encoder,
                    ffi::GHOSTTY_MOUSE_ENCODER_OPT_SIZE,
                    (&size as *const ffi::GhosttyMouseEncoderSize).cast(),
                ),
                "setting the mouse geometry",
            );
            check(
                ffi::ghostty_mouse_encoder_set(
                    encoder,
                    ffi::GHOSTTY_MOUSE_ENCODER_OPT_ANY_BUTTON_PRESSED,
                    (&any_button_pressed as *const bool).cast(),
                ),
                "setting the pressed state",
            );
            check(
                ffi::ghostty_mouse_event_set_action(raw, action),
                "setting the mouse action",
            );
            match event.button {
                Some(button) => check(
                    ffi::ghostty_mouse_event_set_button(raw, button.raw()),
                    "setting the mouse button",
                ),
                None => check(
                    ffi::ghostty_mouse_event_clear_button(raw),
                    "clearing the mouse button",
                ),
            }
            check(
                ffi::ghostty_mouse_event_set_mods(raw, event.modifiers.raw()),
                "setting the mouse modifiers",
            );
            check(
                ffi::ghostty_mouse_event_set_position(
                    raw,
                    ffi::GhosttyMousePosition {
                        x: event.x,
                        y: event.y,
                    },
                ),
                "setting the mouse position",
            );
            let bytes = encode(|buffer, length, written| {
                ffi::ghostty_mouse_encoder_encode(encoder, raw, buffer, length, written)
            });
            ffi::ghostty_mouse_event_free(raw);
            ffi::ghostty_mouse_encoder_free(encoder);
            bytes
        }
    }
}

/// Runs an encoder call into a stack buffer, retrying with the size it
/// asks for when the sequence does not fit.
fn encode(mut call: impl FnMut(*mut u8, usize, &mut usize) -> ffi::GhosttyResult) -> Vec<u8> {
    let mut buffer = [0u8; 128];
    let mut written = 0;
    match call(buffer.as_mut_ptr(), buffer.len(), &mut written) {
        ffi::GHOSTTY_OUT_OF_SPACE => {
            let mut bytes = vec![0; written];
            check(
                call(bytes.as_mut_ptr(), bytes.len(), &mut written),
                "encoding input",
            );
            bytes.truncate(written);
            bytes
        }
        result => {
            check(result, "encoding input");
            buffer[..written].to_vec()
        }
    }
}
