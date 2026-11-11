//! A product session's workbench: the layout model and the views its tabs
//! present.
//!
//! The layout lives in `knot_model::workbench` and holds only identities.
//! This entity pairs it with the view entity behind each presentation.
//! Transitions that open tabs or end presentations are methods here, which
//! create and drop views; every other transition reaches the layout through
//! `DerefMut` and leaves the views as they are.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use gpui::{App, AppContext, Context, Entity, EntityId, FocusHandle, Focusable};
use knot_model::workbench as layout;
pub(crate) use knot_model::workbench::{
    Area, Arrangement, BottomDockLayout, CloseConfirmation, CloseRequestOutcome, DockLocation,
    DocumentCloseDisposition, DocumentId, PaneId, PresentationId, SplitDirection, SplitEdge,
    SplitPlacement, StripeButton, Stripes, TabId, TabKind, TabMoveCapture, TabMoveDestination,
    TabMovePlacement, TabSurfaceId, TerminalSessionId, ToolFrameId, WorkbenchLayout, WorkbenchTab,
    neighbor,
};

use super::{
    editor::{EditorView, EditorViewState},
    model::Buffer,
    terminal::view::TerminalView,
    text_view::TextView,
    tree_view::TreeView,
    workspace_tree::WorkspaceTree,
};

/// Native content owned by one contributed view instance.
#[derive(Clone)]
pub(crate) enum ExtensionView {
    Tree(Entity<TreeView>),
    Text(Entity<TextView>),
}

impl ExtensionView {
    pub(crate) fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Tree(view) => view.read(cx).focus_handle(cx),
            Self::Text(view) => view.read(cx).focus_handle(cx),
        }
    }

    pub(crate) fn kind(&self, cx: &App) -> String {
        match self {
            Self::Tree(view) => view.read(cx).kind().to_owned(),
            Self::Text(view) => view.read(cx).kind().to_owned(),
        }
    }

    pub(crate) fn instance_id(&self, cx: &App) -> knot_protocol::ViewId {
        match self {
            Self::Tree(view) => view.read(cx).instance_id(),
            Self::Text(view) => view.read(cx).instance_id(),
        }
    }

    pub(crate) fn handle_command(
        &self,
        command: &knot_protocol::Command,
        cx: &mut App,
    ) -> super::commands::dispatch::CommandClaim {
        match self {
            Self::Tree(view) => view.update(cx, |view, cx| view.handle_command(command, cx)),
            Self::Text(view) => view.update(cx, |view, cx| view.handle_command(command, cx)),
        }
    }

    pub(crate) fn selected_text(&self, cx: &App) -> Option<String> {
        match self {
            Self::Tree(_) => None,
            Self::Text(view) => view.read(cx).selected_text(cx),
        }
    }

    /// The ID of the selected row of a tree.
    pub(crate) fn selected_tree_item(&self, cx: &App) -> Option<String> {
        match self {
            Self::Tree(view) => view.read(cx).selected_item_id(),
            Self::Text(_) => None,
        }
    }

    fn entity_id(&self) -> EntityId {
        match self {
            Self::Tree(view) => view.entity_id(),
            Self::Text(view) => view.entity_id(),
        }
    }
}

/// The native view of a tool.
#[derive(Clone)]
pub(crate) enum ToolContent {
    Workspace(Entity<WorkspaceTree>),
    Extension(ExtensionView),
}

impl ToolContent {
    fn entity_id(&self) -> EntityId {
        match self {
            Self::Workspace(view) => view.entity_id(),
            Self::Extension(view) => view.entity_id(),
        }
    }

    pub(crate) fn command_view(&self) -> CommandView {
        match self {
            Self::Workspace(view) => CommandView::Workspace(view.clone()),
            Self::Extension(view) => CommandView::Extension(view.clone()),
        }
    }
}

#[derive(Clone)]
pub(crate) enum CommandView {
    Editor(Entity<EditorView>),
    Terminal(Entity<TerminalView>),
    Extension(ExtensionView),
    Workspace(Entity<WorkspaceTree>),
}

