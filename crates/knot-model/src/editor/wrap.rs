//! Soft wrap: where each buffer line breaks into the rows a view shows.
//!
//! Rows are measured in monospace cells rather than shaped. A character takes
//! its `unicode-width` cells, wide CJK and emoji two, and a tab advances to the
//! next stop from the start of its row. A row breaks after the last whitespace
//! that follows text on it, or inside a word wider than the row; whitespace may
//! hang past the edge, so a continuation row never starts with the spaces that
//! ended the row before it. Continuation rows are indented like the line's
//! first row when that leaves at least half the row for text.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::buffer::LineChange;

/// What a view wraps lines to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WrapWidth {
    /// Cells per row, at least 1.
    pub columns: usize,
    pub tab_size: usize,
}

/// Where one line breaks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LineWrap {
    /// Byte offsets where continuation rows start, ascending.
    breaks: Vec<usize>,
    /// Cells continuation rows are indented by.
    indent: usize,
}

fn cells(grapheme: &str, column: usize, tab_size: usize) -> usize {
    if grapheme == "\t" {
        tab_size - column % tab_size
    } else {
        grapheme.width().min(2)
    }
}

/// The cells `text`, the start of a row, takes, with tabs expanded from the
/// row's start.
pub fn cells_before(text: &str, tab_size: usize) -> usize {
    let tab_size = tab_size.max(1);
    text.graphemes(true).fold(0, |column, grapheme| {
        column + cells(grapheme, column, tab_size)
    })
}

/// The grapheme boundary of `text`, a row, nearest `column` cells from its
/// start, the earlier one on a tie. The row's end counts only with `end`.
pub fn index_at_cells(text: &str, column: usize, tab_size: usize, end: bool) -> usize {
    let tab_size = tab_size.max(1);
    let mut best = (0, column);
    let mut used: usize = 0;
    for (index, grapheme) in text.grapheme_indices(true) {
        if index > 0 && used.abs_diff(column) < best.1 {
            best = (index, used.abs_diff(column));
        }
        used += cells(grapheme, used, tab_size);
    }
    if end && used.abs_diff(column) < best.1 {
        best = (text.len(), used.abs_diff(column));
    }
    best.0
}

/// The grapheme of `text`, a row, that covers display cell `column` from its
/// start, the last one when the row is shorter, or 0 on an empty row. A tab
/// covers every cell up to its stop.
pub fn index_at_or_before_cells(text: &str, column: usize, tab_size: usize) -> usize {
    let tab_size = tab_size.max(1);
    let mut start = 0;
    let mut used: usize = 0;
    for (index, grapheme) in text.grapheme_indices(true) {
        if used > column {
            break;
        }
        start = index;
        used += cells(grapheme, used, tab_size);
    }
    start
}

/// Break `text`, one line without its terminator, into rows of `width`.
pub fn wrap_line(text: &str, width: WrapWidth) -> LineWrap {
    let columns = width.columns.max(1);
    let tab_size = width.tab_size.max(1);
    let graphemes: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
    let leading = graphemes
        .iter()
        .take_while(|(_, grapheme)| matches!(*grapheme, " " | "\t"))
        .fold(0, |column, (_, grapheme)| {
            column + cells(grapheme, column, tab_size)
        });
    let indent = if leading * 2 <= columns { leading } else { 0 };

    let mut breaks = Vec::new();
    let mut row_start = 0;
    let mut available = columns;
    let mut used = 0;
    // The offset after the last whitespace following text on this row, and
    // the index of the grapheme there.
    let mut opportunity: Option<(usize, usize)> = None;
    let mut content = false;
    let mut index = 0;
    while let Some(&(at, grapheme)) = graphemes.get(index) {
        let width = cells(grapheme, used, tab_size);
        if matches!(grapheme, " " | "\t") {
            used += width;
            if content {
                opportunity = Some((at + grapheme.len(), index + 1));
            }
            index += 1;
            continue;
        }
        if used + width > available && at > row_start {
            let (start, resume) = opportunity.unwrap_or((at, index));
            breaks.push(start);
            row_start = start;
            index = resume;
            available = columns - indent;
            used = 0;
            opportunity = None;
            content = false;
            continue;
        }
        used += width;
        content = true;
        index += 1;
    }
    LineWrap {
        indent: if breaks.is_empty() { 0 } else { indent },
        breaks,
    }
}

/// The rows of every line of one view's buffer. Lines a fold hides have no
/// rows. Without a width or hidden lines, each line is one row and nothing is
/// stored per line.
pub struct WrapMap {
    width: Option<WrapWidth>,
    line_count: usize,
    /// Each line's breaks while wrapping.
    lines: Vec<LineWrap>,
    /// The first row of each line, then the row count, while wrapping or
    /// hiding lines.
    first_rows: Vec<usize>,
    /// The lines that have no rows, as ascending disjoint ranges.
    hidden: Vec<Range<usize>>,
}

