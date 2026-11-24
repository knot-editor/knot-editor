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
use crate::theme::Highlight;

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

/// One line of a read-only block.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockLine {
    pub text: String,
    /// Sorted, non-overlapping syntax highlights, as byte ranges of `text`.
    pub highlights: Vec<(Range<usize>, Highlight)>,
    /// The decoration key whose background bands the line, if any.
    pub key: Option<String>,
}

/// Read-only lines shown above buffer line `line`, in rows that belong to no
/// buffer line.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub line: usize,
    pub lines: Vec<BlockLine>,
}

/// A block with the wrapping of each of its lines.
struct BlockEntry {
    block: Block,
    wraps: Vec<LineWrap>,
}

impl BlockEntry {
    fn new(block: Block, width: Option<WrapWidth>) -> Self {
        let mut entry = Self {
            block,
            wraps: Vec::new(),
        };
        entry.rewrap(width);
        entry
    }

    fn rewrap(&mut self, width: Option<WrapWidth>) {
        self.wraps = self
            .block
            .lines
            .iter()
            .map(|line| width.map_or_else(LineWrap::default, |width| wrap_line(&line.text, width)))
            .collect();
    }

    fn rows(&self) -> usize {
        self.wraps.iter().map(|wrap| wrap.breaks.len() + 1).sum()
    }
}

/// One row of a block, as [`WrapMap::block_row`] reports it.
#[derive(Debug, PartialEq)]
pub struct BlockRow<'a> {
    /// The buffer line the block sits above.
    pub line: usize,
    pub block_line: &'a BlockLine,
    /// The bytes of the block line's text that this row shows.
    pub range: Range<usize>,
    /// How many cells right of the text area's edge the row's text starts.
    pub indent: usize,
}

/// The rows of every line of one view's buffer. Lines a fold hides have no
/// rows. Without a width, hidden lines, or blocks, each line is one row and
/// nothing is stored per line.
///
/// A line's rows are its block's rows, if it has one, then the rows of its
/// text. Block rows belong to no line: [`Self::position`] reports them as the
/// first row of the line below, and [`Self::first_row`] and [`Self::rows`]
/// cover only a line's text.
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
    /// The blocks above lines, ascending by line, at most one per line.
    blocks: Vec<BlockEntry>,
}

impl WrapMap {
    pub fn new(line_count: usize) -> Self {
        Self {
            width: None,
            line_count,
            lines: Vec::new(),
            first_rows: Vec::new(),
            hidden: Vec::new(),
            blocks: Vec::new(),
        }
    }

    /// Whether `first_rows` is stored, because rows are not the lines.
    fn mapped(&self) -> bool {
        self.width.is_some() || !self.hidden.is_empty() || !self.blocks.is_empty()
    }

    fn entry(&self, line: usize) -> Option<&BlockEntry> {
        let at = self.blocks.partition_point(|entry| entry.block.line < line);
        self.blocks.get(at).filter(|entry| entry.block.line == line)
    }

    /// The rows of the block above `line`; none while `line` is hidden.
    fn block_rows(&self, line: usize) -> usize {
        if self.blocks.is_empty() || self.is_hidden(line) {
            return 0;
        }
        self.entry(line).map_or(0, BlockEntry::rows)
    }

