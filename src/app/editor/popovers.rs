//! The popovers an editor floats over its text: the content behind each and
//! its painting. What each shows where it is anchored is
//! [`EditorPopovers`](knot_model::editor::popovers::EditorPopovers) state.
//!
//! A popover's content is a layout of views like a composite tab's, and its
//! views join the view tree as children of the editor's.

use knot_model::editor::popovers::{EditorPopover, PopoverCloseReason, PopoverSpec, place};
use knot_protocol::ExtensionKey;

use super::*;
use crate::app::convert::ThemePaint as _;
use crate::app::view_layout::{GroupSpec, Layout, Sizing, TOOLBAR_HEIGHT, ToolbarButton, toolbar};
use crate::app::view_tree::{ViewKind, ViewNode};
use crate::app::workbench::ExtensionView;

/// Knot closed a popover `owner` opened; see [`PopoverCloseReason`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PopoverClosed {
    pub owner: ExtensionKey,
    pub id: String,
    pub reason: PopoverCloseReason,
}

impl EventEmitter<PopoverClosed> for Editor {}

/// Where the window moves focus when the editor next renders.
pub(super) enum ContentFocus {
    /// To the focused member of the popover `id` of `owner`.
    Popover(ExtensionKey, String),
    /// To the focused member of the zone `id` of `owner`.
    Zone(ExtensionKey, String),
    /// Back to the editor's text, as a focused popover or zone closed.
    Text,
}

/// What `open_popover` sets: where the popover is anchored and how wide it
/// is, and what it shows.
pub(crate) struct PopoverContentSpec {
    pub spec: PopoverSpec,
    pub toolbar: Vec<ToolbarButton>,
    pub layout: GroupSpec,
}

/// The views of one open popover: its toolbar over its layout.
pub(crate) struct PopoverContent {
    toolbar: Vec<ToolbarButton>,
    /// None when the popover holds no view, leaving the toolbar alone.
    pub(super) layout: Option<Layout>,
    /// Whether one of its views has focus.
    pub(super) inside: bool,
    /// Watch the views' focus; set up on the first render after the layout
    /// changes, which has the window.
    focus_subscriptions: Vec<Subscription>,
}

