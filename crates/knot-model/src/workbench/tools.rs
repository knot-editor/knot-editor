//! Native tool definitions, the docks they live in, and the tool-only layout
//! record fresh windows inherit. Tool definitions outlive their visible tab
//! attachments.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::*;

pub const TOOL_LAYOUT_VERSION: u32 = 3;
/// A stable tool ID the tests register in the left dock.
#[cfg(test)]
pub(crate) const LEFT_TOOL_ID: &str = "test.left";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ToolFrameId(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DockLocation {
    Left,
    Right,
    Bottom,
}

impl DockLocation {
    pub const ALL: [Self; 3] = [Self::Left, Self::Right, Self::Bottom];

    /// Usable dock extents in logical pixels. Dividers clamp to this range so
    /// a dragged dock stays visible without squeezing the center shut.
    pub const MIN_SIZE: f32 = 80.;
    pub const MAX_SIZE: f32 = 480.;

    pub fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Bottom => 2,
        }
    }

    pub fn default_size(self) -> f32 {
        match self {
            Self::Left | Self::Right => 240.,
            Self::Bottom => 220.,
        }
    }

    /// The split direction along which the dock's size runs.
    pub fn axis(self) -> SplitDirection {
        match self {
            Self::Left | Self::Right => SplitDirection::Horizontal,
            Self::Bottom => SplitDirection::Vertical,
        }
    }
}

/// Where a hidden tool returns: the pane it left, while that pane is still in
/// its dock, else the dock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ToolPlacement {
    dock: DockLocation,
    pane: Option<PaneId>,
    index: usize,
}

pub struct ToolFrame {
    pub id: ToolFrameId,
    pub stable_id: String,
    pub title: String,
    /// The presentation every tab of this tool shows.
    pub presentation: PresentationId,
    remembered: ToolPlacement,
}

/// One dock of a tool-only layout record.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StoredDock {
    pub size: f32,
    pub visible: bool,
    pub root: Option<StoredNode>,
}

/// A dock's split tree of stable tool IDs.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StoredNode {
    Pane {
        selected: usize,
        tools: Vec<String>,
    },
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<StoredNode>,
        second: Box<StoredNode>,
    },
}

impl StoredNode {
    fn contains(&self, tool: &str) -> bool {
        match self {
            Self::Pane { tools, .. } => tools.iter().any(|id| id == tool),
            Self::Split { first, second, .. } => first.contains(tool) || second.contains(tool),
        }
    }

    /// The tree with only the tools `keep` accepts, collapsing emptied panes
    /// and one-child splits. A pruned selection falls back to the first tool.
    fn prune(&self, keep: &dyn Fn(&str) -> bool) -> Option<Self> {
        match self {
            Self::Pane { selected, tools } => {
                let kept = tools
                    .iter()
                    .filter(|tool| keep(tool))
                    .cloned()
                    .collect::<Vec<_>>();
                let selected = tools
                    .get(*selected)
                    .and_then(|tool| kept.iter().position(|id| id == tool))
                    .unwrap_or(0);
                (!kept.is_empty()).then_some(Self::Pane {
                    selected,
                    tools: kept,
                })
            }
            Self::Split {
                direction,
                ratio,
                first,
                second,
            } => match (first.prune(keep), second.prune(keep)) {
                (Some(first), Some(second)) => Some(Self::Split {
                    direction: *direction,
                    ratio: *ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (Some(remaining), None) | (None, Some(remaining)) => Some(remaining),
                (None, None) => None,
            },
        }
    }

    fn plan(&self, resolve: &dyn Fn(&str) -> Option<ToolFrameId>) -> PlanNode<ToolFrameId> {
        match self {
            Self::Pane { selected, tools } => PlanNode::Pane {
                selected: *selected,
                focused: false,
                pinned: 0,
                tabs: tools.iter().filter_map(|tool| resolve(tool)).collect(),
            },
            Self::Split {
                direction,
                ratio,
                first,
                second,
            } => PlanNode::Split {
                direction: *direction,
                ratio: *ratio,
                first: Box::new(first.plan(resolve)),
                second: Box::new(second.plan(resolve)),
            },
        }
    }
}

/// A hidden tool and the dock it returns to.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HiddenTool {
    pub id: String,
    pub dock: DockLocation,
    pub index: usize,
}

/// The tool-only layout record: each dock's size, visibility, and split tree
/// of stable tool IDs, hidden tools' docks, and the terminal home dock, never
/// documents or terminals.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolLayoutSnapshot {
    pub version: u32,
    /// The left, right, and bottom docks.
    pub docks: [StoredDock; 3],
    pub hidden: Vec<HiddenTool>,
    /// The dock new terminals open in when none lives in a dock.
    pub terminal_home: DockLocation,
    /// Whether the user has ever customized the arrangement. Fresh windows
    /// only auto-show stored tools once set, preserving first-run behavior on
    /// fresh installs.
    #[serde(default)]
    pub customized: bool,
}

/// How a tool-only record places one tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredPlacement {
    /// In a dock's tree.
    Docked(DockLocation),
    Hidden {
        dock: DockLocation,
        index: usize,
    },
}

