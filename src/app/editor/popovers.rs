//! The popovers an editor floats over its text: the content behind each and
//! its painting. What each shows where it is anchored is
//! [`EditorPopovers`](knot_model::editor::popovers::EditorPopovers) state.
//!
//! A popover's content is a layout of views like a composite tab's, and its
//! views join the view tree as children of the editor's.

use knot_model::editor::popovers::{EditorPopover, PopoverSpec};
use knot_protocol::ExtensionKey;

use super::*;
use crate::app::convert::ThemePaint as _;
use crate::app::view_layout::{GroupSpec, Layout, ToolbarButton, toolbar};
use crate::app::view_tree::{ViewKind, ViewNode};
use crate::app::workbench::ExtensionView;

/// What `open_popover` sets: where the popover is anchored and how tall it
/// is, and what it shows.
pub(crate) struct PopoverContentSpec {
    pub spec: PopoverSpec,
    pub toolbar: Vec<ToolbarButton>,
    pub layout: GroupSpec,
}

/// The views of one open popover: its toolbar over its layout.
pub(crate) struct PopoverContent {
    toolbar: Vec<ToolbarButton>,
    layout: Layout,
    /// Whether one of its views has focus.
    inside: bool,
    /// Watch the views' focus; set up on the first render after the layout
    /// changes, which has the window.
    focus_subscriptions: Vec<Subscription>,
}

impl PopoverContent {
    fn new(
        toolbar: Vec<ToolbarButton>,
        layout: GroupSpec,
        window: &str,
        cx: &mut Context<Self>,
    ) -> Self {
        let layout = Layout::new(layout, window, 1., cx);
        cx.on_release(|this, cx| {
            let hosted = this.layout.take_hosted();
            crate::app::view_layout::detach_views(hosted, cx);
        })
        .detach();
        Self {
            toolbar,
            layout,
            inside: false,
            focus_subscriptions: Vec::new(),
        }
    }

    /// Replace what the popover shows, keeping the views of keyed members;
    /// returns the contributed views the new layout added.
    fn update(
        &mut self,
        toolbar: Vec<ToolbarButton>,
        layout: GroupSpec,
        window: &str,
        cx: &mut Context<Self>,
    ) -> Vec<ExtensionView> {
        self.toolbar = toolbar;
        self.focus_subscriptions.clear();
        let change = self.layout.update(layout, window, 1., cx);
        cx.notify();
        change.attached
    }

    /// Record whether a view of the popover has focus, telling observers of
    /// the view tree when it changed.
    fn sync_inside(&mut self, window: &Window, cx: &mut Context<Self>) {
        let inside = self.layout.contains_focus(window, cx);
        if inside != self.inside {
            self.inside = inside;
            crate::app::extensions::host::application_changed(cx);
        }
    }

    /// Treat the `index`th view as the one with focus, as its gaining focus
    /// does.
    #[cfg(test)]
    pub(crate) fn focus_leaf(&mut self, index: usize, cx: &mut Context<Self>) {
        self.layout.set_focused_leaf(index);
        self.inside = true;
        crate::app::extensions::host::application_changed(cx);
    }

    /// The popover's node of the view tree.
    fn node(&self, id: EntityId, cx: &App) -> ViewNode {
        self.layout.node(id, ViewKind::Popover, cx)
    }

    /// The views in layout order.
    #[cfg(test)]
    pub(crate) fn leaves(&self) -> Vec<crate::app::view_layout::Leaf> {
        self.layout.leaves().to_vec()
    }
}

