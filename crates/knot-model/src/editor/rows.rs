//! Visual rows: how the wrap map shows buffer lines, the caret geometry in
//! display cells, and the decoration backgrounds each row paints.

use super::wrap::WrapWidth;
use super::*;

/// One visual row: the bytes of a buffer line that it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub index: usize,
    pub line: usize,
    /// Bytes of the line's content.
    pub range: Range<usize>,
    /// Whether the row ends its line, so it also shows the line's end.
    pub last: bool,
    /// How many cells right of the text area's edge the row's text starts.
    pub indent: usize,
}

impl Row {
    /// Whether the caret at byte `col` of `line` shows on this row.
    pub fn holds(&self, (line, col): (usize, usize)) -> bool {
        self.line == line && self.range.start <= col && (col < self.range.end || self.last)
    }

    /// The first position after this row's positions.
    fn after(&self) -> (usize, usize) {
        if self.last {
            (self.line + 1, 0)
        } else {
            (self.line, self.range.end)
        }
    }

    /// The part of `start..end` that this row shows: whether it begins and
    /// ends here, or `None` when the row shows none of it.
    pub fn span(
        &self,
        start: (usize, usize),
        end: (usize, usize),
    ) -> Option<(Option<usize>, Option<usize>)> {
        let first = (self.line, self.range.start);
        (start < self.after() && end > first).then(|| {
            (
                (start >= first).then_some(start.1),
                (end < self.after()).then_some(end.1),
            )
        })
    }
}

/// A background one decoration paints on one row.
#[derive(Debug, PartialEq)]
pub struct DecorationFill<'a> {
    /// The row's position among the rows painted.
    pub row: usize,
    /// The byte columns of the row's line it covers, or `None` for a band
    /// across the whole text area.
    pub columns: Option<Range<usize>>,
    pub key: &'a str,
}

/// The backgrounds `decorations` paint on `rows`, in paint order: `line`
/// bands, then `background` spans that end at the text on each row, each
/// group with the most severe keys last so they land on top.
pub fn decoration_fills<'a, 'r>(
    decorations: &'a [RenderedDecoration],
    rows: impl IntoIterator<Item = &'r Row>,
) -> Vec<DecorationFill<'a>> {
    let rows: Vec<&Row> = rows.into_iter().collect();
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
        return Vec::new();
    };
    let lines = first.line..=last.line;
    let mut fills = Vec::new();
    for style in [DecorationStyle::Line, DecorationStyle::Background] {
        let mut painted: Vec<&RenderedDecoration> = decorations
            .iter()
            .filter(|decoration| decoration.style == style && lines.contains(&decoration.line))
            .collect();
        painted.sort_by_key(|decoration| decoration_precedence(&decoration.key));
        for decoration in painted {
            for (position, row) in rows.iter().enumerate() {
                if row.line != decoration.line {
                    continue;
                }
                let columns = if style == DecorationStyle::Line {
                    None
                } else {
                    let start = (decoration.line, decoration.start);
                    let end = (decoration.line, decoration.end);
                    let Some((begins, ends)) = row.span(start, end) else {
                        continue;
                    };
                    Some(begins.unwrap_or(row.range.start)..ends.unwrap_or(row.range.end))
                        .filter(|columns| !columns.is_empty())
                };
                if style == DecorationStyle::Line || columns.is_some() {
                    fills.push(DecorationFill {
                        row: position,
                        columns,
                        key: &decoration.key,
                    });
                }
            }
        }
    }
    fills
}

impl EditorState {
    /// Wrap lines to `wrap`, or stop wrapping with `None`, keeping the top
    /// row's text at the top when the rows change.
    pub fn set_wrap(&mut self, wrap: Option<WrapWidth>, model: &BufferModel) {
        let restored = self.restored_top.take();
        if wrap == self.wrap.width() {
            return;
        }
        self.sync_wrap(model);
        let top = restored.unwrap_or_else(|| self.top_offset(model));
        self.wrap
            .set_width(wrap, model.line_count(), |line| line_text(model, line));
        let (line, col) = flat_byte_to_position(model, top);
        self.scroll_top = self.wrap.row_of(line, col) as f32;
        if wrap.is_some() {
            self.scroll_x = 0.;
        }
    }

