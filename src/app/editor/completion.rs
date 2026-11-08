//! Completion requests, aggregation, and acceptance for one view.

use super::*;

impl EditorView {
    pub(super) fn completion_anchor(
        &self,
        editor: &Editor,
        window: &mut Window,
        cx: &App,
    ) -> Point<Pixels> {
        let row = editor.row(self.model.read(cx), editor.caret_row());
        let x = self.text_bounds().origin.x - self.bounds.origin.x
            + self.x_in_row(editor, &row, editor.caret().1, window, cx);
        let y = px((row.index as f32 + 1. - editor.scroll_top()) * self.metrics.line_height);
        // Keep the popup, with a margin, inside the view.
        let popup = |pixels| crate::app::zoom::scaled(pixels, cx);
        point(
            x.min((self.bounds.size.width - popup(380.)).max(px(0.))),
            y.min((self.bounds.size.height - popup(230.)).max(px(0.))),
        )
    }

    pub(super) fn completion_previous(
        &mut self,
        _: &CompletionPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor
            .update(cx, |editor, cx| editor.move_completion_selection(-1, cx));
    }

    pub(super) fn completion_next(
        &mut self,
        _: &CompletionNext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor
            .update(cx, |editor, cx| editor.move_completion_selection(1, cx));
    }

    pub(super) fn completion_accept(
        &mut self,
        _: &CompletionAccept,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor
            .update(cx, |editor, cx| editor.accept_selected_completion(cx));
    }

    pub(super) fn completion_dismiss(
        &mut self,
        _: &CompletionDismiss,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor
            .update(cx, |editor, cx| editor.dismiss_completion(cx));
    }
}
