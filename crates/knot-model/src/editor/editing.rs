//! Editing, clipboard, history, and caret-movement commands.

use super::*;

impl EditorState {
    /// Run the native editing or movement command `command`; `false` when
    /// it is not one or does not apply. Paste, which needs the clipboard's
    /// text, is [`Self::paste`].
    pub fn execute_editing_command(&mut self, command: &str, buffer: &mut BufferModel) -> bool {
        if command == "editor.undo" || command == "editor.redo" {
            self.break_history_group(buffer);
            self.marked_range_utf16 = None;
            self.preferred_column = None;
            let changed = if command == "editor.undo" {
                buffer.undo()
            } else {
                buffer.redo()
            }
            .unwrap_or(false);
            if changed {
                let model = &*buffer;
                self.seen_revision = model.revision();
                self.refresh(model, model.resolve_view_position(self.position_range));
                if let Some(restore) = model.history_view_restore() {
                    self.apply_history_view_restore(model, restore.generation, &restore.state);
                }
                self.sync_view_position(buffer);
            }
            return true;
        }
        if matches!(command, "copy" | "editor.cut") {
            if command == "copy" && !self.has_selection() {
                return false;
            }
            self.break_history_group(buffer);
            self.marked_range_utf16 = None;
            if command == "editor.cut" && !buffer.is_editable() {
                return true;
            }
            if let Some(range) = self.selected_byte_range(buffer) {
                let text = buffer.read(range);
                self.effects.push(EditorEffect::WriteClipboard(text));
                if command == "editor.cut" {
                    self.replace_text_with_grouping(None, "", None, buffer);
                }
            }
            return true;
        }
        if command == "editor.select-all" {
            self.break_history_group(buffer);
            self.marked_range_utf16 = None;
            self.anchor_line = 0;
            self.anchor_col = 0;
            self.cursor_line = self.line_count - 1;
            self.cursor_col = line_end(&*buffer, self.cursor_line);
            self.has_selection = self.cursor_line != 0 || self.cursor_col != 0;
            self.preferred_column = None;
            self.finish_position_change(buffer);
            return true;
        }
        match command {
            "editor.insert-newline" => {
                self.insert_newline(buffer);
                return true;
            }
            "editor.insert-tab" => {
                self.insert_tab(buffer);
                return true;
            }
            "editor.indent-lines" => {
                self.indent_lines(buffer);
                return true;
            }
            "editor.outdent-lines" => {
                self.outdent_lines(buffer);
                return true;
            }
            "editor.reindent-buffer" | "editor.reindent-selected-lines" => {
                self.reindent_lines(command == "editor.reindent-selected-lines", buffer);
                return true;
            }
            "editor.detect-indentation" => {
                self.detect_indentation(buffer);
                return true;
            }
            "editor.convert-indentation-to-spaces" | "editor.convert-indentation-to-tabs" => {
                self.convert_indentation(command == "editor.convert-indentation-to-tabs", buffer);
                return true;
            }
            "editor.trim-trailing-whitespace" => {
                self.trim_trailing_whitespace(buffer);
                return true;
            }
            "editor.copy-lines-up" | "editor.copy-lines-down" => {
                self.copy_lines(command == "editor.copy-lines-down", buffer);
                return true;
            }
            "editor.move-lines-up" | "editor.move-lines-down" => {
                self.move_lines(command == "editor.move-lines-down", buffer);
                return true;
            }
            "editor.delete-lines" => {
                self.delete_lines(buffer);
                return true;
            }
            "editor.join-lines" => {
                self.join_lines(buffer);
                return true;
            }
            "editor.jump-to-matching-bracket" => {
                self.jump_to_matching_bracket(buffer);
                return true;
            }
            "editor.toggle-line-comment" => {
                self.toggle_line_comment(buffer);
                return true;
            }
            "editor.toggle-block-comment" => {
                self.toggle_block_comment(buffer);
                return true;
            }
            "editor.toggle-soft-wrap" => return self.toggle_soft_wrap(buffer),
            "editor.fold" => {
                self.fold_at_caret(buffer);
                return true;
            }
            "editor.unfold" => {
                self.unfold_at_caret(buffer);
                return true;
            }
            "editor.toggle-fold" => {
                if !self.unfold_at_caret(buffer) {
                    self.fold_at_caret(buffer);
                }
                return true;
            }
            "editor.fold-all" => {
                self.fold_all_regions(buffer);
                return true;
            }
            "editor.unfold-all" => {
                self.unfold_all_folds(buffer);
                return true;
            }
            _ => {}
        }
        if matches!(command, "editor.delete-backward" | "editor.delete-forward") {
            if self.has_selection || self.marked_range_utf16.is_some() {
                self.replace_text_with_grouping(None, "", None, buffer);
            } else {
                let backward = command.ends_with("backward");
                let model = &*buffer;
                let range = backward
                    .then(|| {
                        self.auto_closed_deletion_range(model)
                            .or_else(|| self.tab_stop_deletion_range(model))
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
                        buffer,
                    );
                }
            }
            return true;
        }
        if command == "editor.delete-to-line-end" {
            if self.has_selection || self.marked_range_utf16.is_some() {
                self.replace_text_with_grouping(None, "", None, buffer);
                return true;
            }
            let model = &*buffer;
            let caret = to_flat_byte(model, self.cursor_line, self.cursor_col);
            let end = to_flat_byte(model, self.cursor_line, line_end(model, self.cursor_line));
            let range = if caret < end {
                caret..end
            } else {
                self.grapheme_deletion_range(model, false)
            };
            if !range.is_empty() {
                self.break_history_group(buffer);
                self.replace_text_with_grouping(Some(range), "", None, buffer);
            }
            return true;
        }
        let (movement, extend) = if let Some(movement) = command.strip_prefix(EDITOR_MOVE_PREFIX) {
            (movement, false)
        } else if let Some(movement) = command.strip_prefix(EDITOR_SELECT_PREFIX) {
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
        self.break_history_group(buffer);
        self.marked_range_utf16 = None;
        let model = &*buffer;
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
                    .unwrap_or_else(|| self.caret_column(model));
                self.preferred_column = Some(column);
                self.position_at_column(model, target, column)
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
        self.finish_position_change(buffer);
        true
    }

    /// The byte range one grapheme before or after the caret. At a line edge
    /// it covers the whole line terminator, `\n` or `\r\n`.
    pub(crate) fn grapheme_deletion_range(
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
    fn toggle_soft_wrap(&mut self, buffer: &BufferModel) -> bool {
        let Some(document) = buffer.document() else {
            return false;
        };
        self.effects.push(EditorEffect::SetDocumentSoftWrap {
            document,
            wrap: !self.settings.soft_wrap,
        });
        true
    }

    /// Replace the selection with the clipboard's `text`, as one undo step.
    pub fn paste(&mut self, text: Option<&str>, buffer: &mut BufferModel) {
        self.break_history_group(buffer);
        if let Some(text) = text {
            self.replace_text_with_grouping(None, text, None, buffer);
        }
    }

    pub fn has_selection(&self) -> bool {
        self.selection_range().is_some()
    }
}
