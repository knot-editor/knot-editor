//! Terminals in docks: their home dock, the most recently used one, and the
//! toggle that shows, focuses, or hides it.

use super::{Area, DockLocation, PaneId, TabId, TabKind, TerminalSessionId, Workbench};

/// The dock new terminals open in until the user moves them.
pub const DEFAULT_TERMINAL_HOME: DockLocation = DockLocation::Bottom;

/// What Toggle Terminal did, or needs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalToggle {
    /// No terminal lives in a dock: the caller opens one in the home dock.
    NeedsTerminal,
    /// The most recently used dock terminal is shown and focused.
    Shown(PaneId, TabId),
    /// The focused dock terminal's dock is hidden.
    Hidden,
}

impl Workbench {
    /// The dock new terminals open in when none lives in a dock.
    pub fn terminal_home(&self) -> DockLocation {
        self.terminal_home
    }

    pub fn set_terminal_home(&mut self, dock: DockLocation) {
        self.terminal_home = dock;
    }

    /// The most recently focused terminal tab in a dock, else the first one
    /// in layout order.
    pub fn recent_dock_terminal(&self) -> Option<(PaneId, TabId)> {
        let in_dock = |pane: &super::Pane, tab: TabId| {
            matches!(pane.area, Area::Dock(_))
                && pane
                    .tabs
                    .iter()
                    .any(|item| item.id == tab && item.surface.kind() == TabKind::Terminal)
        };
        self.terminal_recent
            .iter()
            .find_map(|tab| {
                self.panes
                    .iter()
                    .find(|pane| in_dock(pane, *tab))
                    .map(|pane| (pane.id, *tab))
            })
            .or_else(|| {
                self.pane_order().into_iter().find_map(|pane_id| {
                    let pane = self.pane(pane_id)?;
                    let tab = pane
                        .tabs
                        .iter()
                        .find(|tab| tab.surface.kind() == TabKind::Terminal)?;
                    matches!(pane.area, Area::Dock(_)).then_some((pane_id, tab.id))
                })
            })
    }

    /// Toggle the dock terminals: show and focus the most recently used one,
    /// or hide its dock while it is focused.
    pub fn toggle_terminal(&mut self) -> TerminalToggle {
        let Some((pane, tab)) = self.recent_dock_terminal() else {
            return TerminalToggle::NeedsTerminal;
        };
        let focused = self.focused_pane_id() == Some(pane)
            && self.pane(pane).is_some_and(|pane| pane.active_tab == tab);
        if focused {
            let dock = self.dock_location(pane).expect("dock terminals are docked");
            self.set_dock_visible(dock, false);
            TerminalToggle::Hidden
        } else {
            self.activate_tab(pane, tab);
            TerminalToggle::Shown(pane, tab)
        }
    }

    /// Open a terminal session in the last focused pane of `dock`, creating
    /// the dock's first pane when it has none, and show the dock.
    pub fn open_terminal_in_dock(
        &mut self,
        dock: DockLocation,
        session: TerminalSessionId,
    ) -> Option<(PaneId, TabId)> {
        if self.has_terminal_session(session) {
            return None;
        }
        let area = Area::Dock(dock);
        let pane = self
            .area_focus(area)
            .unwrap_or_else(|| self.create_pane(area));
        self.open_terminal_tab(pane, session).map(|tab| (pane, tab))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        DocumentId, EXPLORER_TOOL_ID, StoredPlacement, TabMoveDestination, TabSurfaceId,
        ToolLayoutSnapshot,
    };
    use super::*;

    fn session(id: u64) -> TerminalSessionId {
        TerminalSessionId(id)
    }

