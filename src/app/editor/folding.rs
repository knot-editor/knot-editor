//! Folding: collapsed line regions. A collapsed fold keeps its first line
//! visible, with a placeholder after it, and hides the lines through its last
//! from rows, painting, and the caret.
//!
//! A view holds its collapsed folds as anchored byte ranges from the start of
//! the first line to the end of the last line's content, so edits keep a fold
//! over the same text. Folds are per view and never recorded in the buffer.

use super::*;
use std::sync::Arc;

/// One collapsed fold.
pub(super) struct Fold {
    range: Option<AnchoredRangeId>,
    /// The fold's lines, from its first through its last, as of the last
    /// refresh; `None` once an edit left fewer than two lines.
    lines: Option<Range<usize>>,
}

impl Editor {
    /// The collapsed folds as line ranges, the first line to one past the
    /// last, ordered by first line and then outermost first.
    pub(crate) fn folds(&self) -> Vec<Range<usize>> {
        let mut folds: Vec<Range<usize>> = self
            .folds
            .iter()
            .filter_map(|fold| fold.lines.clone())
            .collect();
        folds.sort_by_key(|lines| (lines.start, std::cmp::Reverse(lines.end)));
        folds
    }

    /// The collapsed folds as byte ranges, from the start of the first line to
    /// the end of the last line's content, ordered like [`Self::folds`].
    pub(super) fn fold_bytes(&self, model: &BufferModel) -> Vec<Range<usize>> {
        let mut bytes: Vec<Range<usize>> = self
            .folds
            .iter()
            .filter(|fold| fold.lines.is_some())
            .filter_map(|fold| model.resolve_view_position(fold.range))
            .collect();
        bytes.sort_by_key(|range| (range.start, std::cmp::Reverse(range.end)));
        bytes
    }

    /// Replace the collapsed folds with those over the byte ranges `bytes`,
    /// clamped to the text; ranges left within one line are dropped.
    pub(super) fn restore_folds(&mut self, bytes: &[Range<usize>], cx: &mut Context<Self>) {
        self.unfold_all_folds(cx);
        let model = self.model.read(cx);
        let line = |offset: usize| {
            let mut offset = offset.min(model.len());
            while !model.is_char_boundary(offset) {
                offset -= 1;
            }
            flat_byte_to_position(model, offset).0
        };
        let lines: Vec<Range<usize>> = bytes
            .iter()
            .map(|range| line(range.start)..line(range.end) + 1)
            .collect();
        self.fold_all_of(&lines, cx);
    }

    /// Whether `line` is the first line of a collapsed fold.
    pub(super) fn is_fold_start(&self, line: usize) -> bool {
        self.folds
            .iter()
            .any(|fold| fold.lines.as_ref().is_some_and(|lines| lines.start == line))
    }

