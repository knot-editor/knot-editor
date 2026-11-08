//! Native tool definitions outlive their visible tab attachments.

use serde::{Deserialize, Serialize};

use super::*;

pub const TOOL_LAYOUT_VERSION: u32 = 1;
pub const EXPLORER_TOOL_ID: &str = "knot.explorer";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ToolFrameId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DockLocation {
    Left,
    Right,
    Bottom,
}

impl DockLocation {
    pub const ALL: [Self; 3] = [Self::Left, Self::Right, Self::Bottom];

    /// Usable dock extents in logical pixels. Dividers clamp to this range so
    /// a dragged dock stays visible without squeezing the editor shut.
    pub const MIN_SIZE: f32 = 80.;
    pub const MAX_SIZE: f32 = 480.;

    pub fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Bottom => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolPlacement {
    Dock(DockLocation),
    Group(PaneId),
    DocumentGroup,
}

pub struct ToolFrame {
    pub id: ToolFrameId,
    pub stable_id: String,
    pub title: String,
    /// The presentation every tab of this tool shows.
    pub presentation: PresentationId,
    remembered: ToolPlacement,
    index: usize,
}

/// Where a tool-only layout record places one tool.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StoredLocation {
    Left,
    Right,
    Bottom,
    DocumentGroup,
}

impl StoredLocation {
    pub fn placement(self) -> ToolPlacement {
        match self {
            Self::Left => ToolPlacement::Dock(DockLocation::Left),
            Self::Right => ToolPlacement::Dock(DockLocation::Right),
            Self::Bottom => ToolPlacement::Dock(DockLocation::Bottom),
            Self::DocumentGroup => ToolPlacement::DocumentGroup,
        }
    }
}

