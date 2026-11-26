//! A center tab whose layout of child views a package describes.
//!
//! A package registers a kind and opens instances; the instance owns the
//! views it contains, which the user cannot rearrange. Each child is an
//! ordinary view, so commands, keymaps, and editor-driven packages work in
//! it as anywhere else.

use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{prelude::*, *};
use knot_model::diff::{DiffSide, LineHunk, map_line};
use knot_model::workbench::CompositeViewId;
use knot_protocol::IconRef;

use super::editor::{Editor, Scrolled};
use super::resource::ResourceUri;
use super::view_layout::{
    GroupSpec, Layout, LayoutChange, RunCommand, Shared, Sizing, ToolbarButton, toolbar,
};
use super::view_tree::{ViewFocusChanged, ViewKind, ViewNode, ViewTree, ViewTreeChanged};
use super::workbench::{CommandView, ExtensionView};

static NEXT_COMPOSITE_ID: AtomicU64 = AtomicU64::new(1);

/// The identity of a new composite view.
pub(crate) fn allocate_composite_id() -> CompositeViewId {
    CompositeViewId(NEXT_COMPOSITE_ID.fetch_add(1, Ordering::Relaxed))
}

/// What the tab of a composite view shows, and its toolbar.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CompositeTab {
    pub title: String,
    pub icon: Option<IconRef>,
    /// The resource whose decorations the tab shows, as a document tab does.
    pub resource: Option<ResourceUri>,
    pub toolbar: Vec<ToolbarButton>,
    /// Two editors whose scrolling follows each other.
    pub link: Option<ScrollLink>,
}

/// Which lines of two editors lie side by side, so scrolling one scrolls the
/// other to the same place.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScrollLink {
    /// The key of the first editor's member.
    pub first: String,
    /// The key of the second editor's member.
    pub second: String,
    /// The regions where the two texts differ, ascending and disjoint; lines
    /// between them match one for one.
    pub map: Vec<LinkedLines>,
}

/// A region of the first editor's lines that stands for a region of the
/// second's.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LinkedLines {
    pub first: Range<u32>,
    pub second: Range<u32>,
}

impl ScrollLink {
    /// Whether the link joins two different editors, which `is_editor`
    /// finds by member key.
    pub(crate) fn joins(&self, is_editor: impl Fn(&str) -> bool) -> bool {
        self.first != self.second && is_editor(&self.first) && is_editor(&self.second)
    }

    fn hunks(&self) -> Vec<LineHunk> {
        self.map
            .iter()
            .map(|region| LineHunk {
                old: region.first.clone(),
                new: region.second.clone(),
            })
            .collect()
    }
}

/// A scroll link resolved to its editors.
struct Linked {
    first: Entity<Editor>,
    second: Entity<Editor>,
    hunks: Vec<LineHunk>,
    /// Where each editor was scrolled to when last brought in line.
    synced: [f32; 2],
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone)]
pub(crate) struct CompositeCommandTarget {
    shared: Rc<Shared>,
}

impl CompositeCommandTarget {
    pub(crate) fn command_view(&self) -> CommandView {
        self.shared.focused_leaf().command_view()
    }
}

pub(crate) struct CompositeView {
    tab: CompositeTab,
    layout: Layout,
    /// The editors the tab's scroll link joins, when it names two the layout
    /// has.
    linked: Option<Linked>,
}

impl CompositeView {
    /// A view of `layout` in the window `window`; the layout is a row or
    /// column that holds at least one view.
    pub(crate) fn new(
        tab: CompositeTab,
        layout: GroupSpec,
        window: &str,
        cx: &mut Context<Self>,
    ) -> Self {
        let this = cx.weak_entity();
        let run: RunCommand = Rc::new(move |window, command, cx| {
            if let Some(view) = this.upgrade() {
                crate::app::product::run_toolbar_command(window, view, command, cx);
            }
        });
        let layout = Layout::new(layout, window, Sizing::Fill, run, cx);
        cx.on_release(|this, cx| {
            let hosted = this.layout.take_hosted();
            crate::app::view_layout::detach_views(hosted, cx);
        })
        .detach();
        let mut view = Self {
            tab,
            layout,
            linked: None,
        };
        view.relink(cx);
        view
    }

    /// Replace the layout with `layout`; see [`Layout::update`].
    pub(crate) fn update_layout(
        &mut self,
        layout: GroupSpec,
        window: &str,
        cx: &mut Context<Self>,
    ) -> LayoutChange {
        let change = self.layout.update(layout, window, Sizing::Fill, cx);
        self.relink(cx);
        cx.emit(ViewTreeChanged);
        if change.focus_moved {
            cx.emit(ViewFocusChanged);
        }
        cx.notify();
        change
    }

    pub(crate) fn tab(&self) -> &CompositeTab {
        &self.tab
    }