impl CommandView {
    pub(crate) fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Editor(view) => view.focus_handle(cx),
            Self::Terminal(view) => view.focus_handle(cx),
            Self::Extension(view) => view.focus_handle(cx),
            Self::Workspace(view) => view.focus_handle(cx),
        }
    }
}

/// The view behind one presentation.
enum PresentationView {
    Editor(Entity<EditorView>),
    Terminal(Entity<TerminalView>),
    Tool(ToolContent),
}

/// One tab of a restored arrangement, materialized by its kind's step.
pub(crate) enum ArrangedTab {
    Document {
        document: DocumentId,
        model: Entity<Buffer>,
        view: EditorViewState,
    },
    Tool(ToolFrameId),
    Terminal(TerminalSessionId, Option<Entity<TerminalView>>),
}

impl ArrangedTab {
    /// The tab's surface, and how to create its view. A tool presents the
    /// view it was registered with.
    fn split(self) -> (TabSurfaceId, Option<ArrangedView>) {
        match self {
            Self::Document {
                document,
                model,
                view,
            } => (
                TabSurfaceId::Document(document),
                Some(ArrangedView::Editor(model, view)),
            ),
            Self::Tool(id) => (TabSurfaceId::Tool(id), None),
            Self::Terminal(session, view) => (
                TabSurfaceId::Terminal(session),
                view.map(ArrangedView::Terminal),
            ),
        }
    }
}

enum ArrangedView {
    Editor(Entity<Buffer>, EditorViewState),
    Terminal(Entity<TerminalView>),
}

impl ArrangedView {
    fn create(self, cx: &mut Context<Workbench>) -> PresentationView {
        match self {
            Self::Editor(model, state) => PresentationView::Editor(cx.new(|cx| {
                let editor = EditorView::new(model, cx);
                editor
                    .editor()
                    .update(cx, |editor, cx| editor.restore_view_state(state, cx));
                editor
            })),
            Self::Terminal(view) => PresentationView::Terminal(view),
        }
    }
}

pub(crate) struct Workbench {
    layout: layout::Workbench,
    views: HashMap<PresentationId, PresentationView>,
}

impl Deref for Workbench {
    type Target = layout::Workbench;

    fn deref(&self) -> &Self::Target {
        &self.layout
    }
}

impl DerefMut for Workbench {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.layout
    }
}

