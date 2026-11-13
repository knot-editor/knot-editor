//! Frames read from a terminal for painting.
//!
//! A [`RenderState`] copies what one frame needs out of a [`Terminal`]: rows,
//! cells, the cursor, and colors. Only [`RenderState::update`] touches the
//! terminal, and it holds the terminal's lock only for the copy; reading the
//! frame afterwards runs while the terminal keeps parsing output.

use std::{
    mem, ptr,
    sync::{Mutex, PoisonError},
};

use crate::{
    ffi,
    terminal::{Rgb, Terminal, check},
};

/// What changed since the frame was last marked clean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dirty {
    /// Nothing; painting can be skipped.
    Clean,
    /// Some rows; [`Row::dirty`] tells which.
    Partial,
    /// Global state such as size or colors; repaint every row.
    Full,
}

/// The shape the cursor is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    BlockHollow,
    Bar,
    Underline,
}

/// The cursor of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// The column and row in the viewport, or `None` when the cursor is
    /// scrolled out of view.
    pub position: Option<(u16, u16)>,
    /// Whether the cursor sits on the right half of a wide character.
    pub wide_tail: bool,
    /// Whether the program shows the cursor (DECTCEM).
    pub visible: bool,
    pub blinking: bool,
    pub style: CursorStyle,
}

/// The colors of a frame, after any program overrides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    pub foreground: Rgb,
    pub background: Rgb,
    /// The cursor color, when a program or the defaults set one.
    pub cursor: Option<Rgb>,
    pub palette: [Rgb; 256],
}

/// How a cell takes part in a wide character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellWidth {
    Narrow,
    /// The left half of a wide character, which holds its text.
    Wide,
    /// The right half of a wide character; it has no text of its own.
    SpacerTail,
    /// The last column of a row whose wide character wrapped to the next.
    SpacerHead,
}

/// The underline a cell is drawn with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// The text attributes of a cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub blink: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub underline: Underline,
}

impl From<ffi::GhosttyStyle> for Style {
    fn from(style: ffi::GhosttyStyle) -> Self {
        let flags = u32::from(style.flags);
        let has = |flag| flags & flag != 0;
        let underline = match ((flags & ffi::GHOSTTY_STYLE_UNDERLINE_MASK)
            >> ffi::GHOSTTY_STYLE_UNDERLINE_SHIFT)
            as ffi::GhosttySgrUnderline
        {
            ffi::GHOSTTY_SGR_UNDERLINE_SINGLE => Underline::Single,
            ffi::GHOSTTY_SGR_UNDERLINE_DOUBLE => Underline::Double,
            ffi::GHOSTTY_SGR_UNDERLINE_CURLY => Underline::Curly,
            ffi::GHOSTTY_SGR_UNDERLINE_DOTTED => Underline::Dotted,
            ffi::GHOSTTY_SGR_UNDERLINE_DASHED => Underline::Dashed,
            _ => Underline::None,
        };
        Self {
            bold: has(ffi::GHOSTTY_STYLE_FLAG_BOLD),
            italic: has(ffi::GHOSTTY_STYLE_FLAG_ITALIC),
            faint: has(ffi::GHOSTTY_STYLE_FLAG_FAINT),
            blink: has(ffi::GHOSTTY_STYLE_FLAG_BLINK),
            inverse: has(ffi::GHOSTTY_STYLE_FLAG_INVERSE),
            invisible: has(ffi::GHOSTTY_STYLE_FLAG_INVISIBLE),
            strikethrough: has(ffi::GHOSTTY_STYLE_FLAG_STRIKETHROUGH),
            overline: has(ffi::GHOSTTY_STYLE_FLAG_OVERLINE),
            underline,
        }
    }
}

/// One cell of a frame row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell<'a> {
    /// The grapheme cluster, or `""` for an empty cell or a spacer.
    pub text: &'a str,
    pub width: CellWidth,
    pub style: Style,
    /// The cell's own foreground, with palette indices resolved; `None`
    /// draws with the frame's foreground. Inverse and faint are left to the
    /// painter.
    pub foreground: Option<Rgb>,
    /// The cell's own background; `None` draws with the frame's background.
    pub background: Option<Rgb>,
}

/// A frame read from a [`Terminal`], reused from update to update.
pub struct RenderState {
    raw: ffi::GhosttyRenderState,
    rows: ffi::GhosttyRenderStateRowIterator,
    cells: ffi::GhosttyRenderStateRowCells,
    text: Vec<u8>,
}

