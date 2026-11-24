//! The tree of views inside a tab, as extensions and focus routing see it.
//!
//! A tab presents one root view. A view that contains other views lists them
//! as children and emits [`ViewTreeChanged`] when its children change, so the
//! snapshot, focus, handles, and captures read the tree only through
//! [`ViewTree`] and never name a concrete container.

use gpui::{App, Entity, EntityId};

use super::diff_view::DiffView;
use super::editor::EditorView;
use super::terminal::view::TerminalView;
use super::text_view::TextView;
use super::tree_view::TreeView;
use super::workbench::ExtensionView;

/// Emitted by a view whose set of child views changed.
pub(crate) struct ViewTreeChanged;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ViewKind {
    Editor,
    Terminal,
    Tree,
    Text,
    Diff,
}

impl ViewKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::Terminal => "terminal",
            Self::Tree => "tree",
            Self::Text => "text",
            Self::Diff => "diff",
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
}

impl ViewNode {
    fn leaf(id: EntityId, kind: ViewKind) -> Self {
        Self {
            id,
            kind,
            editor: None,
            children: Vec::new(),
        }
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
    fn view_node(&self, _: &App) -> ViewNode {
        ViewNode {
            editor: Some(self.clone()),
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

impl ViewTree for Entity<DiffView> {
    fn view_node(&self, cx: &App) -> ViewNode {
        let diff = self.read(cx);
        ViewNode {
            children: diff
                .visible_editors()
                .into_iter()
                .map(|editor| editor.view_node(cx))
                .collect(),
            ..ViewNode::leaf(self.entity_id(), ViewKind::Diff)
        }
    }
}