    pub(crate) fn set_tab(&mut self, tab: CompositeTab, cx: &mut Context<Self>) {
        let relink = tab.link != self.tab.link;
        self.tab = tab;
        if relink {
            self.relink(cx);
        }
        cx.notify();
    }

    /// Whether an editor member of the layout has this key.
    pub(crate) fn has_editor_key(&self, key: &str, cx: &App) -> bool {
        self.layout.keyed_editor(key, cx).is_some()
    }

    /// Join the editors the tab's link names, which the first leads until
    /// either scrolls.
    fn relink(&mut self, cx: &mut Context<Self>) {
        self.linked = None;
        let Some(link) = self.tab.link.clone() else {
            return;
        };
        let (Some(first), Some(second)) = (
            self.layout.keyed_editor(&link.first, cx),
            self.layout.keyed_editor(&link.second, cx),
        ) else {
            return;
        };
        let [first, second] = [first, second].map(|view| view.read(cx).editor().clone());
        let mut subscriptions = Vec::new();
        for editor in [&first, &second] {
            subscriptions.push(cx.observe(editor, |this, _, cx| this.sync_scroll(cx)));
            subscriptions
                .push(cx.subscribe(editor, |this, _, _: &Scrolled, cx| this.sync_scroll(cx)));
        }
        self.linked = Some(Linked {
            first,
            second,
            hunks: link.hunks(),
            synced: [f32::NAN; 2],
            _subscriptions: subscriptions,
        });
        self.sync_scroll(cx);
    }

    /// Scroll the linked editor that did not move to where the other is,
    /// through the link's map. Scrolling it notifies this again, which finds
    /// both where they were left and stops there.
    fn sync_scroll(&mut self, cx: &mut Context<Self>) {
        let Some(linked) = &self.linked else {
            return;
        };
        let scrolls = [&linked.first, &linked.second].map(|editor| editor.read(cx).scroll_top());
        if scrolls == linked.synced {
            return;
        }
        let (from, to, side) = if scrolls[0] != linked.synced[0] {
            (&linked.first, &linked.second, DiffSide::Original)
        } else {
            (&linked.second, &linked.first, DiffSide::Modified)
        };
        let target = map_line(&linked.hunks, side, from.read(cx).scroll_line());
        let to = to.clone();
        to.update(cx, |editor, cx| editor.scroll_to_line(target, cx));
        let linked = self.linked.as_mut().expect("checked above");
        linked.synced = [&linked.first, &linked.second].map(|editor| editor.read(cx).scroll_top());
    }

    /// The contributed views the composite holds, which the host binds.
    pub(crate) fn hosted_views(&self) -> Vec<ExtensionView> {
        self.layout.hosted_views()
    }

    /// The links of every connector, in layout order, as the lines of each
    /// side and the color key.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn connector_links(&self, cx: &App) -> Vec<(Range<u32>, Range<u32>, String)> {
        self.layout.connector_links(cx)
    }

    /// The editors each connector joins, in layout order.
    #[cfg(test)]
    pub(crate) fn connected_editors(&self, cx: &App) -> Vec<Option<[EntityId; 2]>> {
        self.layout.connected_editors(cx)
    }

    /// The views in layout order.
    pub(crate) fn leaves(&self) -> Vec<crate::app::view_layout::Leaf> {
        self.layout.leaves().to_vec()
    }

    /// Treat the `index`th view as the one with focus, as its gaining focus
    /// does.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_focused_leaf(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.layout.set_focused_leaf(index) {
            cx.emit(ViewFocusChanged);
        }
    }

    pub(crate) fn command_target(&self) -> CompositeCommandTarget {
        CompositeCommandTarget {
            shared: self.layout.shared(),
        }
    }

    /// Show the `index`th tab of the tabs group `group` and return the handle
    /// of the view to give focus.
    #[cfg(test)]
    pub(crate) fn select_tab(
        &self,
        group: usize,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<FocusHandle> {
        self.layout.select_tab(group, index, cx)
    }
}

impl ViewTree for Entity<CompositeView> {
    fn view_node(&self, cx: &App) -> ViewNode {
        self.read(cx)
            .layout
            .node(self.entity_id(), ViewKind::Composite, cx)
    }
}

impl EventEmitter<ViewTreeChanged> for CompositeView {}
impl EventEmitter<ViewFocusChanged> for CompositeView {}

impl Focusable for CompositeView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.layout.focused_handle(cx)
    }
}

impl Render for CompositeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.layout.watch_focus(
            window,
            cx,
            |this: &Self| &this.layout,
            |_, cx| cx.emit(ViewFocusChanged),
        );
        let view = cx.entity();
        let toolbar = toolbar(
            &self.tab.toolbar,
            "composite-toolbar",
            move |window, command, cx| {
                crate::app::product::run_toolbar_command(window, view.clone(), command, cx);
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
