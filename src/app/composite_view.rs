//! A center tab whose layout of child views a package describes.
//!
//! A package registers a kind and opens instances; the instance owns the
//! views it contains, which the user cannot rearrange. Each child is an
//! ordinary view, so commands, keymaps, and editor-driven packages work in
//! it as anywhere else.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{prelude::*, *};
use knot_model::workbench::{CompositeViewId, DocumentId};
use knot_protocol::{Command, CommandArgumentValue, IconRef, ViewId};

use super::convert::ThemePaint as _;
use super::editor::EditorView;
use super::icons::{AppIcon, ref_definition};
use super::model::Buffer;
use super::resource::ResourceUri;
use super::text_view::TextView;
use super::theme::{color, theme};
use super::tree_view::TreeView;
use super::view_tree::{ViewFocusChanged, ViewKind, ViewNode, ViewTree, ViewTreeChanged};
use super::workbench::{CommandView, ExtensionView};
use super::zoom;

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
}

/// An icon button of a composite's toolbar, which runs a command with the
/// composite as its target.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ToolbarButton {
    /// The tooltip.
    pub label: String,
    pub icon: IconRef,
    pub command: String,
    #[serde(default = "no_arguments")]
    pub arguments: CommandArgumentValue,
    /// Shown pressed.
    #[serde(default)]
    pub toggled: bool,
    /// Shown dimmed, and does nothing when clicked.
    #[serde(default)]
    pub disabled: bool,
}

fn no_arguments() -> CommandArgumentValue {
    CommandArgumentValue::Null
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
    /// The name that keeps the member's view when the package replaces the
    /// layout, unique in the layout. A member without one is always new.
    pub key: Option<String>,
    pub size: LayoutSize,
    /// Has focus when the composite opens; a group gives it to its first
    /// view. In tabs it also selects the member.
    pub focused: bool,
    /// The label of the member's tab, for a member of tabs.
    pub title: Option<String>,
    pub content: ContentSpec,
}

pub(crate) enum ContentSpec {
    /// An editor on `buffer`, which is the text of `document` when given and
    /// an extension buffer otherwise.
    Editor {
        buffer: Entity<Buffer>,
        document: Option<DocumentId>,
    },
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