impl WrapMap {
    pub fn new(line_count: usize) -> Self {
        Self {
            width: None,
            line_count,
            lines: Vec::new(),
            first_rows: Vec::new(),
            hidden: Vec::new(),
        }
    }

    /// Whether `first_rows` is stored, because rows are not the lines.
    fn mapped(&self) -> bool {
        self.width.is_some() || !self.hidden.is_empty()
    }

    /// Whether `line` has no rows.
    pub fn is_hidden(&self, line: usize) -> bool {
        self.hidden_range(line).is_some()
    }

    /// The run of hidden lines holding `line`.
    pub fn hidden_range(&self, line: usize) -> Option<Range<usize>> {
        let at = self.hidden.partition_point(|range| range.end <= line);
        self.hidden
            .get(at)
            .filter(|range| range.start <= line)
            .cloned()
    }

    /// Give the lines in `hidden`, ascending and disjoint, no rows.
    pub fn set_hidden(&mut self, hidden: Vec<Range<usize>>) {
        if hidden == self.hidden {
            return;
        }
        self.hidden = hidden;
        self.rebuild_rows(0);
    }

    /// Recompute the first rows of the lines from `first` on.
    fn rebuild_rows(&mut self, first: usize) {
        if !self.mapped() {
            self.first_rows = Vec::new();
            return;
        }
        let first = if self.hidden.is_empty() { first } else { 0 };
        self.first_rows.resize(self.line_count + 1, 0);
        for line in first..self.line_count {
            let rows = if self.is_hidden(line) {
                0
            } else {
                self.lines.get(line).map_or(0, |wrap| wrap.breaks.len()) + 1
            };
            self.first_rows[line + 1] = self.first_rows[line] + rows;
        }
    }

    pub fn width(&self) -> Option<WrapWidth> {
        self.width
    }

    pub fn row_count(&self) -> usize {
        self.first_rows.last().copied().unwrap_or(self.line_count)
    }

    pub fn first_row(&self, line: usize) -> usize {
        if self.mapped() {
            self.first_rows[line.min(self.line_count)]
        } else {
            line
        }
    }

    /// The rows `line` occupies.
    pub fn rows(&self, line: usize) -> Range<usize> {
        self.first_row(line)..self.first_row(line + 1)
    }

    /// The line `row` belongs to and its index among that line's rows.
    pub fn position(&self, row: usize) -> (usize, usize) {
        let row = row.min(self.row_count().saturating_sub(1));
        if !self.mapped() {
            return (row, 0);
        }
        let line = self.first_rows[..self.line_count].partition_point(|first| *first <= row) - 1;
        (line, row - self.first_rows[line])
    }

    /// The row showing byte `col` of `line`. An offset where a row breaks
    /// starts the next row.
    pub fn row_of(&self, line: usize, col: usize) -> usize {
        self.first_row(line)
            + self
                .lines
                .get(line)
                .map_or(0, |wrap| wrap.breaks.partition_point(|at| *at <= col))
    }

    /// The bytes of `line` that its row `sub` shows, for a line with `len`
    /// bytes of content.
    pub fn row_range(&self, line: usize, sub: usize, len: usize) -> Range<usize> {
        let breaks = self.lines.get(line).map_or(&[][..], |wrap| &wrap.breaks);
        let start = if sub == 0 { 0 } else { breaks[sub - 1] };
        start..breaks.get(sub).copied().unwrap_or(len)
    }

    /// Cells row `sub` of `line` is indented by.
    pub fn indent(&self, line: usize, sub: usize) -> usize {
        match self.lines.get(line) {
            Some(wrap) if sub > 0 => wrap.indent,
            _ => 0,
        }
    }

    /// Wrap to `width`, or stop wrapping with `None`, rebuilding every line
    /// when it changes. Returns whether it changed.
    pub fn set_width(
        &mut self,
        width: Option<WrapWidth>,
        line_count: usize,
        text: impl Fn(usize) -> String,
    ) -> bool {
        if width == self.width {
            return false;
        }
        self.width = width;
        self.edit(line_count, None, text);
        true
    }

