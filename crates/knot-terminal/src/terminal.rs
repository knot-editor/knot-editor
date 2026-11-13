//! The terminal screen: parsing, geometry, colors, and the viewport.

use std::ptr;

use crate::ffi;

/// An sRGB color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// The color packed as `0xrrggbb`.
    pub const fn packed(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    /// The color of a packed `0xrrggbb` value.
    pub const fn from_packed(value: u32) -> Self {
        Self::new((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }
}

impl From<ffi::GhosttyColorRgb> for Rgb {
    fn from(color: ffi::GhosttyColorRgb) -> Self {
        Self::new(color.r, color.g, color.b)
    }
}

impl From<Rgb> for ffi::GhosttyColorRgb {
    fn from(color: Rgb) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

/// libghostty's built-in 256-color palette: its own 16 named colors, the
/// 6×6×6 color cube, and the grayscale ramp.
pub fn default_palette() -> [Rgb; 256] {
    let mut palette = [ffi::GhosttyColorRgb { r: 0, g: 0, b: 0 }; 256];
    // SAFETY: the library writes exactly 256 colors.
    unsafe { ffi::ghostty_color_palette_default(palette.as_mut_ptr()) };
    palette.map(Rgb::from)
}

/// The grid and cell geometry of a terminal.
///
/// Zero columns or rows are raised to one. The cell size in pixels feeds
/// size reports and pixel mouse reporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
    pub cell_width: u32,
    pub cell_height: u32,
}

/// How [`Terminal::scroll_viewport`] moves the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scroll {
    /// To the oldest scrollback row.
    Top,
    /// To the active screen, where new output appears.
    Bottom,
    /// By a number of rows; negative moves into the scrollback.
    Delta(isize),
}

/// Where the viewport sits in the scrollable rows of the active screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scrollbar {
    /// Rows in the scrollback and the active screen together.
    pub total: u64,
    /// The first visible row, counted from the oldest scrollback row.
    pub offset: u64,
    /// Visible rows.
    pub len: u64,
}

impl Scrollbar {
    /// Rows between the bottom of the viewport and the bottom of the active
    /// screen; zero while following new output.
    pub fn rows_below(&self) -> u64 {
        self.total.saturating_sub(self.offset + self.len)
    }
}

/// A libghostty terminal: the VT parser, screens, scrollback, and modes.
///
/// The terminal is not thread-safe. It is [`Send`], so one owner can keep
/// it behind a mutex shared by the PTY reader and the foreground.
pub struct Terminal {
    pub(crate) raw: ffi::GhosttyTerminal,
}

// SAFETY: the library creates no threads and keeps no thread-local state for
// a terminal, so moving the handle between threads is sound; `&mut self` on
// every mutating call, or the caller's mutex, serializes access.
unsafe impl Send for Terminal {}

impl Terminal {
    /// A terminal of `size` that keeps about `scrollback_lines` rows of
    /// history on the primary screen.
    pub fn new(size: TerminalSize, scrollback_lines: usize) -> Self {
        let (columns, rows) = grid(size);
        let mut raw = ptr::null_mut();
        // SAFETY: `raw` is a valid out pointer; the default allocator is used.
        check(
            unsafe { ffi::ghostty_terminal_new(ptr::null(), &mut raw, columns, rows) },
            "ghostty_terminal_new",
        );
        let mut terminal = Self { raw };
        // SAFETY: the option takes a `size_t*` that only needs to live for the call.
        check(
            unsafe {
                ffi::ghostty_terminal_set(
                    terminal.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES,
                    (&scrollback_lines as *const usize).cast(),
                )
            },
            "setting the scrollback limit",
        );
        terminal.resize(size);
        terminal
    }

    /// Parses `bytes` of program output.
    pub fn write(&mut self, bytes: &[u8]) {
        // SAFETY: the pointer and length describe `bytes`.
        unsafe { ffi::ghostty_terminal_vt_write(self.raw, bytes.as_ptr(), bytes.len()) };
    }