impl From<DockLocation> for StoredLocation {
    fn from(dock: DockLocation) -> Self {
        match dock {
            DockLocation::Left => Self::Left,
            DockLocation::Right => Self::Right,
            DockLocation::Bottom => Self::Bottom,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoredTool {
    pub id: String,
    pub location: StoredLocation,
    pub index: usize,
    pub visible: bool,
}

/// The tool-only layout record: stable tool IDs, locations, order, sizes,
/// visibility, and each dock's selection, never documents or terminals.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolLayoutSnapshot {
    pub version: u32,
    pub tools: Vec<StoredTool>,
    pub dock_sizes: [f32; 3],
    pub dock_selected: [Option<String>; 3],
    /// Whether the user has ever customized the arrangement. Fresh windows
    /// only auto-show stored tools once set, preserving first-run behavior
    /// (no eager explorer before any folder or toggle) on fresh installs.
    #[serde(default)]
    pub customized: bool,
}

impl ToolLayoutSnapshot {
    pub fn product_default() -> Self {
        Self {
            version: TOOL_LAYOUT_VERSION,
            tools: vec![StoredTool {
                id: EXPLORER_TOOL_ID.to_owned(),
                location: StoredLocation::Left,
                index: 0,
                visible: true,
            }],
            dock_sizes: [240., 240., 220.],
            dock_selected: [Some(EXPLORER_TOOL_ID.to_owned()), None, None],
            customized: false,
        }
    }

    pub fn tool(&self, id: &str) -> Option<&StoredTool> {
        self.tools.iter().find(|tool| tool.id == id)
    }
}

impl Workbench {
    /// Define a hidden tool that first shows in `dock`.
    pub fn register_tool(
        &mut self,
        stable_id: impl Into<String>,
        title: impl Into<String>,
        dock: DockLocation,
    ) -> ToolFrameId {
        let id = ToolFrameId(self.next_tool_id);
        self.next_tool_id = self
            .next_tool_id
            .checked_add(1)
            .expect("tool identity space exhausted");
        let presentation = self.allocate_presentation();
        self.tools.push(ToolFrame {
            id,
            stable_id: stable_id.into(),
            title: title.into(),
            presentation,
            remembered: ToolPlacement::Dock(dock),
            index: usize::MAX,
        });
        id
    }

    pub fn tools(&self) -> &[ToolFrame] {
        &self.tools
    }

    pub fn tool(&self, id: ToolFrameId) -> Option<&ToolFrame> {
        self.tools.iter().find(|tool| tool.id == id)
    }

    pub fn tool_tab(&self, id: ToolFrameId) -> Option<(PaneId, TabId)> {
        self.panes.iter().find_map(|pane| {
            pane.tabs
                .iter()
                .find(|tab| tab.surface_id() == TabSurfaceId::Tool(id))
                .map(|tab| (pane.id, tab.id))
        })
    }

    pub fn dock_location(&self, pane: PaneId) -> Option<DockLocation> {
        self.docks
            .iter()
            .find(|(id, _)| *id == pane)
            .map(|(_, dock)| *dock)
    }

    pub fn dock_pane(&self, dock: DockLocation) -> Option<PaneId> {
        self.docks
            .iter()
            .find(|(_, location)| *location == dock)
            .map(|(pane, _)| *pane)
    }

    pub fn dock_size(&self, dock: DockLocation) -> f32 {
        self.dock_sizes[dock.index()]
    }

    pub fn set_dock_size(&mut self, dock: DockLocation, size: f32) {
        self.dock_sizes[dock.index()] = size.clamp(DockLocation::MIN_SIZE, DockLocation::MAX_SIZE);
        self.debug_assert_invariants();
    }

    pub fn set_dock_sizes(&mut self, sizes: [f32; 3]) {
        for dock in DockLocation::ALL {
            self.dock_sizes[dock.index()] =
                sizes[dock.index()].clamp(DockLocation::MIN_SIZE, DockLocation::MAX_SIZE);
        }
        self.debug_assert_invariants();
    }

    pub fn tool_by_stable_id(&self, stable_id: &str) -> Option<ToolFrameId> {
        self.tools
            .iter()
            .find(|tool| tool.stable_id == stable_id)
            .map(|tool| tool.id)
    }

    /// Remember a placement without showing the tool. Restore uses this for
    /// hidden tools so remembered locations survive without visible tabs.
    pub fn remember_tool_placement(
        &mut self,
        id: ToolFrameId,
        placement: ToolPlacement,
        index: usize,
    ) {
        if let Some(tool) = self.tools.iter_mut().find(|tool| tool.id == id) {
            tool.remembered = placement;
            tool.index = index;
        }
        self.debug_assert_invariants();
    }

    /// Capture the tool-only layout record. Docks map to stable locations;
    /// a tool alone in an ordinary split pane is remembered as the dock on
    /// the side its split faces, while tools sharing a group with documents
    /// flatten to the document group and vanished panes resolve the same way.
    /// Documents, terminals, and live handles never enter the record, and
    /// selection maps to stable namespaced IDs.
    pub fn tool_layout_snapshot(&self) -> ToolLayoutSnapshot {
        let stable = |id: ToolFrameId| {
            self.tools
                .iter()
                .find(|tool| tool.id == id)
                .map(|tool| tool.stable_id.to_owned())
        };
        let tools = self
            .tools
            .iter()
            .map(|tool| {
                if let Some((pane, tab)) = self.tool_tab(tool.id) {
                    let index = self
                        .pane(pane)
                        .unwrap()
                        .tabs
                        .iter()
                        .position(|item| item.id == tab)
                        .unwrap();
                    let location = match self.dock_location(pane) {
                        Some(dock) => StoredLocation::from(dock),
                        None => self.ordinary_group_location(pane),
                    };
                    StoredTool {
                        id: tool.stable_id.to_owned(),
                        location,
                        index,
                        visible: true,
                    }
                } else {
                    let location = match tool.remembered {
                        ToolPlacement::Dock(dock) => StoredLocation::from(dock),
                        ToolPlacement::DocumentGroup => StoredLocation::DocumentGroup,
                        ToolPlacement::Group(pane) => match self.dock_location(pane) {
                            Some(dock) => StoredLocation::from(dock),
                            None => self.ordinary_group_location(pane),
                        },
                    };
                    StoredTool {
                        id: tool.stable_id.to_owned(),
                        location,
                        index: tool.index,
                        visible: false,
                    }
                }
            })
            .collect();
        ToolLayoutSnapshot {
            version: TOOL_LAYOUT_VERSION,
            tools,
            dock_sizes: self.dock_sizes,
            dock_selected: DockLocation::ALL.map(|dock| {
                // A visible dock's selection is its active tab; a hidden dock
                // keeps its remembered selection so reopening restores it.
                if let Some((pane, _)) = self.docks.iter().find(|(_, location)| *location == dock)
                    && let Some(TabSurfaceId::Tool(id)) =
                        self.pane(*pane).map(|pane| pane.active_tab().surface_id())
                {
                    return stable(id);
                }
                self.dock_selected[dock.index()].and_then(stable)
            }),
            customized: false,
        }
    }

    /// Where an ordinary (non-dock) group belongs in a tool-only record. A
    /// group shared with documents flattens to the document group so document
    /// structure is never reproduced; a tool-only split pane maps to the dock
    /// on the side its split faces, or the document group for top splits and
    /// unsplit windows. Vanished panes resolve to the document group.
    fn ordinary_group_location(&self, pane: PaneId) -> StoredLocation {
        let Some(group) = self.pane(pane) else {
            return StoredLocation::DocumentGroup;
        };
        if group.tabs().iter().any(|tab| tab.document_id().is_some()) {
            return StoredLocation::DocumentGroup;
        }
        self.split_side(pane)
            .map(StoredLocation::from)
            .unwrap_or(StoredLocation::DocumentGroup)
    }

    /// The dock on the side of the split directly containing the pane, if any.
    /// Only the innermost split counts, matching the gesture that placed the
    /// pane; top splits have no dock and report nothing.
    fn split_side(&self, target: PaneId) -> Option<DockLocation> {
        fn walk(
            layout: &WorkbenchLayout,
            target: PaneId,
        ) -> Option<(SplitDirection, SplitPlacement)> {
            match layout {
                WorkbenchLayout::Pane(_) => None,
                WorkbenchLayout::Split {
                    direction,
                    first,
                    second,
                    ..
                } => {
                    if first.contains_pane(target) {
                        walk(first, target).or(Some((*direction, SplitPlacement::Before)))
                    } else if second.contains_pane(target) {
                        walk(second, target).or(Some((*direction, SplitPlacement::After)))
                    } else {
                        None
                    }
                }
            }
        }
        let layout = self.layout.as_ref()?;
        match walk(layout, target)? {
            (SplitDirection::Horizontal, SplitPlacement::After) => Some(DockLocation::Right),
            (SplitDirection::Horizontal, SplitPlacement::Before) => Some(DockLocation::Left),
            (SplitDirection::Vertical, SplitPlacement::After) => Some(DockLocation::Bottom),
            (SplitDirection::Vertical, SplitPlacement::Before) => None,
        }
    }

    pub fn normal_pane(&self) -> Option<PaneId> {
        self.focused_pane
            .filter(|pane| self.dock_location(*pane).is_none())
            .or_else(|| {
                self.panes
                    .iter()
                    .find(|pane| self.dock_location(pane.id).is_none())
                    .map(Pane::id)
            })
    }

    /// Remove a tool definition and its visible attachment. Remembered layout
    /// records live outside the workbench and are intentionally left untouched.
    pub fn unregister_tool(&mut self, id: ToolFrameId) {
        self.hide_tool(id);
        self.tools.retain(|tool| tool.id != id);
        for selected in &mut self.dock_selected {
            if *selected == Some(id) {
                *selected = None;
            }
        }
        self.debug_assert_invariants();
    }

    pub fn hide_tool(&mut self, id: ToolFrameId) -> bool {
        let Some((pane, tab)) = self.tool_tab(id) else {
            return false;
        };
        let placement = match self.dock_location(pane) {
            Some(dock) => ToolPlacement::Dock(dock),
            None => self.remembered_group_placement(pane),
        };
        let index = self
            .pane(pane)
            .unwrap()
            .tabs
            .iter()
            .position(|item| item.id == tab)
            .unwrap();
        let tool = self.tools.iter_mut().find(|tool| tool.id == id).unwrap();
        tool.remembered = placement;
        tool.index = index;
        self.remove_tab(pane, tab);
        true
    }

    /// Placement remembered when hiding a tool from an ordinary group. A group
    /// shared with documents is remembered directly so reopening rejoins it;
    /// a tool-only split pane collapses on hide, so it remembers its dock side
    /// instead of a pane that is about to disappear.
    fn remembered_group_placement(&self, pane: PaneId) -> ToolPlacement {
        let shared = self
            .pane(pane)
            .is_some_and(|group| group.tabs().iter().any(|tab| tab.document_id().is_some()));
        if !shared && let Some(side) = self.split_side(pane) {
            return ToolPlacement::Dock(side);
        }
        ToolPlacement::Group(pane)
    }

    pub fn show_tool(&mut self, id: ToolFrameId) -> Option<(PaneId, TabId)> {
        if let Some((pane, tab)) = self.tool_tab(id) {
            self.activate_tab(pane, tab);
            return Some((pane, tab));
        }
        let tool = self.tool(id)?;
        let placement = tool.remembered;
        let index = tool.index;
        let pane = match placement {
            ToolPlacement::Dock(dock) => self.ensure_dock(dock),
            ToolPlacement::Group(pane) if self.pane(pane).is_some() => pane,
            ToolPlacement::Group(_) | ToolPlacement::DocumentGroup => self
                .normal_pane()
                .unwrap_or_else(|| self.create_group(None)),
        };
        let tab = self.new_tab(TabSurfaceId::Tool(id), false);
        let tab_id = tab.id;
        let group = self
            .panes
            .iter_mut()
            .find(|group| group.id == pane)
            .unwrap();
        group.tabs.insert(group.section_index(false, index), tab);
        group.select(tab_id);
        self.focused_pane = Some(pane);
        self.debug_assert_invariants();
        Some((pane, tab_id))
    }

    pub fn place_tool(&mut self, id: ToolFrameId, placement: ToolPlacement) -> bool {
        if self.tool(id).is_none()
            || matches!(placement, ToolPlacement::Group(pane) if self.pane(pane).is_none() || self.dock_location(pane).is_some())
        {
            return false;
        }
        // A group that contains only this tool would disappear during detach.
        if matches!(placement, ToolPlacement::Group(pane) if self.tool_tab(id).is_some_and(|(source, _)| source == pane))
        {
            return true;
        }
        self.hide_tool(id);
        let tool = self.tools.iter_mut().find(|tool| tool.id == id).unwrap();
        tool.remembered = placement;
        tool.index = usize::MAX;
        self.show_tool(id).is_some()
    }

    pub fn toggle_dock(&mut self, dock: DockLocation) {
        let visible = self
            .docks
            .iter()
            .find(|(_, location)| *location == dock)
            .map(|(pane, _)| *pane);
        if let Some(pane) = visible {
            let ids = self
                .pane(pane)
                .unwrap()
                .tabs
                .iter()
                .filter_map(|tab| match tab.surface_id() {
                    TabSurfaceId::Tool(id) => Some(id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            self.dock_selected[dock.index()] =
                match self.pane(pane).unwrap().active_tab().surface_id() {
                    TabSurfaceId::Tool(id) => Some(id),
                    _ => None,
                };
            for id in ids.into_iter().rev() {
                self.hide_tool(id);
            }
        } else {
            let mut ids = self
                .tools
                .iter()
                .filter(|tool| tool.remembered == ToolPlacement::Dock(dock))
                .map(|tool| (tool.index, tool.id))
                .collect::<Vec<_>>();
            ids.sort_by_key(|(index, _)| *index);
            for (_, id) in ids {
                self.show_tool(id);
            }
            if let Some(id) = self.dock_selected[dock.index()] {
                self.show_tool(id);
            }
        }
    }

    fn ensure_dock(&mut self, dock: DockLocation) -> PaneId {
        self.docks
            .iter()
            .find(|(_, location)| *location == dock)
            .map(|(pane, _)| *pane)
            .unwrap_or_else(|| self.create_group(Some(dock)))
    }

    pub(super) fn create_group(&mut self, dock: Option<DockLocation>) -> PaneId {
        let pane = self.allocate_pane_id();
        let leaf = WorkbenchLayout::Pane(pane);
        self.layout = Some(match self.layout.take() {
            None => leaf,
            Some(mut layout) if dock.is_none() => {
                let target = layout.first_pane();
                let (direction, placement) = match self.dock_location(target) {
                    Some(DockLocation::Left) => (SplitDirection::Horizontal, SplitPlacement::After),
                    Some(DockLocation::Right) => {
                        (SplitDirection::Horizontal, SplitPlacement::Before)
                    }
                    Some(DockLocation::Bottom) => {
                        (SplitDirection::Vertical, SplitPlacement::Before)
                    }
                    None => (SplitDirection::Horizontal, SplitPlacement::After),
                };
                assert!(layout.split_pane(target, pane, direction, placement));
                layout
            }
            Some(layout) => {
                let (direction, first, second) = match dock {
                    Some(DockLocation::Left) => (SplitDirection::Horizontal, leaf, layout),
                    Some(DockLocation::Right) | None => (SplitDirection::Horizontal, layout, leaf),
                    Some(DockLocation::Bottom) => (SplitDirection::Vertical, layout, leaf),
                };
                WorkbenchLayout::split(direction, first, second)
            }
        });
        if let Some(dock) = dock {
            self.docks.push((pane, dock));
        }
        // The caller installs the first tab before publishing this transition.
        self.panes.push(Pane::new(pane, Vec::new(), TabId(0)));
        pane
    }

    pub fn open_terminal_in_normal_group(
        &mut self,
        session: TerminalSessionId,
    ) -> Option<(PaneId, TabId)> {
        if self.has_terminal_session(session) {
            return None;
        }
        let pane = self
            .normal_pane()
            .unwrap_or_else(|| self.create_group(None));
        self.open_terminal_tab(pane, session).map(|tab| (pane, tab))
    }

    pub fn open_document_in_normal_group(&mut self, document: DocumentId) -> (PaneId, TabId) {
        let pane = self
            .normal_pane()
            .unwrap_or_else(|| self.create_group(None));
        let tab = self.open_tab_for_document(pane, document).unwrap();
        (pane, tab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explorer(workbench: &mut Workbench) -> ToolFrameId {
        workbench.register_tool(EXPLORER_TOOL_ID, "Explorer", DockLocation::Left)
    }

    #[test]
    fn tools_preserve_native_identity_group_order_and_dock_size() {
        let workbench = &mut Workbench::empty();
        {
            let document = DocumentId::new(1);
            let (pane, doc_tab) = workbench.open_document_in_normal_group(document);
            let id = explorer(workbench);
            let presentation = workbench.tool(id).unwrap().presentation;
            let (dock, _) = workbench.show_tool(id).unwrap();
            assert_eq!(workbench.dock_location(dock), Some(DockLocation::Left));
            workbench.set_dock_size(DockLocation::Left, 315.);
            assert!(workbench.hide_tool(id));
            assert!(workbench.pane(dock).is_none());
            assert!(matches!(workbench.layout(), Some(WorkbenchLayout::Pane(id)) if *id == pane));
            let (reopened, _) = workbench.show_tool(id).unwrap();
            assert_ne!(reopened, dock);
            assert_eq!(workbench.dock_size(DockLocation::Left), 315.);
            assert_eq!(workbench.tool(id).unwrap().presentation, presentation);
            assert!(workbench.place_tool(id, ToolPlacement::Group(pane)));
            let (_, tool_tab) = workbench.tool_tab(id).unwrap();
            let capture = workbench.capture_tab_move(pane, tool_tab).unwrap();
            assert_eq!(
                workbench
                    .pane(pane)
                    .unwrap()
                    .tabs()
                    .iter()
                    .map(WorkbenchTab::id)
                    .collect::<Vec<_>>(),
                [doc_tab, tool_tab]
            );
            assert_eq!(workbench.view_count(document), 1);
            workbench.hide_tool(id);
            assert!(!workbench.contains_tab_move(capture));
            let (same_pane, new_tab) = workbench.show_tool(id).unwrap();
            assert_eq!(same_pane, pane);
            assert_ne!(new_tab, tool_tab);
            assert_eq!(
                workbench.pane(pane).unwrap().tabs()[1].surface_id(),
                TabSurfaceId::Tool(id)
            );
        }
    }

    #[test]
    fn vanished_mixed_group_reopens_without_historical_document_layout() {
        let workbench = &mut Workbench::empty();
        {
            let id = explorer(workbench);
            workbench.place_tool(id, ToolPlacement::DocumentGroup);
            let old = workbench.tool_tab(id).unwrap().0;
            workbench.hide_tool(id);
            assert!(workbench.panes().is_empty());
            let (new, _) = workbench.show_tool(id).unwrap();
            assert_ne!(new, old);
            assert_eq!(workbench.panes().len(), 1);
            assert_eq!(workbench.panes()[0].tabs().len(), 1);
            assert_eq!(workbench.dock_location(new), None);
            workbench.place_tool(id, ToolPlacement::Dock(DockLocation::Bottom));
            let bottom = workbench.tool_tab(id).unwrap().0;
            assert_eq!(workbench.dock_location(bottom), Some(DockLocation::Bottom));
            workbench.toggle_dock(DockLocation::Bottom);
            assert!(workbench.panes().is_empty());
            workbench.toggle_dock(DockLocation::Bottom);
            assert_eq!(workbench.panes().len(), 1);
            let document = DocumentId::new(2);
            workbench.open_document_in_normal_group(document);
            assert!(
                matches!(workbench.layout(), Some(WorkbenchLayout::Split { direction: SplitDirection::Vertical, second, .. }) if **second == WorkbenchLayout::Pane(workbench.tool_tab(id).unwrap().0))
            );
            let capture = workbench
                .capture_tab_move(
                    workbench.tool_tab(id).unwrap().0,
                    workbench.tool_tab(id).unwrap().1,
                )
                .unwrap();
            let mut destination = Workbench::empty();
            assert!(
                workbench
                    .transfer_tab_to(&mut destination, capture, TabMoveDestination::Empty)
                    .is_none()
            );
            assert!(destination.panes().is_empty());
            assert!(workbench.contains_tab_move(capture));
        }
    }
    #[test]
    fn dock_toggle_restores_selector_order_and_selection() {
        let workbench = &mut Workbench::empty();
        {
            let ids = [
                ("knot.first", "First"),
                ("knot.second", "Second"),
                ("knot.third", "Third"),
            ]
            .map(|(stable_id, title)| {
                let id = workbench.register_tool(stable_id, title, DockLocation::Right);
                workbench.show_tool(id);
                id
            });
            workbench.show_tool(ids[1]);
            workbench.toggle_dock(DockLocation::Right);
            assert!(workbench.layout().is_none());
            workbench.toggle_dock(DockLocation::Right);
            let pane = workbench.focused_pane().unwrap();
            assert_eq!(
                pane.tabs()
                    .iter()
                    .map(WorkbenchTab::surface_id)
                    .collect::<Vec<_>>(),
                ids.map(TabSurfaceId::Tool)
            );
            assert_eq!(pane.active_tab().surface_id(), TabSurfaceId::Tool(ids[1]));
        }
    }

    #[test]
    fn tool_moves_between_groups_edges_and_docks_preserve_frame() {
        let workbench = &mut Workbench::empty();
        {
            let document = DocumentId::new(3);
            let (doc_pane, _) = workbench.open_document_in_normal_group(document);
            let id = explorer(workbench);
            let presentation = workbench.tool(id).unwrap().presentation;
            let (dock, _) = workbench.show_tool(id).unwrap();
            assert_eq!(workbench.dock_location(dock), Some(DockLocation::Left));

            // Pane-edge split moves the frame next to the document group and
            // collapses the emptied dock.
            let source = workbench
                .capture_tab_move(dock, workbench.tool_tab(id).unwrap().1)
                .unwrap();
            let anchor = workbench
                .capture_tab_move(doc_pane, workbench.pane(doc_pane).unwrap().tabs()[0].id())
                .unwrap();
            let moved = workbench
                .relocate_tab(
                    source,
                    super::TabMoveDestination::At {
                        anchor,
                        placement: super::TabMovePlacement::Split(super::SplitEdge::Right),
                    },
                )
                .unwrap();
            assert_eq!(workbench.tool(id).unwrap().presentation, presentation);
            assert_eq!(moved.destination.tab_id, source.tab.tab_id);
            assert!(!workbench.contains_tab_move(source));
            assert!(workbench.pane(dock).is_none());
            assert_eq!(workbench.focused_pane_id(), Some(moved.destination.pane_id));

            // Workbench-edge drop recreates the dock around the whole layout.
            let source = workbench
                .capture_tab_move(moved.destination.pane_id, moved.destination.tab_id)
                .unwrap();
            let docked = workbench
                .relocate_tab(
                    source,
                    super::TabMoveDestination::Dock(DockLocation::Bottom),
                )
                .unwrap();
            assert_eq!(workbench.tool(id).unwrap().presentation, presentation);
            assert_eq!(
                workbench.dock_location(docked.destination.pane_id),
                Some(DockLocation::Bottom)
            );
            assert_eq!(
                workbench.focused_pane_id(),
                Some(docked.destination.pane_id)
            );

            // Joining the document group keeps the document view count at one.
            let anchor = workbench
                .capture_tab_move(doc_pane, workbench.pane(doc_pane).unwrap().tabs()[0].id())
                .unwrap();
            let joined = workbench
                .relocate_tab(
                    workbench
                        .capture_tab_move(docked.destination.pane_id, docked.destination.tab_id)
                        .unwrap(),
                    super::TabMoveDestination::At {
                        anchor,
                        placement: super::TabMovePlacement::End,
                    },
                )
                .unwrap();
            assert_eq!(joined.destination.pane_id, doc_pane);
            assert_eq!(workbench.view_count(document), 1);
            assert!(workbench.pane(docked.destination.pane_id).is_none());
            assert_eq!(workbench.tool(id).unwrap().presentation, presentation);
        }
    }

    #[test]
    fn dock_sizes_clamp_and_survive_hide_reopen() {
        let workbench = &mut Workbench::empty();
        {
            let id = explorer(workbench);
            workbench.show_tool(id).unwrap();
            workbench.set_dock_size(DockLocation::Left, 1.);
            assert_eq!(
                workbench.dock_size(DockLocation::Left),
                DockLocation::MIN_SIZE
            );
            workbench.set_dock_size(DockLocation::Left, 10_000.);
            assert_eq!(
                workbench.dock_size(DockLocation::Left),
                DockLocation::MAX_SIZE
            );
            workbench.set_dock_size(DockLocation::Bottom, 133.);
            workbench.place_tool(id, ToolPlacement::Dock(DockLocation::Bottom));
            assert_eq!(workbench.dock_size(DockLocation::Bottom), 133.);
            assert!(workbench.hide_tool(id));
            workbench.set_dock_size(DockLocation::Bottom, 200.);
            let (pane, _) = workbench.show_tool(id).unwrap();
            assert_eq!(workbench.dock_location(pane), Some(DockLocation::Bottom));
            assert_eq!(workbench.dock_size(DockLocation::Bottom), 200.);
        }
    }

    #[test]
    fn tool_snapshots_record_only_tools_with_stable_ids() {
        let workbench = &mut Workbench::empty();
        {
            let document = DocumentId::new(4);
            let (doc_pane, _) = workbench.open_document_in_normal_group(document);
            workbench.open_terminal_tab(doc_pane, super::TerminalSessionId(7));
            let id = explorer(workbench);
            workbench.show_tool(id).unwrap();
            workbench.set_dock_size(DockLocation::Left, 311.);
            // A visible tool in an ordinary group flattens to the document group.
            workbench.place_tool(id, ToolPlacement::DocumentGroup);
            let snapshot = workbench.tool_layout_snapshot();
            assert_eq!(
                snapshot.tools,
                [StoredTool {
                    id: EXPLORER_TOOL_ID.to_owned(),
                    location: StoredLocation::DocumentGroup,
                    index: 2,
                    visible: true,
                }]
            );
            assert_eq!(snapshot.dock_sizes[DockLocation::Left.index()], 311.);
            let payload = serde_json::to_string(&snapshot).unwrap();
            for forbidden in ["Document", "Terminal", "Pane(", "Tab(", "presentation"] {
                assert!(
                    !payload.contains(forbidden),
                    "tool record leaks {forbidden}"
                );
            }
            let decoded: ToolLayoutSnapshot = serde_json::from_str(&payload).unwrap();
            assert_eq!(decoded, snapshot);
            // Hidden tools keep their remembered dock while staying invisible.
            workbench.place_tool(id, ToolPlacement::Dock(DockLocation::Bottom));
            workbench.hide_tool(id);
            let hidden = workbench.tool_layout_snapshot();
            assert_eq!(hidden.tools[0].location, StoredLocation::Bottom);
            assert!(!hidden.tools[0].visible);
            assert_eq!(doc_pane, workbench.normal_pane().unwrap());
        }
    }

    #[test]
    fn tool_only_splits_remember_their_side_as_a_dock() {
        use {StoredLocation, StoredTool};
        let workbench = &mut Workbench::empty();
        {
            let document = DocumentId::new(5);
            let (doc_pane, _) = workbench.open_document_in_normal_group(document);
            let id = explorer(workbench);
            workbench.show_tool(id).unwrap();
            let split = |workbench: &mut Workbench, edge: super::SplitEdge| {
                let (_, tool_tab) = workbench.tool_tab(id).unwrap();
                let source = workbench
                    .capture_tab_move(workbench.tool_tab(id).unwrap().0, tool_tab)
                    .unwrap();
                let doc_tab = workbench.pane(doc_pane).unwrap().tabs()[0].id();
                let anchor = workbench.capture_tab_move(doc_pane, doc_tab).unwrap();
                workbench
                    .relocate_tab(
                        source,
                        super::TabMoveDestination::At {
                            anchor,
                            placement: super::TabMovePlacement::Split(edge),
                        },
                    )
                    .unwrap();
            };
            let location =
                |workbench: &Workbench| workbench.tool_layout_snapshot().tools[0].clone();
            split(workbench, super::SplitEdge::Right);
            assert_eq!(
                location(workbench),
                StoredTool {
                    id: EXPLORER_TOOL_ID.to_owned(),
                    location: StoredLocation::Right,
                    index: 0,
                    visible: true,
                }
            );
            // Hiding in a tool-only split remembers the dock side.
            workbench.hide_tool(id);
            let hidden = location(workbench);
            assert_eq!(hidden.location, StoredLocation::Right);
            assert!(!hidden.visible);
            workbench.show_tool(id).unwrap();
            split(workbench, super::SplitEdge::Bottom);
            assert_eq!(location(workbench).location, StoredLocation::Bottom);
            split(workbench, super::SplitEdge::Left);
            assert_eq!(location(workbench).location, StoredLocation::Left);
            // Top splits have no dock and flatten to the document group.
            split(workbench, super::SplitEdge::Top);
            assert_eq!(location(workbench).location, StoredLocation::DocumentGroup);
        }
    }

    #[test]
    fn tool_snapshots_flatten_vanished_groups() {
        let workbench = &mut Workbench::empty();
        {
            let id = explorer(workbench);
            workbench.place_tool(id, ToolPlacement::DocumentGroup);
            workbench.hide_tool(id);
            // The remembered pane still exists as an ordinary group.
            let remembered = workbench.tool_layout_snapshot();
            assert_eq!(remembered.tools[0].location, StoredLocation::DocumentGroup);
            // A vanished pane flattens the same way instead of naming a dead group.
            workbench.remember_tool_placement(id, ToolPlacement::Group(PaneId(999)), 3);
            let vanished = workbench.tool_layout_snapshot();
            assert_eq!(vanished.tools[0].location, StoredLocation::DocumentGroup);
            assert_eq!(vanished.tools[0].index, 3);
            assert!(!vanished.tools[0].visible);
        }
    }
}
