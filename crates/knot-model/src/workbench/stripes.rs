//! The buttons of the tool stripes: one per tool and one for the dock
//! terminals, grouped by the dock each belongs to.

use super::{Area, DockLocation, TabKind, ToolFrameId, Workbench};

/// What a stripe button stands for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StripeButton {
    Tool(ToolFrameId),
    /// The dock terminals (Toggle Terminal).
    Terminal,
}

/// The stripe buttons by home dock. The left stripe shows `left` at its top
/// and `bottom` at its bottom end; the right stripe shows `right`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Stripes {
    pub left: Vec<StripeButton>,
    pub right: Vec<StripeButton>,
    pub bottom: Vec<StripeButton>,
}

impl Stripes {
    pub fn group(&self, dock: DockLocation) -> &[StripeButton] {
        match dock {
            DockLocation::Left => &self.left,
            DockLocation::Right => &self.right,
            DockLocation::Bottom => &self.bottom,
        }
    }

    fn group_mut(&mut self, dock: DockLocation) -> &mut Vec<StripeButton> {
        match dock {
            DockLocation::Left => &mut self.left,
            DockLocation::Right => &mut self.right,
            DockLocation::Bottom => &mut self.bottom,
        }
    }
}

impl Workbench {
    /// The dock a tool belongs to: the one it sits in, else the one it
    /// returns to.
    pub fn tool_home(&self, id: ToolFrameId) -> Option<DockLocation> {
        let tool = self.tool(id)?;
        Some(
            self.tool_tab(id)
                .and_then(|(pane, _)| self.dock_location(pane))
                .unwrap_or_else(|| tool.remembered_dock()),
        )
    }

    /// Every tool's button by home dock, in registration order, then the
    /// terminal button in the terminal home's group.
    pub fn stripes(&self) -> Stripes {
        let mut stripes = Stripes::default();
        for tool in &self.tools {
            let home = self
                .tool_home(tool.id)
                .expect("registered tools have homes");
            stripes.group_mut(home).push(StripeButton::Tool(tool.id));
        }
        stripes
            .group_mut(self.terminal_home())
            .push(StripeButton::Terminal);
        stripes
    }

    /// Whether a terminal is in a shown dock.
    pub fn dock_terminals_visible(&self) -> bool {
        self.panes.iter().any(|pane| {
            matches!(pane.area, Area::Dock(_))
                && self.area_shown(pane.area)
                && pane
                    .tabs
                    .iter()
                    .any(|tab| tab.surface.kind() == TabKind::Terminal)
        })
    }

    /// Whether a stripe button's surface is showing.
    pub fn stripe_button_visible(&self, button: StripeButton) -> bool {
        match button {
            StripeButton::Tool(id) => self.tool_visible(id),
            StripeButton::Terminal => self.dock_terminals_visible(),
        }
    }

    /// Make `dock` the home of a tool that is not shown, leaving it hidden.
    /// A shown tool moves with Move Tab to Dock instead.
    pub fn rehome_hidden_tool(&mut self, id: ToolFrameId, dock: DockLocation) -> bool {
        if self.tool_tab(id).is_some() {
            return false;
        }
        self.remember_tool_placement(id, dock, usize::MAX);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::{DocumentId, TerminalSessionId};

    #[test]
    fn buttons_group_by_home_and_track_visibility() {
        let workbench = &mut Workbench::empty();
        workbench.open_document_in_center(DocumentId::new(1));
        let explorer = workbench.register_tool("knot.explorer", "Explorer", DockLocation::Left);
        let log = workbench.register_tool("x.log", "Log", DockLocation::Bottom);
        let outline = workbench.register_tool("x.outline", "Outline", DockLocation::Right);

        // Hidden tools sit in the group of the dock they return to; the
        // terminal button follows the terminal home.
        let stripes = workbench.stripes();
        assert_eq!(
            stripes,
            Stripes {
                left: vec![StripeButton::Tool(explorer)],
                right: vec![StripeButton::Tool(outline), StripeButton::Terminal],
                bottom: vec![StripeButton::Tool(log)],
            }
        );
        assert!(!workbench.stripe_button_visible(StripeButton::Tool(explorer)));

        workbench.show_tool(explorer);
        assert!(workbench.stripe_button_visible(StripeButton::Tool(explorer)));

        // A shown tool's dock is its home, whatever it declared.
        workbench.show_tool(log);
        workbench.place_tool(log, DockLocation::Left);
        assert_eq!(workbench.tool_home(log), Some(DockLocation::Left));
        assert_eq!(
            workbench.stripes().left,
            vec![StripeButton::Tool(explorer), StripeButton::Tool(log)]
        );

        workbench.set_terminal_home(DockLocation::Bottom);
        assert_eq!(workbench.stripes().bottom, vec![StripeButton::Terminal]);
        assert!(!workbench.dock_terminals_visible());
        workbench.open_terminal_in_dock(DockLocation::Bottom, TerminalSessionId(1));
        assert!(workbench.dock_terminals_visible());
        workbench.set_dock_visible(DockLocation::Bottom, false);
        assert!(!workbench.stripe_button_visible(StripeButton::Terminal));
    }

    #[test]
    fn a_hidden_tool_rehomes_without_showing_and_a_shown_one_does_not() {
        let workbench = &mut Workbench::empty();
        let tool = workbench.register_tool("x.log", "Log", DockLocation::Bottom);
        assert!(workbench.rehome_hidden_tool(tool, DockLocation::Right));
        assert_eq!(workbench.tool_home(tool), Some(DockLocation::Right));
        assert!(!workbench.tool_visible(tool));
        workbench.show_tool(tool);
        assert_eq!(
            workbench.dock_location(workbench.tool_tab(tool).unwrap().0),
            Some(DockLocation::Right)
        );
        assert!(!workbench.rehome_hidden_tool(tool, DockLocation::Left));
    }
}
