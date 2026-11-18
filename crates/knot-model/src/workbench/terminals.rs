//! Terminals in container tool windows.

use super::{Area, PaneId, TabId, TerminalSessionId, ToolFrameId, ToolKind, Workbench};

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