impl ToolLayoutSnapshot {
    pub fn product_default() -> Self {
        let dock = |dock: DockLocation, root| StoredDock {
            size: dock.default_size(),
            visible: true,
            root,
        };
        Self {
            version: TOOL_LAYOUT_VERSION,
            docks: [
                dock(DockLocation::Left, None),
                dock(DockLocation::Right, None),
                dock(DockLocation::Bottom, None),
            ],
            hidden: Vec::new(),
            terminal_home: DEFAULT_TERMINAL_HOME,
            customized: false,
        }
    }

    pub fn dock(&self, dock: DockLocation) -> &StoredDock {
        &self.docks[dock.index()]
    }

    pub fn dock_sizes(&self) -> [f32; 3] {
        DockLocation::ALL.map(|dock| self.dock(dock).size)
    }

    pub fn placement(&self, id: &str) -> Option<StoredPlacement> {
        DockLocation::ALL
            .into_iter()
            .find(|dock| {
                self.dock(*dock)
                    .root
                    .as_ref()
                    .is_some_and(|root| root.contains(id))
            })
            .map(StoredPlacement::Docked)
            .or_else(|| {
                self.hidden
                    .iter()
                    .find(|tool| tool.id == id)
                    .map(|tool| StoredPlacement::Hidden {
                        dock: tool.dock,
                        index: tool.index,
                    })
            })
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
            remembered: ToolPlacement {
                dock,
                pane: None,
                index: usize::MAX,
            },
        });
        id
    }

    pub fn tools(&self) -> &[ToolFrame] {
        &self.tools
    }

    pub fn tool(&self, id: ToolFrameId) -> Option<&ToolFrame> {
        self.tools.iter().find(|tool| tool.id == id)
    }

    pub fn tool_by_stable_id(&self, stable_id: &str) -> Option<ToolFrameId> {
        self.tools
            .iter()
            .find(|tool| tool.stable_id == stable_id)
            .map(|tool| tool.id)
    }

    /// The tab showing a tool, in a visible or a hidden dock.
    pub fn tool_tab(&self, id: ToolFrameId) -> Option<(PaneId, TabId)> {
        self.panes.iter().find_map(|pane| {
            pane.tabs
                .iter()
                .find(|tab| tab.surface_id() == TabSurfaceId::Tool(id))
                .map(|tab| (pane.id, tab.id))
        })
    }

    /// Whether a tool has a tab in a painted dock.
    pub fn tool_visible(&self, id: ToolFrameId) -> bool {
        self.tool_tab(id)
            .is_some_and(|(pane, _)| self.pane_shown(pane))
    }

    pub fn dock_location(&self, pane: PaneId) -> Option<DockLocation> {
        match self.pane_area(pane)? {
            Area::Dock(dock) => Some(dock),
            Area::Center => None,
        }
    }

    pub fn dock_size(&self, dock: DockLocation) -> f32 {
        self.docks[dock.index()].size
    }

    pub fn set_dock_size(&mut self, dock: DockLocation, size: f32) {
        self.docks[dock.index()].size = size.clamp(DockLocation::MIN_SIZE, DockLocation::MAX_SIZE);
    }

    pub fn set_dock_sizes(&mut self, sizes: [f32; 3]) {
        for dock in DockLocation::ALL {
            self.set_dock_size(dock, sizes[dock.index()]);
        }
    }

    /// The dock's visible flag, which it keeps while empty.
    pub fn dock_visible(&self, dock: DockLocation) -> bool {
        self.docks[dock.index()].visible
    }

    /// Show or hide a dock with its arrangement. Hiding moves focus out of
    /// it; showing it into an unfocused workbench focuses it.
    pub fn set_dock_visible(&mut self, dock: DockLocation, visible: bool) {
        self.docks[dock.index()].visible = visible;
        self.settle_focus(Some(Area::Dock(dock)));
        self.debug_assert_invariants();
    }

    /// Remember where a hidden tool shows next without showing it. Restore
    /// uses this so hidden tools keep their dock without visible tabs.
    pub fn remember_tool_placement(&mut self, id: ToolFrameId, dock: DockLocation, index: usize) {
        if let Some(tool) = self.tools.iter_mut().find(|tool| tool.id == id) {
            tool.remembered = ToolPlacement {
                dock,
                pane: None,
                index,
            };
        }
    }

    /// Remove a tool definition and its visible attachment. Remembered layout
    /// records live outside the workbench and are intentionally left untouched.
    pub fn unregister_tool(&mut self, id: ToolFrameId) {
        self.hide_tool(id);
        self.tools.retain(|tool| tool.id != id);
        self.debug_assert_invariants();
    }

    /// Remove a tool's tab, remembering its pane and position.
    pub fn hide_tool(&mut self, id: ToolFrameId) -> bool {
        let Some((pane, tab)) = self.tool_tab(id) else {
            return false;
        };
        let dock = self
            .dock_location(pane)
            .expect("tool tabs live in the docks");
        let index = self
            .pane(pane)
            .unwrap()
            .tabs
            .iter()
            .position(|item| item.id == tab)
            .unwrap();
        let tool = self.tools.iter_mut().find(|tool| tool.id == id).unwrap();
        tool.remembered = ToolPlacement {
            dock,
            pane: Some(pane),
            index,
        };
        self.remove_tab(pane, tab);
        true
    }

    /// Show and focus a tool: its tab, showing a hidden dock, else a new tab
    /// at its remembered pane and position, else at its dock.
    pub fn show_tool(&mut self, id: ToolFrameId) -> Option<(PaneId, TabId)> {
        if let Some((pane, tab)) = self.tool_tab(id) {
            self.activate_tab(pane, tab);
            return Some((pane, tab));
        }
        let ToolPlacement { dock, pane, index } = self.tool(id)?.remembered;
        let area = Area::Dock(dock);
        let pane = pane
            .filter(|pane| self.pane_area(*pane) == Some(area))
            .or_else(|| self.area_focus(area))
            .unwrap_or_else(|| self.create_pane(area));
        let tab = self.new_tab(TabSurfaceId::Tool(id), false);
        let tab_id = tab.id;
        let group = self
            .panes
            .iter_mut()
            .find(|group| group.id == pane)
            .unwrap();
        group.tabs.insert(group.section_index(false, index), tab);
        group.select(tab_id);
        self.set_focus(pane);
        self.debug_assert_invariants();
        Some((pane, tab_id))
    }

    /// Move a tool to the end of a dock and show it.
    pub fn place_tool(&mut self, id: ToolFrameId, dock: DockLocation) -> bool {
        if self.tool(id).is_none() {
            return false;
        }
        self.hide_tool(id);
        self.remember_tool_placement(id, dock, usize::MAX);
        self.show_tool(id).is_some()
    }

    /// Toggle a tool: a hidden or unfocused tool shows and takes focus, and a
    /// focused one hides, returning focus to the center. Returns whether the
    /// tool is shown afterwards.
    pub fn toggle_tool(&mut self, id: ToolFrameId) -> bool {
        let focused = self.tool_tab(id).is_some_and(|(pane, tab)| {
            self.focused_pane_id() == Some(pane) && self.pane(pane).unwrap().active_tab == tab
        });
        if focused {
            self.hide_tool(id);
            self.focus_center();
            false
        } else {
            self.show_tool_replacing(id).is_some()
        }
    }

    /// Show and focus a tool as the switcher of its pane would: a tool that
    /// is not showing takes the place of the other tools in the pane it
    /// returns to, which hide but keep their places. Terminals stay, and a
    /// tool already showing only takes focus, so tabs the user grouped stay.
    pub fn show_tool_replacing(&mut self, id: ToolFrameId) -> Option<(PaneId, TabId)> {
        let showing = self.tool_tab(id).is_some();
        let (pane, tab) = self.show_tool(id)?;
        if !showing {
            let others = self
                .pane(pane)?
                .tabs
                .iter()
                .filter_map(|item| match item.surface_id() {
                    TabSurfaceId::Tool(other) if other != id => Some(other),
                    _ => None,
                });
            for other in others.collect::<Vec<_>>() {
                self.hide_tool(other);
            }
            self.set_focus(pane);
        }
        Some((pane, tab))
    }

    /// Show and focus a tool in a pane of its own beside the last focused
    /// pane of its shown dock, stacked the way the dock lays out its panes.
    /// A tool already showing, or whose dock is hidden or empty, shows as
    /// [`show_tool`](Self::show_tool) does.
    pub fn show_tool_beside(&mut self, id: ToolFrameId) -> Option<(PaneId, TabId)> {
        if self.tool_tab(id).is_some() {
            return self.show_tool(id);
        }
        let dock = self.tool(id)?.remembered.dock;
        let area = Area::Dock(dock);
        let Some(anchor) = self.area_focus(area).filter(|_| self.area_shown(area)) else {
            return self.show_tool(id);
        };
        let direction = match dock {
            DockLocation::Left | DockLocation::Right => SplitDirection::Vertical,
            DockLocation::Bottom => SplitDirection::Horizontal,
        };
        let pane = self.split_with(
            anchor,
            direction,
            SplitPlacement::After,
            TabSurfaceId::Tool(id),
        )?;
        let tab = self.pane(pane)?.active_tab;
        Some((pane, tab))
    }

    /// Toggle a dock with its arrangement: a hidden dock shows and takes
    /// focus, a shown but unfocused one takes focus, and a focused one hides,
    /// returning focus to the center. An empty dock shows the hidden tools
    /// that return to it.
    pub fn toggle_dock(&mut self, dock: DockLocation) {
        let area = Area::Dock(dock);
        if self.focused_pane().is_some_and(|pane| pane.area == area) {
            self.set_dock_visible(dock, false);
        } else if let Some(pane) = self.area_focus(area) {
            self.set_focus(pane);
            self.debug_assert_invariants();
        } else {
            let mut ids = self
                .tools
                .iter()
                .filter(|tool| tool.remembered.dock == dock)
                .map(|tool| (tool.remembered.index, tool.id))
                .collect::<Vec<_>>();
            ids.sort_by_key(|(index, _)| *index);
            for (_, id) in ids {
                self.show_tool(id);
            }
        }
    }

    fn dock_holds_terminals(&self, dock: DockLocation) -> bool {
        self.panes.iter().any(|pane| {
            pane.area == Area::Dock(dock)
                && pane
                    .tabs
                    .iter()
                    .any(|tab| tab.surface.kind() == TabKind::Terminal)
        })
    }

    /// Move every terminal tab of a dock to the center.
    fn evict_dock_terminals(&mut self, dock: DockLocation) {
        let terminals = self
            .panes
            .iter()
            .filter(|pane| pane.area == Area::Dock(dock))
            .flat_map(|pane| pane.tabs.iter().map(move |tab| (pane.id, tab)))
            .filter(|(_, tab)| tab.surface.kind() == TabKind::Terminal)
            .map(|(pane, tab)| (pane, tab.id))
            .collect::<Vec<_>>();
        for (pane, tab) in terminals {
            if let Some(source) = self.capture_tab_move(pane, tab) {
                self.relocate_tab(source, TabMoveDestination::Area(Area::Center));
            }
        }
    }

    /// A dock's tree of stable tool IDs, as a tool-only record keeps it.
    fn stored_dock_tree(&self, dock: DockLocation) -> Option<StoredNode> {
        fn node(workbench: &Workbench, layout: &WorkbenchLayout) -> Option<StoredNode> {
            match layout {
                WorkbenchLayout::Pane(id) => {
                    let pane = workbench.pane(*id)?;
                    let mut tools = Vec::new();
                    let mut selected = 0;
                    for tab in &pane.tabs {
                        let TabSurfaceId::Tool(tool) = tab.surface_id() else {
                            continue;
                        };
                        if tab.id == pane.active_tab {
                            selected = tools.len();
                        }
                        tools.push(workbench.tool(tool)?.stable_id.clone());
                    }
                    (!tools.is_empty()).then_some(StoredNode::Pane { selected, tools })
                }
                WorkbenchLayout::Split {
                    direction,
                    ratio,
                    first,
                    second,
                } => match (node(workbench, first), node(workbench, second)) {
                    (Some(first), Some(second)) => Some(StoredNode::Split {
                        direction: *direction,
                        ratio: *ratio,
                        first: Box::new(first),
                        second: Box::new(second),
                    }),
                    (Some(remaining), None) | (None, Some(remaining)) => Some(remaining),
                    (None, None) => None,
                },
            }
        }
        node(self, self.area_layout(Area::Dock(dock))?)
    }

    /// Capture the tool-only layout record: docks' trees of stable tool IDs,
    /// sizes, and visibility, and the dock each hidden tool returns to.
    /// Documents, terminals, and live handles never enter the record.
    pub fn tool_layout_snapshot(&self) -> ToolLayoutSnapshot {
        ToolLayoutSnapshot {
            version: TOOL_LAYOUT_VERSION,
            docks: DockLocation::ALL.map(|dock| StoredDock {
                size: self.dock_size(dock),
                visible: self.dock_visible(dock),
                root: self.stored_dock_tree(dock),
            }),
            hidden: self
                .tools
                .iter()
                .filter(|tool| self.tool_tab(tool.id).is_none())
                .map(|tool| HiddenTool {
                    id: tool.stable_id.clone(),
                    dock: tool.remembered.dock,
                    index: tool.remembered.index,
                })
                .collect(),
            terminal_home: self.terminal_home,
            customized: false,
        }
    }

    /// Replace a dock's tools with `root`, a tree of registered tools, and
    /// set its visibility. Displaced tools are hidden, and the dock's
    /// terminals move to the center.
    pub fn arrange_dock(
        &mut self,
        dock: DockLocation,
        root: Option<PlanNode<ToolFrameId>>,
        visible: bool,
    ) {
        let area = Area::Dock(dock);
        let displaced = self
            .area_panes(area)
            .into_iter()
            .flat_map(|pane| self.pane(pane).unwrap().tabs.iter())
            .filter_map(|tab| match tab.surface_id() {
                TabSurfaceId::Tool(id) => Some(id),
                _ => None,
            })
            .collect::<Vec<_>>();
        let placed = root.as_ref().map_or_else(HashSet::new, |root| {
            let mut ids = HashSet::new();
            root.for_each(&mut |id| {
                ids.insert(*id);
            });
            ids
        });
        for id in displaced.into_iter().chain(placed.iter().copied()) {
            self.hide_tool(id);
        }
        self.evict_dock_terminals(dock);
        debug_assert!(self.area_layout(area).is_none(), "the dock is empty");
        let mut focused = None;
        let mut installed = Vec::new();
        let root = root.map(|root| root.map(&mut |id| (TabSurfaceId::Tool(id), ())));
        self.tree_mut(area).layout =
            root.map(|root| self.install(root, area, &mut focused, &mut installed));
        self.docks[dock.index()].visible = visible;
        self.settle_focus(None);
        self.debug_assert_invariants();
    }

    /// Install a whole tool-only record over the registered tools: each dock
    /// gets its size, visibility, and stored tree of registered tools, and
    /// hidden tools return to their stored docks. Tools displaced from a dock
    /// are hidden.
    pub fn restore_tool_layout(&mut self, snapshot: &ToolLayoutSnapshot) {
        self.set_dock_sizes(snapshot.dock_sizes());
        self.terminal_home = snapshot.terminal_home;
        let registered = self
            .tools
            .iter()
            .map(|tool| tool.stable_id.clone())
            .collect::<HashSet<_>>();
        for dock in DockLocation::ALL {
            let stored = snapshot.dock(dock);
            let root = stored
                .root
                .as_ref()
                .and_then(|root| root.prune(&|tool| registered.contains(tool)))
                .map(|root| root.plan(&|tool| self.tool_by_stable_id(tool)));
            self.arrange_dock(dock, root, stored.visible);
        }
        for hidden in &snapshot.hidden {
            if let Some(id) = self.tool_by_stable_id(&hidden.id)
                && self.tool_tab(id).is_none()
            {
                self.remember_tool_placement(id, hidden.dock, hidden.index);
            }
        }
    }

    /// Place a newly registered tool as `snapshot` records it: in its stored
    /// dock (see [`restore_stored_tool`](Self::restore_stored_tool)) or
    /// hidden with its stored dock. A tool the record does not name returns
    /// to `dock` at `index` and shows when `visible`. Returns whether the
    /// record named the tool.
    pub fn place_registered_tool(
        &mut self,
        id: ToolFrameId,
        snapshot: Option<&ToolLayoutSnapshot>,
        dock: DockLocation,
        index: usize,
        visible: bool,
    ) -> bool {
        let Some(stable) = self.tool(id).map(|tool| tool.stable_id.clone()) else {
            return false;
        };
        match snapshot.and_then(|snapshot| Some((snapshot, snapshot.placement(&stable)?))) {
            Some((snapshot, StoredPlacement::Docked(_))) => {
                self.restore_stored_tool(id, snapshot);
                true
            }
            Some((_, StoredPlacement::Hidden { dock, index })) => {
                self.remember_tool_placement(id, dock, index);
                true
            }
            None => {
                self.remember_tool_placement(id, dock, index);
                if visible {
                    self.show_tool(id);
                }
                false
            }
        }
    }

    /// Show a newly registered tool in the dock `snapshot` docks it in. When
    /// that dock still holds exactly the record's arrangement of the other
    /// registered tools, the record's tree is installed over it, so tools
    /// attaching one by one rebuild the stored arrangement; otherwise the tool
    /// joins the dock. Returns false when the record does not dock the tool.
    pub fn restore_stored_tool(&mut self, id: ToolFrameId, snapshot: &ToolLayoutSnapshot) -> bool {
        let Some(tool) = self.tool(id) else {
            return false;
        };
        let stable = tool.stable_id.clone();
        let Some(StoredPlacement::Docked(dock)) = snapshot.placement(&stable) else {
            return false;
        };
        let stored = snapshot.dock(dock);
        let registered = self
            .tools
            .iter()
            .map(|tool| tool.stable_id.clone())
            .collect::<HashSet<_>>();
        let others = stored
            .root
            .as_ref()
            .and_then(|root| root.prune(&|tool| tool != stable && registered.contains(tool)));
        if self.stored_dock_tree(dock) == others
            && self.tool_tab(id).is_none()
            && !self.dock_holds_terminals(dock)
        {
            let root = stored
                .root
                .as_ref()
                .and_then(|root| root.prune(&|tool| registered.contains(tool)))
                .map(|root| root.plan(&|tool| self.tool_by_stable_id(tool)));
            self.arrange_dock(dock, root, stored.visible);
        } else {
            self.remember_tool_placement(id, dock, usize::MAX);
            self.show_tool(id);
        }
        true
    }
}