    #[test]
    fn toggling_the_terminal_shows_focuses_then_hides_the_recent_dock_terminal() {
        let workbench = &mut Workbench::new_for_document(DocumentId::new(1));
        let center = workbench.focused_pane_id().unwrap();
        // Center terminals are ignored.
        workbench.open_terminal_in_center(session(1)).unwrap();
        assert_eq!(workbench.toggle_terminal(), TerminalToggle::NeedsTerminal);

        let home = workbench.terminal_home();
        assert_eq!(home, DEFAULT_TERMINAL_HOME);
        let (dock, first) = workbench.open_terminal_in_dock(home, session(2)).unwrap();
        assert_eq!(workbench.dock_location(dock), Some(home));
        assert_eq!(workbench.focused_pane_id(), Some(dock));
        let second = workbench.open_terminal_tab(dock, session(3)).unwrap();

        // Focused: the dock hides and focus returns to the center.
        assert_eq!(workbench.toggle_terminal(), TerminalToggle::Hidden);
        assert!(!workbench.dock_visible(home));
        assert_eq!(workbench.focused_pane_id(), Some(center));

        // Hidden: the most recently used terminal shows and takes focus.
        assert_eq!(
            workbench.toggle_terminal(),
            TerminalToggle::Shown(dock, second)
        );
        assert!(workbench.dock_visible(home));
        assert_eq!(workbench.focused_pane_id(), Some(dock));

        // Visible but unfocused: it takes focus without hiding.
        workbench.activate_tab(dock, first);
        workbench.focus_pane(center);
        assert_eq!(
            workbench.toggle_terminal(),
            TerminalToggle::Shown(dock, first)
        );
        assert_eq!(workbench.focused_pane_id(), Some(dock));
    }

    #[test]
    fn the_most_recently_focused_dock_terminal_wins_across_docks() {
        let workbench = &mut Workbench::new_for_document(DocumentId::new(1));
        let (right, right_tab) = workbench
            .open_terminal_in_dock(DockLocation::Right, session(1))
            .unwrap();
        let (bottom, bottom_tab) = workbench
            .open_terminal_in_dock(DockLocation::Bottom, session(2))
            .unwrap();
        assert_eq!(workbench.recent_dock_terminal(), Some((bottom, bottom_tab)));
        workbench.focus_pane(right);
        assert_eq!(workbench.recent_dock_terminal(), Some((right, right_tab)));
        // One that leaves the docks falls back to the next.
        workbench.relocate_tab(
            workbench.capture_tab_move(right, right_tab).unwrap(),
            TabMoveDestination::Area(Area::Center),
        );
        assert_eq!(workbench.recent_dock_terminal(), Some((bottom, bottom_tab)));
    }

    #[test]
    fn dock_terminals_survive_stored_tools_attaching_and_leave_on_reset() {
        let workbench = &mut Workbench::new_for_document(DocumentId::new(1));
        let tool = workbench.register_tool("knot.tool", "Tool", DockLocation::Right);
        let (dock, _) = workbench
            .open_terminal_in_dock(DockLocation::Right, session(1))
            .unwrap();
        let mut snapshot = ToolLayoutSnapshot::product_default();
        snapshot.docks[DockLocation::Right.index()].root = Some(super::super::StoredNode::Pane {
            selected: 0,
            tools: vec!["knot.tool".into()],
        });
        assert!(workbench.restore_stored_tool(tool, &snapshot));
        assert_eq!(
            workbench.dock_location(workbench.tool_tab(tool).unwrap().0),
            Some(DockLocation::Right)
        );
        assert!(workbench.pane(dock).is_some(), "the terminal stayed");

        // Restoring the whole record moves the dock's terminals to the center.
        snapshot.terminal_home = DockLocation::Bottom;
        workbench.restore_tool_layout(&snapshot);
        assert_eq!(workbench.terminal_home(), DockLocation::Bottom);
        assert_eq!(workbench.recent_dock_terminal(), None);
        assert!(
            workbench
                .tabs()
                .any(|tab| tab.surface_id() == TabSurfaceId::Terminal(session(1)))
        );
    }

    #[test]
    fn the_snapshot_records_the_terminal_home() {
        let workbench = &mut Workbench::empty();
        workbench.register_tool(EXPLORER_TOOL_ID, "Explorer", DockLocation::Left);
        assert_eq!(
            workbench.tool_layout_snapshot().terminal_home,
            DEFAULT_TERMINAL_HOME
        );
        workbench.set_terminal_home(DockLocation::Left);
        let snapshot = workbench.tool_layout_snapshot();
        assert_eq!(snapshot.terminal_home, DockLocation::Left);
        let decoded: ToolLayoutSnapshot =
            serde_json::from_str(&serde_json::to_string(&snapshot).unwrap()).unwrap();
        assert_eq!(decoded.terminal_home, DockLocation::Left);
        assert_eq!(
            snapshot.placement(EXPLORER_TOOL_ID),
            Some(StoredPlacement::Hidden {
                dock: DockLocation::Left,
                index: usize::MAX
            })
        );
    }
}
