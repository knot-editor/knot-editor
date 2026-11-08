//! The view that renders an [`Editor`] and forwards pointer and platform
//! text input to it.

use super::*;

pub struct EditorView {
    pub(super) element_id: usize,
    pub(super) rendering: EditorRenderingOptions,
    pub(super) editor: Entity<Editor>,
    /// The editor's model, which never changes.
    pub(super) model: Entity<BufferModel>,
    /// Text size and line height from the settings in the editor's scope.
    pub(super) metrics: EditorMetrics,
    /// Width of one monospace cell, which wrapping measures rows in.
    pub(super) cell_width: f32,
    /// Widest line width shaped since the text last changed. Bounds horizontal
    /// scrolling without shaping lines that were never displayed.
    pub(super) max_line_width: f32,
    /// The model revision `max_line_width` was measured at.
    pub(super) measured_revision: u64,
    /// The gutter's lanes as of the last paint.
    pub(super) gutter: GutterLayout,
    /// Whether the pointer is over the gutter, which shows fold chevrons.
    pub(super) gutter_hovered: bool,
    /// The painted bounds of each fold placeholder, with its line.
    pub(super) fold_placeholders: Vec<(Bounds<Pixels>, usize)>,
    /// Syntax highlights of the visible lines and some overscan, shared by
    /// painting and the caret, hit-testing, and IME geometry on those lines.
    pub(super) syntax_cache: RefCell<Option<SyntaxCache>>,
    /// Last measured editor pane bounds; written by the element during paint
    /// and read by the pointer handlers to map a position to a caret.
    pub(super) bounds: Bounds<Pixels>,
    /// What a drag with the left button held extends from, from mouse-down
    /// to mouse-up.
    pub(super) drag_anchor: Option<DragAnchor>,
    /// An action-region press owns the gesture through mouse-up; pointer
    /// jitter must not turn it into a text drag from the old caret.
    pub(super) suppress_drag_selection: bool,
    pub(super) caret_blink: CaretBlink,
    pub(super) focus: FocusHandle,
    pub(super) history_was_focused: bool,
    #[cfg(test)]
    pub(super) paint_count: u64,
    _editor_subscription: Subscription,
    _settings_subscription: Subscription,
    _zoom_subscription: Subscription,
}

impl EditorView {
    /// Build a view of a new editor over `model`.
    pub fn new(model: Entity<BufferModel>, cx: &mut Context<Self>) -> Self {
        Self::with_rendering(
            model,
            0,
            EditorRenderingOptions::default(),
            EDITOR_KEY_CONTEXT,
            cx,
        )
    }

    /// Build a view of a new editor over `model` inside the contributed view
    /// `kind`, without a gutter until the view asks for one.
    pub(crate) fn for_contributed_view(
        model: Entity<BufferModel>,
        kind: impl Into<Arc<str>>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_rendering(
            model,
            0,
            EditorRenderingOptions { show_gutter: false },
            kind,
            cx,
        )
    }

    #[cfg(test)]
    pub(crate) fn new_with_options(
        model: Entity<BufferModel>,
        element_id: usize,
        rendering: EditorRenderingOptions,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_rendering(model, element_id, rendering, EDITOR_KEY_CONTEXT, cx)
    }