impl Workbench {
    pub(crate) fn empty() -> Self {
        Self {
            layout: layout::Workbench::empty(),
            views: HashMap::new(),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn new_for_document(
        document_id: DocumentId,
        model: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut workbench = Self {
            layout: layout::Workbench::new_for_document(document_id),
            views: HashMap::new(),
        };
        let tab = workbench
            .focused_pane()
            .unwrap()
            .active_tab()
            .presentation();
        workbench.attach_editor(tab, model, cx);
        workbench
    }

    #[cfg(test)]
    pub(crate) fn new(document: &super::documents::Document, cx: &mut Context<Self>) -> Self {
        Self::new_for_document(document.id(), document.model().clone(), cx)
    }

    /// The editor a document tab presents.
    pub(crate) fn editor(&self, tab: &WorkbenchTab) -> Option<&Entity<EditorView>> {
        match self.views.get(&tab.presentation())? {
            PresentationView::Editor(editor) => Some(editor),
            PresentationView::Terminal(_) | PresentationView::Tool(_) => None,
        }
    }

    /// The view attached to a terminal tab, if any.
    pub(crate) fn terminal_view(&self, tab: &WorkbenchTab) -> Option<&Entity<TerminalView>> {
        match self.views.get(&tab.presentation())? {
            PresentationView::Terminal(view) => Some(view),
            PresentationView::Editor(_) | PresentationView::Tool(_) => None,
        }
    }

    /// The editor of the focused pane's active tab.
    pub(crate) fn focused_editor(&self) -> Option<&Entity<EditorView>> {
        self.editor(self.focused_pane()?.active_tab())
    }

    /// The view of the focused pane's active terminal tab.
    #[cfg(test)]
    pub(crate) fn focused_terminal_view(&self) -> Option<&Entity<TerminalView>> {
        self.terminal_view(self.focused_pane()?.active_tab())
    }

    pub(crate) fn focused_command_view(&self) -> Option<CommandView> {
        self.command_view(self.focused_pane()?.active_tab())
    }

    pub(crate) fn tool_content(&self, id: ToolFrameId) -> Option<&ToolContent> {
        match self.views.get(&self.tool(id)?.presentation)? {
            PresentationView::Tool(content) => Some(content),
            PresentationView::Editor(_) | PresentationView::Terminal(_) => None,
        }
    }

    pub(crate) fn command_view(&self, tab: &WorkbenchTab) -> Option<CommandView> {
        Some(match self.views.get(&tab.presentation())? {
            PresentationView::Editor(editor) => CommandView::Editor(editor.clone()),
            PresentationView::Terminal(view) => CommandView::Terminal(view.clone()),
            PresentationView::Tool(content) => content.command_view(),
        })
    }

    /// The tool whose view is `content`.
    pub(crate) fn tool_presenting(&self, content: EntityId) -> Option<ToolFrameId> {
        self.tools().iter().map(|tool| tool.id).find(|id| {
            self.tool_content(*id)
                .is_some_and(|tool| tool.entity_id() == content)
        })
    }

    fn presentation_of(&self, pane: PaneId, tab: TabId) -> PresentationId {
        self.tab(pane, tab)
            .expect("the layout just reported this tab")
            .presentation()
    }

    fn attach_editor(
        &mut self,
        presentation: PresentationId,
        model: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) {
        let editor = cx.new(|cx| EditorView::new(model, cx));
        self.views
            .insert(presentation, PresentationView::Editor(editor));
    }

    /// Drop the views whose presentation no tab or tool still shows.
    fn drop_ended_views(&mut self) {
        let layout = &self.layout;
        self.views.retain(|presentation, _| {
            layout.tabs().any(|tab| tab.presentation() == *presentation)
                || layout
                    .tools()
                    .iter()
                    .any(|tool| tool.presentation == *presentation)
        });
    }

    pub(crate) fn open_tab_for_document(
        &mut self,
        pane_id: PaneId,
        document_id: DocumentId,
        model: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        let tab = self.layout.open_tab_for_document(pane_id, document_id)?;
        self.attach_editor(self.presentation_of(pane_id, tab), model, cx);
        Some(tab)
    }

    pub(crate) fn open_document_in_center(
        &mut self,
        document: DocumentId,
        model: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> (PaneId, TabId) {
        let (pane, tab) = self.layout.open_document_in_center(document);
        self.attach_editor(self.presentation_of(pane, tab), model, cx);
        (pane, tab)
    }

    /// Open a document as a pane's preview tab; see
    /// [`layout::Workbench::open_preview_for_document`].
    pub(crate) fn open_preview_for_document(
        &mut self,
        pane_id: PaneId,
        document_id: DocumentId,
        model: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Option<(PaneId, TabId, Option<DocumentId>)> {
        let opened = self
            .layout
            .open_preview_for_document(pane_id, document_id)?;
        self.attach_editor(self.presentation_of(opened.0, opened.1), model, cx);
        self.drop_ended_views();
        Some(opened)
    }

    /// Open a document as the center's preview tab; see
    /// [`layout::Workbench::open_preview_in_center`].
    pub(crate) fn open_preview_in_center(
        &mut self,
        document_id: DocumentId,
        model: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> (PaneId, TabId, Option<DocumentId>) {
        let opened = self.layout.open_preview_in_center(document_id);
        self.attach_editor(self.presentation_of(opened.0, opened.1), model, cx);
        self.drop_ended_views();
        opened
    }

    pub(crate) fn replace_tab_document(
        &mut self,
        pane_id: PaneId,
        tab_id: TabId,
        document_id: DocumentId,
        model: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .layout
            .replace_tab_document(pane_id, tab_id, document_id)
        {
            return false;
        }
        self.attach_editor(self.presentation_of(pane_id, tab_id), model, cx);
        self.drop_ended_views();
        true
    }

    pub(crate) fn open_terminal_tab(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        view: Entity<TerminalView>,
    ) -> Option<TabId> {
        let tab = self.layout.open_terminal_tab(pane_id, session_id)?;
        self.views.insert(
            self.presentation_of(pane_id, tab),
            PresentationView::Terminal(view),
        );
        Some(tab)
    }

    pub(crate) fn open_terminal_in_center(
        &mut self,
        session: TerminalSessionId,
        view: Entity<TerminalView>,
    ) -> Option<(PaneId, TabId)> {
        let (pane, tab) = self.layout.open_terminal_in_center(session)?;
        self.views.insert(
            self.presentation_of(pane, tab),
            PresentationView::Terminal(view),
        );
        Some((pane, tab))
    }

    /// Open a terminal in `dock`'s last focused pane, creating the dock's
    /// first pane when it has none, and show the dock.
    pub(crate) fn open_terminal_in_dock(
        &mut self,
        dock: DockLocation,
        session: TerminalSessionId,
        view: Entity<TerminalView>,
    ) -> Option<(PaneId, TabId)> {
        let (pane, tab) = self.layout.open_terminal_in_dock(dock, session)?;
        self.views.insert(
            self.presentation_of(pane, tab),
            PresentationView::Terminal(view),
        );
        Some((pane, tab))
    }

    /// Split one captured pane and show its active document in a fresh view.
    pub(crate) fn split_pane(
        &mut self,
        pane_id: PaneId,
        direction: SplitDirection,
        placement: SplitPlacement,
        cx: &mut Context<Self>,
    ) -> Option<PaneId> {
        let active = self.pane(pane_id)?.active_tab();
        let model = self.editor(active)?.read(cx).model().clone();
        let pane = self.layout.split_pane(pane_id, direction, placement)?;
        let presentation = self.pane(pane).unwrap().active_tab().presentation();
        self.attach_editor(presentation, model, cx);
        Some(pane)
    }

    /// Split a captured pane, giving a terminal's split the independent
    /// session `new_session_id` and its view.
    pub(crate) fn split_pane_with_terminal(
        &mut self,
        pane_id: PaneId,
        direction: SplitDirection,
        placement: SplitPlacement,
        new_session_id: TerminalSessionId,
        view: Entity<TerminalView>,
        cx: &mut Context<Self>,
    ) -> Option<PaneId> {
        if self.pane(pane_id)?.active_tab().document_id().is_some() {
            return self.split_pane(pane_id, direction, placement, cx);
        }
        let pane =
            self.layout
                .split_pane_with_terminal(pane_id, direction, placement, new_session_id)?;
        let presentation = self.pane(pane).unwrap().active_tab().presentation();
        self.views
            .insert(presentation, PresentationView::Terminal(view));
        Some(pane)
    }

    /// Define a tool presenting `content`, or return the tool that already
    /// presents it.
    pub(crate) fn register_tool(
        &mut self,
        stable_id: impl Into<String>,
        title: impl Into<String>,
        content: ToolContent,
        dock: DockLocation,
    ) -> ToolFrameId {
        if let Some(id) = self.tool_presenting(content.entity_id()) {
            return id;
        }
        let id = self.layout.register_tool(stable_id, title, dock);
        let presentation = self.tool(id).unwrap().presentation;
        self.views
            .insert(presentation, PresentationView::Tool(content));
        id
    }

    pub(crate) fn unregister_tool(&mut self, id: ToolFrameId) {
        self.layout.unregister_tool(id);
        self.drop_ended_views();
    }

    pub(crate) fn remove_document_tabs(&mut self) -> Vec<DocumentId> {
        let removed = self.layout.remove_document_tabs();
        self.drop_ended_views();
        removed
    }

    pub(crate) fn request_close_tab(
        &mut self,
        pane_id: PaneId,
        tab_id: TabId,
        document_id: DocumentId,
        document_is_dirty: bool,
        open_view_count: usize,
    ) -> Option<CloseRequestOutcome> {
        let outcome = self.layout.request_close_tab(
            pane_id,
            tab_id,
            document_id,
            document_is_dirty,
            open_view_count,
        );
        self.drop_ended_views();
        outcome
    }

    pub(crate) fn request_close_terminal_tab(
        &mut self,
        pane_id: PaneId,
        tab_id: TabId,
        session_id: TerminalSessionId,
    ) -> Option<layout::CloseTransition> {
        let transition = self
            .layout
            .request_close_terminal_tab(pane_id, tab_id, session_id);
        self.drop_ended_views();
        transition
    }

    pub(crate) fn resolve_pending_close(
        &mut self,
        pending: layout::PendingClose,
        confirmation: CloseConfirmation,
        open_view_count: usize,
    ) -> Option<layout::CloseTransition> {
        let transition = self
            .layout
            .resolve_pending_close(pending, confirmation, open_view_count);
        self.drop_ended_views();
        transition
    }

    /// Move a document or terminal tab into `destination` with its view. A
    /// terminal's view passes through `handoff`, which returns the view the
    /// destination attaches.
    pub(crate) fn transfer_tab_to(
        &mut self,
        destination: &mut Workbench,
        source: TabMoveCapture,
        target: TabMoveDestination,
        handoff: impl FnOnce(Entity<TerminalView>) -> Entity<TerminalView>,
    ) -> Option<layout::TabMoveTransition> {
        let moved = self
            .layout
            .transfer_tab_to(&mut destination.layout, source, target)?;
        let arrived =
            destination.presentation_of(moved.destination.pane_id, moved.destination.tab_id);
        let view = match self.views.remove(&source.presentation) {
            Some(PresentationView::Terminal(view)) => {
                Some(PresentationView::Terminal(handoff(view)))
            }
            view => view,
        };
        if let Some(view) = view {
            destination.views.insert(arrived, view);
        }
        Some(moved)
    }

    /// Install one restored tab; see [`layout::Workbench::insert_arranged`].
    pub(crate) fn insert_arranged(
        &mut self,
        pane: PaneId,
        index: usize,
        tab: ArrangedTab,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> (PaneId, TabId) {
        let (surface, view) = tab.split();
        let (pane, tab) = self.layout.insert_arranged(pane, index, surface, pinned);
        if let Some(view) = view {
            let view = view.create(cx);
            self.views.insert(self.presentation_of(pane, tab), view);
        }
        (pane, tab)
    }

    /// Replace every tab with a restored arrangement, each document tab in
    /// its own view with its restored state; see
    /// [`layout::Workbench::arrange`].
    pub(crate) fn arrange(
        &mut self,
        arrangement: Arrangement<ArrangedTab>,
        cx: &mut Context<Self>,
    ) {
        let installed = self
            .layout
            .arrange(arrangement.map(&mut ArrangedTab::split));
        self.drop_ended_views();
        for (presentation, view) in installed {
            if let Some(view) = view {
                let view = view.create(cx);
                self.views.insert(presentation, view);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, TestAppContext};

    use super::*;
    use crate::app::documents::DocumentCollection;
    use crate::app::model::BufferModel;

    #[gpui::test]
    fn tabs_and_splits_own_distinct_views_of_one_model(cx: &mut TestAppContext) {
        let model = cx.new(|_| Buffer::new(BufferModel::from_text("one\ntwo\nthree")));
        let mut documents = DocumentCollection::new();
        let document = cx.update(|cx| documents.create_untitled("Shared", model.clone(), cx));
        let workbench = cx.new(|cx| Workbench::new(documents.get(document).unwrap(), cx));

        workbench.update(cx, |workbench, cx| {
            let first_pane = workbench.focused_pane_id().unwrap();
            let active_editor = |workbench: &Workbench, pane| {
                workbench
                    .editor(workbench.pane(pane).unwrap().active_tab())
                    .unwrap()
                    .clone()
            };
            let first_editor = active_editor(workbench, first_pane);
            workbench
                .open_tab_for_document(first_pane, document, model.clone(), cx)
                .unwrap();
            let second_editor = active_editor(workbench, first_pane);
            let split_pane = workbench
                .split_pane(
                    first_pane,
                    SplitDirection::Vertical,
                    SplitPlacement::After,
                    cx,
                )
                .unwrap();
            let third_editor = active_editor(workbench, split_pane);

            assert_ne!(first_editor, second_editor);
            assert_ne!(second_editor, third_editor);
            assert_eq!(first_editor.read(cx).model(), &model);
            assert_eq!(second_editor.read(cx).model(), &model);
            assert_eq!(third_editor.read(cx).model(), &model);

            let state = |caret, scroll_x| EditorViewState {
                anchor: caret,
                caret,
                top: 0,
                scroll_x,
                folds: Vec::new(),
            };
            for (editor, state) in [
                (&first_editor, state(1, 10.)),
                (&second_editor, state(4, 20.)),
                (&third_editor, state(9, 30.)),
            ] {
                editor.update(cx, |editor, cx| {
                    editor
                        .editor()
                        .clone()
                        .update(cx, |editor, cx| editor.restore_view_state(state, cx))
                });
            }
            let view_state = |editor: &Entity<EditorView>, cx: &App| {
                editor.read(cx).editor().read(cx).view_state(cx)
            };
            assert_eq!(view_state(&first_editor, cx), state(1, 10.));
            assert_eq!(view_state(&second_editor, cx), state(4, 20.));
            assert_eq!(view_state(&third_editor, cx), state(9, 30.));
        });
    }

    #[gpui::test]
    fn closing_or_replacing_a_tab_drops_its_view_and_keeps_the_document(cx: &mut TestAppContext) {
        let first_model = cx.new(|_| Buffer::new(BufferModel::from_text("first")));
        let second_model = cx.new(|_| Buffer::new(BufferModel::from_text("second")));
        let second_model_weak = second_model.downgrade();
        let mut documents = DocumentCollection::new();
        let first = cx.update(|cx| documents.create_untitled("First", first_model.clone(), cx));
        let second = cx.update(|cx| documents.create_untitled("Second", second_model.clone(), cx));
        drop(second_model);
        let workbench = cx.new(|cx| Workbench::new(documents.get(first).unwrap(), cx));

        let (closed, replaced) = workbench.update(cx, |workbench, cx| {
            let pane = workbench.focused_pane_id().unwrap();
            let first_tab = workbench.focused_pane().unwrap().active_tab_id();
            let replaced = workbench
                .editor(workbench.focused_pane().unwrap().active_tab())
                .unwrap()
                .downgrade();
            let model = documents.get(second).unwrap().model().clone();
            let second_tab = workbench
                .open_tab_for_document(pane, second, model, cx)
                .unwrap();
            let closed = workbench
                .editor(workbench.tab(pane, second_tab).unwrap())
                .unwrap()
                .downgrade();
            let outcome = workbench
                .request_close_tab(pane, second_tab, second, false, 1)
                .unwrap();
            assert!(matches!(outcome, CloseRequestOutcome::Closed(_)));
            assert_eq!(workbench.focused_pane().unwrap().active_tab_id(), first_tab);

            assert!(workbench.replace_tab_document(
                pane,
                first_tab,
                first,
                first_model.clone(),
                cx
            ));
            (closed, replaced)
        });
        cx.run_until_parked();
        assert!(closed.upgrade().is_none());
        assert!(replaced.upgrade().is_none());
        workbench.read_with(cx, |workbench, _| {
            let tab = workbench.focused_pane().unwrap().active_tab();
            assert!(workbench.editor(tab).is_some());
        });
        assert!(second_model_weak.upgrade().is_some());
        assert!(cx.update(|cx| documents.remove(second, cx)));
        assert!(second_model_weak.upgrade().is_none());
    }

    #[gpui::test]
    fn unregistering_a_tool_drops_its_view_but_hiding_keeps_it(cx: &mut TestAppContext) {
        let workbench = cx.new(|_| Workbench::empty());
        let tree = cx.new(|cx| {
            WorkspaceTree::new(
                crate::app::workspace::WorkspaceState::default().snapshot(),
                cx,
            )
        });
        let weak = tree.downgrade();
        workbench.update(cx, |workbench, _| {
            let id = workbench.register_tool(
                "knot.explorer",
                "Explorer",
                ToolContent::Workspace(tree.clone()),
                DockLocation::Left,
            );
            let again = workbench.register_tool(
                "knot.explorer",
                "Explorer",
                ToolContent::Workspace(tree),
                DockLocation::Left,
            );
            assert_eq!(again, id);
            workbench.show_tool(id).unwrap();
            assert!(workbench.hide_tool(id));
            assert!(workbench.tool_content(id).is_some());
            workbench.unregister_tool(id);
            assert!(workbench.tool_content(id).is_none());
        });
        cx.run_until_parked();
        assert!(weak.upgrade().is_none());
    }
}
