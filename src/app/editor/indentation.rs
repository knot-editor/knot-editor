//! Indentation commands: Enter indents from the language's indent query or
//! keeps the indentation, Tab inserts or indents,
//! lines indent and outdent by one unit or reindent from the language's
//! indent query, and Backspace in leading spaces deletes back to the previous
//! tab stop.

use super::*;
use crate::app::syntax::indent::LineIndent;

/// A line whose indentation waits for the indent query, valid while the
/// buffer stays at `revision`.
pub(super) struct PendingIndent {
    revision: u64,
    line: usize,
    /// For typing, the caret column the typed text reached: the line only
    /// outdents, and only when the typing completed a closing node.
    typed: Option<usize>,
}

impl EditorView {
    /// Replace the selection with a line break followed by the indentation
    /// before it on its line, then let the indent query correct the new
    /// line once the tree covers the edit. Between a bracket pair that opens
    /// lines, the break opens an indented middle line with the closer below
    /// it. A line holding only whitespace keeps none of it.
    pub(super) fn insert_newline(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
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
            let unit = self.indent_unit(model, cx);
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
        self.break_history_group(cx);
        let view = HistoryViewChange {
            before: self.history_view_state(self.model.read(cx)),
            after: HistoryViewState {
                context: self.history_context,
                anchor: caret,
                caret,
            },
        };
        self.preferred_x = None;
        self.splice(range, &text, None, view, cx);
        self.marked_range_utf16 = None;
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(self.model.read(cx), caret);
        self.has_selection = false;
        self.finish_position_change(cx);
        self.pending_indent = Some(PendingIndent {
            revision: self.model.read(cx).revision(),
            line: self.cursor_line,
            typed: None,
        });
        self.apply_pending_indent(cx);
    }

    /// After typing, outdent the caret's line when the typed text completed
    /// a node that closes or branches an indented one at the start of the
    /// line's content, such as `}` or `else`.
    pub(super) fn outdent_typed_line(&mut self, cx: &mut Context<Self>) {
        self.pending_indent = Some(PendingIndent {
            revision: self.model.read(cx).revision(),
            line: self.cursor_line,
            typed: Some(self.cursor_col),
        });
        self.apply_pending_indent(cx);
    }

    /// Indent the line a line break or typing left for the indent query,
    /// once the tree is a parse of the text the edit produced, as part of the
    /// edit's undo step. A later edit drops the request, leaving the
    /// indentation as it was.
    pub(super) fn apply_pending_indent(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = &self.pending_indent else {
            return;
        };
        let model = self.model.read(cx);
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
        let unit = self.indent_unit(model, cx);
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
        let changed = self.model.update(cx, |model, cx| {
            let changed = model
                .amend_last_edit_from_view(edits, after)
                .unwrap_or(false);
            if changed {
                cx.notify();
            }
            changed
        });
        if !changed {
            return;
        }
        let model = self.model.read(cx);
        self.text_changed(model);
        self.refresh(model, None);
        (self.anchor_line, self.anchor_col) = flat_byte_to_position(model, anchor);
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, caret);
        self.has_selection = anchor != caret;
        self.preferred_x = None;
        self.finish_position_change(cx);
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

    /// Indent the selected lines from the indent query of the language at
    /// each, leaving lines of a language without one as they are.
    pub(super) fn reindent_lines(&mut self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let unit = self.indent_unit(model, cx);
        let edits = model
            .reindent(self.selected_lines(), unit.size)
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
        self.commit_line_edits(edits, cx);
    }

    /// Rewrite the leading whitespace of every line with tabs or spaces at
    /// the same columns, as one undo step, and make that the document's
    /// indentation style.
    pub(super) fn convert_indentation(&mut self, tabs: bool, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        if !model.is_editable() {
            return;
        }
        let unit = IndentUnit {
            tabs,
            ..self.indent_unit(model, cx)
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
            self.commit_line_edits(edits, cx);
        }
        if let Some(document) = document {
            settings::set_document_insert_spaces(document, !tabs, cx);
            crate::app::application_observation::changed(cx);
        }
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
    pub(super) fn commit_line_edits(
        &mut self,
        edits: Vec<(Range<usize>, String)>,
        cx: &mut Context<Self>,
    ) {
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
    pub(super) fn selected_lines(&self) -> Range<usize> {
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
