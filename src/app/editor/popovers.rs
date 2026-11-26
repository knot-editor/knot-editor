//! The popovers an editor floats over its text: the view behind each and its
//! painting. What each shows and where it is anchored is
//! [`EditorPopovers`](knot_model::editor::popovers::EditorPopovers) state.

use knot_model::editor::popovers::{EditorPopover, PopoverSpec};
use knot_protocol::ExtensionKey;

use super::*;
use crate::app::convert::ThemePaint as _;

/// The view of one open popover, which shows `buffer`.
pub(super) struct PopoverView {
    owner: ExtensionKey,
    id: String,
    buffer: Entity<Buffer>,
    view: Entity<EditorView>,
}

/// Space left around a popover, in logical pixels.
const MARGIN: f32 = 8.;

impl Editor {
    /// Open `owner`'s popover `id` over the text, showing `buffer`, or update
    /// it in place. An update keeps the view while it shows the same buffer.
    pub(crate) fn open_popover(
        &mut self,
        owner: ExtensionKey,
        id: &str,
        spec: PopoverSpec,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) {
        let model = self.model.clone();
        model.update(cx, |model, _| {
            self.state.popovers_mut().open(owner, id, spec, model)
        });
        let kept = self
            .popover_views
            .iter()
            .position(|popover| popover.owner == owner && popover.id == id);
        match kept {
            Some(index) if self.popover_views[index].buffer == buffer => {}
            Some(index) => {
                self.popover_views[index] = PopoverView {
                    owner,
                    id: id.to_owned(),
                    view: cx.new(|cx| EditorView::new(buffer.clone(), cx)),
                    buffer,
                };
            }
            None => self.popover_views.push(PopoverView {
                owner,
                id: id.to_owned(),
                view: cx.new(|cx| EditorView::new(buffer.clone(), cx)),
                buffer,
            }),
        }
        self.sync_popover_views(cx);
    }

    /// Close `owner`'s popover `id`; false when it has none.
    pub(crate) fn close_popover(
        &mut self,
        owner: ExtensionKey,
        id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let model = self.model.clone();
        let closed = model.update(cx, |model, _| {
            self.state.popovers_mut().close(owner, id, model)
        });
        self.sync_popover_views(cx);
        closed
    }

    /// Drop the views of popovers the state closed, and order the rest as it
    /// does.
    pub(super) fn sync_popover_views(&mut self, cx: &mut Context<Self>) {
        let open = self.state.popovers().popovers();
        if self.popover_views.len() == open.len()
            && self
                .popover_views
                .iter()
                .zip(open)
                .all(|(view, popover)| view.owner == popover.owner && view.id == popover.id)
        {
            return;
        }
        let mut views = std::mem::take(&mut self.popover_views);
        self.popover_views = open
            .iter()
            .filter_map(|popover| {
                let index = views
                    .iter()
                    .position(|view| view.owner == popover.owner && view.id == popover.id)?;
                Some(views.swap_remove(index))
            })
            .collect();
        cx.notify();
    }

    /// The views of the open popovers, bottom to top.
    #[cfg(test)]
    pub(crate) fn popover_view_entities(&self) -> Vec<Entity<EditorView>> {
        self.popover_views
            .iter()
            .map(|popover| popover.view.clone())
            .collect()
    }

    fn popover_view(&self, popover: &EditorPopover) -> Option<&Entity<EditorView>> {
        self.popover_views
            .iter()
            .find(|view| view.owner == popover.owner && view.id == popover.id)
            .map(|view| &view.view)
    }
}

impl EditorView {
    /// The popovers over the text, bottom to top, each below its anchor's
    /// last line. Positions are relative to the text area's top left.
    pub(super) fn render_popovers(&self, cx: &App) -> Vec<AnyElement> {
        let editor = self.editor.read(cx);
        let model = self.model.read(cx);
        let theme = theme(cx);
        let margin = crate::app::zoom::scaled(MARGIN, cx);
        editor
            .popovers()
            .popovers()
            .iter()
            .filter_map(|popover| {
                let lines = editor.popovers().anchor_lines(popover, model)?;
                let content = editor.popover_view(popover)?.clone();
                let row = editor.wrap().first_row(lines.end as usize) as f32;
                let top = px((row - editor.scroll_top()) * self.metrics.line_height);
                let height = crate::app::zoom::scaled(popover.spec.height, cx);
                Some(
                    deferred(
                        div()
                            .absolute()
                            .top(top)
                            .left(px(self.gutter.width) + margin)
                            .right(margin)
                            .h(height)
                            .occlude()
                            .overflow_hidden()
                            .rounded(crate::app::zoom::rems(crate::app::tokens::radius::SM))
                            .border_1()
                            .border_color(theme.paint_color(color::POPOVER_BORDER))
                            .bg(theme.paint_color(color::POPOVER_BACKGROUND))
                            .child(content),
                    )
                    .with_priority(1)
                    .into_any_element(),
                )
            })
            .collect()
    }
}
