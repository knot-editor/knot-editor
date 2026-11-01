//! Indentation commands: Enter keeps the indentation, Tab inserts or indents,
//! lines indent and outdent by one unit, and Backspace in leading spaces
//! deletes back to the previous tab stop.

use super::*;

impl EditorView {
    /// Replace the selection with a line break followed by the indentation
    /// before it on its line.
    pub(super) fn insert_newline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let range = self.implicit_replacement_range(model);
        let text = format!(
            "\n{}",
            leading_whitespace(&text_before_on_line(model, range.start))
        );
        self.break_history_group(cx);
        self.replace_text_with_grouping(Some(range), &text, None, window, cx);
    }

    /// Indent the selected lines when the selection spans lines; otherwise
    /// replace the selection with one indentation unit.
    pub(super) fn insert_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .selection_range()
            .is_some_and(|(start, end)| start.0 != end.0)
        {
            self.indent_lines(cx);
            return;
        }
        let model = self.model.read(cx);
        let range = self.implicit_replacement_range(model);
        let text = self
            .indent_unit(model, cx)
            .insertion(&text_before_on_line(model, range.start));
        self.break_history_group(cx);
        self.replace_text_with_grouping(Some(range), &text, None, window, cx);
    }

    /// Indent each selected line by one unit, skipping empty lines when there
    /// are several.
    pub(super) fn indent_lines(&mut self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let unit = self.indent_unit(model, cx);
        let lines = self.selected_lines();
        let several = lines.len() > 1;
        let edits = lines
            .filter_map(|line| {
                let text = line_text(model, line);
                if several && text.is_empty() {
                    return None;
                }
                let (col, indent) = unit.indent(&text);
                let at = to_flat_byte(model, line, col);
                Some((at..at, indent))
            })
            .collect();
        self.commit_line_edits(edits, cx);
    }

    /// Outdent each selected line by one unit.
    pub(super) fn outdent_lines(&mut self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let unit = self.indent_unit(model, cx);
        let edits = self
            .selected_lines()
            .filter_map(|line| {
                let range = unit.outdent(&line_text(model, line));
                (!range.is_empty()).then(|| {
                    (
                        to_flat_byte(model, line, range.start)
                            ..to_flat_byte(model, line, range.end),
                        String::new(),
                    )
                })
            })
            .collect();
        self.commit_line_edits(edits, cx);
    }

    /// The range Backspace deletes when only spaces precede the caret on its
    /// line: back to the previous tab stop.
    pub(super) fn tab_stop_deletion_range(
        &self,
        model: &BufferModel,
        cx: &App,
    ) -> Option<Range<usize>> {
        let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
        self.indent_unit(model, cx)
            .backspace(&text_before_on_line(model, caret))
            .map(|length| caret - length..caret)
    }

    /// Commit edits to whole lines, keeping the selection on the same text. A
    /// selection that starts at a line start keeps starting there, so it
    /// still covers the lines' new indentation.
    fn commit_line_edits(&mut self, edits: Vec<(Range<usize>, String)>, cx: &mut Context<Self>) {
        let state = self.history_view_state(self.model.read(cx));
        let start = state.anchor.min(state.caret);
        let start_stays = self
            .selection_range()
            .is_some_and(|(start, _)| start.1 == 0);
        let map = |offset| map_offset(&edits, offset, start_stays && offset == start);
        let (anchor, caret) = (map(state.anchor), map(state.caret));
        self.commit_edits(edits, anchor, caret, cx);
    }

    /// The lines a line command acts on: those the selection touches, except
    /// a last line it reaches only at column 0, or else the caret's line.
    fn selected_lines(&self) -> Range<usize> {
        match self.selection_range() {
            Some((start, end)) if end.0 > start.0 && end.1 == 0 => start.0..end.0,
            Some((start, end)) => start.0..end.0 + 1,
            None => self.cursor_line..self.cursor_line + 1,
        }
    }
}

/// The text from the start of `offset`'s line up to `offset`.
fn text_before_on_line(model: &BufferModel, offset: usize) -> String {
    model.read(model.line_range(model.line_of_offset(offset)).start..offset)
}
