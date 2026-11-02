//! Brackets in the editor: the highlight of the bracket at the caret and
//! its partner, Jump to Matching Bracket, and typing pairs. Typing an opener
//! wraps a selection or inserts its closer, and the view tracks the pairs it
//! closed while the caret stays inside them, so typing their closer steps
//! over it and Backspace between them deletes both.

use super::*;

/// A pair auto-closing inserted, anchored from the opener's start to the
/// closer's end.
pub(super) struct AutoClosed {
    range: Option<AnchoredRangeId>,
    open: String,
    close: String,
}

impl AutoClosed {
    /// The opener's and closer's ranges, while the text still holds them.
    fn resolve(&self, model: &BufferModel) -> Option<(Range<usize>, Range<usize>)> {
        let range = model.resolve_view_position(self.range)?;
        if range.len() < self.open.len() + self.close.len() {
            return None;
        }
        let open = range.start..range.start + self.open.len();
        let close = range.end - self.close.len()..range.end;
        let holds = |range: &Range<usize>, text: &str| {
            model.is_char_boundary(range.start)
                && model.is_char_boundary(range.end)
                && model.read(range.clone()) == text
        };
        (holds(&open, &self.open) && holds(&close, &self.close)).then_some((open, close))
    }
}

/// The brackets matched at a caret, valid while the syntax generation and
/// the caret stay the same.
pub(super) struct BracketCache {
    generation: u64,
    caret: usize,
    matched: Option<[Range<usize>; 2]>,
}

impl EditorView {
    /// The byte ranges of the bracket touching the caret and its partner, as
    /// the highlight paints them.
    pub(super) fn matched_brackets(&self, model: &BufferModel) -> Option<[Range<usize>; 2]> {
        let generation = model.syntax_generation()?;
        let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
        let mut cache = self.bracket_cache.borrow_mut();
        if let Some(cache) = cache
            .as_ref()
            .filter(|cache| cache.generation == generation && cache.caret == caret)
        {
            return cache.matched.clone();
        }
        let matched = model
            .bracket_at(caret)
            .map(|(bracket, partner)| [bracket, partner]);
        *cache = Some(BracketCache {
            generation,
            caret,
            matched: matched.clone(),
        });
        matched
    }

    /// Move the caret before the partner of the bracket it touches, or
    /// before the closer of the innermost pair around it, as VS Code's Go to
    /// Bracket does. Clears the selection.
    pub(super) fn jump_to_matching_bracket(&mut self, cx: &mut Context<Self>) {
        self.break_history_group(cx);
        let model = self.model.read(cx);
        let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
        let target = match model.bracket_at(caret) {
            // Between an adjacent pair, go before the opener.
            Some((bracket, partner)) if partner.start == caret => bracket.start.min(partner.start),
            Some((_, partner)) => partner.start,
            None => match model.enclosing_brackets(caret) {
                Some(pair) => pair.close.start,
                None => return,
            },
        };
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, target);
        self.has_selection = false;
        self.marked_range_utf16 = None;
        self.preferred_x = None;
        self.finish_position_change(cx);
    }

    /// Handle typed `text` replacing `range`, or the selection when `None`,
    /// as a bracket: wrap the selection in the pair it opens, step over a
    /// tracked closer, or insert the closer of the pair it opens. Returns
    /// whether it handled the input.
    pub(super) fn type_bracket(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let model = self.model.read(cx);
        if !model.is_editable() || text.is_empty() || text.contains(['\n', '\r']) {
            return false;
        }
        let implicit = self.implicit_replacement_range(model);
        if range.is_some_and(|range| range != implicit) {
            return false;
        }
        let scope = self.settings_scope(model, cx);
        let Some(editing) = model.editing_at(implicit.start) else {
            return false;
        };
        if !implicit.is_empty() {
            if !settings::editor_auto_surround(&scope, cx) {
                return false;
            }
            let Some(bracket) = editing.surround(text) else {
                return false;
            };
            let state = self.history_view_state(model);
            let inner = implicit.start + bracket.open.len()..implicit.end + bracket.open.len();
            let (anchor, caret) = if state.caret < state.anchor {
                (inner.end, inner.start)
            } else {
                (inner.start, inner.end)
            };
            let edits = vec![
                (implicit.start..implicit.start, bracket.open.clone()),
                (implicit.end..implicit.end, bracket.close.clone()),
            ];
            self.commit_edits(edits, anchor, caret, cx);
            return true;
        }
        if !settings::editor_auto_closing_brackets(&scope, cx) {
            return false;
        }
        let caret = implicit.start;
        if let Some(index) = self.auto_closed.iter().position(|pair| {
            pair.close == text
                && pair
                    .resolve(model)
                    .is_some_and(|(_, close)| close.start == caret)
        }) {
            let pair = self.auto_closed.remove(index);
            self.model
                .update(cx, |model, _| model.remove_view_position(pair.range));
            let model = self.model.read(cx);
            (self.cursor_line, self.cursor_col) =
                flat_byte_to_position(model, caret + pair.close.len());
            self.preferred_x = None;
            self.finish_position_change(cx);
            return true;
        }
        let line = model.line_of_offset(caret);
        let before = model.read(model.line_range(line).start..caret);
        let after = model.read(caret..content_range(model, line).end);
        let family = model.syntax_family_at(caret);
        let Some(bracket) = editing.auto_close(&before, text, &after, family) else {
            return false;
        };
        let (open, close) = (bracket.open.clone(), bracket.close.clone());
        let typed_end = caret + text.len();
        let view = HistoryViewChange {
            before: self.history_view_state(model),
            after: HistoryViewState {
                context: self.history_context,
                anchor: typed_end,
                caret: typed_end,
            },
        };
        self.preferred_x = None;
        self.splice(
            caret..caret,
            &format!("{text}{close}"),
            Some(EditGroupKind::Typing),
            view,
            cx,
        );
        self.marked_range_utf16 = None;
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(self.model.read(cx), typed_end);
        self.has_selection = false;
        self.finish_position_change(cx);
        let tracked = typed_end - open.len()..typed_end + close.len();
        let range = self
            .model
            .update(cx, |model, _| model.add_view_position(tracked));
        self.auto_closed.push(AutoClosed { range, open, close });
        true
    }

    /// The range Backspace deletes when the caret sits between a tracked
    /// opener and its closer: both of them.
    pub(super) fn auto_closed_deletion_range(&self, model: &BufferModel) -> Option<Range<usize>> {
        let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
        self.auto_closed.iter().find_map(|pair| {
            let (open, close) = pair.resolve(model)?;
            (open.end == caret && close.start == caret).then_some(open.start..close.end)
        })
    }

    /// Stop tracking the pairs the caret has left, whose text changed, or
    /// all of them while text is selected.
    pub(super) fn prune_auto_closed(&mut self, cx: &mut Context<Self>) {
        if self.auto_closed.is_empty() {
            return;
        }
        let model = self.model.read(cx);
        let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
        let selected = self.has_selection;
        let (kept, dropped): (Vec<_>, Vec<_>) = std::mem::take(&mut self.auto_closed)
            .into_iter()
            .partition(|pair| {
                !selected
                    && pair
                        .resolve(model)
                        .is_some_and(|(open, close)| open.end <= caret && caret <= close.start)
            });
        self.auto_closed = kept;
        self.release_auto_closed(dropped, cx);
    }

    pub(super) fn release_auto_closed(&self, pairs: Vec<AutoClosed>, cx: &mut App) {
        if pairs.is_empty() {
            return;
        }
        self.model.update(cx, |model, _| {
            for pair in pairs {
                model.remove_view_position(pair.range);
            }
        });
    }
}