// SAFETY: the handles are owned, the library keeps no thread-local state for
// them, and every access goes through `&self` or `&mut self`.
unsafe impl Send for RenderState {}

impl Default for RenderState {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderState {
    pub fn new() -> Self {
        let mut raw = ptr::null_mut();
        let mut rows = ptr::null_mut();
        let mut cells = ptr::null_mut();
        // SAFETY: each call receives a valid out pointer and the default allocator.
        unsafe {
            check(
                ffi::ghostty_render_state_new(ptr::null(), &mut raw),
                "ghostty_render_state_new",
            );
            check(
                ffi::ghostty_render_state_row_iterator_new(ptr::null(), &mut rows),
                "ghostty_render_state_row_iterator_new",
            );
            check(
                ffi::ghostty_render_state_row_cells_new(ptr::null(), &mut cells),
                "ghostty_render_state_row_cells_new",
            );
        }
        Self {
            raw,
            rows,
            cells,
            text: Vec::new(),
        }
    }

    /// Copies the terminal's current frame, holding `terminal`'s lock only
    /// while copying. Dirty state accumulates until [`Self::clean`].
    pub fn update(&mut self, terminal: &Mutex<Terminal>) {
        {
            let terminal = terminal.lock().unwrap_or_else(PoisonError::into_inner);
            // SAFETY: both handles are live, and the lock gives this call
            // exclusive access to the terminal.
            check(
                unsafe { ffi::ghostty_render_state_begin_update(self.raw, terminal.raw) },
                "ghostty_render_state_begin_update",
            );
        }
        // SAFETY: the end phase touches only the render state.
        check(
            unsafe { ffi::ghostty_render_state_end_update(self.raw) },
            "ghostty_render_state_end_update",
        );
    }

    /// Marks the frame painted: the dirty state becomes [`Dirty::Clean`]
    /// and every row clean.
    pub fn clean(&mut self) {
        // SAFETY: the handle is live.
        check(
            unsafe { ffi::ghostty_render_state_clean(self.raw) },
            "ghostty_render_state_clean",
        );
    }

    pub fn dirty(&self) -> Dirty {
        let dirty: ffi::GhosttyRenderStateDirty =
            self.get(ffi::GHOSTTY_RENDER_STATE_DATA_DIRTY, "the dirty state");
        match dirty {
            ffi::GHOSTTY_RENDER_STATE_DIRTY_FALSE => Dirty::Clean,
            ffi::GHOSTTY_RENDER_STATE_DIRTY_PARTIAL => Dirty::Partial,
            _ => Dirty::Full,
        }
    }

    /// The viewport's columns and rows.
    pub fn size(&self) -> (u16, u16) {
        (
            self.get(ffi::GHOSTTY_RENDER_STATE_DATA_COLS, "the columns"),
            self.get(ffi::GHOSTTY_RENDER_STATE_DATA_ROWS, "the rows"),
        )
    }

    pub fn cursor(&self) -> Cursor {
        let cursor: ffi::GhosttyRenderStateCursor =
            self.get_sized(ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR, "the cursor");
        let style = match cursor.visual_style {
            ffi::GHOSTTY_CURSOR_STYLE_BAR => CursorStyle::Bar,
            ffi::GHOSTTY_CURSOR_STYLE_UNDERLINE => CursorStyle::Underline,
            ffi::GHOSTTY_CURSOR_STYLE_BLOCK_HOLLOW => CursorStyle::BlockHollow,
            _ => CursorStyle::Block,
        };
        Cursor {
            position: cursor
                .viewport_has_value
                .then_some((cursor.viewport_x, cursor.viewport_y)),
            wide_tail: cursor.viewport_has_value && cursor.wide_tail,
            visible: cursor.visible,
            blinking: cursor.blinking,
            style,
        }
    }

    pub fn colors(&self) -> Colors {
        let colors: ffi::GhosttyRenderStateColors =
            self.get_sized(ffi::GHOSTTY_RENDER_STATE_DATA_COLORS, "the colors");
        Colors {
            foreground: colors.foreground.into(),
            background: colors.background.into(),
            cursor: colors.cursor_has_value.then(|| colors.cursor.into()),
            palette: colors.palette.map(Rgb::from),
        }
    }

