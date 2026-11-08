//! Save hygiene: trimming trailing whitespace and ending the file with a line
//! terminator, as one undoable edit.

use super::*;

impl Editor {
    /// Clean the text up for a save as the settings of this editor's document
    /// ask. The line of the cursor keeps its trailing whitespace, so saving
    /// mid-line never eats indentation just typed.
    pub(crate) fn clean_up_for_save(&mut self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let scope = self.settings_scope(model, cx);
        let mut edits = if settings::files_trim_trailing_whitespace(&scope, cx) {
            trailing_whitespace_edits(model, Some(self.cursor_line))
        } else {
            Vec::new()
        };
        if settings::files_insert_final_newline(&scope, cx) {
            add_final_newline(model, &mut edits);
        }
        self.commit_cleanup(edits, cx);
    }

    /// Remove the spaces and tabs that end every line, the cursor's included.
    pub(super) fn trim_trailing_whitespace(&mut self, cx: &mut Context<Self>) {
        let edits = trailing_whitespace_edits(self.model.read(cx), None);
        self.commit_cleanup(edits, cx);
    }

    /// Commit the cleanup as one undo step, keeping the selection on the same
    /// text. An offset at an insertion stays before it, so a caret at the end
    /// of the text does not move to a new line.
    fn commit_cleanup(&mut self, edits: Vec<(Range<usize>, String)>, cx: &mut Context<Self>) {
        let state = self.history_view_state(self.model.read(cx));
        let (anchor, caret) = (
            map_offset(&edits, state.anchor, true),
            map_offset(&edits, state.caret, true),
        );
        self.commit_edits(edits, anchor, caret, cx);
    }
}

/// Deletions of the spaces and tabs ending each line except `skip`.
fn trailing_whitespace_edits(
    model: &BufferModel,
    skip: Option<usize>,
) -> Vec<(Range<usize>, String)> {
    (0..model.line_count())
        .filter(|line| Some(*line) != skip)
        .filter_map(|line| {
            let content = content_range(model, line);
            let text = model.read(content.clone());
            let kept = text.trim_end_matches([' ', '\t']).len();
            (kept < text.len()).then(|| (content.start + kept..content.end, String::new()))
        })
        .collect()
}

/// Make `edits` end a non-empty text with the buffer's line terminator, unless
/// the text already ends with one or the edits leave it empty.
fn add_final_newline(model: &BufferModel, edits: &mut Vec<(Range<usize>, String)>) {
    let end = model.len();
    let trims_last = edits.last().is_some_and(|(range, _)| range.end == end);
    let last = line_text(model, model.line_count() - 1);
    let last_empty =
        last.is_empty() || (trims_last && last.trim_end_matches([' ', '\t']).is_empty());
    if end == 0 || last_empty {
        return;
    }
    let ending = model.line_ending().to_owned();
    match edits.last_mut() {
        // The last line's trimmed tail becomes the terminator.
        Some((range, text)) if range.end == end => *text = ending,
        _ => edits.push((end..end, ending)),
    }
}
