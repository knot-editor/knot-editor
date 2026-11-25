//! A center tab whose layout of child views a package describes.
//!
//! A package registers a kind and opens instances; the instance owns the
//! views it contains, which the user cannot rearrange. Each child is an
//! ordinary view, so commands, keymaps, and editor-driven packages work in
//! it as anywhere else.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{prelude::*, *};
use knot_model::workbench::CompositeViewId;
use knot_protocol::{IconRef, ViewId};

use super::convert::ThemePaint as _;
use super::editor::EditorView;
use super::model::Buffer;
use super::resource::ResourceUri;
use super::text_view::TextView;
use super::theme::{color, theme};
use super::tree_view::TreeView;
use super::view_tree::{ViewFocusChanged, ViewKind, ViewNode, ViewTree, ViewTreeChanged};
use super::workbench::{CommandView, ExtensionView};

static NEXT_COMPOSITE_ID: AtomicU64 = AtomicU64::new(1);

/// The identity of a new composite view.
pub(crate) fn allocate_composite_id() -> CompositeViewId {
    CompositeViewId(NEXT_COMPOSITE_ID.fetch_add(1, Ordering::Relaxed))
}

/// What the tab of a composite view shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CompositeTab {
    pub title: String,
    pub icon: Option<IconRef>,
    /// The resource whose decorations the tab shows, as a document tab does.
    pub resource: Option<ResourceUri>,
}

/// How much of a group's main axis a member takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LayoutSize {
    /// A share of the space the pixel members leave, in proportion to the
    /// other weights.
    Weight(f32),
    Pixels(f32),
}

impl Default for LayoutSize {
    fn default() -> Self {
        Self::Weight(1.)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Arrangement {
    /// Members side by side.
    Row,
    /// Members stacked.
    Column,
    /// One member shown at a time, chosen from a strip of their titles.
    Tabs,
}

/// A group of members the package describes, in order.
pub(crate) struct GroupSpec {
    pub arrangement: Arrangement,
    pub members: Vec<MemberSpec>,
}

pub(crate) struct MemberSpec {
    pub size: LayoutSize,
    /// Has focus when the composite opens; a group gives it to its first
    /// view. In tabs it also selects the member.
    pub focused: bool,
    /// The label of the member's tab, for a member of tabs.
    pub title: Option<String>,
    pub content: ContentSpec,
}

pub(crate) enum ContentSpec {
    Editor(Entity<Buffer>),
    /// A text frame of the contributed view of this kind.
    Text(String),
    /// A tree of the contributed view of this kind.
    Tree(String),
    Group(GroupSpec),
}

impl ContentSpec {
    fn leaf_count(&self) -> usize {
        match self {
            Self::Group(group) => group.leaf_count(),
            _ => 1,
        }
    }
}

impl GroupSpec {
    fn leaf_count(&self) -> usize {
        self.members
            .iter()
            .map(|member| member.content.leaf_count())
            .sum()
    }

    /// The layout-order index of the view that has focus on open, if the
    /// layout names one.
    fn focused_leaf(&self) -> Option<usize> {
        let mut before = 0;
        for member in &self.members {
            let count = member.content.leaf_count();
            if member.focused && count > 0 {
                return Some(before);
            }
            if let ContentSpec::Group(group) = &member.content
                && let Some(inner) = group.focused_leaf()
            {
                return Some(before + inner);
            }
            before += count;
        }
        None
    }
}

/// A view a composite lays out and can give focus to.
#[derive(Clone)]
pub(crate) enum Leaf {
    Editor(Entity<EditorView>),
    Text(Entity<TextView>),
    Tree(Entity<TreeView>),
}

impl Leaf {
    #[cfg(test)]
    pub(crate) fn editor(&self) -> Option<&Entity<EditorView>> {
        match self {
            Self::Editor(view) => Some(view),
            _ => None,
        }
    }

    pub(crate) fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Editor(view) => view.focus_handle(cx),
            Self::Text(view) => view.focus_handle(cx),
            Self::Tree(view) => view.focus_handle(cx),
        }
    }

    fn entity_id(&self) -> EntityId {
        match self {
            Self::Editor(view) => view.entity_id(),
            Self::Text(view) => view.entity_id(),
            Self::Tree(view) => view.entity_id(),
        }
    }

