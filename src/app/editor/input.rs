//! Pointer gestures and platform text input, including IME composition.

use super::gutter::GutterLane;
use super::*;

impl EditorView {
    /// Mouse-down hit-tests the click onto a (line, byte_col) caret and moves
    /// it there. Uses `closest_index_for_x` on the shaped line — the public
    /// `LineLayout` API — so the caret lands on the nearest glyph boundary
    /// rather than a guessed byte offset. With shift held, the existing caret
    /// becomes the selection anchor and the click position becomes the new
    /// caret (extending the selection); without shift, any selection is
    /// cleared and the caret jumps to the click.
    pub(super) fn on_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.break_history_group(cx);
        self.drag_anchor = None;
        self.marked_range_utf16 = None;
        let (line, col) = self.hit_test(self.model.read(cx), ev.position, window, cx);
        let action = match self.gutter_lane_at(ev.position) {
            Some(GutterLane::Markers) => self
                .gutter_marker(line)
                .and_then(|marker| Some((marker.command.clone()?, marker.source, marker.range))),
            Some(GutterLane::Numbers) => {
                self.suppress_drag_selection = false;
                self.select_lines_from_gutter(line, ev.modifiers.shift, cx);
                return;
            }
            None => self
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
        self.preferred_x = None;
        let unit = match ev.click_count {
            _ if ev.modifiers.shift => SelectionUnit::Character,
            0 | 1 => SelectionUnit::Character,
            2 => SelectionUnit::Word,
            _ => SelectionUnit::Line,
        };
        if unit != SelectionUnit::Character {
            let (start, end) = unit_range(self.model.read(cx), unit, line, col);
            self.drag_anchor = Some(DragAnchor { unit, start, end });
            (self.anchor_line, self.anchor_col) = start;
            (self.cursor_line, self.cursor_col) = end;
            self.has_selection = start != end;
            self.finish_position_change(cx);
            return;
        }
        let anchor = if !ev.modifiers.shift {
            (line, col)
        } else if self.has_selection {
            (self.anchor_line, self.anchor_col)
        } else {
            (self.cursor_line, self.cursor_col)
        };
        self.drag_anchor = Some(DragAnchor {
            unit,
            start: anchor,
            end: anchor,
        });
        if ev.modifiers.shift {
            if !self.has_selection {
                self.anchor_line = self.cursor_line;
                self.anchor_col = self.cursor_col;
            }
            self.cursor_line = line;
            self.cursor_col = col;
            self.has_selection =
                self.cursor_line != self.anchor_line || self.cursor_col != self.anchor_col;
        } else {
            self.cursor_line = line;
            self.cursor_col = col;
            self.has_selection = false;
        }
        self.finish_position_change(cx);
    }