impl ToolFrame {
    /// The dock the tool returns to when shown while hidden.
    pub fn remembered_dock(&self) -> DockLocation {
        self.remembered.dock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explorer(workbench: &mut Workbench) -> ToolFrameId {
        workbench.register_tool(LEFT_TOOL_ID, "Explorer", DockLocation::Left)
    }

    fn tool_ids(workbench: &Workbench, pane: PaneId) -> Vec<TabSurfaceId> {
        workbench
            .pane(pane)
            .unwrap()
            .tabs()
            .iter()
            .map(WorkbenchTab::surface_id)
            .collect()
    }

    #[test]
    fn showing_a_tool_replaces_the_other_tools_of_its_pane_but_not_terminals() {
        let workbench = &mut Workbench::empty();
        workbench.open_document_in_center(DocumentId::new(1));
        let [first, second] = ["knot.first", "knot.second"]
            .map(|name| workbench.register_tool(name, name, DockLocation::Left));
        let (pane, _) = workbench.show_tool(first).unwrap();
        assert!(workbench.toggle_tool(second));
        assert!(!workbench.tool_visible(first));
        assert_eq!(tool_ids(workbench, pane), [TabSurfaceId::Tool(second)]);
        assert_eq!(workbench.focused_pane_id(), Some(pane));

        // The replaced tool returns to the same pane, replacing in turn.
        assert!(workbench.toggle_tool(first));
        assert_eq!(tool_ids(workbench, pane), [TabSurfaceId::Tool(first)]);

        // Tabs grouped on purpose stay: a showing tool only takes focus.
        workbench.show_tool(second);
        assert_eq!(tool_ids(workbench, pane).len(), 2);
        workbench.show_tool_replacing(first);
        assert_eq!(tool_ids(workbench, pane).len(), 2);

        // A terminal in the pane is not displaced.
        let third = workbench.register_tool("knot.third", "third", DockLocation::Left);
        workbench.hide_tool(first);
        workbench
            .open_terminal_tab(pane, TerminalSessionId(1))
            .unwrap();
        workbench.show_tool(first);
        workbench.show_tool_replacing(third);
        assert!(!workbench.tool_visible(first) && !workbench.tool_visible(second));
        assert_eq!(
            tool_ids(workbench, pane),
            [
                TabSurfaceId::Terminal(TerminalSessionId(1)),
                TabSurfaceId::Tool(third)
            ]
        );
    }

    #[test]
    fn a_tool_shows_beside_the_docks_last_pane_in_a_pane_of_its_own() {
        let workbench = &mut Workbench::empty();
        workbench.open_document_in_center(DocumentId::new(1));
        let [first, second, bottom] = [
            ("knot.first", DockLocation::Left),
            ("knot.second", DockLocation::Left),
            ("knot.bottom", DockLocation::Bottom),
        ]
        .map(|(name, dock)| workbench.register_tool(name, name, dock));

        // With nothing in the dock it shows as usual.
        let (pane, _) = workbench.show_tool_beside(first).unwrap();
        let (beside, _) = workbench.show_tool_beside(second).unwrap();
        assert_ne!(pane, beside);
        assert_eq!(workbench.dock_location(beside), Some(DockLocation::Left));
        assert_eq!(workbench.focused_pane_id(), Some(beside));
        assert!(workbench.tool_visible(first) && workbench.tool_visible(second));
        let Some(WorkbenchLayout::Split { direction, .. }) =
            workbench.area_layout(Area::Dock(DockLocation::Left))
        else {
            panic!("the left dock splits");
        };
        assert_eq!(*direction, SplitDirection::Vertical);

        // A hidden dock shows with the tool alone.
        let (alone, _) = workbench.show_tool_beside(bottom).unwrap();
        assert_eq!(workbench.dock_location(alone), Some(DockLocation::Bottom));
    }

    #[test]
    fn tools_keep_identity_and_return_to_their_pane_and_position() {
        let workbench = &mut Workbench::empty();
        let document = DocumentId::new(1);
        let (center, _) = workbench.open_document_in_center(document);
        let id = explorer(workbench);
        let presentation = workbench.tool(id).unwrap().presentation;
        let (dock, _) = workbench.show_tool(id).unwrap();
        assert_eq!(workbench.dock_location(dock), Some(DockLocation::Left));
        assert_eq!(workbench.center(), Some(&WorkbenchLayout::Pane(center)));
        workbench.set_dock_size(DockLocation::Left, 315.);
        assert!(workbench.hide_tool(id));
        assert!(workbench.pane(dock).is_none());
        assert_eq!(workbench.focused_pane_id(), Some(center));
        let (reopened, _) = workbench.show_tool(id).unwrap();
        assert_eq!(workbench.dock_location(reopened), Some(DockLocation::Left));
        assert_eq!(workbench.dock_size(DockLocation::Left), 315.);
        assert_eq!(workbench.tool(id).unwrap().presentation, presentation);

        // A hidden tool returns to the pane and position it left.
        let [first, second] = ["knot.first", "knot.second"]
            .map(|stable| workbench.register_tool(stable, stable, DockLocation::Left));
        workbench.show_tool(first);
        workbench.show_tool(second);
        let source = workbench
            .capture_tab_move(reopened, workbench.tool_tab(second).unwrap().1)
            .unwrap();
        let moved = workbench
            .relocate_tab(
                source,
                TabMoveDestination::At {
                    anchor: source,
                    placement: TabMovePlacement::Split(SplitEdge::Bottom),
                },
            )
            .unwrap();
        let lower = moved.destination.pane_id;
        assert_eq!(workbench.dock_location(lower), Some(DockLocation::Left));
        assert!(workbench.hide_tool(first));
        assert_eq!(
            tool_ids(workbench, reopened),
            [TabSurfaceId::Tool(id)],
            "the hidden tool left its pane"
        );
        assert_eq!(workbench.show_tool(first).unwrap().0, reopened);
        assert_eq!(
            tool_ids(workbench, reopened),
            [TabSurfaceId::Tool(id), TabSurfaceId::Tool(first)]
        );
        // Placing moves a tool to the end of another dock.
        assert!(workbench.place_tool(first, DockLocation::Bottom));
        let bottom = workbench.tool_tab(first).unwrap().0;
        assert_eq!(workbench.dock_location(bottom), Some(DockLocation::Bottom));
        assert_eq!(workbench.view_count(document), 1);
    }

    #[test]
    fn tools_and_documents_stay_in_their_areas() {
        let workbench = &mut Workbench::empty();
        let document = DocumentId::new(2);
        let id = explorer(workbench);
        let (dock, tool_tab) = workbench.show_tool(id).unwrap();
        assert_eq!(workbench.center(), None);
        assert_eq!(workbench.open_tab_for_document(dock, document), None);
        // A document opened from a dock goes to the center.
        let (pane, _, _) = workbench.open_preview_for_document(dock, document).unwrap();
        assert_eq!(workbench.pane_area(pane), Some(Area::Center));
        let doc_tab = workbench.pane(pane).unwrap().active_tab_id();
        let tool = workbench.capture_tab_move(dock, tool_tab).unwrap();
        let doc = workbench.capture_tab_move(pane, doc_tab).unwrap();
        for (source, anchor) in [(tool, doc), (doc, tool)] {
            assert!(
                workbench
                    .relocate_tab(
                        source,
                        TabMoveDestination::At {
                            anchor,
                            placement: TabMovePlacement::End,
                        },
                    )
                    .is_none()
            );
        }
        assert!(
            workbench
                .relocate_tab(
                    doc,
                    TabMoveDestination::Area(Area::Dock(DockLocation::Right))
                )
                .is_none()
        );
        // Tools never move to another window.
        let mut destination = Workbench::empty();
        assert!(
            workbench
                .transfer_tab_to(&mut destination, tool, TabMoveDestination::Empty)
                .is_none()
        );
        assert!(destination.panes().is_empty());
        assert!(workbench.contains_tab_move(tool));
    }

    #[test]
    fn hidden_docks_keep_their_arrangement_and_move_focus_to_the_center() {
        let workbench = &mut Workbench::empty();
        let (center, _) = workbench.open_document_in_center(DocumentId::new(4));
        let ids = ["knot.first", "knot.second", "knot.third"]
            .map(|stable| workbench.register_tool(stable, stable, DockLocation::Right));
        for id in ids {
            workbench.show_tool(id);
        }
        let (dock, _) = workbench.show_tool(ids[1]).unwrap();
        workbench.toggle_dock(DockLocation::Right);
        assert!(!workbench.area_shown(Area::Dock(DockLocation::Right)));
        assert_eq!(workbench.focused_pane_id(), Some(center));
        assert!(workbench.tool_tab(ids[0]).is_some());
        assert!(!workbench.tool_visible(ids[0]));
        workbench.toggle_dock(DockLocation::Right);
        assert_eq!(workbench.focused_pane_id(), Some(dock));
        assert_eq!(tool_ids(workbench, dock), ids.map(TabSurfaceId::Tool));
        assert_eq!(
            workbench.pane(dock).unwrap().active_tab().surface_id(),
            TabSurfaceId::Tool(ids[1])
        );
        // Showing a tool of a hidden dock shows the dock.
        workbench.set_dock_visible(DockLocation::Right, false);
        workbench.show_tool(ids[2]);
        assert!(workbench.tool_visible(ids[0]));
        assert_eq!(workbench.focused_pane_id(), Some(dock));

        // An empty dock shows the hidden tools that return to it.
        for id in ids.into_iter().rev() {
            workbench.hide_tool(id);
        }
        assert_eq!(workbench.area_layout(Area::Dock(DockLocation::Right)), None);
        workbench.toggle_dock(DockLocation::Right);
        let dock = workbench.tool_tab(ids[0]).unwrap().0;
        assert_eq!(tool_ids(workbench, dock), ids.map(TabSurfaceId::Tool));

        // A shown dock without focus takes focus before it hides.
        workbench.focus_pane(center);
        workbench.toggle_dock(DockLocation::Right);
        assert_eq!(workbench.focused_pane_id(), Some(dock));
        assert!(workbench.area_shown(Area::Dock(DockLocation::Right)));
        workbench.toggle_dock(DockLocation::Right);
        assert_eq!(workbench.focused_pane_id(), Some(center));

        // With no center, hiding the only dock leaves nothing focused.
        workbench.toggle_dock(DockLocation::Right);
        workbench.remove_document_tabs();
        workbench.toggle_dock(DockLocation::Right);
        assert_eq!(workbench.focused_pane_id(), None);
        workbench.toggle_dock(DockLocation::Right);
        assert_eq!(workbench.focused_pane_id(), Some(dock));
    }

    #[test]
    fn toggling_a_tool_shows_focuses_then_hides_it_and_returns_to_the_center() {
        let workbench = &mut Workbench::empty();
        let (center, _) = workbench.open_document_in_center(DocumentId::new(6));
        let [first, second] = ["knot.first", "knot.second"]
            .map(|stable| workbench.register_tool(stable, stable, DockLocation::Left));
        workbench.show_tool(first);
        workbench.show_tool(second);
        let (dock, _) = workbench.tool_tab(first).unwrap();
        workbench.focus_pane(center);

        // Visible but unfocused, behind another tab: it takes focus.
        assert!(workbench.toggle_tool(first));
        assert_eq!(workbench.focused_pane_id(), Some(dock));
        assert_eq!(
            workbench.pane(dock).unwrap().active_tab().surface_id(),
            TabSurfaceId::Tool(first)
        );
        // Focused: it hides, and focus returns to the center although its
        // pane survives with the other tool.
        assert!(!workbench.toggle_tool(first));
        assert!(workbench.tool_tab(first).is_none());
        assert_eq!(workbench.focused_pane_id(), Some(center));
        // Hidden: it returns to its pane and position, focused, replacing
        // the tool there.
        assert!(workbench.toggle_tool(first));
        assert_eq!(tool_ids(workbench, dock), [TabSurfaceId::Tool(first)]);
        assert_eq!(workbench.focused_pane_id(), Some(dock));
        // A tool of a hidden dock shows the dock.
        workbench.toggle_dock(DockLocation::Left);
        assert!(!workbench.tool_visible(first));
        assert!(workbench.toggle_tool(second));
        assert!(workbench.tool_visible(second) && !workbench.tool_visible(first));
        assert_eq!(workbench.focused_pane_id(), Some(dock));

        // With an empty center, hiding a focused tool leaves nothing focused.
        workbench.remove_document_tabs();
        workbench.toggle_tool(second);
        assert_eq!(workbench.focused_pane_id(), None);
    }

    #[test]
    fn dock_sizes_clamp_and_survive_hide_reopen() {
        let workbench = &mut Workbench::empty();
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
        workbench.place_tool(id, DockLocation::Bottom);
        assert_eq!(workbench.dock_size(DockLocation::Bottom), 133.);
        assert!(workbench.hide_tool(id));
        workbench.set_dock_size(DockLocation::Bottom, 200.);
        let (pane, _) = workbench.show_tool(id).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Bottom));
        assert_eq!(workbench.dock_size(DockLocation::Bottom), 200.);
    }

    #[test]
    fn tool_snapshots_record_dock_trees_and_hidden_docks_only() {
        let workbench = &mut Workbench::empty();
        let (center, _) = workbench.open_document_in_center(DocumentId::new(5));
        workbench.open_terminal_tab(center, TerminalSessionId(7));
        let id = explorer(workbench);
        let other = workbench.register_tool("knot.other", "Other", DockLocation::Left);
        let hidden = workbench.register_tool("knot.hidden", "Hidden", DockLocation::Bottom);
        workbench.show_tool(id).unwrap();
        workbench.show_tool(other).unwrap();
        let (dock, tab) = workbench.tool_tab(other).unwrap();
        let source = workbench.capture_tab_move(dock, tab).unwrap();
        workbench.relocate_tab(
            source,
            TabMoveDestination::At {
                anchor: source,
                placement: TabMovePlacement::Split(SplitEdge::Bottom),
            },
        );
        workbench.set_dock_size(DockLocation::Left, 311.);
        workbench.set_dock_visible(DockLocation::Left, false);
        let snapshot = workbench.tool_layout_snapshot();
        let pane = |tool: &str| StoredNode::Pane {
            selected: 0,
            tools: vec![tool.to_owned()],
        };
        assert_eq!(
            snapshot.docks[0],
            StoredDock {
                size: 311.,
                visible: false,
                root: Some(StoredNode::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                    first: Box::new(pane(LEFT_TOOL_ID)),
                    second: Box::new(pane("knot.other")),
                }),
            }
        );
        assert_eq!(snapshot.docks[2].root, None);
        assert_eq!(
            snapshot.hidden,
            [HiddenTool {
                id: "knot.hidden".into(),
                dock: DockLocation::Bottom,
                index: usize::MAX,
            }]
        );
        assert_eq!(
            snapshot.placement("knot.other"),
            Some(StoredPlacement::Docked(DockLocation::Left))
        );
        assert_eq!(snapshot.placement("gone"), None);
        let payload = serde_json::to_string(&snapshot).unwrap();
        for forbidden in ["document", "presentation"] {
            assert!(
                !payload.contains(forbidden),
                "tool record leaks {forbidden}"
            );
        }
        assert_eq!(payload.matches("terminal").count(), 1, "only the home");
        let decoded: ToolLayoutSnapshot = serde_json::from_str(&payload).unwrap();
        assert_eq!(decoded, snapshot);
        assert!(workbench.tool(hidden).is_some());
    }

    #[test]
    fn tools_attaching_one_by_one_rebuild_the_stored_dock() {
        let mut source = Workbench::empty();
        let ids = ["knot.first", "knot.second", "knot.third"]
            .map(|stable| source.register_tool(stable, stable, DockLocation::Bottom));
        for id in ids {
            source.show_tool(id);
        }
        let (dock, tab) = source.tool_tab(ids[2]).unwrap();
        let moved = source.capture_tab_move(dock, tab).unwrap();
        source.relocate_tab(
            moved,
            TabMoveDestination::At {
                anchor: moved,
                placement: TabMovePlacement::Split(SplitEdge::Right),
            },
        );
        let (dock, tab) = source.tool_tab(ids[0]).unwrap();
        source.activate_tab(dock, tab);
        let snapshot = source.tool_layout_snapshot();

        let workbench = &mut Workbench::empty();
        for stable in ["knot.third", "knot.second", "knot.first"] {
            let id = workbench.register_tool(stable, stable, DockLocation::Left);
            assert!(workbench.restore_stored_tool(id, &snapshot));
        }
        assert_eq!(
            workbench.tool_layout_snapshot().docks,
            snapshot.docks,
            "attaching in any order rebuilds the stored tree and selection"
        );

        // A dock the user changed is joined instead of rebuilt.
        let workbench = &mut Workbench::empty();
        let first = workbench.register_tool("knot.first", "First", DockLocation::Left);
        workbench.restore_stored_tool(first, &snapshot);
        workbench.place_tool(first, DockLocation::Right);
        let second = workbench.register_tool("knot.second", "Second", DockLocation::Left);
        assert!(workbench.restore_stored_tool(second, &snapshot));
        let bottom = workbench.tool_tab(second).unwrap().0;
        assert_eq!(workbench.dock_location(bottom), Some(DockLocation::Bottom));
        assert_eq!(tool_ids(workbench, bottom), [TabSurfaceId::Tool(second)]);
        let unknown = workbench.register_tool("knot.unknown", "Unknown", DockLocation::Left);
        assert!(!workbench.restore_stored_tool(unknown, &snapshot));
    }
}