    /// Resolve every fold against `model` and hide their lines.
    pub(super) fn sync_folds(&mut self, model: &BufferModel) {
        if self.folds.is_empty() {
            return;
        }
        for fold in &mut self.folds {
            fold.lines = model
                .resolve_view_position(fold.range)
                .map(|range| {
                    let first = flat_byte_to_position(model, range.start).0;
                    let last = flat_byte_to_position(model, range.end).0;
                    first..last + 1
                })
                .filter(|lines| lines.len() > 1);
        }
        let mut hidden: Vec<Range<usize>> = self
            .folds
            .iter()
            .filter_map(|fold| fold.lines.as_ref())
            .map(|lines| lines.start + 1..lines.end)
            .collect();
        hidden.sort_by_key(|lines| lines.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for lines in hidden {
            match merged.last_mut() {
                Some(last) if lines.start <= last.end => last.end = last.end.max(lines.end),
                _ => merged.push(lines),
            }
        }
        self.wrap.set_hidden(merged);
    }

    /// Collapse the lines `lines`, from the first through one before the end,
    /// keeping the first visible. Does nothing for a fold already collapsed.
    /// Returns whether `lines` is a range of at least two lines in the text.
    pub(crate) fn fold(&mut self, lines: Range<usize>, cx: &mut Context<Self>) -> bool {
        let valid = lines.len() >= 2 && lines.end <= self.model.read(cx).line_count();
        if valid {
            self.fold_all_of(&[lines], cx);
        }
        valid
    }

    /// Collapse every range of `all` that is a valid, uncollapsed fold, and
    /// hide their lines once.
    fn fold_all_of(&mut self, all: &[Range<usize>], cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let top = self.top_offset(model);
        let existing = self.folds();
        let new: Vec<Range<usize>> = all
            .iter()
            .filter(|lines| {
                lines.len() >= 2 && lines.end <= model.line_count() && !existing.contains(lines)
            })
            .cloned()
            .collect();
        if new.is_empty() {
            return;
        }
        let anchors: Vec<_> = self.model.update(cx, |model, _| {
            new.into_iter()
                .map(|lines| {
                    let start = to_flat_byte(model, lines.start, 0);
                    let last = lines.end - 1;
                    let end = to_flat_byte(model, last, line_end(model, last));
                    (model.add_view_position(start..end), lines)
                })
                .collect()
        });
        for (range, lines) in anchors {
            self.folds.push(Fold {
                range,
                lines: Some(lines),
            });
        }
        self.after_fold_change(top, cx);
        // The caret and selection leave the lines the folds hide.
        let model = self.model.read(cx);
        if self.wrap.is_hidden(self.cursor_line) || self.wrap.is_hidden(self.anchor_line) {
            let line = self
                .wrap
                .hidden_range(self.cursor_line)
                .map_or(self.cursor_line, |hidden| hidden.start - 1);
            self.cursor_line = line;
            self.cursor_col = self.cursor_col.min(line_end(model, line));
            self.has_selection = false;
            self.preferred_column = None;
            self.finish_position_change(cx);
        }
    }

    /// The regions the language's `folds` query or, without one, the text's
    /// indentation offers, as line ranges ordered by first line.
    pub(crate) fn fold_regions(&self, cx: &App) -> Arc<[Range<usize>]> {
        self.model.read(cx).fold_regions()
    }

    /// The uncollapsed region the Fold command collapses: the innermost one
    /// that starts on the caret's line or encloses it.
    pub(super) fn fold_at_caret(&mut self, cx: &mut Context<Self>) {
        let line = self.cursor_line;
        let collapsed = self.folds();
        let region = self
            .fold_regions(cx)
            .iter()
            .filter(|lines| lines.start <= line && line < lines.end && !collapsed.contains(lines))
            .max_by_key(|lines| (lines.start, std::cmp::Reverse(lines.end)))
            .cloned();
        if let Some(region) = region {
            self.fold_all_of(&[region], cx);
        }
    }

    /// Expand the outermost fold collapsed on the caret's line.
    pub(super) fn unfold_at_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let line = self.cursor_line;
        let Some(lines) = self.folds().into_iter().find(|lines| lines.start == line) else {
            return false;
        };
        self.unfold(&lines, cx);
        true
    }

    /// Toggle the fold that starts on `line`: expand the outermost collapsed
    /// one, else collapse the region starting there. `recursive` also
    /// expands or collapses every fold or region nested inside.
    pub(super) fn toggle_fold_at_line(
        &mut self,
        line: usize,
        recursive: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(lines) = self.folds().into_iter().find(|lines| lines.start == line) {
            if recursive {
                self.unfold_where(cx, |fold| {
                    lines.start <= fold.start && fold.end <= lines.end
                });
            } else {
                self.unfold(&lines, cx);
            }
            return;
        }
        let regions = self.fold_regions(cx);
        let Some(region) = regions.iter().find(|lines| lines.start == line) else {
            return;
        };
        if recursive {
            let nested: Vec<_> = regions
                .iter()
                .filter(|lines| region.start <= lines.start && lines.end <= region.end)
                .cloned()
                .collect();
            self.fold_all_of(&nested, cx);
        } else {
            self.fold_all_of(std::slice::from_ref(region), cx);
        }
    }

