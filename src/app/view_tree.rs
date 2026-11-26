//! The tree of views inside a tab, as extensions and focus routing see it.
//!
//! A tab presents one root view. A view that contains other views lists them
//! as children and emits [`ViewTreeChanged`] when its children change, so the
//! snapshot, focus, handles, and captures read the tree only through
//! [`ViewTree`] and never name a concrete container.

use gpui::{App, Entity, EntityId};

use super::editor::EditorView;
use super::terminal::view::TerminalView;
use super::text_view::TextView;
use super::tree_view::TreeView;
use super::workbench::ExtensionView;

/// Emitted by a view whose set of child views changed.
pub(crate) struct ViewTreeChanged;

/// Emitted by a view when focus moves between the views it contains.
pub(crate) struct ViewFocusChanged;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ViewKind {
    Editor,
    Terminal,
    Tree,
    Text,
    Composite,
    Popover,
}

impl ViewKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::Terminal => "terminal",
            Self::Tree => "tree",
            Self::Text => "text",
            Self::Composite => "composite",
            Self::Popover => "popover",
        }
    }
}

/// One view and the views it contains. The identity is the view's entity.
#[derive(Clone)]
pub(crate) struct ViewNode {
    pub id: EntityId,
    pub kind: ViewKind,
    /// The editor, for an editor view.
    pub editor: Option<Entity<EditorView>>,
    pub children: Vec<ViewNode>,
    /// The child that has focus, or had it last while focus is elsewhere.
    pub focus: Option<usize>,
}

impl ViewNode {
    fn leaf(id: EntityId, kind: ViewKind) -> Self {
        Self {
            id,
            kind,
            editor: None,
            children: Vec::new(),
            focus: None,
        }
    }

    /// The view that has focus within this one: the focused child's focused
    /// leaf, or this view when it has none.
    pub(crate) fn focused_leaf(&self) -> &ViewNode {
        match self.focus.and_then(|index| self.children.get(index)) {
            Some(child) => child.focused_leaf(),
            None => self,
        }
    }

    /// The views from this one down to the view `id`, this one first, or
    /// none when `id` is not beneath it.
    pub(crate) fn path_to(&self, id: EntityId) -> Option<Vec<&ViewNode>> {
        if self.id == id {
            return Some(vec![self]);
        }
        let mut path = self.children.iter().find_map(|child| child.path_to(id))?;
        path.insert(0, self);
        Some(path)
    }

    /// The editors in popovers beneath this view.
    pub(crate) fn popover_editors(&self) -> Vec<&Entity<EditorView>> {
        self.descendants()
            .into_iter()
            .filter(|node| node.kind == ViewKind::Popover)
            .flat_map(|popover| popover.descendants())
            .filter_map(|node| node.editor.as_ref())
            .collect()
    }

    /// This view and every view beneath it, parents first.
    pub(crate) fn descendants(&self) -> Vec<&ViewNode> {
        let mut nodes = vec![self];
        for child in &self.children {
            nodes.extend(child.descendants());
        }
        nodes
    }
}

/// A view that lists the views it contains.
pub(crate) trait ViewTree {
    fn view_node(&self, cx: &App) -> ViewNode;
}

impl ViewTree for Entity<EditorView> {
    /// The editor's node, with the popovers floating over it as children.
    fn view_node(&self, cx: &App) -> ViewNode {
        let (children, focus) = self.read(cx).editor().read(cx).popover_nodes(cx);
        ViewNode {
            editor: Some(self.clone()),
            children,
            focus,
            ..ViewNode::leaf(self.entity_id(), ViewKind::Editor)
        }
    }
}

impl ViewTree for Entity<TerminalView> {
    fn view_node(&self, _: &App) -> ViewNode {
        ViewNode::leaf(self.entity_id(), ViewKind::Terminal)
    }
}

impl ViewTree for Entity<TreeView> {
    fn view_node(&self, _: &App) -> ViewNode {
        ViewNode::leaf(self.entity_id(), ViewKind::Tree)
    }
}

impl ViewTree for Entity<TextView> {
    fn view_node(&self, _: &App) -> ViewNode {
        ViewNode::leaf(self.entity_id(), ViewKind::Text)
    }
}

impl ViewTree for ExtensionView {
    fn view_node(&self, cx: &App) -> ViewNode {
        match self {
            Self::Tree(view) => view.view_node(cx),
            Self::Text(view) => view.view_node(cx),
        }
    }
}
