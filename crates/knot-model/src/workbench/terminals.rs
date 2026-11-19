//! Terminals in container tool windows.

use super::{
    Area, PaneId, TabId, TabSurfaceId, TerminalSessionId, ToolFrameId, ToolKind, Workbench,
};

/// A terminal's label: `base`, or `base (N)` with the lowest `N` from 2 that
/// `taken` does not hold.
pub fn free_label(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|label| label == base) {
        return base.to_owned();
    }
    (2..)
        .map(|number| format!("{base} ({number})"))
        .find(|label| !taken.contains(label))
        .expect("the labels are finite")
}

impl Workbench {
    /// Open a terminal session in the last focused pane of a container tool
    /// window, creating its first pane when it has none, and show the window.
    pub fn open_terminal_in_tool(
        &mut self,
        tool: ToolFrameId,
        session: TerminalSessionId,
    ) -> Option<(PaneId, TabId)> {
        if self.tool(tool)?.kind() != ToolKind::Container || self.has_terminal_session(session) {
            return None;
        }
        let area = Area::Tool(tool);
        let pane = self
            .area_focus(area)
            .unwrap_or_else(|| self.create_pane(area));
        self.open_terminal_tab(pane, session).map(|tab| (pane, tab))
    }

    /// The pane and tab that hold a terminal session.
    pub fn terminal_tab(&self, session: TerminalSessionId) -> Option<(PaneId, TabId)> {
        self.panes.iter().find_map(|pane| {
            pane.tabs
                .iter()
                .find(|tab| tab.surface_id() == TabSurfaceId::Terminal(session))
                .map(|tab| (pane.id, tab.id))
        })
    }

    /// Whether the session's tab is shown: the active tab of a pane that is
    /// painted.
    fn terminal_shown(&self, session: TerminalSessionId) -> bool {
        self.panes.iter().any(|pane| {
            self.pane_shown(pane.id)
                && pane.tabs.iter().any(|tab| {
                    tab.id == pane.active_tab && tab.surface_id() == TabSurfaceId::Terminal(session)
                })
        })
    }

    /// Record a bell rung by a terminal. It marks the tab only while the tab
    /// is not shown. Returns whether a mark was added.
    pub fn mark_bell(&mut self, session: TerminalSessionId) -> bool {
        self.has_terminal_session(session)
            && !self.terminal_shown(session)
            && self.bell_marks.insert(session)
    }

    /// Whether the session's tab carries a bell mark.
    pub fn bell_marked(&self, session: TerminalSessionId) -> bool {
        self.bell_marks.contains(&session)
    }

    /// Whether a tool window holds a marked terminal tab.
    pub fn tool_has_bell(&self, id: ToolFrameId) -> bool {
        let area = Area::Tool(id);
        self.panes
            .iter()
            .filter(|pane| pane.area == area)
            .flat_map(|pane| &pane.tabs)
            .any(|tab| matches!(tab.surface_id(), TabSurfaceId::Terminal(s) if self.bell_marks.contains(&s)))
    }

    /// Drop the marks of tabs that are shown or gone.
    pub(super) fn prune_bell_marks(&mut self) {
        if self.bell_marks.is_empty() {
            return;
        }
        let marks = std::mem::take(&mut self.bell_marks);
        self.bell_marks = marks
            .into_iter()
            .filter(|session| self.has_terminal_session(*session) && !self.terminal_shown(*session))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DocumentId, Slot, StripeClick, TabMoveDestination};
    use super::*;

    fn session(id: u64) -> TerminalSessionId {
        TerminalSessionId(id)
    }