    fn view_node(&self, cx: &App) -> ViewNode {
        match self {
            Self::Editor(view) => view.view_node(cx),
            Self::Text(view) => view.view_node(cx),
            Self::Tree(view) => view.view_node(cx),
        }
    }

    pub(crate) fn command_view(&self) -> CommandView {
        match self {
            Self::Editor(view) => CommandView::Editor(view.clone()),
            Self::Text(view) => CommandView::Extension(ExtensionView::Text(view.clone())),
            Self::Tree(view) => CommandView::Extension(ExtensionView::Tree(view.clone())),
        }
    }

    fn element(&self) -> AnyElement {
        match self {
            Self::Editor(view) => view.clone().into_any_element(),
            Self::Text(view) => view.clone().into_any_element(),
            Self::Tree(view) => view.clone().into_any_element(),
        }
    }

    /// The contributed view this is, if it is one.
    fn hosted(&self) -> Option<ExtensionView> {
        match self {
            Self::Editor(_) => None,
            Self::Text(view) => Some(ExtensionView::Text(view.clone())),
            Self::Tree(view) => Some(ExtensionView::Tree(view.clone())),
        }
    }
}

/// A member of a built group.
struct Member {
    size: LayoutSize,
    title: Option<String>,
    content: Content,
}

enum Content {
    Leaf(Leaf),
    Group(Entity<GroupView>),
}

impl Content {
    /// The view that has focus on entering this content: the first, or the
    /// shown member of tabs.
    fn entry_leaf(&self, cx: &App) -> Option<Leaf> {
        match self {
            Self::Leaf(leaf) => Some(leaf.clone()),
            Self::Group(group) => {
                let group = &group.read(cx).0;
                group.members.get(group.shown())?.content.entry_leaf(cx)
            }
        }
    }

    fn element(&self) -> AnyElement {
        match self {
            Self::Leaf(leaf) => leaf.element(),
            Self::Group(group) => group.clone().into_any_element(),
        }
    }
}

/// A built group. A nested one is a view of its own, so it has an identity
/// in the view tree.
struct Group {
    arrangement: Arrangement,
    members: Vec<Member>,
    /// The member tabs show.
    active: usize,
}

struct GroupView(Group);

impl Group {
    /// Build `spec` for the window `window`. `leaf` counts the leaves built
    /// so far and `focus` is the index of the one with focus, whose tabs are
    /// the ones shown.
    fn build(spec: GroupSpec, window: &str, focus: usize, leaf: &mut usize, cx: &mut App) -> Self {
        let mut active = 0;
        let members = spec
            .members
            .into_iter()
            .enumerate()
            .map(|(index, member)| {
                let first = *leaf;
                let content = match member.content {
                    ContentSpec::Editor(buffer) => {
                        *leaf += 1;
                        Content::Leaf(Leaf::Editor(cx.new(|cx| EditorView::new(buffer, cx))))
                    }
                    ContentSpec::Text(kind) => {
                        *leaf += 1;
                        Content::Leaf(Leaf::Text(
                            cx.new(|cx| TextView::new(kind, window.to_owned(), cx)),
                        ))
                    }
                    ContentSpec::Tree(kind) => {
                        *leaf += 1;
                        Content::Leaf(Leaf::Tree(
                            cx.new(|cx| TreeView::new(kind, window.to_owned(), cx)),
                        ))
                    }
                    ContentSpec::Group(group) => {
                        let group = Self::build(group, window, focus, leaf, cx);
                        Content::Group(cx.new(|_| GroupView(group)))
                    }
                };
                if (first..*leaf).contains(&focus) {
                    active = index;
                }
                Member {
                    size: member.size,
                    title: member.title,
                    content,
                }
            })
            .collect();
        Self {
            arrangement: spec.arrangement,
            members,
            active,
        }
    }

    /// The index of the member that has focus or shows, for the group's node.
    fn shown(&self) -> usize {
        match self.arrangement {
            Arrangement::Tabs => self.active,
            _ => 0,
        }
    }

    /// The views beneath this group in layout order.
    fn leaves(&self, cx: &App) -> Vec<Leaf> {
        let mut leaves = Vec::new();
        for member in &self.members {
            match &member.content {
                Content::Leaf(leaf) => leaves.push(leaf.clone()),
                Content::Group(group) => leaves.extend(group.read(cx).0.leaves(cx)),
            }
        }
        leaves
    }