    /// Mouse-move while the left button is held extends the selection from
    /// the anchor (the position, word, or line where the drag began) to the
    /// position, word, or line under the pointer. `MouseMoveEvent::dragging()` is true
    /// when the platform reports the left button as currently pressed, so we
    /// don't need a separate mouse-up handler to know we're still dragging.
    /// Window-level pointer listeners keep the gesture active outside the pane.
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
        let model = self.model.read(cx);
        let (line, col) = self.hit_test(model, ev.position, window, cx);
        // Extending by units keeps the clicked unit selected and reaches the
        // far edge of the unit under the pointer.
        let (hovered_start, hovered_end) = unit_range(model, anchor.unit, line, col);
        let (fixed, moving) = if hovered_start < anchor.start {
            (anchor.end, hovered_start)
        } else {
            (anchor.start, hovered_end.max(anchor.end))
        };
        (self.anchor_line, self.anchor_col) = fixed;
        (self.cursor_line, self.cursor_col) = moving;
        self.preferred_x = None;
        self.has_selection =
            self.cursor_line != self.anchor_line || self.cursor_col != self.anchor_col;
        self.finish_position_change(cx);
    }

    /// Map a window-space point to a (line, byte_col) caret position using the
    /// last paint's `bounds`. The candidate positions are grapheme boundaries,
    /// not glyph starts: a single emoji glyph may cover multiple UTF-8 code
    /// points, and its start alone is not enough to place a caret on either
    /// visible edge.
    pub(super) fn hit_test(
        &self,
        model: &BufferModel,
        p: Point<Pixels>,
        window: &Window,
        cx: &App,
    ) -> (usize, usize) {
        let text_bounds = self.text_bounds();
        let origin = text_bounds.origin;
        let line = ((f32::from(p.y - origin.y) + self.scroll) / LINE_HEIGHT).floor() as usize;
        let line = line.min(self.line_count - 1);
        let text = line_text(model, line);
        if text.is_empty() {
            return (line, 0);
        }
        let shaped = self.shape_line(model, line, &text, window, cx);
        let tab_size = self.tab_size(model, cx);
        let x = p.x - origin.x - self.line_x_offset(&text, shaped.width, text_bounds.size.width);
        let col = text
            .grapheme_indices(true)
            .map(|(start, _)| start)
            .chain(std::iter::once(text.len()))
            .min_by(|a, b| {
                let distance =
                    |index| f32::from((x_for_index_dir(&shaped, index, &text, tab_size) - x).abs());
                distance(*a).total_cmp(&distance(*b))
            })
            .unwrap_or(0);
        (line, col)
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

    /// Return the current selection as a `UTF16Selection`. When there is no
    /// selection (caret only), return a zero-length range at the caret
    /// position. `reversed` is true when the caret (head) is before the
    /// anchor (tail) in document order.
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let model = self.model.read(cx);
        let caret = to_flat_utf16(model, self.cursor_line, self.cursor_col);
        if self.has_selection {
            let anchor = to_flat_utf16(model, self.anchor_line, self.anchor_col);
            let (start, end, reversed) = if anchor <= caret {
                (anchor, caret, false)
            } else {
                (caret, anchor, true)
            };
            Some(UTF16Selection {
                range: start..end,
                reversed,
            })
        } else {
            Some(UTF16Selection {
                range: caret..caret,
                reversed: false,
            })
        }
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range_utf16.clone()
    }

    /// Commit the preedit marking without changing its text or selection.
    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.marked_range_utf16.take().is_some() {
            self.break_history_group(cx);
            cx.notify();
        }
    }

    /// Replace text at the given UTF-16 range (or the current selection /
    /// marked range if `range` is `None`) with `text`. This is the
    /// `insertText:` callback — it commits text into the document. After
    /// replacement the marked range is cleared and the caret moves to the
    /// end of the inserted text.
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A composition commits in place: its preedit is already in the text.
        if self.marked_range_utf16.is_none()
            && let Some(queue) = &self.held_input
        {
            queue.push(crate::app::commands::key_hold::HeldInput::Text(
                text.to_owned(),
            ));
            return;
        }
        if self.marked_range_utf16.is_none()
            && let Some(command) = self.text_command(cx)
            && let Some(route) = cx
                .try_global::<crate::app::commands::key_hold::TextCommandRouter>()
                .map(|router| router.0)
        {
            let queue = crate::app::commands::key_hold::HeldInputQueue::default();
            let editor = cx.entity().downgrade();
            if route(editor, command, text.to_owned(), queue.clone(), window, cx) {
                self.held_input = Some(queue);
                return;
            }
        }
        let grouping = if self.marked_range_utf16.is_some() {
            EditGroupKind::Composition
        } else {
            EditGroupKind::Typing
        };
        let range = self.bytes_for_utf16(range, cx);
        let bracket =
            grouping == EditGroupKind::Typing && self.type_bracket(range.clone(), text, cx);
        if !bracket {
            self.replace_text_with_grouping(range, text, Some(grouping), window, cx);
        }
        if grouping == EditGroupKind::Typing && !text.contains(['\n', '\r']) {
            self.outdent_typed_line(cx);
        }
    }

    /// Replace text at the given range (or current selection / marked range
    /// if `None`) with `new_text`, and mark the result as IME composing text.
    /// `new_selected_range` is relative to the start of the marked text (per
    /// Apple's `setMarkedText:selectedRange:replacementRange:`). The caret
    /// moves to the end of `new_selected_range`, and the anchor to its start,
    /// giving a visible selection within the preedit string.
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Text a binding claims never reaches the buffer, so neither does
        // its composition; the commit arrives as ordinary text.
        if !self.model.read(cx).is_editable() || self.text_command(cx).is_some() {
            return;
        }
        self.preferred_x = None;
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

    /// Return the bounds (in window-local px) of the given UTF-16 range, used
    /// by macOS to position the IME candidate window. We shape the line
    /// containing the range start and return a rect spanning from the start
    /// column's x to the end column's x (or at least the caret x if they
    /// coincide), at the line's vertical position.
    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let model = self.model.read(cx);
        let (start_line, start_col) = flat_utf16_to_position(model, range_utf16.start);
        let (end_line, end_col) = flat_utf16_to_position(model, range_utf16.end);
        let line = start_line;
        let line_str = line_text(model, line);
        let top = element_bounds.origin.y + px(line as f32 * LINE_HEIGHT) - px(self.scroll);

        if line_str.is_empty() {
            return Some(Bounds {
                origin: point(element_bounds.origin.x, top),
                size: size(px(1.), px(LINE_HEIGHT)),
            });
        }

        let shaped = self.shape_line(model, line, &line_str, window, cx);
        let tab_size = self.tab_size(model, cx);
        let x0 = x_for_index_dir(&shaped, start_col, &line_str, tab_size);
        let x1 = if start_line == end_line {
            x_for_index_dir(&shaped, end_col, &line_str, tab_size)
        } else {
            element_bounds.size.width
        };
        let (origin_x, width) = if start_line == end_line && x0 > x1 {
            (x1, (x0 - x1).max(px(1.)))
        } else {
            (x0, (x1 - x0).max(px(1.)))
        };
        let x_off = self.line_x_offset(&line_str, shaped.width, element_bounds.size.width);
        Some(Bounds {
            origin: point(element_bounds.origin.x + x_off + origin_x, top),
            size: size(width, px(LINE_HEIGHT)),
        })
    }

    /// Map a window-space point to a flat UTF-16 offset, for IME hit-testing.
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let model = self.model.read(cx);
        let (line, col) = self.hit_test(model, point, window, cx);
        Some(to_flat_utf16(model, line, col))
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