    /// The blocks, ascending by line.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter().map(|entry| &entry.block)
    }

    /// Show `blocks` above their lines: lines past the last clamp to it, and
    /// blocks on one line join in the order given. Returns whether the blocks
    /// changed.
    pub fn set_blocks(&mut self, blocks: Vec<Block>) -> bool {
        let entries = self.normalized(
            blocks
                .into_iter()
                .map(|block| BlockEntry::new(block, self.width))
                .collect(),
        );
        if entries.iter().map(|entry| &entry.block).eq(self.blocks()) {
            return false;
        }
        self.blocks = entries;
        self.rebuild_rows(0);
        true
    }

    /// `entries` clamped to the lines, ascending, and one per line.
    fn normalized(&self, mut entries: Vec<BlockEntry>) -> Vec<BlockEntry> {
        let last = self.line_count.saturating_sub(1);
        for entry in &mut entries {
            entry.block.line = entry.block.line.min(last);
        }
        entries.retain(|entry| !entry.block.lines.is_empty());
        entries.sort_by_key(|entry| entry.block.line);
        let mut merged: Vec<BlockEntry> = Vec::new();
        for entry in entries {
            match merged.last_mut() {
                Some(last) if last.block.line == entry.block.line => {
                    last.block.lines.extend(entry.block.lines);
                    last.wraps.extend(entry.wraps);
                }
                _ => merged.push(entry),
            }
        }
        merged
    }

    /// Move the blocks with the lines they sit above when `change` took
    /// `old_count` lines to `line_count`: a block above a replaced line stays
    /// at the start of the replacement.
    fn follow_blocks(&mut self, old_count: usize, change: Option<LineChange>) {
        if self.blocks.is_empty() {
            return;
        }
        let line_count = self.line_count;
        let change = change.filter(|change| {
            change.old_end <= old_count
                && change.new_end <= line_count
                && old_count - change.old_end == line_count - change.new_end
        });
        if let Some(change) = change {
            for entry in &mut self.blocks {
                let line = &mut entry.block.line;
                if *line >= change.old_end {
                    *line = *line + change.new_end - change.old_end;
                } else if *line > change.first {
                    *line = change.first;
                }
            }
        }
        let entries = std::mem::take(&mut self.blocks);
        self.blocks = self.normalized(entries);
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
        // Hidden lines and blocks are not tracked by the lines that changed.
        let first = if self.hidden.is_empty() && self.blocks.is_empty() {
            first
        } else {
            0
        };
        self.first_rows.resize(self.line_count + 1, 0);
        for line in first..self.line_count {
            let rows = if self.is_hidden(line) {
                0
            } else {
                self.block_rows(line) + self.lines.get(line).map_or(0, |wrap| wrap.breaks.len()) + 1
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

    /// The first row of `line`'s text, below its block.
    pub fn first_row(&self, line: usize) -> usize {
        if self.mapped() {
            let line = line.min(self.line_count);
            self.first_rows[line] + self.block_rows(line)
        } else {
            line
        }
    }

    /// Where `line`'s rows begin, its block's first.
    fn group_start(&self, line: usize) -> usize {
        if self.mapped() {
            self.first_rows[line.min(self.line_count)]
        } else {
            line
        }
    }

    /// The rows of `line`'s text.
    pub fn rows(&self, line: usize) -> Range<usize> {
        self.first_row(line)..self.group_start(line + 1)
    }

    /// The line `row` belongs to and its index among that line's text rows.
    /// A block row reports the first row of the line below it.
    pub fn position(&self, row: usize) -> (usize, usize) {
        let row = row.min(self.row_count().saturating_sub(1));
        if !self.mapped() {
            return (row, 0);
        }
        let line = self.first_rows[..self.line_count].partition_point(|first| *first <= row) - 1;
        (line, row.saturating_sub(self.first_row(line)))
    }

    /// The part of a block that `row` shows, or `None` for a row of text.
    pub fn block_row(&self, row: usize) -> Option<BlockRow<'_>> {
        if self.blocks.is_empty() || row >= self.row_count() {
            return None;
        }
        let (line, _) = self.position(row);
        let mut index = row.checked_sub(self.group_start(line))?;
        let entry = self.entry(line).filter(|_| index < self.block_rows(line))?;
        for (block_line, wrap) in entry.block.lines.iter().zip(&entry.wraps) {
            let rows = wrap.breaks.len() + 1;
            if index < rows {
                let start = if index == 0 {
                    0
                } else {
                    wrap.breaks[index - 1]
                };
                return Some(BlockRow {
                    line,
                    block_line,
                    range: start
                        ..wrap
                            .breaks
                            .get(index)
                            .copied()
                            .unwrap_or(block_line.text.len()),
                    indent: if index == 0 { 0 } else { wrap.indent },
                });
            }
            index -= rows;
        }
        None
    }

    /// The row of text nearest `row`: `row` itself unless it shows a block,
    /// then the first row of the line below it, or with `forward` false the
    /// last row of the line above, falling back to the first when none.
    pub fn text_row(&self, row: usize, forward: bool) -> usize {
        let row = row.min(self.row_count().saturating_sub(1));
        if self.block_row(row).is_none() {
            return row;
        }
        let (line, _) = self.position(row);
        match self.group_start(line) {
            0 => self.first_row(line),
            start if !forward => start - 1,
            _ => self.first_row(line),
        }
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
        for entry in &mut self.blocks {
            entry.rewrap(width);
        }
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
        self.follow_blocks(old_count, change);
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

    fn block(line: usize, texts: &[&str]) -> Block {
        Block {
            line,
            lines: texts
                .iter()
                .map(|text| BlockLine {
                    text: (*text).into(),
                    highlights: Vec::new(),
                    key: None,
                })
                .collect(),
        }
    }

    #[test]
    fn block_rows_sit_above_their_line_and_belong_to_none() {
        let mut map = WrapMap::new(4);
        assert!(map.set_blocks(vec![block(2, &["x", "y"]), block(0, &["z"])]));
        assert!(!map.set_blocks(vec![block(0, &["z"]), block(2, &["x", "y"])]));
        // z, l0, l1, x, y, l2, l3
        assert_eq!(map.row_count(), 7);
        assert_eq!((map.first_row(0), map.rows(0)), (1, 1..2));
        assert_eq!(map.rows(1), 2..3);
        assert_eq!((map.first_row(2), map.rows(2)), (5, 5..6));
        assert_eq!(map.rows(3), 6..7);
        assert_eq!(map.row_of(2, 0), 5);
        let positions: Vec<_> = (0..7).map(|row| map.position(row)).collect();
        assert_eq!(
            positions,
            [(0, 0), (0, 0), (1, 0), (2, 0), (2, 0), (2, 0), (3, 0)]
        );
        let shown: Vec<_> = (0..7)
            .map(|row| map.block_row(row).map(|row| row.block_line.text.as_str()))
            .collect();
        assert_eq!(
            shown,
            [Some("z"), None, None, Some("x"), Some("y"), None, None]
        );
        // A step onto a block row goes on in its direction.
        assert_eq!(map.text_row(3, true), 5);
        assert_eq!(map.text_row(4, false), 2);
        assert_eq!(map.text_row(0, false), 1);
        assert_eq!(map.text_row(2, false), 2);
        // Blocks past the last line clamp to it, and ones on a line join.
        map.set_blocks(vec![block(9, &["a"]), block(3, &["b"])]);
        assert_eq!(map.blocks().map(|b| b.line).collect::<Vec<_>>(), [3]);
        assert_eq!(map.blocks().next().unwrap().lines.len(), 2);
        map.set_blocks(Vec::new());
        assert_eq!((map.row_count(), map.rows(2)), (4, 2..3));
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn block_rows_wrap_and_hide_with_their_line() {
        let text = ["one", "two", "three", "four"];
        let width = WrapWidth {
            columns: 8,
            tab_size: 4,
        };
        let mut map = WrapMap::new(4);
        map.set_blocks(vec![block(1, &["aaa bbb ccc ddd", "e"]), block(3, &["f"])]);
        map.set_width(Some(width), 4, |line| text[line].into());
        // The first block line wraps into two rows.
        assert_eq!(map.row_count(), 4 + 3 + 1);
        let rows: Vec<_> = (1..4)
            .map(|row| map.block_row(row).map(|row| row.range))
            .collect();
        assert_eq!(rows, [Some(0..8), Some(8..15), Some(0..1)]);
        assert_eq!(map.rows(1), 4..5);
        // A fold hides the blocks above its lines.
        map.set_hidden([1..3].into());
        assert_eq!(map.row_count(), 1 + 2);
        assert_eq!((map.rows(1), map.rows(3)), (1..1, 2..3));
        assert!(map.block_row(1).is_some());
        assert_eq!(map.position(1), (3, 0));
        map.set_hidden(Vec::new());
        assert_eq!(map.row_count(), 8);
        // Narrowing the width rewraps blocks.
        map.set_width(None, 4, |line| text[line].into());
        assert_eq!(map.row_count(), 4 + 2 + 1);
    }

    #[test]
    fn blocks_follow_their_lines_through_edits() {
        let text = ["a", "b", "c", "d", "e"];
        let at = |map: &WrapMap| map.blocks().map(|block| block.line).collect::<Vec<_>>();
        let change = |first, old_end, new_end| {
            Some(LineChange {
                first,
                old_end,
                new_end,
            })
        };
        let mut map = WrapMap::new(5);
        map.set_blocks(vec![block(1, &["x"]), block(3, &["y"])]);
        // A line inserted above shifts both; one inserted in between, only
        // the lower.
        map.edit(6, change(0, 1, 2), |line| text[line.min(4)].into());
        assert_eq!(at(&map), [2, 4]);
        map.edit(7, change(3, 3, 4), |line| text[line.min(4)].into());
        assert_eq!(at(&map), [2, 5]);
        // Lines replaced under a block keep it at the replacement's start.
        map.edit(5, change(1, 3, 1), |line| text[line.min(4)].into());
        assert_eq!(at(&map), [1, 3]);
        // Merging replaced lines joins their blocks.
        map.edit(3, change(0, 4, 2), |line| text[line.min(4)].into());
        assert_eq!(at(&map), [0]);
        assert_eq!(map.blocks().next().unwrap().lines.len(), 2);
        // An edit that is not described clamps them to the text.
        map.edit(1, None, |line| text[line].into());
        assert_eq!((at(&map), map.row_count()), (vec![0], 3));
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
