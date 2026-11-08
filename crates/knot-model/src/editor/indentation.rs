//! Indentation commands: Enter indents from the language's indent query or
//! keeps the indentation, Tab inserts or indents,
//! lines indent and outdent by one unit or reindent from the language's
//! indent query, and Backspace in leading spaces deletes back to the previous
//! tab stop.

use super::*;
use crate::syntax::indent::LineIndent;

/// A line whose indentation waits for the indent query, valid while the
/// buffer stays at `revision`.
pub struct PendingIndent {
    revision: u64,
    line: usize,
    /// For typing, the caret column the typed text reached: the line only
    /// outdents, and only when the typing completed a closing node.
    typed: Option<usize>,
}

impl EditorState {
    /// Replace the selection with a line break followed by the indentation
    /// before it on its line, then let the indent query correct the new
    /// line once the tree covers the edit. Between a bracket pair that opens
    /// lines, the break opens an indented middle line with the closer below
    /// it. A line holding only whitespace keeps none of it.
    pub(crate) fn insert_newline(&mut self, buffer: &mut BufferModel) {
        let model = &*buffer;
        if !model.is_editable() {
            return;
        }
        let range = self.implicit_replacement_range(model);
        let line_start = model.line_range(model.line_of_offset(range.start)).start;
        let line_end = content_range(model, model.line_of_offset(range.end)).end;
        let before = model.read(line_start..range.start);
        let after = model.read(range.end..line_end);
        let indent = leading_whitespace(&before).to_owned();
        let ending = model.line_ending().to_owned();
        let open = before.trim_end_matches([' ', '\t']);
        let close = after.trim_start_matches([' ', '\t']);
        let between_pair = model.editing_at(range.start).is_some_and(|editing| {
            editing.brackets.iter().any(|bracket| {
                bracket.newline
                    && open.ends_with(&bracket.open)
                    && close.starts_with(&bracket.close)
            })
        });
        let (range, text, caret) = if before.trim().is_empty() && after.trim().is_empty() {
            let text = format!("{ending}{indent}");
            let caret = text.len();
            (line_start..line_end, text, caret)
        } else if between_pair {
            let unit = self.indent_unit();
            let middle = indent.clone() + &unit.whitespace(unit.size);
            let text = format!("{ending}{middle}{ending}{indent}");
            let caret = ending.len() + middle.len();
            let end = range.end + (after.len() - close.len());
            (line_start + open.len()..end, text, caret)
        } else {
            let text = format!("{ending}{indent}");
            let caret = text.len();
            (range, text, caret)
        };
        let caret = range.start + caret;
        self.break_history_group(buffer);
        let view = HistoryViewChange {
            before: self.history_view_state(&*buffer),
            after: HistoryViewState {
                context: self.history_context,
                anchor: caret,
                caret,
            },
        };
        self.preferred_column = None;
        self.splice(range, &text, None, view, buffer);
        self.marked_range_utf16 = None;
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(&*buffer, caret);
        self.has_selection = false;
        self.finish_position_change(buffer);
        self.pending_indent = Some(PendingIndent {
            revision: buffer.revision(),
            line: self.cursor_line,
            typed: None,
        });
        self.apply_pending_indent(buffer);
    }

    /// After typing, outdent the caret's line when the typed text completed
    /// a node that closes or branches an indented one at the start of the
    /// line's content, such as `}` or `else`.
    pub(crate) fn outdent_typed_line(&mut self, buffer: &mut BufferModel) {
        self.pending_indent = Some(PendingIndent {
            revision: buffer.revision(),
            line: self.cursor_line,
            typed: Some(self.cursor_col),
        });
        self.apply_pending_indent(buffer);
    }

