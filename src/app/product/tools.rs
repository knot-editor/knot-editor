//! Contributed views, the native explorer, workspace roots, and tool-layout memory.

use super::*;
use crate::app::workbench::TabMoveDestination;
#[cfg(any(test, feature = "test-support"))]
use crate::app::workbench::ToolFrameId;
use knot_protocol::CommandOutcome;

impl KnotWindow {
    /// Reconcile this window's contributed frames with the application-owned
    /// definitions. Attaching applies remembered placement or registration
    /// defaults once; later host notifications never override a window-local
    /// programmatic hide.
    pub(super) fn sync_contributed_views(&mut self, cx: &mut Context<Self>) {
        let definitions = crate::app::extensions::host::view_definitions(cx);
        let live = definitions
            .iter()
            .map(|definition| definition.id.as_str())
            .collect::<HashSet<_>>();
        let stale = self
            .contributed_views
            .keys()
            .filter(|id| !live.contains(id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for id in stale {
            self.detach_contributed_view(&id, cx);
        }
        for definition in definitions {
            if !self.contributed_views.contains_key(&definition.id) {
                self.attach_contributed_view(definition, cx);
            }
        }
    }

    pub(super) fn attach_contributed_view(
        &mut self,
        definition: crate::app::extensions::semantics::ViewDefinition,
        cx: &mut Context<Self>,
    ) {
        let window = window_id(cx.entity_id());
        let (content, event, observer) = match definition.content {
            knot_protocol::ViewContentKind::Tree => {
                let view = cx.new(|cx| {
                    crate::app::tree_view::TreeView::new(
                        definition.id.clone(),
                        window.clone(),
                        cx,
                    )
                });
                let event = cx.subscribe(&view, |_, _, event, cx| {
                    let event = event.clone();
                    cx.defer(move |cx| crate::app::extensions::host::route_tree_event(&event, cx));
                    cx.notify();
                });
                let observer = cx.observe(&view, |_, _, cx| cx.notify());
                (
                    crate::app::workbench::ExtensionView::Tree(view),
                    Some(event),
                    observer,
                )
            }
            knot_protocol::ViewContentKind::Text => {
                let view = cx.new(|cx| {
                    crate::app::text_view::TextView::new(
                        definition.id.clone(),
                        window.clone(),
                        cx,
                    )
                });
                let observer = cx.observe(&view, |_, _, cx| cx.notify());
                (
                    crate::app::workbench::ExtensionView::Text(view),
                    None,
                    observer,
                )
            }
        };
        if !crate::app::extensions::host::attach_view_instance(
            definition.id.clone(),
            window,
            content.clone(),
            cx,
        ) {
            return;
        }
        let stored = Self::stored_tool_layout(cx);
        let workbench_view = content.clone();
        self.workbench.update(cx, |workbench, _| {
            let previous = workbench.focused_pane_id();
            let id = workbench.register_tool(
                definition.id.clone(),
                definition.title.clone(),
                ToolContent::Extension(workbench_view),
                definition.location,
            );
            workbench.place_registered_tool(
                id,
                stored.as_ref(),
                definition.location,
                definition.order,
                definition.visible,
            );
            if let Some(pane) = previous {
                workbench.focus_pane(pane);
            }
        });
        self.contributed_views.insert(
            definition.id,
            ContributedView {
                content,
                _event: event,
                _observer: observer,
            },
        );
    }

    pub(super) fn detach_contributed_view(&mut self, stable_id: &str, cx: &mut Context<Self>) {
        let Some(instance) = self.contributed_views.remove(stable_id) else {
            return;
        };
        let view_id = instance.instance_id(cx);
        self.workbench.update(cx, |workbench, _| {
            if let Some(frame) = workbench.tool_by_stable_id(stable_id) {
                workbench.unregister_tool(frame);
            }
        });
        crate::app::extensions::host::detach_view_instance(view_id, cx);
    }

    /// Show or hide a contributed frame without publishing a fresh-window
    /// default. Returns false when this window has no live instance.
    pub(crate) fn set_contributed_view_visible(
        &mut self,
        stable_id: &str,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(content) = self
            .contributed_views
            .get(stable_id)
            .map(|instance| instance.content.clone())
        else {
            return false;
        };
        let Some(frame) = self.workbench.read(cx).tool_by_stable_id(stable_id) else {
            return false;
        };
        let visible_now = self.workbench.read(cx).tool_visible(frame);
        if visible_now == visible {
            return true;
        }
        if visible {
            self.workbench.update(cx, |workbench, _| {
                workbench.show_tool(frame);
            });
            if let crate::app::workbench::ExtensionView::Tree(tree) = content {
                tree.update(cx, |tree, cx| tree.refresh_root(cx));
            }
        } else {
            self.workbench.update(cx, |workbench, _| {
                workbench.hide_tool(frame);
            });
        }
        cx.notify();
        true
    }

    /// Show and focus a contributed frame in this window.
    pub(crate) fn focus_contributed_view(
        &mut self,
        stable_id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.set_contributed_view_visible(stable_id, true, cx) {
            return false;
        }
        let Some(frame) = self.workbench.read(cx).tool_by_stable_id(stable_id) else {
            return false;
        };
        self.workbench.update(cx, |workbench, _| {
            workbench.show_tool(frame);
        });
        let Some(view) = self
            .contributed_views
            .get(stable_id)
            .map(|instance| instance.content.clone())
        else {
            return false;
        };
        self.focus_view(&view.focus_handle(cx), cx);
        cx.notify();
        true
    }

    /// The tree a contributed view shows in this window, with the window.
    pub(crate) fn contributed_tree(
        &self,
        kind: &str,
    ) -> Option<(Entity<crate::app::tree_view::TreeView>, AnyWindowHandle)> {
        match &self.contributed_views.get(kind)?.content {
            crate::app::workbench::ExtensionView::Tree(tree) => Some((tree.clone(), self.window?)),
            crate::app::workbench::ExtensionView::Text(_) => None,
        }
    }

    /// Move a tab to the end of another area's last focused pane. Unavailable
    /// when the tab is already there or its kind does not allow the area.
    /// Moving a tool rehomes it, publishing the fresh-window default as any
    /// explicit move does.
    pub(crate) fn move_tab_to_area(
        &mut self,
        pane: PaneId,
        tab: TabId,
        area: Area,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        let source = self
            .capture_tab_relocation(pane, tab, cx)
            .ok_or(CommandOutcome::InvalidTarget)?;
        let destination = TabMoveDestination::Area(area);
        let workbench = self.workbench.read(cx);
        if workbench.pane_area(pane) == Some(area)
            || !workbench.accepts_tab_move(source.tab.tab.surface.kind(), destination)
        {
            return Err(CommandOutcome::Unavailable);
        }
        let destination = relocate::TabRelocationDestination {
            shell: cx.weak_entity(),
            workbench: self.workbench.downgrade(),
            tab: destination,
        };
        match self.relocate_tab_to(&source, &destination, cx) {
            CommandOutcome::Completed => Ok(()),
            outcome => Err(outcome),
        }
    }

    /// Toggle a tool by stable ID: show and focus it, focus it, or hide it
    /// and return to the center (see [`Workbench::toggle_tool`]). Toggling
    /// the explorer registers it first.
    pub(crate) fn toggle_tool(&mut self, stable_id: &str, cx: &mut Context<Self>) {
        let Some(id) = self.workbench.read(cx).tool_by_stable_id(stable_id) else {
            return;
        };
        let shown = self
            .workbench
            .update(cx, |workbench, _| workbench.toggle_tool(id));
        if shown
            && let Some(crate::app::workbench::ExtensionView::Tree(tree)) = self
                .contributed_views
                .get(stable_id)
                .map(|instance| instance.content.clone())
        {
            tree.update(cx, |tree, cx| tree.refresh_root(cx));
        }
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
    }

    /// Toggle a dock (see [`Workbench::toggle_dock`]).
    pub(super) fn toggle_dock(&mut self, dock: DockLocation, cx: &mut Context<Self>) {
        self.workbench
            .update(cx, |workbench, _| workbench.toggle_dock(dock));
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
    }

    pub(super) fn install_workspace(
        &mut self,
        root: crate::app::resource::ResourceUri,
        cx: &mut Context<Self>,
    ) -> crate::app::workspace::WorkspaceSnapshot {
        self.replace_workspace_roots(vec![root], cx)
    }

    pub(super) fn replace_workspace_roots(
        &mut self,
        roots: Vec<crate::app::resource::ResourceUri>,
        cx: &mut Context<Self>,
    ) -> crate::app::workspace::WorkspaceSnapshot {
        let workspace = self.workspace.replace_roots(roots);
        // The new leases are taken before the old ones drop, so a retained
        // root keeps its index.
        let providers = cx
            .try_global::<super::ApplicationFileSystems>()
            .map(|filesystems| filesystems.0.clone());
        let mut leases = Vec::new();
        if let Some(providers) = &providers {
            for root in workspace.roots() {
                leases.extend(crate::app::file_index::ApplicationFileIndex::lease(
                    cx, providers, &root.uri,
                ));
            }
        }
        self.file_index_leases = leases;
        cx.notify();
        workspace
    }

    /// The last user-customized tool arrangement, or product defaults when no
    /// store is installed. Fresh windows restore from this; live windows never
    /// read it again after creation.
    pub(super) fn stored_tool_layout(
        cx: &App,
    ) -> Option<crate::app::tool_layout::ToolLayoutSnapshot> {
        cx.try_global::<crate::app::tool_layout::ApplicationToolLayout>()
            .map(|store| store.0.current())
    }

    /// Publish this window's live arrangement as the fresh-window default.
    /// Call only for explicit user moves, resizes, and show/hide changes:
    /// restore, activation, and automatic presentation must not publish.
    pub(crate) fn publish_tool_layout(&self, cx: &App) {
        let Some(store) = cx.try_global::<crate::app::tool_layout::ApplicationToolLayout>() else {
            return;
        };
        store
            .0
            .publish(self.workbench.read(cx).tool_layout_snapshot());
    }

    /// Reset this window's tools to product defaults and publish the reset.
    pub(super) fn reset_tool_layout(&mut self, cx: &mut Context<Self>) {
        let defaults = crate::app::tool_layout::ToolLayoutSnapshot::product_default();
        let definitions = crate::app::extensions::host::view_definitions(cx);
        self.workbench.update(cx, |workbench, _| {
            workbench.restore_tool_layout(&defaults);
            for definition in definitions {
                let Some(id) = workbench.tool_by_stable_id(&definition.id) else {
                    continue;
                };
                workbench.hide_tool(id);
                workbench.remember_tool_placement(id, definition.location, definition.order);
                if definition.visible {
                    workbench.show_tool(id);
                }
            }
        });
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
    }

    /// Inherit the last customized arrangement on a fresh window. Only runs
    /// at window creation: already-open windows never re-read the default.
    /// Hidden tools stay hidden and contributed tools take their stored
    /// places as they attach. Never publishes.
    pub(super) fn restore_fresh_window_layout(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = Self::stored_tool_layout(cx) else {
            return;
        };
        self.workbench.update(cx, |workbench, _| {
            workbench.set_dock_sizes(snapshot.dock_sizes());
            workbench.set_terminal_home(snapshot.terminal_home);
        });
    }

    /// Register a left dock tool for tests of the workbench, standing in for
    /// the contributed explorer, which the knot crate's tests do not load.
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn ensure_test_tool(&mut self, show: bool, cx: &mut Context<Self>) {
        if self.test_tool_frame(cx).is_some() {
            return;
        }
        let window = super::window_id(cx.entity_id());
        let tree = cx.new(|cx| crate::app::tree_view::TreeView::new(TEST_TOOL_ID, window, cx));
        let stored = Self::stored_tool_layout(cx);
        self.workbench.update(cx, |workbench, _| {
            let previous = workbench.focused_pane_id();
            let id = workbench.register_tool(
                TEST_TOOL_ID,
                "Test Tool",
                ToolContent::Extension(crate::app::workbench::ExtensionView::Tree(tree)),
                DockLocation::Left,
            );
            if let Some(snapshot) = &stored {
                workbench.set_dock_sizes(snapshot.dock_sizes());
            }
            workbench.place_registered_tool(id, stored.as_ref(), DockLocation::Left, 0, true);
            if show {
                workbench.show_tool(id);
            }
            if let Some(pane) = previous {
                workbench.focus_pane(pane);
            }
        });
    }

    /// The tree of the tool `ensure_test_tool` registers.
    #[cfg(test)]
    pub(super) fn test_tool_tree(&self, cx: &App) -> Entity<crate::app::tree_view::TreeView> {
        let frame = self
            .test_tool_frame(cx)
            .expect("the test tool is registered");
        match self.workbench.read(cx).tool_content(frame) {
            Some(ToolContent::Extension(crate::app::workbench::ExtensionView::Tree(tree))) => {
                tree.clone()
            }
            _ => panic!("the test tool shows a tree"),
        }
    }

    /// Open a file the way a click in a tool would: as a preview or a tab,
    /// beside the tool's own tab.
    #[cfg(test)]
    pub(super) fn open_from_test_tool(
        &mut self,
        uri: crate::app::resource::ResourceUri,
        preview: bool,
        cx: &mut Context<Self>,
    ) {
        let frame = self
            .test_tool_frame(cx)
            .expect("the test tool is registered");
        let (pane, tab) = self
            .workbench
            .read(cx)
            .tool_tab(frame)
            .expect("the test tool is shown");
        let target = OpenTarget::at(cx.weak_entity(), &self.workbench, pane, tab, cx);
        drop(self.start_open_operation_with(
            OpenRequest::from_uri_for_product(uri),
            target,
            operations::OpenOptions {
                preview,
                ..Default::default()
            },
            cx,
        ));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn test_tool_frame(&self, cx: &App) -> Option<ToolFrameId> {
        self.workbench.read(cx).tool_by_stable_id(TEST_TOOL_ID)
    }
}

/// While set, every window registers the test tool as it attaches, as a
/// contributed view would.
#[cfg(test)]
pub(crate) struct TestToolInEveryWindow;

#[cfg(test)]
impl gpui::Global for TestToolInEveryWindow {}

/// The stable ID of the tool `ensure_test_tool` registers.
#[cfg(any(test, feature = "test-support"))]
pub(crate) const TEST_TOOL_ID: &str = "test.tool";