    /// The foldable lines among `lines` that are not hidden: each with whether
    /// it starts a collapsed fold, in line order.
    pub(super) fn fold_marks(&self, lines: Range<usize>, cx: &App) -> Vec<(usize, bool)> {
        let regions = self.fold_regions(cx);
        let first = regions.partition_point(|region| region.start < lines.start);
        let mut marks: Vec<(usize, bool)> = regions[first..]
            .iter()
            .take_while(|region| region.start < lines.end)
            .map(|region| (region.start, self.is_fold_start(region.start)))
            .collect();
        // Manual folds have no region behind them.
        for fold in self.folds() {
            if lines.contains(&fold.start) && !marks.iter().any(|(line, _)| *line == fold.start) {
                marks.push((fold.start, true));
            }
        }
        marks.sort_by_key(|(line, _)| *line);
        marks.dedup_by_key(|(line, _)| *line);
        marks.retain(|(line, _)| !self.wrap.is_hidden(*line));
        marks
    }

    /// Collapse every region of the text.
    pub(super) fn fold_all_regions(&mut self, cx: &mut Context<Self>) {
        let regions = self.fold_regions(cx);
        self.fold_all_of(&regions, cx);
    }

    /// Expand every fold.
    pub(super) fn unfold_all_folds(&mut self, cx: &mut Context<Self>) {
        self.unfold_where(cx, |_| true);
    }

    /// Expand the collapsed fold over exactly `lines`, if there is one.
    pub(crate) fn unfold(&mut self, lines: &Range<usize>, cx: &mut Context<Self>) {
        self.unfold_where(cx, |fold| fold == lines);
    }

    /// Expand the folds hiding the caret's line, so the caret shows.
    pub(super) fn reveal_caret_folds(&mut self, cx: &mut Context<Self>) {
        let line = self.cursor_line;
        if self.wrap.is_hidden(line) {
            self.unfold_where(cx, |fold| fold.start < line && line < fold.end);
        }
    }

    /// Remove the folds `remove` selects, and any whose text is gone.
    fn unfold_where(&mut self, cx: &mut Context<Self>, remove: impl Fn(&Range<usize>) -> bool) {
        let top = self.top_offset(self.model.read(cx));
        let (gone, kept): (Vec<Fold>, Vec<Fold>) = std::mem::take(&mut self.folds)
            .into_iter()
            .partition(|fold| fold.lines.as_ref().is_none_or(&remove));
        self.folds = kept;
        if gone.is_empty() {
            return;
        }
        self.model.update(cx, |model, _| {
            for fold in gone {
                model.remove_view_position(fold.range);
            }
        });
        self.after_fold_change(top, cx);
    }

    /// Rehide the lines after the folds changed, keeping the text that was at
    /// the top of the viewport (`top`, a byte offset) there.
    fn after_fold_change(&mut self, top: usize, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        if self.folds.is_empty() {
            self.wrap.set_hidden(Vec::new());
        } else {
            self.sync_folds(model);
        }
        let (line, col) = flat_byte_to_position(model, top);
        self.scroll_top = self.wrap.row_of(line, col) as f32;
        self.clamp_scroll();
        cx.notify();
    }

    /// Release the anchors of every fold.
    pub(super) fn release_folds(&mut self, cx: &mut App) {
        let folds = std::mem::take(&mut self.folds);
        self.model.update(cx, |model, _| {
            for fold in folds {
                model.remove_view_position(fold.range);
            }
        });
    }

    /// `position` moved off the lines a fold hides: to the start of the line
    /// after them when moving `forward`, or the end of the fold's first line;
    /// the end of the fold's first line also when the text ends inside them.
    pub(super) fn out_of_folds(
        &self,
        model: &BufferModel,
        position: (usize, usize),
        forward: bool,
    ) -> (usize, usize) {
        let Some(hidden) = self.wrap.hidden_range(position.0) else {
            return position;
        };
        if forward && hidden.end < model.line_count() {
            (hidden.end, 0)
        } else {
            let first = hidden.start - 1;
            (first, line_end(model, first))
        }
    }

    /// The buffer line a vertical move by lines from `line` reaches in the
    /// direction, stepping over the lines a fold hides; `line` itself when
    /// the move would end inside them.
    pub(super) fn step_visible_line(&self, line: usize, target: usize, forward: bool) -> usize {
        match self.wrap.hidden_range(target) {
            Some(hidden) if forward => {
                if hidden.end < self.line_count {
                    hidden.end
                } else {
                    line
                }
            }
            Some(hidden) => hidden.start - 1,
            None => target,
        }
    }
}