    /// The viewport's rows, top to bottom.
    pub fn rows(&mut self) -> Rows<'_> {
        // SAFETY: the data kind fills the owned row iterator.
        check(
            unsafe {
                ffi::ghostty_render_state_get(
                    self.raw,
                    ffi::GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                    (&mut self.rows as *mut ffi::GhosttyRenderStateRowIterator).cast(),
                )
            },
            "reading the rows",
        );
        Rows {
            state: self,
            next: 0,
        }
    }

    fn get<T: Copy>(&self, data: ffi::GhosttyRenderStateData, what: &str) -> T {
        let mut value = mem::MaybeUninit::<T>::uninit();
        // SAFETY: callers pair each data kind with its documented output type.
        unsafe {
            check(
                ffi::ghostty_render_state_get(self.raw, data, value.as_mut_ptr().cast()),
                &format!("reading {what}"),
            );
            value.assume_init()
        }
    }

    /// Reads a sized struct whose first field is its `size_t` size.
    fn get_sized<T: Copy>(&self, data: ffi::GhosttyRenderStateData, what: &str) -> T {
        // SAFETY: `T` is a plain C struct for which all zeroes is valid; its
        // leading size field is set before the call, as the ABI requires.
        unsafe {
            let mut value = mem::zeroed::<T>();
            ptr::write((&mut value as *mut T).cast::<usize>(), mem::size_of::<T>());
            check(
                ffi::ghostty_render_state_get(self.raw, data, (&mut value as *mut T).cast()),
                &format!("reading {what}"),
            );
            value
        }
    }
}

impl Drop for RenderState {
    fn drop(&mut self) {
        // SAFETY: the handles are owned and never used again.
        unsafe {
            ffi::ghostty_render_state_row_cells_free(self.cells);
            ffi::ghostty_render_state_row_iterator_free(self.rows);
            ffi::ghostty_render_state_free(self.raw);
        }
    }
}

/// The rows of a frame, read with [`Rows::next`].
pub struct Rows<'a> {
    state: &'a mut RenderState,
    next: u16,
}

impl Rows<'_> {
    /// The next row down, or `None` after the last.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Row<'_>> {
        // SAFETY: the iterator was filled from the live render state.
        if !unsafe { ffi::ghostty_render_state_row_iterator_next(self.state.rows) } {
            return None;
        }
        let index = self.next;
        self.next += 1;
        Some(Row {
            state: self.state,
            index,
        })
    }
}

/// One row of a frame.
pub struct Row<'a> {
    state: &'a mut RenderState,
    index: u16,
}

impl Row<'_> {
    /// The row's position in the viewport, from the top.
    pub fn index(&self) -> u16 {
        self.index
    }

    /// Whether the row changed since the frame was last marked clean.
    pub fn dirty(&self) -> bool {
        let mut dirty = false;
        // SAFETY: the iterator is positioned on this row; the kind writes a bool.
        check(
            unsafe {
                ffi::ghostty_render_state_row_get(
                    self.state.rows,
                    ffi::GHOSTTY_RENDER_STATE_ROW_DATA_DIRTY,
                    (&mut dirty as *mut bool).cast(),
                )
            },
            "reading a row's dirty state",
        );
        dirty
    }

    /// The first and last selected columns of the row, if the selection
    /// reaches it.
    pub fn selection(&self) -> Option<(u16, u16)> {
        // SAFETY: all zeroes is a valid value; the leading size is set as
        // the sized-struct ABI requires.
        let mut selection = unsafe { mem::zeroed::<ffi::GhosttyRenderStateRowSelection>() };
        selection.size = mem::size_of::<ffi::GhosttyRenderStateRowSelection>();
        // SAFETY: the iterator is positioned on this row; the kind writes one
        // row selection.
        let result = unsafe {
            ffi::ghostty_render_state_row_get(
                self.state.rows,
                ffi::GHOSTTY_RENDER_STATE_ROW_DATA_SELECTION,
                (&mut selection as *mut ffi::GhosttyRenderStateRowSelection).cast(),
            )
        };
        if result == ffi::GHOSTTY_NO_VALUE {
            return None;
        }
        check(result, "reading a row's selection");
        Some((selection.start_x, selection.end_x))
    }

    /// The row's cells, left to right.
    pub fn cells(&mut self) -> Cells<'_> {
        // SAFETY: the iterator is positioned on this row; the kind fills the
        // owned cells handle.
        check(
            unsafe {
                ffi::ghostty_render_state_row_get(
                    self.state.rows,
                    ffi::GHOSTTY_RENDER_STATE_ROW_DATA_CELLS,
                    (&mut self.state.cells as *mut ffi::GhosttyRenderStateRowCells).cast(),
                )
            },
            "reading a row's cells",
        );
        Cells { state: self.state }
    }
}

