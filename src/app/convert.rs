//! The edge between Knot's own vocabulary types and the toolkit's. Views and
//! windows convert here; the rest of the application uses Knot's types.

use gpui::{FontStyle as GpuiFontStyle, FontWeight, Hsla, StrikethroughStyle, UnderlineStyle, px};
use knot_protocol::DecorationStyle;

use super::{
    keys,
    style::{self, Color, HighlightStyle},
    theme::{Highlight, Theme},
};

pub(crate) fn modifiers_from_gpui(modifiers: gpui::Modifiers) -> keys::Modifiers {
    keys::Modifiers {
        control: modifiers.control,
        alt: modifiers.alt,
        shift: modifiers.shift,
        platform: modifiers.platform,
        function: modifiers.function,
    }
}

pub(crate) fn modifiers_to_gpui(modifiers: keys::Modifiers) -> gpui::Modifiers {
    gpui::Modifiers {
        control: modifiers.control,
        alt: modifiers.alt,
        shift: modifiers.shift,
        platform: modifiers.platform,
        function: modifiers.function,
    }
}

pub(crate) fn keystroke_from_gpui(keystroke: &gpui::Keystroke) -> keys::Keystroke {
    keys::Keystroke {
        modifiers: modifiers_from_gpui(keystroke.modifiers),
        key: keystroke.key.clone(),
        key_char: keystroke.key_char.clone(),
    }
}

pub(crate) fn keystroke_to_gpui(keystroke: &keys::Keystroke) -> gpui::Keystroke {
    gpui::Keystroke {
        modifiers: modifiers_to_gpui(keystroke.modifiers),
        key: keystroke.key.clone(),
        key_char: keystroke.key_char.clone(),
    }
}

pub(crate) fn color_to_gpui(color: Color) -> Hsla {
    gpui::rgba(
        u32::from(color.red) << 24
            | u32::from(color.green) << 16
            | u32::from(color.blue) << 8
            | u32::from(color.alpha),
    )
    .into()
}

pub(crate) fn highlight_to_gpui(style: &HighlightStyle) -> gpui::HighlightStyle {
    gpui::HighlightStyle {
        color: style.color.map(color_to_gpui),
        font_weight: style
            .font_weight
            .map(|weight| FontWeight(f32::from(weight))),
        font_style: style.font_style.map(|font_style| match font_style {
            style::FontStyle::Normal => GpuiFontStyle::Normal,
            style::FontStyle::Italic => GpuiFontStyle::Italic,
        }),
        underline: style.underline.map(|underline| UnderlineStyle {
            thickness: px(underline.thickness),
            color: underline.color.map(color_to_gpui),
            wavy: underline.wavy,
        }),
        strikethrough: style.strikethrough.then_some(StrikethroughStyle {
            thickness: px(1.),
            color: None,
        }),
        ..Default::default()
    }
}

/// The theme's values as the toolkit paints them.
pub(crate) trait ThemePaint {
    /// The resolved color of `key`.
    fn paint_color(&self, key: &str) -> Hsla;
    /// The style of a syntax highlight.
    fn highlight_style(&self, highlight: Highlight) -> gpui::HighlightStyle;
    /// The text style of a decoration naming color `key`.
    fn decoration_style(&self, key: &str, style: DecorationStyle) -> gpui::HighlightStyle;
}

impl ThemePaint for Theme {
    fn paint_color(&self, key: &str) -> Hsla {
        color_to_gpui(self.color(key))
    }

    fn highlight_style(&self, highlight: Highlight) -> gpui::HighlightStyle {
        highlight_to_gpui(&self.style(highlight))
    }

    fn decoration_style(&self, key: &str, style: DecorationStyle) -> gpui::HighlightStyle {
        highlight_to_gpui(&self.decoration(key, style))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST_OS: gpui::OperatingSystem = if cfg!(target_os = "macos") {
        gpui::OperatingSystem::Mac
    } else if cfg!(windows) {
        gpui::OperatingSystem::Windows
    } else {
        gpui::OperatingSystem::Linux
    };

    #[test]
    fn keystrokes_round_trip_through_gpui() {
        for source in ["ctrl-shift-a", "alt-s->ß", "cmd-,", "f5", "ctrl--"] {
            let knot = keys::Keystroke::parse(source).unwrap();
            let gpui = keystroke_to_gpui(&knot);
            assert_eq!(keystroke_from_gpui(&gpui), knot, "{source}");
        }
    }

    #[test]
    fn colors_convert_to_the_toolkit_exactly() {
        assert_eq!(
            color_to_gpui(Color::rgb(0x1e2f3a)),
            gpui::rgb(0x1e2f3a).into()
        );
        assert_eq!(
            color_to_gpui(Color::rgba(0x1e2f3a80)),
            gpui::rgba(0x1e2f3a80).into()
        );
        assert_eq!(color_to_gpui(Color::default()), Hsla::default());
    }

    #[test]
    fn highlight_styles_convert_every_field() {
        let style = HighlightStyle {
            color: Some(Color::rgb(0x112233)),
            font_weight: Some(700),
            font_style: Some(style::FontStyle::Italic),
            underline: Some(style::Underline {
                thickness: 1.5,
                color: Some(Color::rgb(0x445566)),
                wavy: true,
            }),
            strikethrough: true,
        };
        let gpui = highlight_to_gpui(&style);
        assert_eq!(gpui.color, Some(gpui::rgb(0x112233).into()));
        assert_eq!(gpui.font_weight, Some(FontWeight::BOLD));
        assert_eq!(gpui.font_style, Some(GpuiFontStyle::Italic));
        assert_eq!(
            gpui.underline,
            Some(UnderlineStyle {
                thickness: px(1.5),
                color: Some(gpui::rgb(0x445566).into()),
                wavy: true,
            })
        );
        assert_eq!(
            gpui.strikethrough,
            Some(StrikethroughStyle {
                thickness: px(1.),
                color: None
            })
        );
        assert_eq!(
            highlight_to_gpui(&HighlightStyle::default()),
            gpui::HighlightStyle::default()
        );
    }

    #[test]
    fn parsing_agrees_with_gpui() {
        for source in [
            "ctrl-shift-a",
            "Shift-CTRL-a",
            "A",
            "shift",
            "ctrl",
            "secondary-q",
            "alt-s->ß",
            "cmd-,",
            "ctrl--",
            "ENTER",
            "fn-f1",
            "super-x",
        ] {
            let knot = keys::Keystroke::parse(source).unwrap();
            let gpui = gpui::Keystroke::parse_with_os(source, HOST_OS).unwrap();
            assert_eq!(keystroke_to_gpui(&knot), gpui, "{source}");
            assert_eq!(knot.unparse(), gpui.unparse(HOST_OS), "{source}");
        }
        for source in ["ctrl-ab-c", "a-b"] {
            assert!(gpui::Keystroke::parse_with_os(source, HOST_OS).is_err());
            assert!(keys::Keystroke::parse(source).is_err());
        }
    }

    #[test]
    fn simulated_text_and_matching_agree_with_gpui() {
        for source in [
            "a", "A", "space", "tab", "left", "f5", "ctrl-a", "alt-x", "shift-/",
        ] {
            let knot = keys::Keystroke::parse(source)
                .unwrap()
                .with_simulated_text();
            let gpui = gpui::Keystroke::parse(source).unwrap().with_simulated_ime();
            assert_eq!(keystroke_to_gpui(&knot), gpui, "{source}");
        }
    }
}