    fn collect_documents(&self, documents: &mut Vec<DocumentId>) {
        match self {
            Self::Editor {
                document: Some(document),
                ..
            } if !documents.contains(document) => documents.push(*document),
            Self::Group(group) => group.collect_documents(documents),
            _ => {}
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

    /// The documents the editors beneath the group show, each once.
    pub(crate) fn documents(&self) -> Vec<DocumentId> {
        let mut documents = Vec::new();
        self.collect_documents(&mut documents);
        documents
    }

    fn collect_documents(&self, documents: &mut Vec<DocumentId>) {
        for member in &self.members {
            member.content.collect_documents(documents);
        }
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

    pub(crate) fn entity_id(&self) -> EntityId {
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

/// What a built member shows, which a layout update compares to decide
/// whether a view of the same key is still the right one.
#[derive(Clone, PartialEq)]
enum Source {
    Buffer(Entity<Buffer>),
    Text(String),
    Tree(String),
    Group,
}

impl Source {
    fn of(content: &ContentSpec) -> Self {
        match content {
            ContentSpec::Editor { buffer, .. } => Self::Buffer(buffer.clone()),
            ContentSpec::Text(kind) => Self::Text(kind.clone()),
            ContentSpec::Tree(kind) => Self::Tree(kind.clone()),
            ContentSpec::Group(_) => Self::Group,
        }
    }
}

/// The instances of the contributed views among `leaves`.
fn hosted_ids(leaves: &[Leaf], cx: &App) -> Vec<ViewId> {
    leaves
        .iter()
        .filter_map(Leaf::hosted)
        .map(|view| view.instance_id(cx))
        .collect()
}

/// A member of a built group.
struct Member {
    key: Option<String>,
    source: Source,
    size: LayoutSize,
    title: Option<String>,
    content: Content,
}

/// The keyed views of the layout being replaced, which the new one takes
/// from.
type Kept = HashMap<String, (Source, Content)>;

#[derive(Clone)]
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
    /// Build `spec` for the window `window`. A keyed member takes the view
    /// of the same key from `kept` when it shows the same thing. `shown` is
    /// the key of the member tabs showed before, which stays shown if it is
    /// still there.
    fn build(
        spec: GroupSpec,
        window: &str,
        kept: &mut Kept,
        shown: Option<&str>,
        cx: &mut App,
    ) -> Self {
        let mut active = None;
        let mut named = None;
        let members: Vec<Member> = spec
            .members
            .into_iter()
            .enumerate()
            .map(|(index, member)| {
                let source = Source::of(&member.content);
                let reused = member.key.as_ref().and_then(|key| {
                    if kept.get(key).is_some_and(|(found, _)| *found == source) {
                        kept.remove(key).map(|(_, content)| content)
                    } else {
                        None
                    }
                });
                if member.focused && active.is_none() {
                    active = Some(index);
                }
                if member.key.as_deref().is_some_and(|key| Some(key) == shown) {
                    named = Some(index);
                }
                let content = match (member.content, reused) {
                    (ContentSpec::Group(group), Some(Content::Group(entity))) => {
                        let before = entity.read(cx).0.shown_key();
                        entity.update(cx, |view, cx| {
                            view.0 = Self::build(group, window, kept, before.as_deref(), cx);
                            cx.notify();
                        });
                        Content::Group(entity)
                    }
                    (_, Some(content)) => content,
                    (ContentSpec::Editor { buffer, .. }, None) => {
                        Content::Leaf(Leaf::Editor(cx.new(|cx| EditorView::new(buffer, cx))))
                    }
                    (ContentSpec::Text(kind), None) => Content::Leaf(Leaf::Text(
                        cx.new(|cx| TextView::new(kind, window.to_owned(), cx)),
                    )),
                    (ContentSpec::Tree(kind), None) => Content::Leaf(Leaf::Tree(
                        cx.new(|cx| TreeView::new(kind, window.to_owned(), cx)),
                    )),
                    (ContentSpec::Group(group), None) => {
                        let group = Self::build(group, window, kept, None, cx);
                        Content::Group(cx.new(|_| GroupView(group)))
                    }
                };
                Member {
                    key: member.key,
                    source,
                    size: member.size,
                    title: member.title,
                    content,
                }
            })
            .collect();
        Self {
            arrangement: spec.arrangement,
            members,
            active: named.or(active).unwrap_or(0),
        }
    }

    /// The key of the member tabs show, if it has one.
    fn shown_key(&self) -> Option<String> {
        match self.arrangement {
            Arrangement::Tabs => self.members.get(self.active)?.key.clone(),
            _ => None,
        }
    }

    /// Move the keyed members beneath this group into `kept`.
    fn keep(&self, kept: &mut Kept, cx: &App) {
        for member in &self.members {
            if let Some(key) = &member.key {
                kept.insert(key.clone(), (member.source.clone(), member.content.clone()));
            }
            if let Content::Group(group) = &member.content {
                group.read(cx).0.keep(kept, cx);
            }
        }
    }

    /// Show the members that lead to the view `focus`, so the tabs show it.
    /// Returns whether the view is beneath this group.
    fn reveal(&mut self, focus: EntityId, cx: &mut App) -> bool {
        for (index, member) in self.members.iter().enumerate() {
            let contains = match &member.content {
                Content::Leaf(leaf) => leaf.entity_id() == focus,
                Content::Group(group) => group.update(cx, |view, cx| {
                    let found = view.0.reveal(focus, cx);
                    cx.notify();
                    found
                }),
            };
            if contains {
                if self.arrangement == Arrangement::Tabs {
                    self.active = index;
                }
                return true;
            }
        }
        false
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
struct Shared {
    leaves: RefCell<Rc<[Leaf]>>,
    focused: Cell<usize>,
}

impl Shared {
    fn leaves(&self) -> Rc<[Leaf]> {
        self.leaves.borrow().clone()
    }

    fn focused_leaf(&self) -> Leaf {
        self.leaves.borrow()[self.focused.get()].clone()
    }
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

/// What replacing a layout did beyond the views themselves.
pub(crate) struct LayoutChange {
    /// The contributed views the new layout added, which the host binds.
    pub attached: Vec<ExtensionView>,
    /// The focused view was removed, so another has focus.
    pub focus_moved: bool,
}

pub(crate) struct CompositeView {
    tab: CompositeTab,
    root: Group,
    /// The views in layout order, which the layout holds at least one of, and
    /// the index of the one that has focus or had it last.
    shared: Rc<Shared>,
    /// The instances of the contributed views, which the host unbinds when
    /// the composite is released.
    hosted: Vec<ViewId>,
    /// Watches each view's focus; set up on the first render after the layout
    /// changes, which has the window.
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
        let mut root = Group::build(layout, window, &mut Kept::new(), None, cx);
        let leaves: Rc<[Leaf]> = root.leaves(cx).into();
        root.reveal(leaves[focused].entity_id(), cx);
        let hosted = hosted_ids(&leaves, cx);
        cx.on_release(|this, cx| {
            let hosted = std::mem::take(&mut this.hosted);
            if !hosted.is_empty() {
                cx.defer(move |cx| {
                    for view in hosted {
                        crate::app::extensions::host::detach_view_instance(view, cx);
                    }
                });
            }
        })
        .detach();
        Self {
            tab,
            root,
            shared: Rc::new(Shared {
                leaves: RefCell::new(leaves),
                focused: Cell::new(focused),
            }),
            hosted,
            _focus_subscriptions: Vec::new(),
        }
    }

    /// Replace the layout with `layout`, which like the first is a row or
    /// column that holds at least one view. A keyed member keeps its view
    /// when it shows the same thing, and so keeps the view's handle, state,
    /// and focus. When the focused view goes, the layout's own initial focus
    /// takes over.
    pub(crate) fn update_layout(
        &mut self,
        layout: GroupSpec,
        window: &str,
        cx: &mut Context<Self>,
    ) -> LayoutChange {
        debug_assert!(layout.leaf_count() > 0 && layout.arrangement != Arrangement::Tabs);
        let before = self.shared.leaves();
        let focused = self.shared.focused_leaf().entity_id();
        let initial = layout.focused_leaf().unwrap_or(0);
        let mut kept = Kept::new();
        self.root.keep(&mut kept, cx);
        self.root = Group::build(layout, window, &mut kept, None, cx);
        let leaves: Rc<[Leaf]> = self.root.leaves(cx).into();
        let kept_focus = leaves.iter().position(|leaf| leaf.entity_id() == focused);
        let index = kept_focus.unwrap_or(initial);
        self.root.reveal(leaves[index].entity_id(), cx);
        let new = |leaf: &&Leaf| !before.iter().any(|old| old.entity_id() == leaf.entity_id());
        let attached = leaves.iter().filter(new).filter_map(Leaf::hosted).collect();
        let gone: Vec<ViewId> = before
            .iter()
            .filter(|old| {
                !leaves
                    .iter()
                    .any(|leaf| leaf.entity_id() == old.entity_id())
            })
            .filter_map(Leaf::hosted)
            .map(|view| view.instance_id(cx))
            .collect();
        self.hosted = hosted_ids(&leaves, cx);
        *self.shared.leaves.borrow_mut() = leaves;
        self.shared.focused.set(index);
        self._focus_subscriptions.clear();
        if !gone.is_empty() {
            cx.defer(move |cx| {
                for view in gone {
                    crate::app::extensions::host::detach_view_instance(view, cx);
                }
            });
        }
        let focus_moved = kept_focus.is_none();
        cx.emit(ViewTreeChanged);
        if focus_moved {
            cx.emit(ViewFocusChanged);
        }
        cx.notify();
        LayoutChange {
            attached,
            focus_moved,
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
        self.shared
            .leaves()
            .iter()
            .filter_map(Leaf::hosted)
            .collect()
    }

    /// The views in layout order.
    #[cfg(test)]
    pub(crate) fn leaves(&self) -> Vec<Leaf> {
        self.shared.leaves().to_vec()
    }

    /// The focus handle of the view commands act on.
    fn focused_handle(&self, cx: &App) -> FocusHandle {
        self.shared.focused_leaf().focus_handle(cx)
    }

    /// Treat the `index`th view as the one with focus, as its gaining focus
    /// does.
    pub(crate) fn set_focused_leaf(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.shared.focused.replace(index) != index {
            cx.emit(ViewFocusChanged);
        }
    }

    pub(crate) fn command_target(&self) -> CompositeCommandTarget {
        CompositeCommandTarget {
            shared: self.shared.clone(),
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
        view.root
            .node(self.entity_id(), view.shared.focused_leaf().entity_id(), cx)
    }
}

impl EventEmitter<ViewTreeChanged> for CompositeView {}
impl EventEmitter<ViewFocusChanged> for CompositeView {}

impl Focusable for CompositeView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.focused_handle(cx)
    }
}

impl CompositeView {
    /// The bar of buttons above the layout, when the composite has any.
    fn toolbar(&self, cx: &Context<Self>) -> Option<Stateful<Div>> {
        if self.tab.toolbar.is_empty() {
            return None;
        }
        let theme = theme(cx);
        let text = theme.paint_color(color::TEXT);
        let hover = theme.paint_color(color::BUTTON_HOVER_BACKGROUND);
        let pressed = theme.paint_color(color::LIST_SELECTED_BACKGROUND);
        let view = cx.entity();
        let buttons = self.tab.toolbar.iter().enumerate().map(|(index, button)| {
            let label = SharedString::from(button.label.clone());
            let command = Command {
                name: button.command.as_str().into(),
                arguments: button.arguments.clone(),
            };
            let view = view.clone();
            div()
                .id(("composite-toolbar-button", index))
                .debug_selector(move || format!("composite-toolbar-{index}"))
                .size(zoom::rems(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(zoom::rems(6.))
                .text_color(text)
                .when(button.toggled, |button| button.bg(pressed))
                .when(button.disabled, |button| button.opacity(0.4))
                .when(!button.disabled, |button| {
                    button
                        .cursor_pointer()
                        .hover(|style| style.bg(hover))
                        .on_click(move |_, window, cx| {
                            let handle = window.window_handle();
                            let (view, command) = (view.clone(), command.clone());
                            cx.defer(move |cx| {
                                crate::app::product::run_toolbar_command(handle, view, command, cx);
                            });
                        })
                })
                .child(AppIcon::new(ref_definition(&button.icon, false, cx)))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label.clone())
                        .text_xs()
                        .build(window, cx)
                })
        });
        Some(
            div()
                .id("composite-toolbar")
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(zoom::rems(4.))
                .px(zoom::rems(8.))
                .py(zoom::rems(5.))
                .bg(theme.paint_color(color::EDITOR_BAR_BACKGROUND))
                .border_b_1()
                .border_color(theme.paint_color(color::BORDER))
                .children(buttons),
        )
    }
}

impl Render for CompositeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self._focus_subscriptions.is_empty() {
            for leaf in self.shared.leaves().iter() {
                let handle = leaf.focus_handle(cx);
                let id = leaf.entity_id();
                self._focus_subscriptions.push(cx.on_focus_in(
                    &handle,
                    window,
                    move |this, _, cx| {
                        let index = this
                            .shared
                            .leaves()
                            .iter()
                            .position(|leaf| leaf.entity_id() == id);
                        if let Some(index) = index {
                            this.set_focused_leaf(index, cx);
                        }
                    },
                ));
            }
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .children(self.toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(self.root.render(cx)),
            )
    }
}
