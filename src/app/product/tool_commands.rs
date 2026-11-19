//! Window operations behind the tool-window commands: hiding and jumping
//! between tool windows, stretching, splits, and renaming a terminal session.

use super::*;
use crate::app::transient::InputSpec;
use crate::app::workbench::{Slot, SplitEdge, TabMoveDestination, TabMovePlacement, ToolKind};
use knot_protocol::CommandOutcome;

impl KnotWindow {
    /// The tool window a command acts on: the captured pane's, else the one
    /// focus entered last.
    fn command_tool(&self, pane: Option<PaneId>, cx: &App) -> Option<ToolFrameId> {
        let workbench = self.workbench.read(cx);
        pane.and_then(|pane| workbench.pane_tool(pane))
            .or_else(|| workbench.active_tool())
    }

    /// Hide the active tool window.
    pub(super) fn hide_active_tool(
        &mut self,
        pane: Option<PaneId>,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        let tool = self
            .command_tool(pane, cx)
            .ok_or(CommandOutcome::Unavailable)?;
        self.hide_tool_from_pointer(tool, cx);
        Ok(())
    }

    /// Show and focus the tool window focus was in before the current one.
    pub(super) fn jump_to_last_tool(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        let tool = self
            .workbench
            .read(cx)
            .last_tool()
            .ok_or(CommandOutcome::Unavailable)?;
        let shown = self
            .workbench
            .update(cx, |workbench, _| workbench.show_tool(tool).is_some());
        if shown {
            self.refresh_tool_tree(tool, cx);
        } else {
            self.run_new_tab(tool, cx);
        }
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
        Ok(())
    }

    /// Hide every tool window, or show the ones the last call hid.
    pub(super) fn hide_all_tools(&mut self, cx: &mut Context<Self>) -> Result<(), CommandOutcome> {
        if !self
            .workbench
            .update(cx, |workbench, _| workbench.toggle_hide_all_tools())
        {
            return Err(CommandOutcome::Unavailable);
        }
        self.publish_tool_layout(cx);
        self.focus_active_surface(cx);
        cx.notify();
        Ok(())
    }

    /// Move the edge of the active tool window's side toward `edge`.
    pub(super) fn stretch_active_tool(
        &mut self,
        pane: Option<PaneId>,
        edge: SplitEdge,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        let tool = self
            .command_tool(pane, cx)
            .ok_or(CommandOutcome::Unavailable)?;
        if !self
            .workbench
            .update(cx, |workbench, _| workbench.stretch_tool(tool, edge))
        {
            return Err(CommandOutcome::Unavailable);
        }
        self.publish_tool_layout(cx);
        cx.notify();
        Ok(())
    }

    /// Stretch a tool window's side from its menu.
    pub(super) fn stretch_tool_from_menu(
        &mut self,
        tool: ToolFrameId,
        edge: SplitEdge,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        if !self
            .workbench
            .update(cx, |workbench, _| workbench.stretch_tool(tool, edge))
        {
            return Err(CommandOutcome::Unavailable);
        }
        self.publish_tool_layout(cx);
        cx.notify();
        Ok(())
    }

    /// Move the active tool window to `slot`.
    pub(super) fn move_active_tool(
        &mut self,
        pane: Option<PaneId>,
        slot: Slot,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        let tool = self
            .command_tool(pane, cx)
            .ok_or(CommandOutcome::Unavailable)?;
        if self.workbench.read(cx).tool(tool).map(|tool| tool.slot()) == Some(slot) {
            return Err(CommandOutcome::Unavailable);
        }
        self.move_tool_to_slot(tool, slot, cx);
        Ok(())
    }

    /// Maximize the focused pane's tool window, or restore the layout.
    pub(super) fn maximize_active_tool(
        &mut self,
        pane: PaneId,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        if self.workbench.read(cx).pane_tool(pane).is_none() {
            return Err(CommandOutcome::Unavailable);
        }
        self.toggle_maximize(pane, cx);
        Ok(())
    }

    /// Move a tab into a new pane split off its own on `edge`.
    pub(super) fn split_and_move(
        &mut self,
        pane: PaneId,
        tab: TabId,
        edge: SplitEdge,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        self.maximized = None;
        let source = self
            .capture_tab_relocation(pane, tab, cx)
            .ok_or(CommandOutcome::InvalidTarget)?;
        if self
            .workbench
            .read(cx)
            .pane(pane)
            .is_none_or(|pane| pane.tabs().len() < 2)
        {
            return Err(CommandOutcome::Unavailable);
        }
        let destination = relocate::TabRelocationDestination {
            window: cx.weak_entity(),
            workbench: self.workbench.downgrade(),
            tab: TabMoveDestination::At {
                anchor: source.tab,
                placement: TabMovePlacement::Split(edge),
            },
        };
        match self.relocate_tab_to(&source, &destination, cx) {
            CommandOutcome::Completed => Ok(()),
            outcome => Err(outcome),
        }
    }

    /// Merge every pane of `pane`'s area into it.
    pub(super) fn unsplit_pane(
        &mut self,
        pane: PaneId,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        self.maximized = None;
        if !self
            .workbench
            .update(cx, |workbench, _| workbench.unsplit(pane))
        {
            return Err(CommandOutcome::Unavailable);
        }
        self.focus_active_surface(cx);
        cx.notify();
        Ok(())
    }

    /// Move an editor tab back to the container tool window that accepts it.
    pub(super) fn return_to_tool_window(
        &mut self,
        pane: PaneId,
        tab: TabId,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        let workbench = self.workbench.read(cx);
        let surface = workbench
            .tab(pane, tab)
            .map(|tab| tab.surface_id())
            .ok_or(CommandOutcome::InvalidTarget)?;
        let container = workbench
            .tools()
            .iter()
            .find(|tool| {
                tool.kind() == ToolKind::Container && workbench.allows(surface, Area::Tool(tool.id))
            })
            .map(|tool| Area::Tool(tool.id))
            .ok_or(CommandOutcome::Unavailable)?;
        self.move_tab_to_area(pane, tab, container, cx)
    }

    /// Ask for a new label for a terminal session's tab.
    pub(super) fn start_rename_session(
        &mut self,
        pane: PaneId,
        tab: TabId,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        let Some(TabSurfaceId::Terminal(id)) = self
            .workbench
            .read(cx)
            .tab(pane, tab)
            .map(|tab| tab.surface_id())
        else {
            return Err(CommandOutcome::Unavailable);
        };
        let initial = Self::terminal_session_label(id, cx).unwrap_or_default();
        let (receiver, _) = self
            .begin_owned_input(
                None,
                InputSpec {
                    title: "Rename Session".into(),
                    initial,
                    ..InputSpec::default()
                },
                None,
                cx,
            )
            .map_err(|_| CommandOutcome::Unavailable)?;
        let task = cx.spawn(async move |this, cx| {
            let Ok(TransientOutcome::Input(name, _)) = receiver.await else {
                return;
            };
            let name = name.trim().to_owned();
            if name.is_empty() {
                return;
            }
            let _ = this.update(cx, |_, cx| {
                Self::set_terminal_name(id, name, cx);
                cx.notify();
            });
        });
        self.tasks.push(task);
        Ok(())
    }
}
