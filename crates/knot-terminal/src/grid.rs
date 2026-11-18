//! Cell queries outside painting: the selection, text, and hyperlinks.
//!
//! These look cells up one at a time, which is too slow for drawing a frame;
//! [`RenderState`](crate::RenderState) is for that.

use std::{ptr, slice};

use crate::{
    ffi,
    terminal::{Terminal, check},
};

const EMPTY_REF: ffi::GhosttyGridRef = ffi::GhosttyGridRef {
    node: ptr::null_mut(),
    x: 0,
    y: 0,
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

/// The pointer over the viewport: the cell it is in and its position in
/// pixels from the grid's top left, using the terminal's cell size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointer {
    pub cell: Point,
    pub x: f64,
    pub y: f64,
}

/// The longest gap between presses that still repeat a click. Presses the
/// caller says repeat are spaced well inside it, the others well beyond.
const REPEAT_INTERVAL_NS: u64 = 1_000_000;

impl Terminal {
    /// Selects the cells from `anchor` to `to`, both included, in reading
    /// order, without a gesture.
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

    /// Starts a selection gesture at `at`, replacing the selection.
    ///
    /// `click_count` is the caller's count of presses in a row: one selects
    /// cell by cell, and larger counts select by larger units while the
    /// press is held. The emulator counts repeats from press times and
    /// distances, so the press carries a synthetic time that repeats exactly
    /// when `click_count` is above one. The terminal tracks the selected
    /// cells as output scrolls them, and drops the selection when they leave
    /// the scrollback.
    pub fn select_press(&mut self, at: Pointer, click_count: u8) {
        self.gesture_clock_ns += if click_count > 1 {
            1
        } else {
            2 * REPEAT_INTERVAL_NS
        };
        self.clear_selection();
        let Some(reference) = self.grid_ref(at.cell) else {
            return;
        };
        let event = GestureEvent::new(ffi::GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_PRESS);
        event.set(ffi::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REF, &reference);
        event.set(
            ffi::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_POSITION,
            &position(at),
        );
        event.set(
            ffi::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_TIME_NS,
            &self.gesture_clock_ns,
        );
        event.set(
            ffi::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REPEAT_INTERVAL_NS,
            &REPEAT_INTERVAL_NS,
        );
        event.set(
            ffi::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REPEAT_DISTANCE,
            &f64::MAX,
        );
        let cells = ffi::GhosttySelectionGestureBehaviors {
            single_click: ffi::GHOSTTY_SELECTION_GESTURE_BEHAVIOR_CELL,
            double_click: ffi::GHOSTTY_SELECTION_GESTURE_BEHAVIOR_CELL,
            triple_click: ffi::GHOSTTY_SELECTION_GESTURE_BEHAVIOR_CELL,
        };
        event.set(ffi::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_BEHAVIORS, &cells);
        self.apply_gesture(&event);
    }

    /// Moves the gesture's pointer to `at`, extending the selection from
    /// the press by the unit the press chose. Does nothing without a press.
    pub fn select_drag(&mut self, at: Pointer) {
        let Some(reference) = self.grid_ref(at.cell) else {
            return;
        };
        let size = self.size();
        let geometry = ffi::GhosttySelectionGestureGeometry {
            columns: size.columns.into(),
            cell_width: size.cell_width.max(1),
            padding_left: 0,
            screen_height: (u32::from(size.rows) * size.cell_height).max(1),
        };
        let event = GestureEvent::new(ffi::GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_DRAG);
        event.set(ffi::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REF, &reference);
        event.set(
            ffi::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_POSITION,
            &position(at),
        );
        event.set(ffi::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_GEOMETRY, &geometry);
        self.apply_gesture(&event);
    }

    /// Ends the gesture at `at`. A press that dragged first takes the drag
    /// to `at`; a plain click selects nothing.
    pub fn select_release(&mut self, at: Pointer) {
        let mut dragged = false;
        // SAFETY: the data kind writes one `bool`.
        let result = unsafe {
            ffi::ghostty_selection_gesture_get(
                self.gesture,
                self.raw,
                ffi::GHOSTTY_SELECTION_GESTURE_DATA_DRAGGED,
                (&mut dragged as *mut bool).cast(),
            )
        };
        if result == ffi::GHOSTTY_SUCCESS && dragged {
            self.select_drag(at);
        }
        let event = GestureEvent::new(ffi::GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_RELEASE);
        if let Some(reference) = self.grid_ref(at.cell) {
            event.set(ffi::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REF, &reference);
        }
        self.apply_gesture(&event);
    }

    /// Applies `event` and installs the selection it produces, if any.
    fn apply_gesture(&mut self, event: &GestureEvent) {
        let mut selection = ffi::GhosttySelection {
            start: EMPTY_REF,
            end: EMPTY_REF,
            rectangle: false,
        };
        // SAFETY: the handles are live; the output is a valid selection.
        let result = unsafe {
            ffi::ghostty_selection_gesture_event(self.gesture, self.raw, event.0, &mut selection)
        };
        match result {
            ffi::GHOSTTY_NO_VALUE => {}
            result => {
                check(result, "applying a selection gesture event");
                self.set_selection(&selection);
            }
        }
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

fn position(at: Pointer) -> ffi::GhosttySurfacePosition {
    ffi::GhosttySurfacePosition { x: at.x, y: at.y }
}

/// An owned gesture event, freed on drop.
struct GestureEvent(ffi::GhosttySelectionGestureEvent);

impl GestureEvent {
    fn new(kind: ffi::GhosttySelectionGestureEventType) -> Self {
        let mut event = ptr::null_mut();
        // SAFETY: `event` is a valid out pointer; the default allocator is used.
        check(
            unsafe { ffi::ghostty_selection_gesture_event_new(ptr::null(), &mut event, kind) },
            "ghostty_selection_gesture_event_new",
        );
        Self(event)
    }

    /// Sets `option`, whose documented value type is `T`.
    fn set<T>(&self, option: ffi::GhosttySelectionGestureEventOption, value: &T) {
        // SAFETY: the handle is live; the library copies the value.
        check(
            unsafe {
                ffi::ghostty_selection_gesture_event_set(self.0, option, (value as *const T).cast())
            },
            "setting a selection gesture event option",
        );
    }
}

impl Drop for GestureEvent {
    fn drop(&mut self) {
        // SAFETY: owned and never used again.
        unsafe { ffi::ghostty_selection_gesture_event_free(self.0) };
    }
}
