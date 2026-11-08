//! Typed text and IME composition.

use super::*;

impl EditorState {
    /// The selection as document-wide UTF-16 offsets, a caret-only range
    /// without one. `reversed` when the caret is before the anchor.
    pub fn selected_utf16(&self, model: &BufferModel) -> (Range<usize>, bool) {
        let caret = to_flat_utf16(model, self.cursor_line, self.cursor_col);
        if !self.has_selection {
            return (caret..caret, false);
        }
        let anchor = to_flat_utf16(model, self.anchor_line, self.anchor_col);
        if anchor <= caret {
            (anchor..caret, false)
        } else {
            (caret..anchor, true)
        }
    }

    /// Commit the preedit marking without changing its text or selection.
    pub fn unmark_text(&mut self, buffer: &mut BufferModel) {
        if self.marked_range_utf16.take().is_some() {
            self.break_history_group(buffer);
        }
    }

    /// Insert typed or committed `text` at a UTF-16 range, or over the
    /// marked text, selection, or caret: a composition commits in its own
    /// undo group, while typing groups with typing and may close a bracket
    /// or outdent the line.
    pub fn type_text(&mut self, range: Option<Range<usize>>, text: &str, buffer: &mut BufferModel) {
        let grouping = if self.marked_range_utf16.is_some() {
            EditGroupKind::Composition
        } else {
            EditGroupKind::Typing
        };
        let range = self.bytes_for_utf16(range, buffer);
        let bracket =
            grouping == EditGroupKind::Typing && self.type_bracket(range.clone(), text, buffer);
        if !bracket {
            self.replace_text_with_grouping(range, text, Some(grouping), buffer);
        }
        if grouping == EditGroupKind::Typing && !text.contains(['\n', '\r']) {
            self.outdent_typed_line(buffer);
        }
    }

    /// Replace a UTF-16 range, or the marked text, selection, or caret,
    /// with composing `new_text` and mark it; `new_selected_range`, relative
    /// to the marked text, becomes the selection. Nothing changes in a
    /// read-only buffer.
    pub fn mark_text(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        buffer: &mut BufferModel,
    ) {
        if !buffer.is_editable() {
            return;
        }
        self.preferred_column = None;
        let range = self.bytes_for_utf16(range, buffer);
        let model = &*buffer;
        let range = range.unwrap_or_else(|| self.implicit_replacement_range(model));
        let marked_start_utf16 = model.utf16_of_offset(range.start);
        let marked_utf16_len: usize = new_text.chars().map(char::len_utf16).sum();
        let selected = new_selected_range.unwrap_or(marked_utf16_len..marked_utf16_len);
        let anchor = range.start + utf16_to_byte_col(new_text, selected.start);
        let caret = range.start + utf16_to_byte_col(new_text, selected.end);
        let before = self.history_view_state(model);

        self.splice(
            range,
            new_text,
            Some(EditGroupKind::Composition),
            HistoryViewChange {
                before,
                after: HistoryViewState {
                    context: self.history_context,
                    anchor,
                    caret,
                },
            },
            buffer,
        );

        self.marked_range_utf16 = Some(marked_start_utf16..marked_start_utf16 + marked_utf16_len);
        let model = &*buffer;
        (self.anchor_line, self.anchor_col) = flat_byte_to_position(model, anchor);
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, caret);
        self.has_selection = anchor != caret;

        self.finish_position_change(buffer);
    }
}

/// The `unit` containing the caret position `(line, col)`, as start and end
/// positions: the position itself, the word segment at it (see
/// [`word_segments`]), or the whole line
/// with its terminator.
pub fn unit_range(
    model: &BufferModel,
    unit: SelectionUnit,
    line: usize,
    col: usize,
) -> ((usize, usize), (usize, usize)) {
    match unit {
        SelectionUnit::Character => ((line, col), (line, col)),
        SelectionUnit::Word => {
            let text = line_text(model, line);
            let (start, end) = word_segments(&text)
                .into_iter()
                .find(|(_, end, _)| col < *end)
                .map_or((text.len(), text.len()), |(start, end, _)| (start, end));
            ((line, start), (line, end))
        }
        SelectionUnit::Line if line + 1 < model.line_count() => ((line, 0), (line + 1, 0)),
        SelectionUnit::Line => ((line, 0), (line, line_end(model, line))),
    }
}