    /// The nested groups beneath this one in layout order, this one's
    /// excluded.
    #[cfg(test)]
    fn groups(&self, cx: &App) -> Vec<Entity<GroupView>> {
        let mut groups = Vec::new();
        for member in &self.members {
            if let Content::Group(group) = &member.content {
                groups.push(group.clone());
                groups.extend(group.read(cx).0.groups(cx));
            }
        }
        groups
    }

    /// The group's node, whose focus is the member holding `focused`.
    fn node(&self, id: EntityId, focused: EntityId, cx: &App) -> ViewNode {
        let children: Vec<ViewNode> = self
            .members
            .iter()
            .map(|member| match &member.content {
                Content::Leaf(leaf) => leaf.view_node(cx),
                Content::Group(group) => group.read(cx).0.node(group.entity_id(), focused, cx),
            })
            .collect();
        ViewNode {
            id,
            kind: ViewKind::Composite,
            editor: None,
            focus: Some(
                children
                    .iter()
                    .position(|child| child.path_to(focused).is_some())
                    .unwrap_or(self.shown()),
            ),
            children,
        }
    }

    /// A row or column of its members.
    fn render(&self, cx: &App) -> Div {
        let border = theme(cx).paint_color(color::BORDER);
        let row = self.arrangement == Arrangement::Row;
        let group = div().flex().size_full().min_w_0().min_h_0();
        let mut group = if row {
            group.flex_row()
        } else {
            group.flex_col()
        };
        for (index, member) in self.members.iter().enumerate() {
            let mut slot = div().min_w_0().min_h_0().flex_shrink(1.);
            match member.size {
                LayoutSize::Weight(weight) => {
                    slot = slot.flex_basis(px(0.));
                    slot.style().flex_grow = Some(weight);
                }
                LayoutSize::Pixels(pixels) => {
                    slot = slot.flex_none();
                    slot = if row {
                        slot.w(px(pixels))
                    } else {
                        slot.h(px(pixels))
                    };
                }
            }
            if index > 0 {
                slot = if row {
                    slot.border_l_1()
                } else {
                    slot.border_t_1()
                }
                .border_color(border);
            }
            group = group.child(slot.child(div().size_full().child(member.content.element())));
        }
        group
    }
}

impl GroupView {
    /// Show the `index`th member of tabs and return the handle of the view
    /// to give focus.
    fn select(&mut self, index: usize, cx: &mut Context<Self>) -> Option<FocusHandle> {
        let member = self.0.members.get(index)?;
        let handle = member.content.entry_leaf(cx)?.focus_handle(cx);
        self.0.active = index;
        cx.notify();
        Some(handle)
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> Div {
        let theme = theme(cx);
        let entity = cx.entity();
        let strip = div()
            .flex()
            .flex_row()
            .flex_none()
            .items_end()
            .bg(theme.paint_color(color::TAB_BAR_BACKGROUND))
            .border_b_1()
            .border_color(theme.paint_color(color::BORDER))
            .children(self.0.members.iter().enumerate().map(|(index, member)| {
                let active = index == self.0.active;
                let entity = entity.clone();
                div()
                    .id(("composite-tab", index))
                    .debug_selector(move || format!("composite-tab-{index}"))
                    .px_3()
                    .py_1p5()
                    .text_sm()
                    .cursor_pointer()
                    .border_b_2()
                    .when(active, |tab| {
                        tab.bg(theme.paint_color(color::TAB_ACTIVE_BACKGROUND))
                            .text_color(theme.paint_color(color::TAB_ACTIVE_FOREGROUND))
                            .border_color(theme.paint_color(color::TAB_ACCENT))
                    })
                    .when(!active, |tab| {
                        tab.border_color(gpui::transparent_black())
                            .text_color(theme.paint_color(color::TAB_INACTIVE_FOREGROUND))
                            .hover(|style| {
                                style.bg(theme.paint_color(color::LIST_HOVER_BACKGROUND))
                            })
                    })
                    .child(member.title.clone().unwrap_or_default())
                    .on_click(move |_, window, cx| {
                        let handle = entity.update(cx, |group, cx| group.select(index, cx));
                        if let Some(handle) = handle {
                            window.focus(&handle, cx);
                        }
                    })
            }));
        let shown = self.0.members.get(self.0.active).map(|member| {
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .child(div().size_full().child(member.content.element()))
        });
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(strip)
            .children(shown)
    }
}

impl Render for GroupView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.0.arrangement {
            Arrangement::Tabs => self.render_tabs(cx),
            Arrangement::Row | Arrangement::Column => self.0.render(cx),
        }
    }
}