    /// Lay out as a view `rows` rows tall and `columns` cells wide would,
    /// wrapping when `editor.softWrap` is on, for a test without a window.
    #[cfg(any(test, feature = "test-support"))]
    pub fn lay_out_for_test(&mut self, rows: f32, columns: usize, buffer: &BufferModel) {
        let wrap = self.settings.soft_wrap.then_some(WrapWidth {
            columns,
            tab_size: self.settings.tab_size,
        });
        self.set_wrap(wrap, buffer);
        self.lay_out(rows, true);
    }

    /// Visual row `index`, clamped to the last row.
    pub fn row(&self, model: &BufferModel, index: usize) -> Row {
        let (line, sub) = self.wrap.position(index);
        // As in `row_start`, the wrap map may lag the text.
        let line = line.min(model.line_count().saturating_sub(1));
        let rows = self.wrap.rows(line);
        let sub = sub.min(rows.len().saturating_sub(1));
        Row {
            index: rows.start + sub,
            line,
            range: self.wrap.row_range(line, sub, line_end(model, line)),
            last: rows.start + sub + 1 == rows.end,
            indent: self.wrap.indent(line, sub),
        }
    }

    /// One indentation level from the `editor.insertSpaces` and
    /// `editor.tabSize` settings.
    pub(crate) fn indent_unit(&self) -> IndentUnit {
        IndentUnit {
            tabs: !self.settings.insert_spaces,
            size: self.settings.tab_size,
        }
    }

    /// The display cell the caret shows at: the caret's row's indentation
    /// plus the cells of the row's text before it, with tabs expanded.
    pub(crate) fn caret_column(&self, model: &BufferModel) -> usize {
        let row = self.row(model, self.caret_row());
        let text = line_text(model, row.line);
        let col = self.cursor_col.clamp(row.range.start, row.range.end);
        row.indent + wrap::cells_before(&text[row.range.start..col], self.settings.tab_size)
    }

    /// The display cells from the start of buffer line `line` to byte `column`
    /// in it, with tabs expanded from the line's start, measured as soft wrap
    /// measures rows with `tab_size` columns per stop. `None` past the last
    /// line or off a character boundary.
    pub fn line_cells_before(
        line: usize,
        column: usize,
        tab_size: usize,
        model: &BufferModel,
    ) -> Option<usize> {
        if line >= model.line_count() {
            return None;
        }
        let text = line_text(model, line);
        let prefix = text.get(..column)?;
        Some(wrap::cells_before(prefix, tab_size))
    }

    /// The byte column of the character covering display cell `cells` in buffer
    /// line `line` with `tab_size` columns per stop, or the line's last
    /// character when it is shorter. `None` past the last line.
    pub fn line_column_at_cells(
        line: usize,
        cells: usize,
        tab_size: usize,
        model: &BufferModel,
    ) -> Option<usize> {
        if line >= model.line_count() {
            return None;
        }
        let text = line_text(model, line);
        Some(wrap::index_at_or_before_cells(&text, cells, tab_size))
    }

    /// The caret position on row `index` nearest display cell `column`. A
    /// row that does not end its line leaves its end to the next row.
    pub(crate) fn position_at_column(
        &self,
        model: &BufferModel,
        index: usize,
        column: usize,
    ) -> (usize, usize) {
        let row = self.row(model, index);
        let text = line_text(model, row.line);
        let col = wrap::index_at_cells(
            &text[row.range.clone()],
            column.saturating_sub(row.indent),
            self.settings.tab_size,
            row.last,
        );
        (row.line, row.range.start + col)
    }
}
