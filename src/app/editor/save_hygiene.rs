//! Save hygiene: trimming trailing whitespace and ending the file with a line
//! terminator, as one undoable edit.

use super::*;

impl Editor {
    /// Clean the text up for a save as the settings of this editor's document
    /// ask. The line of the cursor keeps its trailing whitespace, so saving
    /// mid-line never eats indentation just typed.
    pub(crate) fn clean_up_for_save(&mut self, cx: &mut Context<Self>) {
        let scope = self.settings_scope(self.model.read(cx), cx);
        let trim = settings::files_trim_trailing_whitespace(&scope, cx);
        let final_newline = settings::files_insert_final_newline(&scope, cx);
        self.clean_up(trim, final_newline, true, cx);
    }

    /// Remove the spaces and tabs that end lines when `trim`, and end a
    /// non-empty text with the buffer's line terminator when `final_newline`,
    /// as one undo step. The cursor's line is left alone when `skip_cursor_line`.
    /// The selection stays on the same text.
    pub(super) fn clean_up(
        &mut self,
        trim: bool,
        final_newline: bool,
        skip_cursor_line: bool,
        cx: &mut Context<Self>,
    ) {
        let model = self.model.read(cx);
        if !model.is_editable() {
            return;
        }
        let mut edits = Vec::new();
        if trim {
            for line in 0..model.line_count() {
                if skip_cursor_line && line == self.cursor_line {
                    continue;
                }
                let content = content_range(model, line);
                let text = model.read(content.clone());
                let kept = text.trim_end_matches([' ', '\t']).len();
                if kept < text.len() {
                    edits.push((content.start + kept..content.end, String::new()));
                }
            }
        }
        if final_newline && model.len() > 0 {
            let trimmed_last = edits
                .last()
                .is_some_and(|(range, _)| range.end == model.len());
            let last = line_text(model, model.line_count() - 1);
            let last_empty =
                last.is_empty() || (trimmed_last && last.trim_matches([' ', '\t']).is_empty());
            if !(last_empty && model.line_count() > 1) {
                let ending = model.line_ending().to_owned();
                match edits.last_mut() {
                    // The last line's trimmed tail becomes the terminator.
                    Some((range, text)) if range.end == model.len() => *text = ending,
                    _ => edits.push((model.len()..model.len(), ending)),
                }
            }
        }
        let state = self.history_view_state(model);
        // An offset at an insertion stays before it, so a caret at the end
        // of the text does not move to a new line.
        let (anchor, caret) = (
            map_offset(&edits, state.anchor, true),
            map_offset(&edits, state.caret, true),
        );
        self.commit_edits(edits, anchor, caret, cx);
    }
}