/// The views of a composite and which one commands act on.
#[derive(Clone)]
pub(crate) struct CompositeCommandTarget {
    leaves: Rc<[Leaf]>,
    focused: Rc<Cell<usize>>,
}

impl CompositeCommandTarget {
    pub(crate) fn command_view(&self) -> CommandView {
        self.leaves[self.focused.get()].command_view()
    }
}

pub(crate) struct CompositeView {
    tab: CompositeTab,
    root: Group,
    /// The views in layout order; the layout holds at least one.
    leaves: Rc<[Leaf]>,
    /// The index of the view that has focus, or had it last.
    focused: Rc<Cell<usize>>,
    /// Watches each view's focus; set up on the first render, which has the
    /// window.
    _focus_subscriptions: Vec<Subscription>,
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
        debug_assert!(layout.leaf_count() > 0 && layout.arrangement != Arrangement::Tabs);
        let focused = layout.focused_leaf().unwrap_or(0);
        let root = Group::build(layout, window, focused, &mut 0, cx);
        let leaves: Rc<[Leaf]> = root.leaves(cx).into();
        let hosted: Vec<ViewId> = leaves
            .iter()
            .filter_map(Leaf::hosted)
            .map(|view| view.instance_id(cx))
            .collect();
        if !hosted.is_empty() {
            cx.on_release(move |_, cx| {
                cx.defer(move |cx| {
                    for view in hosted {
                        crate::app::extensions::host::detach_view_instance(view, cx);
                    }
                });
            })
            .detach();
        }
        Self {
            tab,
            root,
            leaves,
            focused: Rc::new(Cell::new(focused)),
            _focus_subscriptions: Vec::new(),
        }
    }

    pub(crate) fn tab(&self) -> &CompositeTab {
        &self.tab
    }

    pub(crate) fn set_tab(&mut self, tab: CompositeTab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    /// The contributed views the composite holds, which the host binds.
    pub(crate) fn hosted_views(&self) -> Vec<ExtensionView> {
        self.leaves.iter().filter_map(Leaf::hosted).collect()
    }

    /// The views in layout order.
    #[cfg(test)]
    pub(crate) fn leaves(&self) -> &[Leaf] {
        &self.leaves
    }

    /// The focus handle of the view commands act on.
    fn focused_handle(&self, cx: &App) -> FocusHandle {
        self.leaves[self.focused.get()].focus_handle(cx)
    }

    /// Treat the `index`th view as the one with focus, as its gaining focus
    /// does.
    pub(crate) fn set_focused_leaf(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.focused.replace(index) != index {
            cx.emit(ViewFocusChanged);
        }
    }

    pub(crate) fn command_target(&self) -> CompositeCommandTarget {
        CompositeCommandTarget {
            leaves: self.leaves.clone(),
            focused: self.focused.clone(),
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
        let groups = self.root.groups(cx);
        groups
            .get(group)?
            .update(cx, |group, cx| group.select(index, cx))
    }
}

impl ViewTree for Entity<CompositeView> {
    fn view_node(&self, cx: &App) -> ViewNode {
        let view = self.read(cx);
        view.root.node(
            self.entity_id(),
            view.leaves[view.focused.get()].entity_id(),
            cx,
        )
    }
}

impl EventEmitter<ViewTreeChanged> for CompositeView {}
impl EventEmitter<ViewFocusChanged> for CompositeView {}

impl Focusable for CompositeView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.focused_handle(cx)
    }
}

impl Render for CompositeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self._focus_subscriptions.is_empty() {
            for (index, leaf) in self.leaves.iter().enumerate() {
                let handle = leaf.focus_handle(cx);
                self._focus_subscriptions.push(cx.on_focus_in(
                    &handle,
                    window,
                    move |this, _, cx| this.set_focused_leaf(index, cx),
                ));
            }
        }
        self.root.render(cx)
    }
}