    /// Indent the line a line break or typing left for the indent query,
    /// once the tree is a parse of the text the edit produced, as part of the
    /// edit's undo step. A later edit drops the request, leaving the
    /// indentation as it was.
    pub(crate) fn apply_pending_indent(&mut self, buffer: &mut BufferModel) {
        let Some(pending) = &self.pending_indent else {
            return;
        };
        let model = &*buffer;
        if model.revision() != pending.revision || model.syntax_language().is_none() {
            self.pending_indent = None;
            return;
        }
        if !model.syntax_is_current() {
            return;
        }
        let (line, typed) = (pending.line, pending.typed);
        self.pending_indent = None;
        if typed.is_some_and(|col| !model.closes_line(line, col)) {
            return;
        }
        let unit = self.indent_unit();
        let Some(LineIndent::Columns(columns)) = model.line_indent(line, unit.size) else {
            return;
        };
        let text = line_text(model, line);
        let current = leading_whitespace(&text);
        let indent = unit.whitespace(columns);
        if indent == current || (typed.is_some() && columns >= unit.width(current)) {
            return;
        }
        let start = to_flat_byte(model, line, 0);
        let edits = vec![(start..start + current.len(), indent)];
        let state = self.history_view_state(model);
        let (anchor, caret) = (
            map_offset(&edits, state.anchor, false),
            map_offset(&edits, state.caret, false),
        );
        let after = HistoryViewState {
            context: self.history_context,
            anchor,
            caret,
        };
        let changed = buffer
            .amend_last_edit_from_view(edits, after)
            .unwrap_or(false);
        if !changed {
            return;
        }
        let model = &*buffer;
        self.seen_revision = model.revision();
        self.refresh(model, None);
        (self.anchor_line, self.anchor_col) = flat_byte_to_position(model, anchor);
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, caret);
        self.has_selection = anchor != caret;
        self.preferred_column = None;
        self.finish_position_change(buffer);
    }

    /// Indent the selected lines when the selection spans lines; otherwise
    /// replace the selection with one indentation unit.
    pub(crate) fn insert_tab(&mut self, buffer: &mut BufferModel) {
        if self
            .selection_range()
            .is_some_and(|(start, end)| start.0 != end.0)
        {
            self.indent_lines(buffer);
            return;
        }
        let model = &*buffer;
        let range = self.implicit_replacement_range(model);
        let text = self
            .indent_unit()
            .insertion(&text_before_on_line(model, range.start));
        self.break_history_group(buffer);
        self.replace_text_with_grouping(Some(range), &text, None, buffer);
    }

    /// Indent each selected line by one unit, skipping empty lines when there
    /// are several.
    pub(crate) fn indent_lines(&mut self, buffer: &mut BufferModel) {
        let model = &*buffer;
        let unit = self.indent_unit();
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
        self.commit_line_edits(edits, buffer);
    }

    /// Outdent each selected line by one unit.
    pub(crate) fn outdent_lines(&mut self, buffer: &mut BufferModel) {
        let model = &*buffer;
        let unit = self.indent_unit();
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
        self.commit_line_edits(edits, buffer);
    }

    /// Indent the selected lines, or every line, from the indent query of the
    /// language at each, leaving lines of a language without one as they are.
    pub(crate) fn reindent_lines(&mut self, selected_only: bool, buffer: &mut BufferModel) {
        let model = &*buffer;
        let unit = self.indent_unit();
        let lines = if selected_only {
            self.selected_lines()
        } else {
            0..model.line_count()
        };
        let edits = model
            .reindent(lines, unit.size)
            .into_iter()
            .map(|(line, columns)| {
                let text = line_text(model, line);
                let indent = leading_whitespace(&text).len();
                (
                    to_flat_byte(model, line, 0)..to_flat_byte(model, line, indent),
                    unit.whitespace(columns),
                )
            })
            .collect();
        self.commit_line_edits(edits, buffer);
    }

    /// Indent with the style the text uses, dropping the document's own
    /// indentation settings first so a text with no clear style follows the
    /// language and global settings again.
    pub(crate) fn detect_indentation(&mut self, buffer: &BufferModel) {
        if let Some(document) = buffer.document() {
            self.effects
                .push(EditorEffect::DetectIndentation { document });
        }
    }

    /// Rewrite the leading whitespace of every line with tabs or spaces at
    /// the same columns, as one undo step, and make that the document's
    /// indentation style.
    pub(crate) fn convert_indentation(&mut self, tabs: bool, buffer: &mut BufferModel) {
        let model = &*buffer;
        if !model.is_editable() {
            return;
        }
        let unit = IndentUnit {
            tabs,
            ..self.indent_unit()
        };
        let edits = (0..model.line_count())
            .filter_map(|line| {
                let text = line_text(model, line);
                let current = leading_whitespace(&text);
                let indent = unit.whitespace(unit.width(current));
                (indent != current).then(|| {
                    let start = to_flat_byte(model, line, 0);
                    (start..start + current.len(), indent)
                })
            })
            .collect::<Vec<_>>();
        let document = model.document();
        if !edits.is_empty() {
            self.commit_line_edits(edits, buffer);
        }
        if let Some(document) = document {
            self.effects.push(EditorEffect::SetDocumentInsertSpaces {
                document,
                spaces: !tabs,
            });
        }
    }

    /// The range Backspace deletes when only spaces precede the caret on its
    /// line: back to the previous tab stop.
    pub(crate) fn tab_stop_deletion_range(&self, model: &BufferModel) -> Option<Range<usize>> {
        let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
        self.indent_unit()
            .backspace(&text_before_on_line(model, caret))
            .map(|length| caret - length..caret)
    }

    /// Commit edits to whole lines, keeping the selection on the same text. A
    /// selection that starts at a line start keeps starting there, so it
    /// still covers the lines' new indentation.
    pub(crate) fn commit_line_edits(
        &mut self,
        edits: Vec<(Range<usize>, String)>,
        buffer: &mut BufferModel,
    ) {
        let state = self.history_view_state(&*buffer);
        let start = state.anchor.min(state.caret);
        let start_stays = self
            .selection_range()
            .is_some_and(|(start, _)| start.1 == 0);
        let map = |offset| map_offset(&edits, offset, start_stays && offset == start);
        let (anchor, caret) = (map(state.anchor), map(state.caret));
        self.commit_edits(edits, anchor, caret, buffer);
    }

    /// The lines a line command acts on: those the selection touches, except
    /// a last line it reaches only at column 0, or else the caret's line.
    pub(crate) fn selected_lines(&self) -> Range<usize> {
        let lines = match self.selection_range() {
            Some((start, end)) if end.0 > start.0 && end.1 == 0 => start.0..end.0,
            Some((start, end)) => start.0..end.0 + 1,
            None => self.cursor_line..self.cursor_line + 1,
        };
        // A collapsed fold's lines go with its first line.
        match self.wrap.hidden_range(lines.end) {
            Some(hidden) => lines.start..hidden.end,
            None => lines,
        }
    }
}

/// The text from the start of `offset`'s line up to `offset`.
fn text_before_on_line(model: &BufferModel, offset: usize) -> String {
    model.read(model.line_range(model.line_of_offset(offset)).start..offset)
}