    /// Resizes the grid, reflowing the primary screen.
    pub fn resize(&mut self, size: TerminalSize) {
        let (columns, rows) = grid(size);
        // SAFETY: the handle is live and the grid is at least one cell.
        check(
            unsafe {
                ffi::ghostty_terminal_resize(
                    self.raw,
                    columns,
                    rows,
                    size.cell_width,
                    size.cell_height,
                )
            },
            "ghostty_terminal_resize",
        );
    }

    /// Sets the default foreground and background, which programs may
    /// override with OSC 10 and 11.
    pub fn set_default_colors(&mut self, foreground: Rgb, background: Rgb) {
        self.set_color(ffi::GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND, foreground);
        self.set_color(ffi::GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND, background);
    }

    /// Sets the default 256-color palette, which programs may override
    /// entry by entry with OSC 4.
    pub fn set_palette(&mut self, palette: &[Rgb; 256]) {
        let palette = palette.map(ffi::GhosttyColorRgb::from);
        // SAFETY: the option copies exactly 256 colors during the call.
        check(
            unsafe {
                ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_COLOR_PALETTE,
                    palette.as_ptr().cast(),
                )
            },
            "setting the palette",
        );
    }

    /// Moves the viewport through the scrollback.
    pub fn scroll_viewport(&mut self, scroll: Scroll) {
        let mut behavior = ffi::GhosttyTerminalScrollViewport {
            tag: ffi::GHOSTTY_TERMINAL_SCROLL_VIEWPORT_DELTA,
            value: ffi::GhosttyTerminalScrollViewportValue { _padding: [0; 2] },
        };
        match scroll {
            Scroll::Top => behavior.tag = ffi::GHOSTTY_TERMINAL_SCROLL_VIEWPORT_TOP,
            Scroll::Bottom => behavior.tag = ffi::GHOSTTY_TERMINAL_SCROLL_VIEWPORT_BOTTOM,
            Scroll::Delta(delta) => behavior.value.delta = delta,
        }
        // SAFETY: the tag is a known behavior and the value matches it.
        check(
            unsafe { ffi::ghostty_terminal_scroll_viewport(self.raw, behavior) },
            "ghostty_terminal_scroll_viewport",
        );
    }

    /// Where the viewport sits in the scrollback.
    pub fn scrollbar(&self) -> Scrollbar {
        let mut scrollbar = ffi::GhosttyTerminalScrollbar {
            total: 0,
            offset: 0,
            len: 0,
        };
        // SAFETY: the data kind writes one `GhosttyTerminalScrollbar`.
        check(
            unsafe {
                ffi::ghostty_terminal_get(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_DATA_SCROLLBAR,
                    (&mut scrollbar as *mut ffi::GhosttyTerminalScrollbar).cast(),
                )
            },
            "reading the scrollbar",
        );
        Scrollbar {
            total: scrollbar.total,
            offset: scrollbar.offset,
            len: scrollbar.len,
        }
    }

    fn set_color(&mut self, option: ffi::GhosttyTerminalOption, color: Rgb) {
        let color = ffi::GhosttyColorRgb::from(color);
        // SAFETY: the option copies one color during the call.
        check(
            unsafe {
                ffi::ghostty_terminal_set(
                    self.raw,
                    option,
                    (&color as *const ffi::GhosttyColorRgb).cast(),
                )
            },
            "setting a default color",
        );
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // SAFETY: the handle is owned and never used again.
        unsafe { ffi::ghostty_terminal_free(self.raw) };
    }
}

fn grid(size: TerminalSize) -> (u16, u16) {
    (size.columns.max(1), size.rows.max(1))
}

/// Panics unless `result` is success. With valid arguments, libghostty
/// fails only when an allocation fails.
pub(crate) fn check(result: ffi::GhosttyResult, operation: &str) {
    assert_eq!(
        result,
        ffi::GHOSTTY_SUCCESS,
        "{operation} failed with libghostty result {result}"
    );
}
