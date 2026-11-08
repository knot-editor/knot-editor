//! Editing, clipboard, history, and caret-movement commands.

use super::*;

impl Editor {
    /// Run the native editing or movement command `command`; `false` when
    /// it is not one or does not apply.
    pub(crate) fn execute_editing_command(
        &mut self,
        command: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if command == commands::UNDO_COMMAND || command == commands::REDO_COMMAND {
            self.break_history_group(cx);
            self.marked_range_utf16 = None;
            self.preferred_column = None;
            let changed = self.model.update(cx, |model, cx| {
                let changed = if command == commands::UNDO_COMMAND {
                    model.undo()
                } else {
                    model.redo()
                }
                .unwrap_or(false);
                if changed {
                    cx.notify();
                }
                changed
            });
            if changed {
                let model = self.model.read(cx);
                self.seen_revision = model.revision();
                self.refresh(model, model.resolve_view_position(self.position_range));
                if let Some(restore) = model.history_view_restore() {
                    self.apply_history_view_restore(model, restore.generation, &restore.state);
                }
                self.sync_view_position(cx);
                cx.notify();
            }
            return true;
        }
        if matches!(command, commands::COPY_COMMAND | commands::CUT_COMMAND) {
            if command == commands::COPY_COMMAND && !self.has_selection() {
                return false;
            }
            self.break_history_group(cx);
            self.marked_range_utf16 = None;
            if command == commands::CUT_COMMAND && !self.model.read(cx).is_editable() {
                return true;
            }
            if let Some(range) = self.selected_byte_range(cx) {
                let text = self.model.read(cx).read(range);
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                if command == commands::CUT_COMMAND {
                    self.replace_text_with_grouping(None, "", None, cx);
                }
            }
            return true;
        }
        if command == commands::PASTE_COMMAND {
            self.break_history_group(cx);
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.replace_text_with_grouping(None, &text, None, cx);
            }
            return true;
        }
        if command == commands::SELECT_ALL_COMMAND {
            self.break_history_group(cx);
            self.marked_range_utf16 = None;
            self.anchor_line = 0;
            self.anchor_col = 0;
            self.cursor_line = self.line_count - 1;
            self.cursor_col = line_end(self.model.read(cx), self.cursor_line);
            self.has_selection = self.cursor_line != 0 || self.cursor_col != 0;
            self.preferred_column = None;
            self.finish_position_change(cx);
            return true;
        }
        match command {
            commands::INSERT_NEWLINE_COMMAND => {
                self.insert_newline(cx);
                return true;
            }
            commands::INSERT_TAB_COMMAND => {
                self.insert_tab(cx);
                return true;
            }
            commands::INDENT_LINES_COMMAND => {
                self.indent_lines(cx);
                return true;
            }
            commands::OUTDENT_LINES_COMMAND => {
                self.outdent_lines(cx);
                return true;
            }
            commands::REINDENT_BUFFER_COMMAND | commands::REINDENT_SELECTED_LINES_COMMAND => {
                self.reindent_lines(command == commands::REINDENT_SELECTED_LINES_COMMAND, cx);
                return true;
            }
            commands::DETECT_INDENTATION_COMMAND => {
                self.detect_indentation(cx);
                return true;
            }
            commands::CONVERT_INDENTATION_TO_SPACES_COMMAND
            | commands::CONVERT_INDENTATION_TO_TABS_COMMAND => {
                self.convert_indentation(
                    command == commands::CONVERT_INDENTATION_TO_TABS_COMMAND,
                    cx,
                );
                return true;
            }
            commands::TRIM_TRAILING_WHITESPACE_COMMAND => {
                self.trim_trailing_whitespace(cx);
                return true;
            }
            commands::COPY_LINES_UP_COMMAND | commands::COPY_LINES_DOWN_COMMAND => {
                self.copy_lines(command == commands::COPY_LINES_DOWN_COMMAND, cx);
                return true;
            }
            commands::MOVE_LINES_UP_COMMAND | commands::MOVE_LINES_DOWN_COMMAND => {
                self.move_lines(command == commands::MOVE_LINES_DOWN_COMMAND, cx);
                return true;
            }
            commands::DELETE_LINES_COMMAND => {
                self.delete_lines(cx);
                return true;
            }
            commands::JOIN_LINES_COMMAND => {
                self.join_lines(cx);
                return true;
            }
            commands::JUMP_TO_MATCHING_BRACKET_COMMAND => {
                self.jump_to_matching_bracket(cx);
                return true;
            }
            commands::TOGGLE_LINE_COMMENT_COMMAND => {
                self.toggle_line_comment(cx);
                return true;
            }
            commands::TOGGLE_BLOCK_COMMENT_COMMAND => {
                self.toggle_block_comment(cx);
                return true;
            }
            commands::TOGGLE_SOFT_WRAP_COMMAND => return self.toggle_soft_wrap(cx),
            commands::FOLD_COMMAND => {
                self.fold_at_caret(cx);
                return true;
            }
            commands::UNFOLD_COMMAND => {
                self.unfold_at_caret(cx);
                return true;
            }
            commands::TOGGLE_FOLD_COMMAND => {
                if !self.unfold_at_caret(cx) {
                    self.fold_at_caret(cx);
                }
                return true;
            }
            commands::FOLD_ALL_COMMAND => {
                self.fold_all_regions(cx);
                return true;
            }
            commands::UNFOLD_ALL_COMMAND => {
                self.unfold_all_folds(cx);
                return true;
            }
            _ => {}
        }
        if matches!(
            command,
            commands::DELETE_BACKWARD_COMMAND | commands::DELETE_FORWARD_COMMAND
        ) {
            if self.has_selection || self.marked_range_utf16.is_some() {
                self.replace_text_with_grouping(None, "", None, cx);
            } else {
                let backward = command.ends_with("backward");
                let model = self.model.read(cx);
                let range = backward
                    .then(|| {
                        self.auto_closed_deletion_range(model)
                            .or_else(|| self.tab_stop_deletion_range(model, cx))
                    })
                    .flatten()
                    .unwrap_or_else(|| self.grapheme_deletion_range(model, backward));
                if !range.is_empty() {
                    self.replace_text_with_grouping(
                        Some(range),
                        "",
                        Some(if backward {
                            EditGroupKind::DeleteBackward
                        } else {
                            EditGroupKind::DeleteForward
                        }),
                        cx,
                    );
                }
            }
            return true;
        }
        if command == commands::DELETE_TO_LINE_END_COMMAND {
            if self.has_selection || self.marked_range_utf16.is_some() {
                self.replace_text_with_grouping(None, "", None, cx);
                return true;
            }
            let model = self.model.read(cx);
            let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
            let end = to_flat_byte(model, self.cursor_line, line_end(model, self.cursor_line));
            let range = if caret < end {
                caret..end
            } else {
                self.grapheme_deletion_range(model, false)
            };
            if !range.is_empty() {
                self.break_history_group(cx);
                self.replace_text_with_grouping(Some(range), "", None, cx);
            }
            return true;
        }
        let (movement, extend) =
            if let Some(movement) = command.strip_prefix(commands::EDITOR_MOVE_PREFIX) {
                (movement, false)
            } else if let Some(movement) = command.strip_prefix(commands::EDITOR_SELECT_PREFIX) {
                (movement, true)
            } else {
                return false;
            };
        // A `-logical` twin moves by buffer lines; the plain command moves by
        // the rows wrapping shows, which are the lines while nothing wraps.
        let (movement, logical) = match movement.strip_suffix("-logical") {
            Some(movement) => (movement, true),
            None => (movement, false),
        };
        self.break_history_group(cx);
        self.marked_range_utf16 = None;
        let model = self.model.read(cx);
        let last = self.line_count - 1;
        let row = self.row(model, self.caret_row());
        let forward = matches!(
            movement,
            "right" | "down" | "word-right" | "line-end" | "page-down" | "document-end"
        );
        let vertical = matches!(movement, "up" | "down" | "page-up" | "page-down");
        let target = match movement {
            "left" if self.cursor_col > 0 => (
                self.cursor_line,
                prev_boundary(model, self.cursor_line, self.cursor_col),
            ),
            "left" if self.cursor_line > 0 => {
                (self.cursor_line - 1, line_end(model, self.cursor_line - 1))
            }
            "left" => (0, 0),
            "right" if self.cursor_col < line_end(model, self.cursor_line) => (
                self.cursor_line,
                next_boundary(model, self.cursor_line, self.cursor_col),
            ),
            "right" if self.cursor_line < last => (self.cursor_line + 1, 0),
            "right" => (last, line_end(model, last)),
            "line-start" | "smart-home" if !logical && row.range.start > 0 => {
                (self.cursor_line, row.range.start)
            }
            "line-start" => (self.cursor_line, 0),
            "smart-home" => {
                let first = leading_whitespace(&line_text(model, self.cursor_line)).len();
                (
                    self.cursor_line,
                    if self.cursor_col == first { 0 } else { first },
                )
            }
            // A row that does not end its line ends before its last
            // character, since its end offset shows on the next row.
            "line-end" if !logical && !row.last => (
                self.cursor_line,
                prev_boundary(model, self.cursor_line, row.range.end),
            ),
            "line-end" => (self.cursor_line, line_end(model, self.cursor_line)),
            "document-start" => (0, 0),
            "document-end" => (last, line_end(model, last)),
            "word-left" | "word-right" => {
                let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
                flat_byte_to_position(
                    model,
                    word_boundary(model, self.cursor_line, caret, forward),
                )
            }
            "up" | "down" | "page-up" | "page-down" => {
                let rows = if movement.starts_with("page-") {
                    self.viewport_rows.floor().max(1.) as usize
                } else {
                    1
                };
                let step = |from: usize, last: usize| {
                    if forward {
                        from.saturating_add(rows).min(last)
                    } else {
                        from.saturating_sub(rows)
                    }
                };
                let target = if logical {
                    let line = step(self.cursor_line, last);
                    let line = self.step_visible_line(self.cursor_line, line, forward);
                    self.wrap.first_row(line)
                } else {
                    step(row.index, self.wrap.row_count() - 1)
                };
                let column = self
                    .preferred_column
                    .unwrap_or_else(|| self.caret_column(model, cx));
                self.preferred_column = Some(column);
                self.position_at_column(model, target, column, cx)
            }
            _ => return false,
        };
        let target = self.out_of_folds(model, target, forward);
        if extend {
            if !self.has_selection {
                self.anchor_line = self.cursor_line;
                self.anchor_col = self.cursor_col;
            }
            (self.cursor_line, self.cursor_col) = target;
            self.has_selection = target != (self.anchor_line, self.anchor_col);
        } else if self.has_selection && matches!(movement, "left" | "right" | "up" | "down") {
            let (start, end) = self.selection_range().unwrap();
            (self.cursor_line, self.cursor_col) = if forward { end } else { start };
            self.has_selection = false;
            self.preferred_column = None;
        } else {
            (self.cursor_line, self.cursor_col) = target;
            self.has_selection = false;
        }
        if !vertical {
            self.preferred_column = None;
        }
        self.finish_position_change(cx);
        true
    }

    /// The byte range one grapheme before or after the caret. At a line edge
    /// it covers the whole line terminator, `\n` or `\r\n`.
    pub(super) fn grapheme_deletion_range(
        &self,
        model: &BufferModel,
        backward: bool,
    ) -> Range<usize> {
        let (line, col) = (self.cursor_line, self.cursor_col);
        let caret = to_flat_byte(model, line, col);
        if backward {
            if col > 0 {
                to_flat_byte(model, line, prev_boundary(model, line, col))..caret
            } else if line > 0 {
                to_flat_byte(model, line - 1, line_end(model, line - 1))..caret
            } else {
                caret..caret
            }
        } else if col < line_end(model, line) {
            caret..to_flat_byte(model, line, next_boundary(model, line, col))
        } else if line + 1 < self.line_count {
            caret..model.line_range(line + 1).start
        } else {
            caret..caret
        }
    }

    /// Write the opposite of the resolved `editor.softWrap` for this view's
    /// document, so every view of it follows. `false` without a document.
    fn toggle_soft_wrap(&mut self, cx: &mut Context<Self>) -> bool {
        let model = self.model.read(cx);
        let Some(document) = model.document() else {
            return false;
        };
        let wrap = settings::editor_soft_wrap(&self.settings_scope(model, cx), cx);
        settings::set_document_soft_wrap(document, !wrap, cx);
        true
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.selection_range().is_some()
    }
}