impl Render for PopoverContent {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.layout.watch_focus(
            window,
            cx,
            |this: &Self| &this.layout,
            |_, cx| crate::app::extensions::host::application_changed(cx),
        );
        if self.focus_subscriptions.is_empty() {
            for leaf in self.layout.leaves().iter() {
                let handle = leaf.focus_handle(cx);
                self.focus_subscriptions.push(cx.on_focus_in(
                    &handle,
                    window,
                    |this, window, cx| this.sync_inside(window, cx),
                ));
                self.focus_subscriptions.push(cx.on_focus_out(
                    &handle,
                    window,
                    |this, _, window, cx| this.sync_inside(window, cx),
                ));
            }
        }
        let id = cx.entity_id();
        let toolbar = toolbar(
            &self.toolbar,
            "popover-toolbar",
            move |window, command, cx| {
                crate::app::product::run_popover_command(window, id, command, cx);
            },
            cx,
        );
        div().size_full().flex().flex_col().children(toolbar).child(
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .child(self.layout.render(cx)),
        )
    }
}

/// The content behind one open popover.
pub(super) struct PopoverView {
    owner: ExtensionKey,
    id: String,
    content: Entity<PopoverContent>,
}

/// Space left around a popover, in logical pixels.
const MARGIN: f32 = 8.;

impl Editor {
    /// Open `owner`'s popover `id` over the text in the window `window`, or
    /// update it in place. An update keeps the views of keyed members.
    /// Returns the contributed views the content added, which the host binds.
    pub(crate) fn open_popover(
        &mut self,
        owner: ExtensionKey,
        id: &str,
        window: &str,
        content: PopoverContentSpec,
        cx: &mut Context<Self>,
    ) -> Vec<ExtensionView> {
        let model = self.model.clone();
        model.update(cx, |model, _| {
            self.state
                .popovers_mut()
                .open(owner, id, content.spec, model)
        });
        let kept = self
            .popover_views
            .iter()
            .find(|popover| popover.owner == owner && popover.id == id)
            .map(|popover| popover.content.clone());
        let attached = match kept {
            Some(kept) => kept.update(cx, |view, cx| {
                view.update(content.toolbar, content.layout, window, cx)
            }),
            None => {
                let created =
                    cx.new(|cx| PopoverContent::new(content.toolbar, content.layout, window, cx));
                let attached = created.read(cx).layout.hosted_views();
                self.popover_views.push(PopoverView {
                    owner,
                    id: id.to_owned(),
                    content: created,
                });
                attached
            }
        };
        self.sync_popover_views(cx);
        crate::app::extensions::host::application_changed(cx);
        attached
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

    /// Drop the content of popovers the state closed, and order the rest as
    /// it does.
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
        crate::app::extensions::host::application_changed(cx);
        cx.notify();
    }

    /// The content of the open popovers, bottom to top.
    #[cfg(test)]
    pub(crate) fn popover_contents(&self) -> Vec<Entity<PopoverContent>> {
        self.popover_views
            .iter()
            .map(|popover| popover.content.clone())
            .collect()
    }

    /// The nodes of the open popovers, bottom to top, and the one with focus.
    pub(crate) fn popover_nodes(&self, cx: &App) -> (Vec<ViewNode>, Option<usize>) {
        let nodes = self
            .popover_views
            .iter()
            .map(|popover| {
                popover
                    .content
                    .read(cx)
                    .node(popover.content.entity_id(), cx)
            })
            .collect();
        let focus = self
            .popover_views
            .iter()
            .position(|popover| popover.content.read(cx).inside);
        (nodes, focus)
    }

    /// The focus handles of every view in the open popovers.
    pub(crate) fn popover_focus_handles(&self, cx: &App) -> Vec<FocusHandle> {
        self.popover_views
            .iter()
            .flat_map(|popover| popover.content.read(cx).layout.leaves().to_vec())
            .map(|leaf| leaf.focus_handle(cx))
            .collect()
    }

    fn popover_content(&self, popover: &EditorPopover) -> Option<&Entity<PopoverContent>> {
        self.popover_views
            .iter()
            .find(|view| view.owner == popover.owner && view.id == popover.id)
            .map(|view| &view.content)
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
                let content = editor.popover_content(popover)?.clone();
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
