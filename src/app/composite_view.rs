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
use knot_protocol::IconRef;

use super::convert::ThemePaint as _;
use super::editor::EditorView;
use super::model::Buffer;
use super::resource::ResourceUri;
use super::theme::{color, theme};
use super::view_tree::{ViewFocusChanged, ViewKind, ViewNode, ViewTree, ViewTreeChanged};

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
pub(crate) enum LayoutAxis {
    /// Members side by side.
    Row,
    /// Members stacked.
    Column,
}

/// A group of members the package describes, in order.
pub(crate) struct GroupSpec {
    pub axis: LayoutAxis,
    pub members: Vec<MemberSpec>,
}

pub(crate) struct MemberSpec {
    pub size: LayoutSize,
    /// Has focus when the composite opens; a group gives it to its first
    /// editor.
    pub focused: bool,
    pub content: ContentSpec,
}

pub(crate) enum ContentSpec {
    Editor(Entity<Buffer>),
    Group(GroupSpec),
}

impl GroupSpec {
    fn editor_count(&self) -> usize {
        self.members
            .iter()
            .map(|member| match &member.content {
                ContentSpec::Editor(_) => 1,
                ContentSpec::Group(group) => group.editor_count(),
            })
            .sum()
    }

    /// The layout-order index of the editor that has focus on open, if the
    /// layout names one.
    fn focused_editor(&self) -> Option<usize> {
        let mut before = 0;
        for member in &self.members {
            let count = match &member.content {
                ContentSpec::Editor(_) => 1,
                ContentSpec::Group(group) => group.editor_count(),
            };
            if member.focused && count > 0 {
                return Some(before);
            }
            if let ContentSpec::Group(group) = &member.content
                && let Some(inner) = group.focused_editor()
            {
                return Some(before + inner);
            }
            before += count;
        }
        None
    }
}

/// A member of a built group.
struct Member {
    size: LayoutSize,
    content: Content,
}

enum Content {
    Editor(Entity<EditorView>),
    Group(Entity<GroupView>),
}

/// A built group. A nested one is a view of its own, so it has an identity
/// in the view tree.
struct Group {
    axis: LayoutAxis,
    members: Vec<Member>,
}

struct GroupView(Group);

impl Group {
    fn build(spec: GroupSpec, cx: &mut App) -> Self {
        let members = spec
            .members
            .into_iter()
            .map(|member| Member {
                size: member.size,
                content: match member.content {
                    ContentSpec::Editor(buffer) => {
                        Content::Editor(cx.new(|cx| EditorView::new(buffer, cx)))
                    }
                    ContentSpec::Group(group) => {
                        let group = Self::build(group, cx);
                        Content::Group(cx.new(|_| GroupView(group)))
                    }
                },
            })
            .collect();
        Self {
            axis: spec.axis,
            members,
        }
    }

    /// The editors beneath this group in layout order.
    fn editors(&self, cx: &App) -> Vec<Entity<EditorView>> {
        let mut editors = Vec::new();
        for member in &self.members {
            match &member.content {
                Content::Editor(editor) => editors.push(editor.clone()),
                Content::Group(group) => editors.extend(group.read(cx).0.editors(cx)),
            }
        }
        editors
    }

    /// The group's node, whose focus is the member holding `focused`.
    fn node(&self, id: EntityId, focused: EntityId, cx: &App) -> ViewNode {
        let children: Vec<ViewNode> = self
            .members
            .iter()
            .map(|member| match &member.content {
                Content::Editor(editor) => editor.view_node(cx),
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
                    .unwrap_or(0),
            ),
            children,
        }
    }

    fn render(&self, cx: &App) -> Div {
        let border = theme(cx).paint_color(color::BORDER);
        let row = self.axis == LayoutAxis::Row;
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
            group = group.child(slot.child(match &member.content {
                Content::Editor(editor) => div().size_full().child(editor.clone()),
                Content::Group(group) => div().size_full().child(group.clone()),
            }));
        }
        group
    }
}

impl Render for GroupView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.render(cx)
    }
}

/// The editors of a composite and which one commands act on.
#[derive(Clone)]
pub(crate) struct CompositeCommandTarget {
    editors: Rc<[Entity<EditorView>]>,
    focused: Rc<Cell<usize>>,
}

impl CompositeCommandTarget {
    pub(crate) fn editor(&self) -> &Entity<EditorView> {
        &self.editors[self.focused.get()]
    }
}

pub(crate) struct CompositeView {
    tab: CompositeTab,
    root: Group,
    /// The editors in layout order; the layout holds at least one.
    editors: Rc<[Entity<EditorView>]>,
    /// The index of the editor that has focus, or had it last.
    focused: Rc<Cell<usize>>,
    /// Watches each editor's focus; set up on the first render, which has
    /// the window.
    _focus_subscriptions: Vec<Subscription>,
}

impl CompositeView {
    /// A view of `layout`, which holds at least one editor.
    pub(crate) fn new(tab: CompositeTab, layout: GroupSpec, cx: &mut Context<Self>) -> Self {
        debug_assert!(layout.editor_count() > 0);
        let focused = layout.focused_editor().unwrap_or(0);
        let root = Group::build(layout, cx);
        let editors: Rc<[Entity<EditorView>]> = root.editors(cx).into();
        Self {
            tab,
            root,
            editors,
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

    /// The editors in layout order.
    #[cfg(test)]
    pub(crate) fn editors(&self) -> &[Entity<EditorView>] {
        &self.editors
    }

    /// The editor commands act on: the one that has focus, or had it last.
    pub(crate) fn command_editor(&self) -> &Entity<EditorView> {
        &self.editors[self.focused.get()]
    }

    /// Treat the `index`th editor as the one with focus, as its gaining
    /// focus does.
    pub(crate) fn set_focused_editor(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.focused.replace(index) != index {
            cx.emit(ViewFocusChanged);
        }
    }

    pub(crate) fn command_target(&self) -> CompositeCommandTarget {
        CompositeCommandTarget {
            editors: self.editors.clone(),
            focused: self.focused.clone(),
        }
    }
}

impl ViewTree for Entity<CompositeView> {
    fn view_node(&self, cx: &App) -> ViewNode {
        let view = self.read(cx);
        view.root
            .node(self.entity_id(), view.command_editor().entity_id(), cx)
    }
}

impl EventEmitter<ViewTreeChanged> for CompositeView {}
impl EventEmitter<ViewFocusChanged> for CompositeView {}

impl Focusable for CompositeView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.command_editor().read(cx).focus_handle(cx)
    }
}

impl Render for CompositeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self._focus_subscriptions.is_empty() {
            for (index, editor) in self.editors.iter().enumerate() {
                let handle = editor.focus_handle(cx);
                self._focus_subscriptions.push(cx.on_focus_in(
                    &handle,
                    window,
                    move |this, _, cx| this.set_focused_editor(index, cx),
                ));
            }
        }
        self.root.render(cx)
    }
}
