//! The theme's terminal colors as the terminal's defaults.
//!
//! The terminal answers color queries and resolves palette indices itself
//! from these defaults and the program's OSC overrides.

use crate::app::convert::ThemePaint as _;
use gpui::{Hsla, Rgba};
use knot_terminal::{Rgb, Terminal, default_palette};

use crate::app::theme::{Theme, color};

/// The default foreground, background, and 256-color palette: the theme's
/// 16 ANSI colors followed by the standard color cube and grayscale ramp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerminalPalette {
    pub(crate) foreground: Rgb,
    pub(crate) background: Rgb,
    pub(crate) palette: [Rgb; 256],
}

impl TerminalPalette {
    pub(crate) fn new(theme: &Theme) -> Self {
        let paint = |key| rgb(theme.paint_color(key));
        let ansi = [
            color::TERMINAL_ANSI_BLACK,
            color::TERMINAL_ANSI_RED,
            color::TERMINAL_ANSI_GREEN,
            color::TERMINAL_ANSI_YELLOW,
            color::TERMINAL_ANSI_BLUE,
            color::TERMINAL_ANSI_MAGENTA,
            color::TERMINAL_ANSI_CYAN,
            color::TERMINAL_ANSI_WHITE,
            color::TERMINAL_ANSI_BRIGHT_BLACK,
            color::TERMINAL_ANSI_BRIGHT_RED,
            color::TERMINAL_ANSI_BRIGHT_GREEN,
            color::TERMINAL_ANSI_BRIGHT_YELLOW,
            color::TERMINAL_ANSI_BRIGHT_BLUE,
            color::TERMINAL_ANSI_BRIGHT_MAGENTA,
            color::TERMINAL_ANSI_BRIGHT_CYAN,
            color::TERMINAL_ANSI_BRIGHT_WHITE,
        ];
        let mut palette = default_palette();
        for (entry, key) in palette.iter_mut().zip(ansi) {
            *entry = paint(key);
        }
        Self {
            foreground: paint(color::TERMINAL_FOREGROUND),
            background: paint(color::TERMINAL_BACKGROUND),
            palette,
        }
    }

    /// Makes these the terminal's defaults; colors the program set stay.
    pub(crate) fn apply(&self, terminal: &mut Terminal) {
        terminal.set_default_colors(self.foreground, self.background);
        terminal.set_palette(&self.palette);
    }
}

/// `color` without alpha.
fn rgb(color: Hsla) -> Rgb {
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    Rgb::new(channel(rgba.r), channel(rgba.g), channel(rgba.b))
}

/// The faint (SGR 2) version of `color`.
pub(crate) fn dim(color: Rgb) -> Rgb {
    let dim = |channel: u8| (u16::from(channel) * 2 / 3) as u8;
    Rgb::new(dim(color.r), dim(color.g), dim(color.b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use knot_terminal::TerminalSize;

    fn query(terminal: &mut Terminal, query: &[u8]) -> Vec<u8> {
        let replies = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = replies.clone();
        terminal.set_event_handler(move |event| {
            if let knot_terminal::TerminalEvent::Reply(bytes) = event {
                sink.lock().unwrap().extend(bytes);
            }
        });
        terminal.write(query);
        replies.lock().unwrap().clone()
    }

    #[test]
    fn queries_answer_with_the_theme_and_program_overrides() {
        let palette = TerminalPalette::new(&Theme::default());
        let mut terminal = Terminal::new(
            TerminalSize {
                columns: 80,
                rows: 24,
                cell_width: 1,
                cell_height: 1,
            },
            0,
        );
        palette.apply(&mut terminal);
        let hex = |color: Rgb| {
            format!(
                "rgb:{0:02x}{0:02x}/{1:02x}{1:02x}/{2:02x}{2:02x}",
                color.r, color.g, color.b
            )
        };
        assert_eq!(
            String::from_utf8(query(&mut terminal, b"\x1b]10;?\x1b\\\x1b]4;1;?\x1b\\")).unwrap(),
            format!(
                "\x1b]10;{}\x1b\\\x1b]4;1;{}\x1b\\",
                hex(palette.foreground),
                hex(palette.palette[1])
            )
        );
        // Reapplying the theme keeps the program's own colors.
        query(&mut terminal, b"\x1b]4;1;#112233\x1b\\");
        palette.apply(&mut terminal);
        assert_eq!(
            query(&mut terminal, b"\x1b]4;1;?\x1b\\"),
            b"\x1b]4;1;rgb:1111/2222/3333\x1b\\"
        );
        assert_eq!(palette.palette[16..], default_palette()[16..]);
    }
}
