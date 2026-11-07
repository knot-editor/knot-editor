//! Folding: collapsed line regions. A collapsed fold keeps its first line
//! visible, with a placeholder after it, and hides the lines through its last
//! from rows, painting, and the caret.
//!
//! A view holds its collapsed folds as anchored byte ranges from the start of
//! the first line to the end of the last line's content, so edits keep a fold
//! over the same text. Folds are per view and never recorded in the buffer.

use super::*;

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
        let model = self.model.read(cx);
        if lines.len() < 2 || lines.end > model.line_count() {
            return false;
        }
        if self.folds().contains(&lines) {
            return true;
        }
        let top = self.top_offset(model);
        let start = to_flat_byte(model, lines.start, 0);
        let last = lines.end - 1;
        let end = to_flat_byte(model, last, line_end(model, last));
        let range = self
            .model
            .update(cx, |model, _| model.add_view_position(start..end));
        self.folds.push(Fold {
            range,
            lines: Some(lines.clone()),
        });
        self.after_fold_change(top, cx);
        // The caret and selection leave the lines the fold hides.
        let model = self.model.read(cx);
        if self.wrap.is_hidden(self.cursor_line) || self.wrap.is_hidden(self.anchor_line) {
            self.cursor_line = lines.start;
            self.cursor_col = self.cursor_col.min(line_end(model, lines.start));
            self.has_selection = false;
            self.preferred_column = None;
            self.finish_position_change(cx);
        }
        true
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
