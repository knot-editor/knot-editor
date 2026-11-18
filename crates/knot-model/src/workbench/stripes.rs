//! The buttons of the tool stripes, grouped by the slot each tool window sits
//! in.

use super::{Slot, ToolFrameId, Workbench};

/// The stripe buttons by slot, in order then registration order. Tool windows removed
/// from the sidebar have none.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Stripes {
    groups: [Vec<ToolFrameId>; 6],
}

impl Stripes {
    pub fn group(&self, slot: Slot) -> &[ToolFrameId] {
        &self.groups[slot.index()]
    }

    pub fn is_empty(&self) -> bool {
        self.groups.iter().all(Vec::is_empty)
    }
}

impl Workbench {
    pub fn stripes(&self) -> Stripes {
        let mut stripes = Stripes::default();
        let mut tools = self
            .tools
            .iter()
            .filter(|tool| tool.on_sidebar())
            .collect::<Vec<_>>();
        tools.sort_by_key(|tool| tool.order);
        for tool in tools {
            stripes.groups[tool.slot().index()].push(tool.id);
        }
        stripes
    }

    /// The tool windows without a stripe button, which the More tool windows
    /// list offers, in registration order.
    pub fn off_sidebar_tools(&self) -> Vec<ToolFrameId> {
        self.tools
            .iter()
            .filter(|tool| !tool.on_sidebar())
            .map(|tool| tool.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::{DocumentId, ToolKind};

    #[test]
    fn buttons_group_by_slot_and_leave_with_the_sidebar() {
        let workbench = &mut Workbench::empty();
        workbench.open_document_in_center(DocumentId::new(1));
        let explorer =
            workbench.register_tool("knot.explorer", "Explorer", Slot::LeftTop, ToolKind::Fixed);
        let log = workbench.register_tool("x.log", "Log", Slot::BottomRight, ToolKind::Fixed);
        let outline =
            workbench.register_tool("x.outline", "Outline", Slot::LeftTop, ToolKind::Fixed);

        let stripes = workbench.stripes();
        assert_eq!(stripes.group(Slot::LeftTop), [explorer, outline]);
        assert_eq!(stripes.group(Slot::BottomRight), [log]);
        assert!(stripes.group(Slot::RightTop).is_empty());

        // A moved window's button follows it.
        workbench.place_tool(log, Slot::RightBottom);
        assert_eq!(workbench.stripes().group(Slot::RightBottom), [log]);

        // A window removed from the sidebar hides and loses its button; showing
        // it brings the button back.
        workbench.show_tool(explorer);
        assert!(workbench.remove_from_sidebar(explorer));
        assert!(!workbench.tool_visible(explorer));
        assert_eq!(workbench.stripes().group(Slot::LeftTop), [outline]);
        assert_eq!(workbench.off_sidebar_tools(), [explorer]);
        workbench.show_tool(explorer);
        assert!(workbench.off_sidebar_tools().is_empty());
        assert_eq!(
            workbench.stripes().group(Slot::LeftTop),
            [explorer, outline]
        );
    }
}
