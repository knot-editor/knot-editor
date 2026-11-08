//! Color and text-style values the theme resolves. The application shell
//! converts them to the toolkit's types at the view edge.

use serde::{Deserialize, Serialize};

/// An sRGB color with alpha. The default is transparent black.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    /// An opaque color from `0xrrggbb`.
    pub const fn rgb(value: u32) -> Self {
        Self::rgba((value << 8) | 0xff)
    }

    /// A color from `0xrrggbbaa`.
    pub const fn rgba(value: u32) -> Self {
        Self {
            red: (value >> 24) as u8,
            green: (value >> 16) as u8,
            blue: (value >> 8) as u8,
            alpha: value as u8,
        }
    }

    /// Parses `#rrggbb` or `#rrggbbaa`.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let value = u32::from_str_radix(digits, 16).ok()?;
        match digits.len() {
            6 => Some(Self::rgb(value)),
            8 => Some(Self::rgba(value)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}

/// A line under text, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Underline {
    pub thickness: f32,
    /// The line's color; the text color when unset.
    pub color: Option<Color>,
    pub wavy: bool,
}

/// How a span of text is styled; an unset field leaves the text as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HighlightStyle {
    pub color: Option<Color>,
    /// A weight from 100 to 900.
    pub font_weight: Option<u16>,
    pub font_style: Option<FontStyle>,
    pub underline: Option<Underline>,
    /// A one-pixel line through the text.
    pub strikethrough: bool,
}

impl HighlightStyle {
    pub fn color(color: Color) -> Self {
        Self {
            color: Some(color),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse_six_or_eight_hex_digits() {
        assert_eq!(Color::parse("#1e2f3a"), Some(Color::rgb(0x1e2f3a)));
        assert_eq!(Color::parse("#1E2F3A"), Some(Color::rgb(0x1e2f3a)));
        assert_eq!(
            Color::parse("#1e2f3a80"),
            Some(Color {
                red: 0x1e,
                green: 0x2f,
                blue: 0x3a,
                alpha: 0x80
            })
        );
        for text in [
            "1e2f3a", "#fff", "#1e2f3", "#1e2f3a8", "#gggggg", "#+12345", "",
        ] {
            assert_eq!(Color::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn rgb_is_opaque() {
        assert_eq!(Color::rgb(0x112233), Color::rgba(0x112233ff));
        assert_eq!(Color::default().alpha, 0);
    }
}