    #[test]
    fn labels_take_the_lowest_free_suffix() {
        let taken = |labels: &[&str]| {
            labels
                .iter()
                .map(|label| (*label).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(free_label("zsh", &[]), "zsh");
        assert_eq!(free_label("zsh", &taken(&["bash"])), "zsh");
        assert_eq!(free_label("zsh", &taken(&["zsh"])), "zsh (2)");
        assert_eq!(free_label("zsh", &taken(&["zsh", "zsh (3)"])), "zsh (2)");
        assert_eq!(free_label("zsh", &taken(&["zsh", "zsh (2)"])), "zsh (3)");
        assert_eq!(free_label("zsh", &taken(&["zsh (2)"])), "zsh");
    }

    #[test]
    fn bells_mark_only_tabs_that_are_not_shown() {
        let workbench = &mut Workbench::new_for_document(DocumentId::new(1));
        let terminal = workbench.register_tool(
            "knot.terminal",
            "Terminal",
            Slot::BottomLeft,
            ToolKind::Container,
        );
        let (pane, first) = workbench
            .open_terminal_in_tool(terminal, session(1))
            .unwrap();
        let second = workbench.open_terminal_tab(pane, session(2)).unwrap();
        workbench.activate_tab(pane, second);

        // A shown tab marks nothing; a background tab in a shown pane does.
        assert!(!workbench.mark_bell(session(2)));
        assert!(workbench.mark_bell(session(1)));
        assert!(workbench.bell_marked(session(1)));
        assert!(!workbench.mark_bell(session(9)));
        workbench.activate_tab(pane, first);
        assert!(!workbench.bell_marked(session(1)));

        // Hiding the window makes its active tab a background tab, and the
        // mark clears when the window shows again.
        workbench.hide_tool(terminal);
        assert!(workbench.mark_bell(session(1)));
        assert!(workbench.tool_has_bell(terminal));
        workbench.show_tool(terminal);
        assert!(!workbench.bell_marked(session(1)));
        assert!(!workbench.tool_has_bell(terminal));

        // Moving a marked tab to the shown center clears its mark.
        workbench.hide_tool(terminal);
        assert!(workbench.mark_bell(session(1)));
        let source = workbench.capture_tab_move(pane, first).unwrap();
        workbench
            .relocate_tab(source, TabMoveDestination::Area(Area::Center))
            .unwrap();
        assert!(!workbench.bell_marked(session(1)));
    }

    #[test]
    fn a_container_shows_with_its_first_tab_and_hides_with_its_last() {
        let workbench = &mut Workbench::new_for_document(DocumentId::new(1));
        let center = workbench.focused_pane_id().unwrap();
        let terminal = workbench.register_tool(
            "knot.terminal",
            "Terminal",
            Slot::BottomLeft,
            ToolKind::Container,
        );
        assert!(workbench.tool_is_empty(terminal));
        assert_eq!(workbench.stripe_click(terminal), StripeClick::Empty);
        assert!(!workbench.tool_visible(terminal));

        let (pane, first) = workbench
            .open_terminal_in_tool(terminal, session(1))
            .unwrap();
        assert!(workbench.tool_visible(terminal));
        assert_eq!(workbench.focused_pane_id(), Some(pane));
        let second = workbench.open_terminal_tab(pane, session(2)).unwrap();

        // Hiding keeps the tabs, and the click shows them again.
        assert_eq!(workbench.stripe_click(terminal), StripeClick::Hidden);
        assert_eq!(workbench.focused_pane_id(), Some(center));
        assert_eq!(
            workbench.stripe_click(terminal),
            StripeClick::Shown(pane, second)
        );

        // Closing or moving out the last tab hides the window.
        let source = workbench.capture_tab_move(pane, first).unwrap();
        workbench
            .relocate_tab(source, TabMoveDestination::Area(Area::Center))
            .unwrap();
        assert!(workbench.tool_visible(terminal));
        let source = workbench.capture_tab_move(pane, second).unwrap();
        workbench
            .relocate_tab(source, TabMoveDestination::Area(Area::Center))
            .unwrap();
        assert!(!workbench.tool_visible(terminal));
        assert!(workbench.tool_is_empty(terminal));
    }

    #[test]
    fn terminals_live_only_in_the_center_and_containers() {
        let workbench = &mut Workbench::new_for_document(DocumentId::new(1));
        let fixed = workbench.register_tool("x.tree", "Tree", Slot::LeftTop, ToolKind::Fixed);
        let container = workbench.register_tool(
            "x.terminal",
            "Terminal",
            Slot::BottomLeft,
            ToolKind::Container,
        );
        let (pane, _) = workbench.tool_tab(fixed).unwrap();
        assert_eq!(workbench.open_terminal_tab(pane, session(1)), None);
        assert_eq!(workbench.open_terminal_in_tool(fixed, session(1)), None);
        let (_, tab) = workbench
            .open_terminal_in_tool(container, session(1))
            .unwrap();
        let source = workbench
            .capture_tab_move(
                workbench.tool_tab(fixed).unwrap().0,
                workbench.tool_tab(fixed).unwrap().1,
            )
            .unwrap();
        assert!(
            workbench
                .relocate_tab(source, TabMoveDestination::Area(Area::Tool(container)))
                .is_none()
        );
        let _ = tab;
    }
}