impl PopoverContent {
    pub(super) fn new(
        toolbar: Vec<ToolbarButton>,
        layout: GroupSpec,
        window: &str,
        cx: &mut Context<Self>,
    ) -> Self {
        let layout = (layout.leaf_count() > 0).then(|| {
            Layout::new(
                layout,
                window,
                Sizing::Natural,
                crate::app::view_layout::ignore_commands(),
                cx,
            )
        });
        cx.on_release(|this, cx| {
            if let Some(layout) = &mut this.layout {
                let hosted = layout.take_hosted();
                crate::app::view_layout::detach_views(hosted, cx);
            }
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
    pub(super) fn update(
        &mut self,
        toolbar: Vec<ToolbarButton>,
        layout: GroupSpec,
        window: &str,
        cx: &mut Context<Self>,
    ) -> Vec<ExtensionView> {
        self.toolbar = toolbar;
        self.focus_subscriptions.clear();
        let attached = match (&mut self.layout, layout.leaf_count() > 0) {
            (Some(current), true) => current.update(layout, window, Sizing::Natural, cx).attached,
            (None, true) => {
                let layout = Layout::new(
                    layout,
                    window,
                    Sizing::Natural,
                    crate::app::view_layout::ignore_commands(),
                    cx,
                );
                let attached = layout.hosted_views();
                self.layout = Some(layout);
                attached
            }
            (Some(current), false) => {
                let hosted = current.take_hosted();
                crate::app::view_layout::detach_views(hosted, cx);
                self.layout = None;
                self.inside = false;
                Vec::new()
            }
            (None, false) => Vec::new(),
        };
        cx.notify();
        attached
    }

    /// Record whether a view of the popover has focus, telling observers of
    /// the view tree when it changed.
    fn sync_inside(&mut self, window: &Window, cx: &mut Context<Self>) {
        let inside = self
            .layout
            .as_ref()
            .is_some_and(|layout| layout.contains_focus(window, cx));
        if inside != self.inside {
            self.inside = inside;
            crate::app::extensions::host::application_changed(cx);
        }
    }

    /// Treat the `index`th view as the one with focus, as its gaining focus
    /// does.
    #[cfg(test)]
    pub(crate) fn focus_leaf(&mut self, index: usize, cx: &mut Context<Self>) {
        self.layout.as_ref().unwrap().set_focused_leaf(index);
        self.inside = true;
        crate::app::extensions::host::application_changed(cx);
    }

    /// Treat no view as having focus, as losing focus does.
    #[cfg(test)]
    pub(crate) fn unfocus(&mut self, cx: &mut Context<Self>) {
        self.inside = false;
        crate::app::extensions::host::application_changed(cx);
    }

    /// The height the content takes in window pixels, with its toolbar and
    /// border.
    pub(super) fn natural_height(&self, cx: &App) -> f32 {
        let toolbar = if self.toolbar.is_empty() {
            0.
        } else {
            TOOLBAR_HEIGHT * crate::app::zoom::factor(cx)
        };
        let layout = self
            .layout
            .as_ref()
            .map_or(0., |layout| layout.natural_height(cx));
        toolbar + layout + 2.
    }

    /// The node of the view tree for content of kind `kind`.
    pub(super) fn node(&self, id: EntityId, kind: ViewKind, cx: &App) -> ViewNode {
        match &self.layout {
            Some(layout) => layout.node(id, kind, cx),
            None => ViewNode::leaf(id, kind),
        }
    }

    /// The tooltips of the toolbar's buttons.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn toolbar_labels(&self) -> Vec<String> {
        self.toolbar
            .iter()
            .map(|button| button.label.clone())
            .collect()
    }

    pub(super) fn leaves_of_layout(&self) -> Vec<crate::app::view_layout::Leaf> {
        self.layout
            .as_ref()
            .map_or_else(Vec::new, |layout| layout.leaves().to_vec())
    }

    /// The views in layout order.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn leaves(&self) -> Vec<crate::app::view_layout::Leaf> {
        self.layout
            .as_ref()
            .map_or_else(Vec::new, |layout| layout.leaves().to_vec())
    }
}

impl Render for PopoverContent {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(layout) = &mut self.layout {
            layout.watch_focus(
                window,
                cx,
                |this: &Self| this.layout.as_ref().unwrap(),
                |_, cx| crate::app::extensions::host::application_changed(cx),
            );
        }
        if self.focus_subscriptions.is_empty() {
            let leaves = self
                .layout
                .as_ref()
                .map_or_else(Vec::new, |layout| layout.leaves().to_vec());
            for leaf in leaves.iter() {
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
                .children(self.layout.as_ref().map(|layout| layout.render(cx))),
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
        if content.spec.focus {
            self.content_focus = Some(ContentFocus::Popover(owner, id.to_owned()));
        }
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
                let attached = created
                    .read(cx)
                    .layout
                    .as_ref()
                    .map_or_else(Vec::new, |layout| layout.hosted_views());
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
        cx.notify();
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

    /// Close the popover with focus, else the newest, for Escape; false when
    /// none is open.
    pub(crate) fn dismiss_popover(&mut self, cx: &mut Context<Self>) -> bool {
        let index = self
            .popover_nodes(cx)
            .1
            .or_else(|| self.popover_views.len().checked_sub(1));
        let Some(index) = index else {
            return false;
        };
        let model = self.model.clone();
        let closed = model.update(cx, |model, _| {
            self.state
                .popovers_mut()
                .close_at(index, PopoverCloseReason::Escape, model)
        });
        self.sync_popover_views(cx);
        closed
    }

    /// Close every popover, as a click outside them does.
    pub(crate) fn close_popovers_for_click(&mut self, cx: &mut Context<Self>) {
        if self.popover_views.is_empty() {
            return;
        }
        let model = self.model.clone();
        model.update(cx, |model, _| {
            self.state
                .popovers_mut()
                .close_all(PopoverCloseReason::Click, model)
        });
        self.sync_popover_views(cx);
    }

    /// Whether a view inside a popover has focus, which makes an edit of the
    /// buffer one made there.
    pub(super) fn typing_in_popover(&self, cx: &App) -> bool {
        self.popover_views
            .iter()
            .any(|popover| popover.content.read(cx).inside)
    }

    /// Report the popovers the state closed, move focus back to the text if
    /// one with focus went, drop the content of the closed, and order the
    /// rest as the state does.
    pub(super) fn sync_popover_views(&mut self, cx: &mut Context<Self>) {
        for closed in self.state.popovers_mut().take_closed() {
            cx.emit(PopoverClosed {
                owner: closed.owner,
                id: closed.id,
                reason: closed.reason,
            });
        }
        let open = self.state.popovers().popovers();
        let lost_focus = self.popover_views.iter().any(|view| {
            view.content.read(cx).inside
                && !open
                    .iter()
                    .any(|popover| popover.owner == view.owner && popover.id == view.id)
        });
        if lost_focus {
            self.content_focus = Some(ContentFocus::Text);
            cx.notify();
        }
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
    #[cfg(any(test, feature = "test-support"))]
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
                    .node(popover.content.entity_id(), ViewKind::Popover, cx)
            })
            .collect();
        let focus = self
            .popover_views
            .iter()
            .position(|popover| popover.content.read(cx).inside);
        (nodes, focus)
    }

