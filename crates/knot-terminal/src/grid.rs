//! Cell queries outside painting: the selection, text, and hyperlinks.
//!
//! These look cells up one at a time, which is too slow for drawing a frame;
//! [`RenderState`](crate::RenderState) is for that.

use std::{ptr, slice};

use crate::{
    ffi,
    terminal::{Terminal, check},
};

/// A cell in the viewport: its column and its row from the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Point {
    pub column: u16,
    pub row: u16,
}

impl Point {
    pub const fn new(column: u16, row: u16) -> Self {
        Self { column, row }
    }
}

impl Terminal {
    /// Selects the cells from `anchor` to `to`, both included, in reading
    /// order. The terminal tracks the selected cells as output scrolls them,
    /// and drops the selection when they leave the scrollback.
    pub fn select(&mut self, anchor: Point, to: Point) {
        let (Some(start), Some(end)) = (self.grid_ref(anchor), self.grid_ref(to)) else {
            return;
        };
        self.set_selection(&ffi::GhosttySelection {
            start,
            end,
            rectangle: false,
        });
    }

    /// Moves the end of the selection to `to`, keeping its anchor even if
    /// the anchor has scrolled out of the viewport. Does nothing without a
    /// selection.
    pub fn extend_selection(&mut self, to: Point) {
        let (Some(mut selection), Some(end)) = (self.selection(), self.grid_ref(to)) else {
            return;
        };
        selection.end = end;
        self.set_selection(&selection);
    }

    pub fn clear_selection(&mut self) {
        // SAFETY: a null value clears the selection.
        check(
            unsafe {
                ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_SELECTION,
                    ptr::null(),
                )
            },
            "clearing the selection",
        );
    }

    pub fn has_selection(&self) -> bool {
        self.selection().is_some()
    }

    /// The selected text, with soft-wrapped rows joined and trailing spaces
    /// trimmed, or `None` without a selection.
    pub fn selection_text(&self) -> Option<String> {
        let selection = self.selection()?;
        Some(self.format(Some(&selection)))
    }

    /// The text of the active screen and its scrollback, oldest first: one
    /// line per logical line, with soft-wrapped rows joined, trailing
    /// spaces trimmed, and trailing blank lines dropped.
    pub fn text(&self) -> String {
        self.format(None)
    }

    /// The OSC 8 hyperlink URI of the cell at `point`, if it has one.
    pub fn hyperlink(&self, point: Point) -> Option<String> {
        let reference = self.grid_ref(point)?;
        let mut uri = Vec::new();
        loop {
            let mut len = 0;
            // SAFETY: the reference was resolved since the last terminal
            // change; the buffer describes `uri`'s allocation.
            let result = unsafe {
                ffi::ghostty_grid_ref_hyperlink_uri(
                    &reference,
                    uri.as_mut_ptr(),
                    uri.capacity(),
                    &mut len,
                )
            };
            if result == ffi::GHOSTTY_OUT_OF_SPACE {
                uri.reserve(len);
                continue;
            }
            check(result, "reading a hyperlink");
            if len == 0 {
                return None;
            }
            // SAFETY: the library initialized the first `len` bytes.
            unsafe { uri.set_len(len) };
            return Some(String::from_utf8_lossy(&uri).into_owned());
        }
    }

    /// The cell at `point`, valid until the terminal next changes.
    fn grid_ref(&self, point: Point) -> Option<ffi::GhosttyGridRef> {
        let point = ffi::GhosttyPoint {
            tag: ffi::GHOSTTY_POINT_TAG_VIEWPORT,
            value: ffi::GhosttyPointValue {
                coordinate: ffi::GhosttyPointCoordinate {
                    x: point.column,
                    y: point.row.into(),
                },
            },
        };
        let mut reference = ffi::GhosttyGridRef {
            node: ptr::null_mut(),
            x: 0,
            y: 0,
        };
        // SAFETY: the handle is live and the out pointer valid; a point
        // outside the viewport fails without writing.
        let result = unsafe { ffi::ghostty_terminal_grid_ref(self.raw, point, &mut reference) };
        (result == ffi::GHOSTTY_SUCCESS).then_some(reference)
    }

    /// The selection, with ends valid until the terminal next changes.
    fn selection(&self) -> Option<ffi::GhosttySelection> {
        let mut selection = ffi::GhosttySelection {
            start: ffi::GhosttyGridRef {
                node: ptr::null_mut(),
                x: 0,
                y: 0,
            },
            end: ffi::GhosttyGridRef {
                node: ptr::null_mut(),
                x: 0,
                y: 0,
            },
            rectangle: false,
        };
        // SAFETY: the kind writes one `GhosttySelection`.
        let result = unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::GHOSTTY_TERMINAL_DATA_SELECTION,
                (&mut selection as *mut ffi::GhosttySelection).cast(),
            )
        };
        if result == ffi::GHOSTTY_NO_VALUE {
            return None;
        }
        check(result, "reading the selection");
        Some(selection)
    }

    fn set_selection(&mut self, selection: &ffi::GhosttySelection) {
        // SAFETY: both ends were resolved since the last terminal change; the
        // terminal copies them into tracked state.
        check(
            unsafe {
                ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_SELECTION,
                    (selection as *const ffi::GhosttySelection).cast(),
                )
            },
            "setting the selection",
        );
    }

    /// Plain text of `selection`, or of the whole screen, with soft wraps
    /// joined and trailing spaces trimmed.
    fn format(&self, selection: Option<&ffi::GhosttySelection>) -> String {
        let mut formatter = ptr::null_mut();
        // SAFETY: the formatter borrows the live terminal only within this
        // function; the options copy their values; the output is freed with
        // the allocator that made it.
        unsafe {
            check(
                ffi::ghostty_formatter_terminal_new(ptr::null(), &mut formatter, self.raw),
                "ghostty_formatter_terminal_new",
            );
            let unwrap = true;
            check(
                ffi::ghostty_formatter_set(
                    formatter,
                    ffi::GHOSTTY_FORMATTER_OPT_UNWRAP,
                    (&unwrap as *const bool).cast(),
                ),
                "setting the formatter's unwrapping",
            );
            if let Some(selection) = selection {
                check(
                    ffi::ghostty_formatter_set(
                        formatter,
                        ffi::GHOSTTY_FORMATTER_OPT_SELECTION,
                        (selection as *const ffi::GhosttySelection).cast(),
                    ),
                    "setting the formatter's selection",
                );
            }
            let mut text = ptr::null_mut();
            let mut len = 0;
            let result =
                ffi::ghostty_formatter_format_alloc(formatter, ptr::null(), &mut text, &mut len);
            ffi::ghostty_formatter_free(formatter);
            check(result, "formatting text");
            if text.is_null() {
                return String::new();
            }
            let string = String::from_utf8_lossy(slice::from_raw_parts(text, len)).into_owned();
            ffi::ghostty_free(ptr::null(), text, len);
            string
        }
    }
}