/// The cells of a row, read with [`Cells::next`].
pub struct Cells<'a> {
    state: &'a mut RenderState,
}

impl Cells<'_> {
    /// The next cell to the right, or `None` after the last.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Cell<'_>> {
        let cells = self.state.cells;
        // SAFETY: the handle was filled from the current row.
        if !unsafe { ffi::ghostty_render_state_row_cells_next(cells) } {
            return None;
        }
        let raw: ffi::GhosttyCell = cell_get(
            cells,
            ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
            "a cell",
        );
        let mut wide: ffi::GhosttyCellWide = ffi::GHOSTTY_CELL_WIDE_NARROW;
        // SAFETY: the kind writes one `GhosttyCellWide`.
        check(
            unsafe {
                ffi::ghostty_cell_get(
                    raw,
                    ffi::GHOSTTY_CELL_DATA_WIDE,
                    (&mut wide as *mut ffi::GhosttyCellWide).cast(),
                )
            },
            "reading a cell's width",
        );
        let width = match wide {
            ffi::GHOSTTY_CELL_WIDE_WIDE => CellWidth::Wide,
            ffi::GHOSTTY_CELL_WIDE_SPACER_TAIL => CellWidth::SpacerTail,
            ffi::GHOSTTY_CELL_WIDE_SPACER_HEAD => CellWidth::SpacerHead,
            _ => CellWidth::Narrow,
        };
        let style: ffi::GhosttyStyle = cell_get(
            cells,
            ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
            "a cell's style",
        );
        let text = &mut self.state.text;
        read_text(cells, text);
        Some(Cell {
            // The library encodes grapheme clusters as UTF-8.
            text: std::str::from_utf8(text).unwrap_or_default(),
            width,
            style: style.into(),
            foreground: cell_color(cells, ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR),
            background: cell_color(cells, ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR),
        })
    }
}

fn cell_get<T: Copy>(
    cells: ffi::GhosttyRenderStateRowCells,
    data: ffi::GhosttyRenderStateRowCellsData,
    what: &str,
) -> T {
    let mut value = mem::MaybeUninit::<T>::uninit();
    // SAFETY: callers pair each data kind with its documented output type.
    unsafe {
        check(
            ffi::ghostty_render_state_row_cells_get(cells, data, value.as_mut_ptr().cast()),
            &format!("reading {what}"),
        );
        value.assume_init()
    }
}

fn cell_color(
    cells: ffi::GhosttyRenderStateRowCells,
    data: ffi::GhosttyRenderStateRowCellsData,
) -> Option<Rgb> {
    let mut color = ffi::GhosttyColorRgb { r: 0, g: 0, b: 0 };
    // SAFETY: the color kinds write one `GhosttyColorRgb`.
    let result = unsafe {
        ffi::ghostty_render_state_row_cells_get(
            cells,
            data,
            (&mut color as *mut ffi::GhosttyColorRgb).cast(),
        )
    };
    if result == ffi::GHOSTTY_NO_VALUE {
        return None;
    }
    check(result, "reading a cell's color");
    Some(color.into())
}

/// Reads the current cell's grapheme cluster as UTF-8 into `text`.
fn read_text(cells: ffi::GhosttyRenderStateRowCells, text: &mut Vec<u8>) {
    loop {
        let mut buffer = ffi::GhosttyBuffer {
            ptr: text.as_mut_ptr(),
            cap: text.capacity(),
            len: 0,
        };
        // SAFETY: the buffer describes `text`'s allocation; the library writes
        // at most `cap` bytes and reports the length it needs.
        let result = unsafe {
            ffi::ghostty_render_state_row_cells_get(
                cells,
                ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
                (&mut buffer as *mut ffi::GhosttyBuffer).cast(),
            )
        };
        if result == ffi::GHOSTTY_OUT_OF_SPACE {
            text.clear();
            text.reserve(buffer.len);
            continue;
        }
        check(result, "reading a cell's text");
        // SAFETY: the library initialized the first `len` bytes.
        unsafe { text.set_len(buffer.len) };
        return;
    }
}
