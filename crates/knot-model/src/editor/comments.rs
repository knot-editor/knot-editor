//! Comment toggling with the editing data of the language at the selection
//! start, so a code fence comments in its own language.

use super::*;

impl EditorState {
    /// Toggle line comments on the selected lines.
    pub(crate) fn toggle_line_comment(&mut self, buffer: &mut BufferModel) {
        let model = &*buffer;
        let Some(editing) = model.editing_at(self.selection_start(model)) else {
            return;
        };
        let lines = self.selected_lines();
        let texts: Vec<String> = lines.clone().map(|line| line_text(model, line)).collect();
        let borrowed: Vec<&str> = texts.iter().map(String::as_str).collect();
        let edits = editing
            .toggle_comments(&borrowed)
            .into_iter()
            .map(|edit| {
                let line = lines.start + edit.line;
                (
                    to_flat_byte(model, line, edit.range.start)
                        ..to_flat_byte(model, line, edit.range.end),
                    edit.text,
                )
            })
            .collect();
        self.commit_line_edits(edits, buffer);
    }

    /// Wrap the selection in a block comment or unwrap it, keeping the same
    /// text selected. With nothing selected, insert an empty comment with
    /// the caret inside.
    pub(crate) fn toggle_block_comment(&mut self, buffer: &mut BufferModel) {
        let model = &*buffer;
        let state = self.history_view_state(model);
        let range = state.anchor.min(state.caret)..state.anchor.max(state.caret);
        let Some(editing) = model.editing_at(range.start) else {
            return;
        };
        let text = model.read(range.clone());
        // A selection just inside a comment, as wrapping leaves it, unwraps.
        let toggled = editing
            .block_comment
            .as_ref()
            .and_then(|(open, close)| surrounding(model, &range, open, close))
            .unwrap_or_else(|| range.clone());
        let edits: Vec<_> = editing
            .toggle_block_comment(&model.read(toggled.clone()))
            .into_iter()
            .map(|(edit, text)| (toggled.start + edit.start..toggled.start + edit.end, text))
            .collect();
        if edits.is_empty() {
            return;
        }
        let (start, end) = if toggled == range && text.trim().is_empty() {
            let open = editing
                .block_comment
                .as_ref()
                .map_or(0, |(open, _)| open.len());
            let inside = range.start + open + 1;
            (inside, inside)
        } else {
            (
                map_offset(&edits, range.start, false),
                map_offset(&edits, range.end, true),
            )
        };
        let (anchor, caret) = if state.caret < state.anchor {
            (end, start)
        } else {
            (start, end)
        };
        self.commit_edits(edits, anchor, caret, buffer);
    }

    fn selection_start(&self, model: &BufferModel) -> usize {
        let state = self.history_view_state(model);
        state.anchor.min(state.caret)
    }
}

/// `range` widened over `open` and `close` when they enclose it, each with
/// at most one space inside.
fn surrounding(
    model: &BufferModel,
    range: &Range<usize>,
    open: &str,
    close: &str,
) -> Option<Range<usize>> {
    let start = [format!("{open} "), open.to_owned()]
        .into_iter()
        .find_map(|token| {
            let start = range.start.checked_sub(token.len())?;
            (model.is_char_boundary(start) && model.read(start..range.start) == token)
                .then_some(start)
        })?;
    let end = [format!(" {close}"), close.to_owned()]
        .into_iter()
        .find_map(|token| {
            let end = range.end + token.len();
            (end <= model.len()
                && model.is_char_boundary(end)
                && model.read(range.end..end) == token)
                .then_some(end)
        })?;
    Some(start..end)
}