    fn with_rendering(
        model: Entity<BufferModel>,
        element_id: usize,
        rendering: EditorRenderingOptions,
        kind: impl Into<Arc<str>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| Editor::new(model.clone(), kind, cx));
        let editor_subscription = cx.observe(&editor, |this, _, cx| {
            let revision = this.model.read(cx).revision();
            if this.measured_revision != revision {
                this.measured_revision = revision;
                this.max_line_width = 0.;
            }
            cx.notify();
        });
        // Settings resolve during layout, so any change only needs a repaint
        // once the metrics follow it.
        let settings_subscription = cx.observe_global::<ApplicationSettings>(|this, cx| {
            this.max_line_width = 0.;
            this.update_metrics(cx);
            cx.notify();
        });
        let zoom_subscription =
            cx.observe_global::<crate::app::zoom::ApplicationZoom>(|this, cx| {
                this.update_metrics(cx);
                cx.notify();
            });
        let measured_revision = model.read(cx).revision();
        let mut view = Self {
            element_id,
            rendering,
            editor,
            model,
            metrics: EditorMetrics::new(crate::app::typography::TEXT_SIZE),
            cell_width: 0.,
            max_line_width: 0.,
            measured_revision,
            gutter: GutterLayout::default(),
            gutter_hovered: false,
            fold_placeholders: Vec::new(),
            syntax_cache: RefCell::default(),
            bounds: Bounds::default(),
            drag_anchor: None,
            suppress_drag_selection: false,
            caret_blink: CaretBlink::default(),
            focus: cx.focus_handle(),
            history_was_focused: false,
            #[cfg(test)]
            paint_count: 0,
            _editor_subscription: editor_subscription,
            _settings_subscription: settings_subscription,
            _zoom_subscription: zoom_subscription,
        };
        view.update_metrics(cx);
        view
    }

    /// The editor this view renders.
    pub(crate) fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    pub(crate) fn model(&self) -> &Entity<BufferModel> {
        &self.model
    }

    /// Show or hide the gutter of markers and line numbers.
    pub(crate) fn set_show_gutter(&mut self, show: bool, cx: &mut Context<Self>) {
        if self.rendering.show_gutter != show {
            self.rendering.show_gutter = show;
            cx.notify();
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn interaction_bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    /// The part of `bounds` that shows text: everything right of the gutter.
    pub(crate) fn text_bounds(&self) -> Bounds<Pixels> {
        let gutter = px(self.gutter.width).min(self.bounds.size.width);
        Bounds {
            origin: point(self.bounds.origin.x + gutter, self.bounds.origin.y),
            size: size(self.bounds.size.width - gutter, self.bounds.size.height),
        }
    }

    /// The caret's x position with the caret moved to `col` on the first line.
    #[cfg(test)]
    pub(crate) fn caret_x_at(&mut self, col: usize, window: &Window, cx: &mut App) -> Pixels {
        self.editor.update(cx, |editor, _| {
            (editor.cursor_line, editor.cursor_col) = (0, col)
        });
        self.caret_x(self.editor.read(cx), window, cx)
    }

    /// Each text run on `line` as its length and underline color, as the
    /// view paints them.
    #[cfg(test)]
    pub(crate) fn underline_colors_for_test(
        &self,
        line: usize,
        cx: &App,
    ) -> Vec<(usize, Option<Hsla>)> {
        let editor = self.editor.read(cx);
        let text = line_text(self.model.read(cx), line);
        self.line_runs(editor, line, &text, cx)
            .into_iter()
            .map(|run| (run.len, run.underline.and_then(|underline| underline.color)))
            .collect()
    }

    /// Each gutter marker's line and color, as the view paints them.
    #[cfg(test)]
    pub(crate) fn gutter_colors_for_test(&self, cx: &App) -> Vec<(usize, Hsla)> {
        use crate::app::convert::ThemePaint as _;
        self.editor
            .read(cx)
            .gutter_markers
            .iter()
            .map(|marker| (marker.line, theme(cx).paint_color(&marker.key)))
            .collect()
    }

    #[cfg(test)]
    /// Select a source byte range, reveal its caret, and transfer focus here.
    pub(crate) fn select_reveal_and_focus(
        &mut self,
        range: ByteRange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window);
        self.editor.update(cx, |editor, cx| {
            editor.set_selection(
                range.start_byte_offset,
                range.end_byte_offset,
                Reveal::Nearest,
                cx,
            )
        });
    }

    /// Restart blinking with the caret shown when the caret, selection, text,
    /// or focus changed since the last render, and stop it while unfocused.
    pub(super) fn update_caret_blink(&mut self, focused: bool, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        let key = focused.then_some((
            editor.cursor_line,
            editor.cursor_col,
            editor.has_selection,
            editor.seen_revision,
        ));
        if key == self.caret_blink.key {
            return;
        }
        self.caret_blink = CaretBlink {
            key,
            visible: true,
            _task: key.map(|_| {
                cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor().timer(CARET_BLINK_INTERVAL).await;
                        let toggled = this.update(cx, |view, cx| {
                            view.caret_blink.visible = !view.caret_blink.visible;
                            cx.notify();
                        });
                        if toggled.is_err() {
                            break;
                        }
                    }
                })
            }),
        };
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EventEmitter<EditorContributionAction> for EditorView {}