    /// The focus handles of every view in the open popovers and zones.
    pub(crate) fn hosted_focus_handles(&self, cx: &App) -> Vec<FocusHandle> {
        let zones = self.zone_views.iter().map(|zone| &zone.content);
        self.popover_views
            .iter()
            .map(|popover| &popover.content)
            .chain(zones)
            .flat_map(|content| content.read(cx).leaves_of_layout())
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

/// Where one popover floats, relative to the editor's top left.
pub(super) struct PopoverFrame {
    content: Entity<PopoverContent>,
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

impl EditorView {
    /// Move focus into a popover or zone that asked for it when it opened, or
    /// back to the text after the one with focus closed.
    pub(super) fn follow_content_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self
            .editor
            .update(cx, |editor, _| editor.content_focus.take())
        else {
            return;
        };
        let handle = match target {
            ContentFocus::Text => self.focus.clone(),
            ContentFocus::Zone(owner, id) => {
                let editor = self.editor.read(cx);
                let content = editor.zone_content_of(owner, &id);
                match content.and_then(|content| content.read(cx).layout.as_ref()) {
                    Some(layout) => layout.focused_handle(cx),
                    None => return,
                }
            }
            ContentFocus::Popover(owner, id) => {
                let editor = self.editor.read(cx);
                let content = editor
                    .state
                    .popovers()
                    .get(owner, &id)
                    .and_then(|popover| editor.popover_content(popover));
                match content {
                    Some(content) => match &content.read(cx).layout {
                        Some(layout) => layout.focused_handle(cx),
                        None => return,
                    },
                    None => return,
                }
            }
        };
        window.defer(cx, move |window, cx| window.focus(&handle, cx));
    }

    /// The popovers to show, bottom to top, each below or above its anchor
    /// and sized to its content. A popover is hidden while its anchor is out
    /// of view or folded away.
    pub(super) fn popover_frames(&self, cx: &App) -> Vec<PopoverFrame> {
        let editor = self.editor.read(cx);
        let model = self.model.read(cx);
        let zoom = crate::app::zoom::factor(cx);
        let margin = MARGIN * zoom;
        let line_height = self.metrics.line_height;
        let viewport = f32::from(self.bounds.size.height);
        let text_width = f32::from(self.text_bounds().size.width) - 2. * margin;
        editor
            .popovers()
            .popovers()
            .iter()
            .filter_map(|popover| {
                let lines = editor.popovers().anchor_lines(popover, model)?;
                let content = editor.popover_content(popover)?.clone();
                let wrap = editor.wrap();
                let first = wrap.first_row(lines.start as usize);
                let last = wrap.rows(lines.end as usize - 1).end;
                if last <= first {
                    return None;
                }
                let rows = |row: usize| (row as f32 - editor.scroll_top()) * line_height;
                let natural = content.read(cx).natural_height(cx);
                let placement = place(rows(first)..rows(last), viewport, natural, margin)?;
                Some(PopoverFrame {
                    content,
                    left: self.gutter.width + margin,
                    top: placement.top,
                    width: popover.spec.resolve_width(text_width, zoom),
                    height: placement.height,
                })
            })
            .collect()
    }

    /// The popovers over the text, bottom to top.
    pub(super) fn render_popovers(&self, cx: &App) -> Vec<AnyElement> {
        let theme = theme(cx);
        self.popover_frames(cx)
            .into_iter()
            .map(|frame| {
                deferred(
                    div()
                        .absolute()
                        .top(px(frame.top))
                        .left(px(frame.left))
                        .w(px(frame.width))
                        .h(px(frame.height))
                        .occlude()
                        .overflow_hidden()
                        .rounded(crate::app::zoom::rems(crate::app::tokens::radius::SM))
                        .border_1()
                        .border_color(theme.paint_color(color::POPOVER_BORDER))
                        .bg(theme.paint_color(color::POPOVER_BACKGROUND))
                        .child(frame.content),
                )
                .with_priority(1)
                .into_any_element()
            })
            .collect()
    }
}
