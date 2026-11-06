//! Pointer gestures and platform text input, including IME composition.

use super::gutter::GutterLane;
use super::*;

impl EditorView {
    /// Mouse-down hit-tests the click onto a (line, byte_col) caret and moves
    /// it there, on the nearest grapheme boundary. With shift held, the
    /// existing caret becomes the selection anchor and the click position
    /// becomes the new caret (extending the selection); without shift, any
    /// selection is cleared and the caret jumps to the click.
    pub(super) fn on_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drag_anchor = None;
        self.model
            .update(cx, |model, _| model.break_history_group());
        let editor = self.editor.read(cx);
        let (line, col) = self.hit_test(editor, ev.position, window, cx);
        let action = match self.gutter_lane_at(ev.position) {
            Some(GutterLane::Markers) => editor
                .gutter_marker(line)
                .and_then(|marker| Some((marker.command.clone()?, marker.source, marker.range))),
            Some(GutterLane::Numbers) => {
                self.suppress_drag_selection = false;
                self.drag_anchor = Some(self.editor.update(cx, |editor, cx| {
                    editor.select_lines_from_gutter(line, ev.modifiers.shift, cx)
                }));
                return;
            }
            None => editor
                .contribution_actions
                .iter()
                .rev()
                .find(|action| {
                    action.line == line
                        && ((action.start == action.end && col == action.start)
                            || (col >= action.start && col < action.end))
                })
                .map(|action| (action.command.clone(), action.source, action.range)),
        };
        if let Some((command, source, range)) = action {
            self.suppress_drag_selection = true;
            cx.emit(EditorContributionAction {
                command,
                source,
                range,
                model: self.model.clone(),
                window: window.window_handle(),
                focus: self.focus.downgrade(),
            });
            cx.stop_propagation();
            return;
        }
        if self.gutter_lane_at(ev.position) == Some(GutterLane::Markers) {
            // The marker lane selects nothing.
            self.suppress_drag_selection = true;
            return;
        }
        self.suppress_drag_selection = false;
        let unit = match ev.click_count {
            _ if ev.modifiers.shift => SelectionUnit::Character,
            0 | 1 => SelectionUnit::Character,
            2 => SelectionUnit::Word,
            _ => SelectionUnit::Line,
        };
        if unit != SelectionUnit::Character {
            let (start, end) = unit_range(self.model.read(cx), unit, line, col);
            self.drag_anchor = Some(DragAnchor { unit, start, end });
            self.editor
                .update(cx, |editor, cx| editor.select_from(start, end, cx));
            return;
        }
        let editor = self.editor.read(cx);
        let caret = (editor.cursor_line, editor.cursor_col);
        let anchor = if !ev.modifiers.shift {
            (line, col)
        } else if editor.has_selection {
            (editor.anchor_line, editor.anchor_col)
        } else {
            caret
        };
        self.drag_anchor = Some(DragAnchor {
            unit,
            start: anchor,
            end: anchor,
        });
        self.editor
            .update(cx, |editor, cx| editor.select_from(anchor, (line, col), cx));
    }

    /// Mouse-move while the left button is held extends the selection from
    /// the anchor (the position, word, or line where the drag began) to the
    /// position, word, or line under the pointer. Window-level pointer
    /// listeners keep the gesture active outside the pane.
    pub(super) fn on_mouse_move(
        &mut self,
        ev: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ev.pressed_button != Some(MouseButton::Left) {
            self.drag_anchor = None;
            self.suppress_drag_selection = false;
            return;
        }
        if self.suppress_drag_selection {
            return;
        }
        let Some(anchor) = self.drag_anchor else {
            return;
        };
        let (line, col) = self.hit_test(self.editor.read(cx), ev.position, window, cx);
        // Extending by units keeps the clicked unit selected and reaches the
        // far edge of the unit under the pointer.
        let (hovered_start, hovered_end) = unit_range(self.model.read(cx), anchor.unit, line, col);
        let (fixed, moving) = if hovered_start < anchor.start {
            (anchor.end, hovered_start)
        } else {
            (anchor.start, hovered_end.max(anchor.end))
        };
        self.editor
            .update(cx, |editor, cx| editor.select_from(fixed, moving, cx));
    }

    /// Map a window-space point to a (line, byte_col) caret position using the
    /// last paint's `bounds`. The candidate positions are grapheme boundaries,
    /// not glyph starts: a single emoji glyph may cover multiple UTF-8 code
    /// points, and its start alone is not enough to place a caret on either
    /// visible edge.
    pub(super) fn hit_test(
        &self,
        editor: &Editor,
        p: Point<Pixels>,
        window: &Window,
        cx: &App,
    ) -> (usize, usize) {
        let origin = self.text_bounds().origin;
        let row = (f32::from(p.y - origin.y) / self.metrics.line_height + editor.scroll_top)
            .floor()
            .max(0.) as usize;
        self.column_for_x(
            editor,
            row,
            p.x - origin.x + px(editor.scroll_x),
            window,
            cx,
        )
    }
}