    /// Follow commits that left `line_count` lines, rewrapping the lines
    /// `change` covers, or every line without one.
    pub fn edit(
        &mut self,
        line_count: usize,
        change: Option<LineChange>,
        text: impl Fn(usize) -> String,
    ) {
        let old_count = self.line_count;
        self.line_count = line_count;
        let Some(width) = self.width else {
            self.lines = Vec::new();
            self.rebuild_rows(0);
            return;
        };
        let wrap = |line| wrap_line(&text(line), width);
        let change = change.filter(|change| {
            let fits = self.lines.len() == old_count
                && change.old_end <= old_count
                && change.new_end <= line_count
                && old_count - change.old_end == line_count - change.new_end;
            debug_assert!(
                fits,
                "{change:?} does not take {old_count} lines to {line_count}"
            );
            fits
        });
        let first = match change {
            Some(change) => {
                self.lines.splice(
                    change.first..change.old_end,
                    (change.first..change.new_end).map(wrap),
                );
                change.first
            }
            None => {
                self.lines = (0..line_count).map(wrap).collect();
                0
            }
        };
        self.rebuild_rows(first);
    }

    #[cfg(test)]
    pub fn lines(&self) -> &[LineWrap] {
        &self.lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::BufferModel;

    fn rows(text: &str, columns: usize) -> Vec<&str> {
        let wrap = wrap_line(
            text,
            WrapWidth {
                columns,
                tab_size: 4,
            },
        );
        let mut starts = vec![0];
        starts.extend(&wrap.breaks);
        starts.push(text.len());
        starts
            .windows(2)
            .map(|pair| &text[pair[0]..pair[1]])
            .collect()
    }

    #[test]
    fn breaks_after_the_last_whitespace_that_fits() {
        assert_eq!(rows("short", 10), ["short"]);
        assert_eq!(
            rows("one two three four", 9),
            ["one two ", "three ", "four"]
        );
        assert_eq!(rows("exactly ten", 11), ["exactly ten"]);
        // Whitespace hangs past the edge rather than starting a row.
        assert_eq!(rows("abcd     efg", 4), ["abcd     ", "efg"]);
        assert_eq!(rows("", 4), [""]);
    }

    #[test]
    fn breaks_inside_a_word_wider_than_the_row() {
        assert_eq!(rows("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(rows("ab cdefghijk", 4), ["ab ", "cdef", "ghij", "k"]);
        assert_eq!(rows("abc", 1), ["a", "b", "c"]);
    }

    #[test]
    fn wide_characters_take_two_cells_and_tabs_reach_the_next_stop() {
        assert_eq!(rows("日本語の文", 4), ["日本", "語の", "文"]);
        assert_eq!(rows("a日本", 4), ["a日", "本"]);
        assert_eq!(rows("👩‍💻👩‍💻👩‍💻", 4), ["👩‍💻👩‍💻", "👩‍💻"]);
        // A tab after one cell takes three.
        assert_eq!(rows("x\tyz w", 6), ["x\tyz ", "w"]);
        assert_eq!(rows("ab\tc", 4), ["ab\t", "c"]);
    }

    #[test]
    fn the_character_covering_a_cell_is_found_at_or_before_it() {
        // 日 covers cells 1 and 2, 本 cells 3 and 4.
        assert_eq!(index_at_or_before_cells("a日本", 0, 4), 0);
        assert_eq!(index_at_or_before_cells("a日本", 2, 4), 1);
        assert_eq!(index_at_or_before_cells("a日本", 3, 4), 4);
        assert_eq!(index_at_or_before_cells("a日本", 10, 4), 4);
        // A tab covers the cells up to its stop, so cell 2 lands on it.
        assert_eq!(index_at_or_before_cells("\tb", 2, 4), 0);
        assert_eq!(index_at_or_before_cells("\tb", 4, 4), 1);
        assert_eq!(index_at_or_before_cells("", 3, 4), 0);
    }

    #[test]
    fn continuation_rows_keep_the_first_rows_indentation_when_it_fits() {
        let width = WrapWidth {
            columns: 10,
            tab_size: 4,
        };
        let wrap = wrap_line("    aaa bb ccc", width);
        assert_eq!(wrap.breaks, [11]);
        assert_eq!(wrap.indent, 4);
        // Rows after the first have eight cells left.
        let wrap = wrap_line("  aaa bbbbbbbbb", width);
        assert_eq!(wrap.breaks, [6, 14]);
        assert_eq!(wrap.indent, 2);
        // An indent over half the row leaves continuations at column 0.
        let wrap = wrap_line("      aaa bbb", width);
        assert_eq!((wrap.breaks.as_slice(), wrap.indent), (&[10][..], 0));
        let tabs = wrap_line("\taaa bb ccc", width);
        assert_eq!((tabs.breaks.as_slice(), tabs.indent), (&[8][..], 4));
        // An unwrapped line keeps no indent.
        assert_eq!(wrap_line("    a", width).indent, 0);
    }

    #[test]
    fn maps_rows_to_lines_and_positions() {
        let text = ["one two three", "", "four"];
        let mut map = WrapMap::new(3);
        assert_eq!(
            (map.row_count(), map.position(2), map.row_of(0, 9)),
            (3, (2, 0), 0)
        );
        let width = WrapWidth {
            columns: 8,
            tab_size: 4,
        };
        assert!(map.set_width(Some(width), 3, |line| text[line].into()));
        assert!(!map.set_width(Some(width), 3, |line| text[line].into()));
        assert_eq!(map.row_count(), 4);
        assert_eq!(map.rows(0), 0..2);
        assert_eq!(map.rows(2), 3..4);
        assert_eq!(map.position(1), (0, 1));
        assert_eq!(map.position(3), (2, 0));
        assert_eq!(map.row_of(0, 7), 0);
        assert_eq!(map.row_of(0, 8), 1);
        assert_eq!(map.row_of(0, 13), 1);
        assert_eq!(map.row_range(0, 0, 13), 0..8);
        assert_eq!(map.row_range(0, 1, 13), 8..13);
        assert_eq!(map.row_range(1, 0, 0), 0..0);
        map.set_width(None, 3, |line| text[line].into());
        assert_eq!((map.row_count(), map.lines().len()), (3, 0));
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn hidden_lines_have_no_rows() {
        let text = ["one two three", "b", "c", "four", "five"];
        let mut map = WrapMap::new(5);
        map.set_hidden([1..3].into());
        assert_eq!(map.row_count(), 3);
        assert!(map.is_hidden(2) && !map.is_hidden(3));
        assert_eq!(map.rows(1), 1..1);
        assert_eq!(map.rows(3), 1..2);
        assert_eq!(
            (map.position(0), map.position(1), map.position(2)),
            ((0, 0), (3, 0), (4, 0))
        );
        // Hidden lines stay mapped while wrapping, and a trailing run has no
        // rows at the end.
        let width = WrapWidth {
            columns: 8,
            tab_size: 4,
        };
        map.set_width(Some(width), 5, |line| text[line].into());
        assert_eq!(map.row_count(), 4);
        assert_eq!(map.position(2), (3, 0));
        map.set_hidden([3..5].into());
        assert_eq!((map.row_count(), map.position(3)), (4, (2, 0)));
        assert_eq!(map.rows(4), 4..4);
        // Edits keep the hidden lines hidden, and showing them again restores
        // the plain mapping.
        map.edit(4, None, |line| text[line].into());
        assert_eq!(map.row_count(), 4);
        map.set_hidden(Vec::new());
        map.set_width(None, 4, |line| text[line].into());
        assert_eq!((map.row_count(), map.position(2)), (4, (2, 0)));
    }

    #[test]
    fn rewrapping_changed_lines_matches_a_full_rebuild() {
        let width = WrapWidth {
            columns: 7,
            tab_size: 4,
        };
        let line = |model: &BufferModel, line: usize| {
            let range = model.line_range(line);
            model.read(range).trim_end_matches(['\n', '\r']).to_owned()
        };
        let mut model =
            BufferModel::from_text("alpha beta gamma\n\tdelta\nepsilon zeta eta\ntheta");
        let mut map = WrapMap::new(model.line_count());
        map.set_width(Some(width), model.line_count(), |at| line(&model, at));
        type Edit = (fn(&BufferModel) -> Range<usize>, &'static str);
        let edits: [Edit; 7] = [
            (|_| 6..6, "and more words "),
            (
                |model| model.line_range(1).start..model.line_range(2).end,
                "",
            ),
            (|_| 3..3, "\nsplit across\nthree lines here\n"),
            (|model| model.len()..model.len(), "\n\n trailing words"),
            (|_| 0..10, ""),
            (|model| 2..model.len() - 3, "x"),
            (|model| 0..model.len(), "a b c d e f g h\n日本語の文章\n"),
        ];
        for (range, text) in edits {
            let revision = model.revision();
            let range = range(&model);
            model.replace(range, text).unwrap();
            // Several commits between syncs compose.
            model.replace(0..0, "z ").unwrap();
            let change = model.line_changes_since(revision);
            assert!(change.is_some());
            map.edit(model.line_count(), change, |at| line(&model, at));
            let mut full = WrapMap::new(model.line_count());
            full.set_width(Some(width), model.line_count(), |at| line(&model, at));
            assert_eq!(map.lines(), full.lines(), "after {text:?}");
            assert_eq!(map.first_rows, full.first_rows);
        }
        assert_eq!(model.line_changes_since(model.revision()), None);
        assert!(model.line_changes_since(0).is_some());
        for _ in 0..64 {
            model.replace(0..0, "x").unwrap();
        }
        assert_eq!(model.line_changes_since(0), None);
    }
}
