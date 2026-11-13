//! The editor element: layout, viewport-culled painting, and IME registration.

use super::*;
use crate::app::convert::ThemePaint as _;

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        if focused != self.history_was_focused {
            self.model
                .update(cx, |model, _| model.break_history_group());
            self.history_was_focused = focused;
        }
        self.update_caret_blink(focused, cx);
        self.follow_bar_focus(window, cx);
        let bars = self.render_bars(cx);
        let entity = cx.entity();
        let editor = self.editor.read(cx);
        let mut key_context = KeyContext::default();
        key_context.add(EDITOR_KEY_CONTEXT);
        if editor.completion_surface().is_some() {
            key_context.add("editor_completion");
        }
        let completion = editor.completion_surface().map(|surface| {
            let anchor = self.completion_anchor(editor, window, cx);
            crate::app::completion::render_completion(surface, anchor, theme(cx))
        });
        div()
            .id(("editor", self.element_id))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme(cx).paint_color(color::EDITOR_BACKGROUND))
            // Track focus so clicking the pane focuses this view (gpui
            // auto-focuses a tracked element on mouse-down) and so on_key_down
            // listeners below actually receive keystrokes.
            .track_focus(&self.focus)
            .key_context(key_context)
            .on_action(cx.listener(Self::completion_previous))
            .on_action(cx.listener(Self::completion_next))
            .on_action(cx.listener(Self::completion_accept))
            .on_action(cx.listener(Self::completion_dismiss))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            // Mouse-move extends the selection while the left button is held
            // (drag-select). `MouseMoveEvent::dragging()` is true when the
            // platform reports the left button as currently pressed, so the
            // handler self-gates and we don't need a separate mouse-up.
            // Scroll wheel: accumulate pixel delta, clamp against the real
            // viewport height (written each paint by the element). macOS
            // "natural" scroll: trackpad two-finger gesture down moves the
            // content up, i.e. the viewport descends — scroll increases.
            // ScrollEvent delta is positive for swipes upward (toward
            // content top), so we invert it here.
            .on_scroll(cx.listener(|this, ev: &ScrollEvent, _, cx| {
                let (dx, dy) = match ev.delta {
                    ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
                    ScrollDelta::Lines(p) => (
                        p.x * this.metrics.line_height,
                        p.y * this.metrics.line_height,
                    ),
                };
                let rows = -dy / this.metrics.line_height;
                let max_x = this.max_line_width + 2. - f32::from(this.text_bounds().size.width);
                this.editor.update(cx, |editor, cx| {
                    let max_x = if editor.wrap().width().is_some() {
                        0.
                    } else {
                        max_x
                    };
                    editor.scroll_by(rows, -dx, max_x, cx)
                });
            }))
            .children(bars)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(EditorElement { entity })
                    .children(completion),
            )
    }
}
/// The custom `Element`. Owns no state itself; reads everything from the
/// `EditorView` entity and its editor each layout/paint. Implements `Element` directly
/// (not `RenderOnce`) per the `Element` module doc's recommendation for
/// code editors.
pub(super) struct EditorElement {
    entity: Entity<EditorView>,
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorHitboxes;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        let layout_id = window.request_layout(style, [], cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let (gutter, markers) = self.entity.update(cx, |view, cx| {
            view.update_metrics(cx);
            view.update_gutter_layout(window, cx);
            view.update_wrap(
                bounds.size.width - px(view.gutter.width).min(bounds.size.width),
                window,
                cx,
            );
            let editor = view.editor.read(cx);
            let rows = editor.visible_rows();
            let line_height = view.metrics.line_height;
            let markers = editor
                .gutter_markers()
                .iter()
                .filter(|marker| marker.command.is_some() && !editor.wrap().is_hidden(marker.line))
                .map(|marker| editor.wrap().first_row(marker.line))
                .filter(|row| rows.contains(row))
                .map(|row| Bounds {
                    origin: point(
                        bounds.origin.x,
                        bounds.origin.y + px((row as f32 - editor.scroll_top()) * line_height),
                    ),
                    size: size(px(view.gutter.markers_width), px(line_height)),
                })
                .collect::<Vec<_>>();
            (view.gutter, markers)
        });
        let gutter_width = px(gutter.width).min(bounds.size.width);
        let text = Bounds {
            origin: point(bounds.origin.x + gutter_width, bounds.origin.y),
            size: size(bounds.size.width - gutter_width, bounds.size.height),
        };
        EditorHitboxes {
            text: window.insert_hitbox(text, HitboxBehavior::Normal),
            markers: markers
                .into_iter()
                .map(|bounds| window.insert_hitbox(bounds, HitboxBehavior::Normal))
                .collect(),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        hitboxes: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // The gutter keeps the default arrow; markers that run a command show
        // a hand.
        window.set_cursor_style(CursorStyle::IBeam, &hitboxes.text);
        for marker in &hitboxes.markers {
            window.set_cursor_style(CursorStyle::PointingHand, marker);
        }
        self.entity.update(cx, |view, cx| {
            let resized = view.bounds.size != bounds.size;
            view.bounds = bounds;
            let rows = f32::from(bounds.size.height) / view.metrics.line_height;
            let reveal_x = view
                .editor
                .update(cx, |editor, _| editor.lay_out(rows, resized));
            if reveal_x {
                let x = f32::from(view.caret_x(view.editor.read(cx), window, cx));
                let width = f32::from(view.text_bounds().size.width);
                view.editor
                    .update(cx, |editor, _| editor.reveal_x(x, width));
            }
        });
        let drag_listener = window.listener_for(&self.entity, EditorView::on_mouse_move);
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                drag_listener(event, window, cx);
            }
        });
        let release_listener = window.listener_for(&self.entity, |view, _: &MouseUpEvent, _, _| {
            view.drag_anchor = None;
        });
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                release_listener(event, window, cx);
            }
        });
        let (scroll_x, metrics, cell_width, base_font) = {
            let view = self.entity.read(cx);
            (
                view.editor.read(cx).scroll_x(),
                view.metrics,
                view.cell_width,
                view.font.clone(),
            )
        };
        // Read the view state, compute the visible rows, then read just the
        // visible lines and their runs out of the model before painting:
        // `ShapedLine::paint` takes `cx: &mut App`, but `entity.read(cx)`
        // borrows `cx: &App` for as long as the borrow lives. We also pull the
        // caret, focus handle, and theme colors so we can paint without
        // re-borrowing.
        let (
            scroll,
            scroll_max,
            vis,
            tab_size,
            caret,
            selection,
            marked,
            brackets,
            fills,
            gutter,
            line_highlight,
            colors,
            focus_handle,
            entity,
        ): EditorPaintState = {
            let view = self.entity.read(cx);
            let editor = view.editor.read(cx);
            let model = view.model.read(cx);
            let theme = theme(cx);
            let max = (editor.wrap().row_count() as f32) * metrics.line_height;
            let rows = editor.visible_rows();
            let lines = editor.visible_lines();
            view.cache_syntax(model, lines.clone());
            // Each line is read and highlighted once, then sliced per row.
            let mut vis: Vec<PaintRow> = Vec::new();
            let mut line: Option<(usize, String, Vec<TextRun>)> = None;
            for index in rows.clone() {
                let row = editor.row(model, index);
                if line.as_ref().is_none_or(|(at, ..)| *at != row.line) {
                    let text = line_text(model, row.line);
                    let runs = view.line_runs(editor, row.line, &text, cx);
                    line = Some((row.line, text, runs));
                }
                let (_, text, runs) = line.as_ref().expect("the row's line was just read");
                vis.push(PaintRow {
                    text: text[row.range.clone()].to_owned(),
                    runs: slice_runs(runs, row.range.clone()),
                    folded: row.last && editor.is_fold_start(row.line),
                    row,
                });
            }
            // Only paint the caret when it's shown in its blink cycle and on a
            // visible row; otherwise it is clipped anyway, so skip the
            // shaping cost.
            let caret = (view.caret_blink.visible && rows.contains(&editor.caret_row()))
                .then(|| (editor.caret().0, editor.caret().1, editor.caret_shape()));
            // Convert marked UTF-16 range to (line, byte_col) start/end for
            // underline painting.
            let marked = editor.marked_range_utf16().as_ref().map(|mr| {
                let s = flat_utf16_to_position(model, mr.start);
                let e = flat_utf16_to_position(model, mr.end);
                (s, e)
            });
            let brackets = editor
                .matched_brackets(model)
                .into_iter()
                .flatten()
                .map(|range| {
                    let (line, start) = flat_byte_to_position(model, range.start);
                    (line, start, start + range.len())
                })
                .filter(|(line, ..)| lines.contains(line))
                .collect();
            let fills = decoration_fills(editor.decorations(), vis.iter().map(|row| &row.row))
                .into_iter()
                .map(|fill| (fill.row, fill.columns, theme.paint_color(fill.key)))
                .collect();
            // Numbers and markers show on each line's first row.
            let first_row = |line: usize| {
                Some(editor.wrap().first_row(line))
                    .filter(|row| rows.contains(row) && !editor.wrap().is_hidden(line))
            };
            let folds = match view.gutter.folding {
                crate::app::settings::Folding::Off => Vec::new(),
                folding => {
                    let show =
                        folding == crate::app::settings::Folding::Always || view.gutter_hovered;
                    editor
                        .fold_marks(lines.clone(), cx)
                        .into_iter()
                        .filter(|(_, collapsed)| show || *collapsed)
                        .filter_map(|(line, collapsed)| Some((first_row(line)?, collapsed)))
                        .collect()
                }
            };
            let gutter = GutterPaint {
                layout: view.gutter,
                folds,
                numbers: view
                    .gutter_numbers(editor, rows.clone())
                    .into_iter()
                    .map(|(row, label, active)| (row, label.to_string().into(), active))
                    .collect(),
                markers: if view.rendering.show_gutter {
                    editor
                        .gutter_markers()
                        .iter()
                        .filter_map(|marker| {
                            Some((first_row(marker.line)?, theme.paint_color(&marker.key)))
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                background: theme.paint_color(color::EDITOR_GUTTER_BACKGROUND),
                number: theme.paint_color(color::EDITOR_LINE_NUMBER),
                active_number: theme.paint_color(color::EDITOR_LINE_NUMBER_ACTIVE),
            };
            let line_highlight = editor
                .highlighted_line()
                .map(|line| editor.wrap().rows(line))
                .map(|lit| lit.start.max(rows.start)..lit.end.min(rows.end))
                .filter(|lit| !lit.is_empty());
            (
                editor.scroll_top() * metrics.line_height,
                max,
                vis,
                editor.tab_size(model, cx),
                caret,
                editor.selection_range(),
                marked,
                brackets,
                fills,
                gutter,
                line_highlight,
                PaintColors {
                    selection: theme.paint_color(color::EDITOR_SELECTION),
                    caret: theme.paint_color(color::EDITOR_CURSOR),
                    marked_text: theme.paint_color(color::EDITOR_MARKED_TEXT),
                    bracket_match: theme.paint_color(color::EDITOR_BRACKET_MATCH),
                    line_highlight: theme.paint_color(color::EDITOR_LINE_HIGHLIGHT),
                    fold_placeholder: theme.paint_color(color::EDITOR_FOLD_PLACEHOLDER),
                    scrollbar: theme.paint_color(color::SCROLLBAR_THUMB),
                },
                view.focus.clone(),
                self.entity.clone(),
            )
        };

        // Record the painted bounds for pointer hit testing. No notify: this
        // must not trigger a re-render.
        let full_bounds = bounds;
        let bounds = self.entity.update(cx, |view, _cx| {
            view.bounds = full_bounds;
            #[cfg(test)]
            {
                view.paint_count += 1;
            }
            view.text_bounds()
        });

        // Register the IME input handler. `handle_input` self-gates on
        // focus — it only registers if `focus_handle.is_focused(window)`,
        // so calling it unconditionally is safe. Must be called during
        // paint (debug_assert_paint). `ElementInputHandler` wraps our
        // `EntityInputHandler` impl and forwards all calls through
        // `entity.update`.
        if focus_handle.is_focused(window) {
            window.handle_input(&focus_handle, ElementInputHandler::new(bounds, entity), cx);
        }

        let font_size = px(metrics.font_size);
        let line_height = px(metrics.line_height);
        let focused = focus_handle.is_focused(window);
        let top = |row: usize| bounds.origin.y + px(row as f32 * metrics.line_height) - px(scroll);

        // Clip to viewport so paint below the last line / outside horizontal
        // extent doesn't bleed into neighbouring panes.
        let mask = Some(ContentMask { bounds });
        let pane_w = bounds.size.width;
        let widest = window.with_content_mask(mask, |window| {
            // Shape each visible non-empty row once and reuse the
            // `ShapedLine` for every highlight and the text, so we don't pay
            // for shaping twice per frame. The x offset is where the row's
            // text starts: its continuation indent, right-aligned for RTL
            // rows, after horizontal scroll.
            let shaped: Vec<(&PaintRow, Option<ShapedLine>, Pixels)> = vis
                .iter()
                .map(|row| {
                    let shaped = (!row.text.is_empty()).then(|| {
                        shape_editor_line(
                            window,
                            SharedString::from(row.text.clone()),
                            font_size,
                            &row.runs,
                            tab_size,
                        )
                    });
                    let width = shaped.as_ref().map_or(px(0.), |shaped| shaped.width);
                    let rtl = if is_rtl_line(&row.text) {
                        (pane_w - width).max(px(0.))
                    } else {
                        px(0.)
                    };
                    (
                        row,
                        shaped,
                        px(row.row.indent as f32 * cell_width) + rtl - px(scroll_x),
                    )
                })
                .collect();
            // The x of byte `col` of a row's line, from the row's text start.
            let x_at = |row: &PaintRow, shaped: &Option<ShapedLine>, col: usize| {
                let range = &row.row.range;
                shaped.as_ref().map_or(px(0.), |shaped| {
                    x_for_index_dir(
                        shaped,
                        col.clamp(range.start, range.end) - range.start,
                        &row.text,
                        tab_size,
                    )
                })
            };
            // The x extent in the pane of the part of `start..end` a row
            // shows: from where it begins, or the pane's edge when it began
            // on an earlier row, to where it ends, or the other edge when it
            // continues. Right-to-left rows mirror the edges.
            let span = |(row, shaped, x_off): &(&PaintRow, Option<ShapedLine>, Pixels),
                        start: (usize, usize),
                        end: (usize, usize)| {
                let (begins, ends) = row.row.span(start, end)?;
                let at = |col| *x_off + x_at(row, shaped, col);
                let (lo, hi) = if is_rtl_line(&row.text) {
                    (ends.map_or(px(0.), at), begins.map_or(pane_w, at))
                } else {
                    (begins.map_or(px(0.), at), ends.map_or(pane_w, at))
                };
                Some((lo.min(hi), lo.max(hi)))
            };

            // The cursor line's background, behind everything else.
            if let Some(rows) = &line_highlight {
                let rect = Bounds {
                    origin: point(bounds.origin.x, top(rows.start)),
                    size: size(pane_w, line_height * rows.len() as f32),
                };
                window.paint_quad(fill(rect, colors.line_highlight));
            }

            // Decoration backgrounds: line bands across the text area, then
            // spans behind the decorated text of each row.
            for (position, columns, color) in &fills {
                let (row, s, x_off) = &shaped[*position];
                let (lo, hi) = match columns {
                    None => (px(0.), pane_w),
                    Some(columns) => {
                        let x0 = x_at(row, s, columns.start);
                        let x1 = x_at(row, s, columns.end);
                        (*x_off + x0.min(x1), *x_off + x0.max(x1))
                    }
                };
                let rect = Bounds {
                    origin: point(bounds.origin.x + lo, top(row.row.index)),
                    size: size(hi - lo, line_height),
                };
                window.paint_quad(fill(rect, *color));
            }

            // Selection highlight, painted BEHIND the text: the selected span
            // of each row, reaching the pane's edges where the selection
            // continues, so it reads as contiguous across rows and blank
            // lines.
            if let Some((start, end)) = selection {
                for entry in &shaped {
                    if let Some((lo, hi)) = span(entry, start, end) {
                        let rect = Bounds {
                            origin: point(bounds.origin.x + lo, top(entry.0.row.index)),
                            size: size(hi - lo, line_height),
                        };
                        window.paint_quad(fill(rect, colors.selection));
                    }
                }
            }

            // Matched brackets, behind the text.
            for (line, start, end) in &brackets {
                let Some((row, s, x_off)) = shaped
                    .iter()
                    .find(|(row, ..)| row.row.holds((*line, *start)))
                else {
                    continue;
                };
                let x0 = x_at(row, s, *start);
                let x1 = x_at(row, s, *end);
                let (lo, hi) = (x0.min(x1), x0.max(x1));
                let rect = Bounds {
                    origin: point(bounds.origin.x + *x_off + lo, top(row.row.index)),
                    size: size(hi - lo, line_height),
                };
                window.paint_quad(fill(rect, colors.bracket_match));
            }

            // Text, reusing the shaped rows from above.
            for (row, s, x_off) in &shaped {
                if let Some(s) = s {
                    let origin = point(bounds.origin.x + *x_off, top(row.row.index));
                    let _ = s.paint(origin, line_height, TextAlign::Left, None, window, cx);
                }
            }

            // The placeholder after a collapsed fold's first line.
            let mut placeholders = Vec::new();
            for (row, s, x_off) in &shaped {
                if !row.folded {
                    continue;
                }
                let label = SharedString::from("…");
                let run = TextRun {
                    len: label.len(),
                    font: base_font.clone(),
                    color: colors.fold_placeholder,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let glyph = window
                    .text_system()
                    .shape_line(label, font_size, &[run], None);
                let pad = px(4.);
                let left = bounds.origin.x + *x_off + s.as_ref().map_or(px(0.), |s| s.width) + pad;
                let rect = Bounds {
                    origin: point(left, top(row.row.index) + px(2.)),
                    size: size(glyph.width + pad * 2., line_height - px(4.)),
                };
                placeholders.push((rect, row.row.line));
                window.paint_quad(fill(rect, colors.fold_placeholder.opacity(0.2)));
                let _ = glyph.paint(
                    point(left + pad, top(row.row.index)),
                    line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }

            self.entity
                .update(cx, |view, _| view.fold_placeholders = placeholders);

            // IME preedit (marked text) underline: a bar at the bottom of the
            // marked span of each row, partitioned like the selection.
            if let Some((start, end)) = marked {
                let underline_h = px(1.5);
                for entry in &shaped {
                    if let Some((lo, hi)) = span(entry, start, end) {
                        let rect = Bounds {
                            origin: point(
                                bounds.origin.x + lo,
                                top(entry.0.row.index) + line_height - underline_h,
                            ),
                            size: size((hi - lo).max(px(2.)), underline_h),
                        };
                        window.paint_quad(fill(rect, colors.marked_text));
                    }
                }
            }

            // Caret, painted only when the editor holds window focus AND
            // there is no active selection (macOS hides the caret while a
            // selection is drag-held). `caret` is `None` during the hidden
            // half of a blink.
            if focused
                && selection.is_none()
                && let Some((line_ix, col, shape)) = caret
                && let Some((row, s, x_off)) = shaped
                    .iter()
                    .find(|(row, ..)| row.row.holds((line_ix, col)))
            {
                let caret_x = x_at(row, s, col);
                // The character under the caret spans to the next grapheme
                // boundary; past the line's end there is none, so use half
                // the line height.
                let cell_w = row
                    .text
                    .get(col - row.row.range.start..)
                    .and_then(|rest| rest.graphemes(true).next())
                    .map(|grapheme| (x_at(row, s, col + grapheme.len()) - caret_x).abs())
                    .filter(|width| *width > px(0.))
                    .unwrap_or(line_height / 2.);
                let top = top(row.row.index);
                let left = bounds.origin.x + *x_off + caret_x;
                let (origin, extent) = match shape {
                    CaretShape::Bar => (point(left, top), size(px(2.), line_height)),
                    // Translucent, so the character stays legible beneath.
                    CaretShape::Block => (point(left, top), size(cell_w, line_height)),
                    CaretShape::Underline => (
                        point(left, top + line_height - px(2.)),
                        size(cell_w, px(2.)),
                    ),
                };
                let color = match shape {
                    CaretShape::Block => colors.caret.opacity(0.5),
                    _ => colors.caret,
                };
                window.paint_quad(fill(
                    Bounds {
                        origin,
                        size: extent,
                    },
                    color,
                ));
            }

            shaped
                .iter()
                .filter_map(|(_, s, _)| s.as_ref().map(|s| f32::from(s.width)))
                .fold(0., f32::max)
        });

        // The gutter, beside the text area rather than over it.
        let gutter_bounds = Bounds {
            origin: full_bounds.origin,
            size: size(
                px(gutter.layout.width).min(full_bounds.size.width),
                full_bounds.size.height,
            ),
        };
        if gutter.layout.width > 0. {
            window.with_content_mask(
                Some(ContentMask {
                    bounds: gutter_bounds,
                }),
                |window| {
                    window.paint_quad(fill(gutter_bounds, gutter.background));
                    for (row, color) in &gutter.markers {
                        let side = px(7. * metrics.scale());
                        let marker_bounds = Bounds {
                            origin: point(
                                gutter_bounds.origin.x + px(4. * metrics.scale()),
                                top(*row) + (line_height - side) / 2.,
                            ),
                            size: size(side, side),
                        };
                        window.paint_quad(fill(marker_bounds, *color));
                    }
                    for (row, collapsed) in &gutter.folds {
                        let half = px(3. * metrics.scale());
                        let center = point(
                            gutter_bounds.origin.x + px(gutter.layout.folds_center()),
                            top(*row) + line_height / 2.,
                        );
                        let mut path = PathBuilder::stroke(px(1.5 * metrics.scale()));
                        if *collapsed {
                            path.move_to(point(center.x - half / 2., center.y - half));
                            path.line_to(point(center.x + half / 2., center.y));
                            path.line_to(point(center.x - half / 2., center.y + half));
                        } else {
                            path.move_to(point(center.x - half, center.y - half / 2.));
                            path.line_to(point(center.x, center.y + half / 2.));
                            path.line_to(point(center.x + half, center.y - half / 2.));
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(
                                path,
                                if *collapsed {
                                    gutter.active_number
                                } else {
                                    gutter.number
                                },
                            );
                        }
                    }
                    for (row, label, active) in &gutter.numbers {
                        let run = TextRun {
                            len: label.len(),
                            font: base_font.clone(),
                            color: if *active {
                                gutter.active_number
                            } else {
                                gutter.number
                            },
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let shaped =
                            window
                                .text_system()
                                .shape_line(label.clone(), font_size, &[run], None);
                        let origin = point(
                            gutter_bounds.origin.x + px(gutter.layout.numbers_end) - shaped.width,
                            top(*row),
                        );
                        let _ =
                            shaped.paint(origin, line_height, TextAlign::Left, None, window, cx);
                    }
                },
            );
        }
        self.entity.update(cx, |view, _| {
            view.max_line_width = view.max_line_width.max(widest);
        });

        // Scrollbar overlay: paint a thin vertical thumb at the right edge
        // of the editor bounds indicating the current scroll position. Painted
        // OUTSIDE the content-mask so it always shows at the pane's right edge
        // regardless of how far the content has scrolled. Only drawn when the
        // content exceeds the viewport.
        let viewport_h = f32::from(bounds.size.height);
        let content_h = scroll_max;
        if content_h > viewport_h && viewport_h > 0. {
            let track = 6.0_f32;
            let thumb_h = (viewport_h * viewport_h / content_h).max(track);
            let origin_y = f32::from(bounds.origin.y);
            let thumb_y = origin_y + (scroll.max(0.) / content_h) * (viewport_h - thumb_h);
            let thumb_bounds = Bounds {
                origin: point(bounds.origin.x + bounds.size.width - px(track), px(thumb_y)),
                size: size(px(track), px(thumb_h)),
            };
            window.paint_quad(fill(thumb_bounds, colors.scrollbar));
        }
    }
}
pub(super) type EditorPaintState = (
    f32,
    f32,
    Vec<PaintRow>,
    usize,
    Option<(usize, usize, CaretShape)>,
    Option<((usize, usize), (usize, usize))>,
    Option<((usize, usize), (usize, usize))>,
    Vec<(usize, usize, usize)>,
    Vec<(usize, Option<Range<usize>>, Hsla)>,
    GutterPaint,
    Option<Range<usize>>,
    PaintColors,
    FocusHandle,
    Entity<EditorView>,
);

/// One visible row's text and runs, copied out for one paint.
pub(super) struct PaintRow {
    row: Row,
    text: String,
    runs: Vec<TextRun>,
    /// Whether the row ends the first line of a collapsed fold.
    folded: bool,
}

/// Pointer regions inserted before paint: the text area and each visible
/// marker that runs a command.
pub(super) struct EditorHitboxes {
    text: Hitbox,
    markers: Vec<Hitbox>,
}

/// The gutter's lanes and visible content, copied out for one paint.
pub(super) struct GutterPaint {
    layout: GutterLayout,
    /// The row of each visible line number, its label, and whether it is
    /// the cursor's line.
    numbers: Vec<(usize, SharedString, bool)>,
    /// The row of each visible fold chevron, and whether its fold is collapsed.
    folds: Vec<(usize, bool)>,
    /// The row and color of each visible marker.
    markers: Vec<(usize, Hsla)>,
    background: Hsla,
    number: Hsla,
    active_number: Hsla,
}

/// Theme colors copied out for one paint.
pub(super) struct PaintColors {
    selection: Hsla,
    caret: Hsla,
    marked_text: Hsla,
    bracket_match: Hsla,
    line_highlight: Hsla,
    fold_placeholder: Hsla,
    scrollbar: Hsla,
}