impl Editor {
    /// The selection as document-wide UTF-16 offsets, a caret-only range
    /// without one. `reversed` when the caret is before the anchor.
    fn selected_utf16(&self, cx: &App) -> UTF16Selection {
        let model = self.model.read(cx);
        let caret = to_flat_utf16(model, self.cursor_line, self.cursor_col);
        if !self.has_selection {
            return UTF16Selection {
                range: caret..caret,
                reversed: false,
            };
        }
        let anchor = to_flat_utf16(model, self.anchor_line, self.anchor_col);
        if anchor <= caret {
            UTF16Selection {
                range: anchor..caret,
                reversed: false,
            }
        } else {
            UTF16Selection {
                range: caret..anchor,
                reversed: true,
            }
        }
    }

    /// Commit the preedit marking without changing its text or selection.
    pub(super) fn unmark_text(&mut self, cx: &mut Context<Self>) {
        if self.marked_range_utf16.take().is_some() {
            self.break_history_group(cx);
            cx.notify();
        }
    }

    /// Insert typed or committed `text` at a UTF-16 range, or over the
    /// marked text, selection, or caret: a composition commits in its own
    /// undo group, while typing groups with typing and may close a bracket
    /// or outdent the line.
    pub(crate) fn type_text(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let grouping = if self.marked_range_utf16.is_some() {
            EditGroupKind::Composition
        } else {
            EditGroupKind::Typing
        };
        let range = self.bytes_for_utf16(range, cx);
        let bracket =
            grouping == EditGroupKind::Typing && self.type_bracket(range.clone(), text, cx);
        if !bracket {
            self.replace_text_with_grouping(range, text, Some(grouping), cx);
        }
        if grouping == EditGroupKind::Typing && !text.contains(['\n', '\r']) {
            self.outdent_typed_line(cx);
        }
    }

    /// Replace a UTF-16 range, or the marked text, selection, or caret,
    /// with composing `new_text` and mark it; `new_selected_range`, relative
    /// to the marked text, becomes the selection. Nothing changes where a
    /// text binding claims typed text.
    pub(super) fn mark_text(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        // Text a binding claims never reaches the buffer, so neither does
        // its composition; the commit arrives as ordinary text.
        if !self.model.read(cx).is_editable() || self.text_command(cx).is_some() {
            return;
        }
        self.preferred_column = None;
        let range = self.bytes_for_utf16(range, cx);
        let model = self.model.read(cx);
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
            cx,
        );

        self.marked_range_utf16 = Some(marked_start_utf16..marked_start_utf16 + marked_utf16_len);
        let model = self.model.read(cx);
        (self.anchor_line, self.anchor_col) = flat_byte_to_position(model, anchor);
        (self.cursor_line, self.cursor_col) = flat_byte_to_position(model, caret);
        self.has_selection = anchor != caret;

        self.finish_position_change(cx);
    }
}

impl EntityInputHandler for EditorView {
    /// Return the text at the given document-wide UTF-16 range. Report
    /// scalar-aligned offsets when a request splits a UTF-16 surrogate pair.
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let model = self.model.read(cx);
        let start = model.offset_of_utf16(range.start);
        let end = model.offset_of_utf16(range.end);
        if start > end {
            return None;
        }
        *adjusted = Some(model.utf16_of_offset(start)..model.utf16_of_offset(end));
        Some(model.read(start..end))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(self.editor.read(cx).selected_utf16(cx))
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.editor.read(cx).marked_range_utf16.clone()
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| editor.unmark_text(cx));
    }

    /// The `insertText:` callback, which commits text into the document
    /// unless the session takes it, to hold behind a command or to route to
    /// a text binding.
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(route) = cx
            .try_global::<crate::app::commands::key_hold::TypedTextRouter>()
            .map(|router| router.0)
            && route(&self.editor, text, window, cx)
        {
            return;
        }
        self.editor
            .update(cx, |editor, cx| editor.type_text(range, text, cx));
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            editor.mark_text(range, new_text, new_selected_range, cx)
        });
    }

    /// Return the bounds (in window-local px) of the given UTF-16 range, used
    /// by macOS to position the IME candidate window: from the start
    /// column's x to the end column's x on the start's row.
    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let editor = self.editor.read(cx);
        let model = self.model.read(cx);
        let start = flat_utf16_to_position(model, range_utf16.start);
        let end = flat_utf16_to_position(model, range_utf16.end);
        let row = editor.row(model, editor.wrap.row_of(start.0, start.1));
        let top = element_bounds.origin.y
            + px((row.index as f32 - editor.scroll_top) * self.metrics.line_height);
        let x0 = self.x_in_row(editor, &row, start.1, window, cx);
        let x1 = if row.holds(end) {
            self.x_in_row(editor, &row, end.1, window, cx)
        } else {
            element_bounds.size.width
        };
        let (origin_x, width) = if x0 > x1 {
            (x1, (x0 - x1).max(px(1.)))
        } else {
            (x0, (x1 - x0).max(px(1.)))
        };
        Some(Bounds {
            origin: point(element_bounds.origin.x + origin_x, top),
            size: size(width, px(self.metrics.line_height)),
        })
    }

    /// Map a window-space point to a flat UTF-16 offset, for IME hit-testing.
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let (line, col) = self.hit_test(self.editor.read(cx), point, window, cx);
        Some(to_flat_utf16(self.model.read(cx), line, col))
    }
}

/// The `unit` containing the caret position `(line, col)`, as start and end
/// positions: the position itself, the word segment at it (see
/// [`word_segments`]), or the whole line
/// with its terminator.
pub(super) fn unit_range(
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
