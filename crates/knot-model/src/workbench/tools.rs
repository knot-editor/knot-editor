//! Tool windows, the slots they sit in, and the tool-only layout record fresh
//! windows inherit. A tool window keeps its tabs, size, and slot while it is
//! hidden.

use serde::{Deserialize, Serialize};

use super::*;

pub const TOOL_LAYOUT_VERSION: u32 = 4;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ToolFrameId(u64);

impl ToolFrameId {
    pub fn value(self) -> u64 {
        self.0
    }
}

/// A window side that holds tool windows.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DockLocation {
    Left,
    Right,
    Bottom,
}

impl DockLocation {
    pub const ALL: [Self; 3] = [Self::Left, Self::Right, Self::Bottom];

    /// Smallest usable side extent in logical pixels. Dividers floor to this
    /// minimum so a dragged tool window stays visible without squeezing the
    /// center shut; the window edge is the effective maximum (`area_rects`
    /// clamps painted sizes to the available bounds).
    pub const MIN_SIZE: f32 = 80.;

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

    /// The split direction along which the side's size runs.
    pub fn axis(self) -> SplitDirection {
        match self {
            Self::Left | Self::Right => SplitDirection::Horizontal,
            Self::Bottom => SplitDirection::Vertical,
        }
    }

    /// The side's first slot (top, or left for the bottom side) or its second.
    pub fn slot(self, first: bool) -> Slot {
        match (self, first) {
            (Self::Left, true) => Slot::LeftTop,
            (Self::Left, false) => Slot::LeftBottom,
            (Self::Right, true) => Slot::RightTop,
            (Self::Right, false) => Slot::RightBottom,
            (Self::Bottom, true) => Slot::BottomLeft,
            (Self::Bottom, false) => Slot::BottomRight,
        }
    }
}

/// One of the six places a tool window docks. A slot shows at most one tool
/// window.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Slot {
    LeftTop,
    LeftBottom,
    RightTop,
    RightBottom,
    BottomLeft,
    BottomRight,
}

impl Slot {
    /// Every slot, in layout order.
    pub const ALL: [Self; 6] = [
        Self::LeftTop,
        Self::LeftBottom,
        Self::RightTop,
        Self::RightBottom,
        Self::BottomLeft,
        Self::BottomRight,
    ];

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|slot| *slot == self)
            .expect("every slot is listed")
    }

    pub fn side(self) -> DockLocation {
        match self {
            Self::LeftTop | Self::LeftBottom => DockLocation::Left,
            Self::RightTop | Self::RightBottom => DockLocation::Right,
            Self::BottomLeft | Self::BottomRight => DockLocation::Bottom,
        }
    }

    /// Whether this is its side's first slot: top, or left for the bottom
    /// side.
    pub fn is_first(self) -> bool {
        matches!(self, Self::LeftTop | Self::RightTop | Self::BottomLeft)
    }
}

/// How a tool window holds tabs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolKind {
    /// The single tree or text tab the tool registered with.
    Fixed,
    /// Tabs opened into it, such as terminals.
    Container,
}

pub struct ToolFrame {
    pub id: ToolFrameId,
    pub stable_id: String,
    pub title: String,
    /// The presentation of the tool's own tab, when it is fixed.
    pub presentation: PresentationId,
    pub(super) kind: ToolKind,
    pub(super) slot: Slot,
    pub(super) shown: bool,
    pub(super) sidebar: bool,
    pub(super) size: f32,
    pub(super) share: f32,
    /// Where its stripe button sits among the slot's; registration order
    /// breaks ties.
    pub(super) order: usize,
    /// When the window last showed, 0 for never.
    pub(super) shown_at: u64,
    pub(super) tree: AreaTree,
}

impl ToolFrame {
    pub fn kind(&self) -> ToolKind {
        self.kind
    }

    pub fn slot(&self) -> Slot {
        self.slot
    }

    /// Whether the window is shown in its slot.
    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// Whether the window has a stripe button.
    pub fn on_sidebar(&self) -> bool {
        self.sidebar
    }

    pub fn size(&self) -> f32 {
        self.size
    }

    /// The share of its side the window takes when both slots show.
    pub fn share(&self) -> f32 {
        self.share
    }
}

/// One tool window of a tool-only layout record.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StoredTool {
    pub id: String,
    pub slot: Slot,
    pub shown: bool,
    pub sidebar: bool,
    pub size: f32,
    pub share: f32,
}

/// The tool-only layout record: where each tool window sits, whether it shows
/// and has a stripe button, and its size and share of its side; never
/// documents, terminals, or tab trees.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolLayoutSnapshot {
    pub version: u32,
    pub tools: Vec<StoredTool>,
    /// Whether the user has ever customized the arrangement. Fresh windows
    /// only auto-show stored tools once set, preserving first-run behavior on
    /// fresh installs.
    #[serde(default)]
    pub customized: bool,
}

impl ToolLayoutSnapshot {
    pub fn product_default() -> Self {
        Self {
            version: TOOL_LAYOUT_VERSION,
            tools: Vec::new(),
            customized: false,
        }
    }

    pub fn tool(&self, id: &str) -> Option<&StoredTool> {
        self.tools.iter().find(|tool| tool.id == id)
    }
}

/// What a click on a tool's stripe button did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StripeClick {
    Hidden,
    Shown(PaneId, TabId),
    /// A container tool window without tabs: the caller opens its first tab.
    Empty,
}

impl Workbench {
    /// Define a hidden tool window of `kind` that first shows in `slot`. A
    /// fixed one gets its single tab.
    pub fn register_tool(
        &mut self,
        stable_id: impl Into<String>,
        title: impl Into<String>,
        slot: Slot,
        kind: ToolKind,
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
            kind,
            slot,
            shown: false,
            sidebar: true,
            size: slot.side().default_size(),
            share: 0.5,
            order: usize::MAX,
            shown_at: 0,
            tree: AreaTree::default(),
        });
        if kind == ToolKind::Fixed {
            let pane = self.create_pane(Area::Tool(id));
            let tab = self.new_tab(TabSurfaceId::Tool(id), false);
            let tab_id = tab.id;
            let pane = self.panes.iter_mut().find(|item| item.id == pane).unwrap();
            pane.tabs.push(tab);
            pane.select(tab_id);
        }
        self.debug_assert_invariants();
        id
    }

    pub fn tools(&self) -> &[ToolFrame] {
        &self.tools
    }

    pub fn tool(&self, id: ToolFrameId) -> Option<&ToolFrame> {
        self.tools.iter().find(|tool| tool.id == id)
    }

    /// Set where a tool's stripe button sits among its slot's.
    pub fn set_tool_order(&mut self, id: ToolFrameId, order: usize) {
        if self.tool(id).is_some() {
            self.tool_mut(id).order = order;
        }
    }

    pub(super) fn tool_mut(&mut self, id: ToolFrameId) -> &mut ToolFrame {
        self.tools
            .iter_mut()
            .find(|tool| tool.id == id)
            .expect("registered tool")
    }

    pub fn tool_by_stable_id(&self, stable_id: &str) -> Option<ToolFrameId> {
        self.tools
            .iter()
            .find(|tool| tool.stable_id == stable_id)
            .map(|tool| tool.id)
    }

    /// The tool window a pane belongs to, `None` for the center.
    pub fn pane_tool(&self, pane: PaneId) -> Option<ToolFrameId> {
        match self.pane_area(pane)? {
            Area::Tool(id) => Some(id),
            Area::Center => None,
        }
    }

    /// The tab a fixed tool window shows, whether it is shown or hidden.
    pub fn tool_tab(&self, id: ToolFrameId) -> Option<(PaneId, TabId)> {
        self.panes.iter().find_map(|pane| {
            pane.tabs
                .iter()
                .find(|tab| tab.surface_id() == TabSurfaceId::Tool(id))
                .map(|tab| (pane.id, tab.id))
        })
    }

    /// Whether a tool window is shown in its slot.
    pub fn tool_visible(&self, id: ToolFrameId) -> bool {
        self.tool(id).is_some_and(|tool| tool.shown)
    }

    /// Whether a tool window holds no panes.
    pub fn tool_is_empty(&self, id: ToolFrameId) -> bool {
        self.tool(id).is_some_and(|tool| tool.tree.layout.is_none())
    }

    /// The side of the window a pane's tool window docks on.
    pub fn dock_location(&self, pane: PaneId) -> Option<DockLocation> {
        self.tool(self.pane_tool(pane)?)
            .map(|tool| tool.slot.side())
    }

    /// The tool window shown in `slot`.
    pub fn slot_tool(&self, slot: Slot) -> Option<ToolFrameId> {
        self.tools
            .iter()
            .find(|tool| tool.shown && tool.slot == slot)
            .map(|tool| tool.id)
    }

    /// The tool windows shown on a side, first slot first.
    pub fn side_tools(&self, side: DockLocation) -> [Option<ToolFrameId>; 2] {
        [true, false].map(|first| self.slot_tool(side.slot(first)))
    }

    /// A side's extent: the size of the window in its first slot, else of its
    /// second. `None` while the side shows nothing.
    pub fn side_extent(&self, side: DockLocation) -> Option<f32> {
        let [first, second] = self.side_tools(side);
        first
            .or(second)
            .and_then(|id| self.tool(id))
            .map(|tool| tool.size)
    }

    /// Resize a side by setting the size of the window that gives it its
    /// extent.
    pub fn set_side_extent(&mut self, side: DockLocation, size: f32) {
        let [first, second] = self.side_tools(side);
        if let Some(id) = first.or(second) {
            self.set_tool_size(id, size);
        }
    }

    /// The share of its side the first slot's window takes while both slots
    /// show.
    pub fn side_share(&self, side: DockLocation) -> Option<f32> {
        let [first, second] = self.side_tools(side);
        second?;
        first.and_then(|id| self.tool(id)).map(|tool| tool.share)
    }

    /// Set the first slot's share of a side that shows both slots.
    pub fn set_side_share(&mut self, side: DockLocation, share: f32) {
        let [first, second] = self.side_tools(side);
        if let (Some(first), Some(second)) = (first, second) {
            let share = WorkbenchLayout::clamp_ratio(share);
            self.tool_mut(first).share = share;
            self.tool_mut(second).share = 1. - share;
        }
    }

    pub fn tool_size(&self, id: ToolFrameId) -> Option<f32> {
        self.tool(id).map(|tool| tool.size)
    }

    pub fn set_tool_size(&mut self, id: ToolFrameId, size: f32) {
        if self.tool(id).is_some() {
            self.tool_mut(id).size = size.max(DockLocation::MIN_SIZE);
        }
    }

    /// Show a window in its slot without moving focus, hiding the one there.
    /// A window shown this way regains its stripe button.
    pub(super) fn reveal_tool(&mut self, id: ToolFrameId) {
        let Some(tool) = self.tool(id) else {
            return;
        };
        if tool.shown {
            return;
        }
        let slot = tool.slot;
        for other in &mut self.tools {
            if other.slot == slot {
                other.shown = false;
            }
        }
        self.reveal_clock += 1;
        let clock = self.reveal_clock;
        let tool = self.tool_mut(id);
        tool.shown = true;
        tool.sidebar = true;
        tool.shown_at = clock;
    }

    /// Remove a tool window and its panes. A container's terminals move to
    /// the center. Remembered layout records live outside the workbench and
    /// are intentionally left untouched.
    pub fn unregister_tool(&mut self, id: ToolFrameId) {
        self.hide_tool(id);
        self.evict_tool_terminals(id);
        let area = Area::Tool(id);
        self.panes.retain(|pane| pane.area != area);
        self.tools.retain(|tool| tool.id != id);
        self.settle_focus(None);
        self.debug_assert_invariants();
    }

    /// Hide a tool window, which keeps its tabs. Focus moves to the center.
    pub fn hide_tool(&mut self, id: ToolFrameId) -> bool {
        if !self.tool_visible(id) {
            return false;
        }
        self.tool_mut(id).shown = false;
        self.settle_focus(None);
        self.debug_assert_invariants();
        true
    }

    /// Show and focus a tool window in its slot, hiding the one there, which
    /// keeps its tabs. A container without tabs cannot show: the caller opens
    /// its first tab, which shows it.
    pub fn show_tool(&mut self, id: ToolFrameId) -> Option<(PaneId, TabId)> {
        let tool = self.tool(id)?;
        let pane = tool
            .tree
            .focused
            .filter(|pane| self.pane_area(*pane) == Some(Area::Tool(id)))
            .or_else(|| tool.tree.layout.as_ref().map(WorkbenchLayout::first_pane))?;
        self.set_focus(pane);
        self.debug_assert_invariants();
        Some((pane, self.pane(pane)?.active_tab))
    }

    /// A click on a tool's stripe button: a shown window hides, a hidden one
    /// shows in its slot and takes focus.
    pub fn stripe_click(&mut self, id: ToolFrameId) -> StripeClick {
        if self.tool_visible(id) {
            self.hide_tool(id);
            return StripeClick::Hidden;
        }
        match self.show_tool(id) {
            Some((pane, tab)) => StripeClick::Shown(pane, tab),
            None => StripeClick::Empty,
        }
    }

    /// Toggle a tool window: a hidden or unfocused one shows and takes focus,
    /// and a focused one hides, returning focus to the center. Returns
    /// whether the window is shown afterwards; a container without tabs
    /// stays hidden for its caller to fill.
    pub fn toggle_tool(&mut self, id: ToolFrameId) -> bool {
        let focused = self
            .focused_pane_id()
            .is_some_and(|pane| self.pane_tool(pane) == Some(id));
        if focused {
            self.hide_tool(id);
            self.focus_center();
            false
        } else {
            self.show_tool(id).is_some()
        }
    }

    /// Toggle a side: while focus is in one of its windows they all hide;
    /// while the side shows windows, one takes focus; and while it shows
    /// none, each slot shows the window it showed last, else its first on the
    /// sidebar. Returns whether the side shows afterwards.
    pub fn toggle_side(&mut self, side: DockLocation) -> bool {
        let shown = self
            .side_tools(side)
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let focused = self
            .focused_pane_id()
            .and_then(|pane| self.pane_tool(pane))
            .is_some_and(|tool| shown.contains(&tool));
        if focused {
            for id in shown {
                self.hide_tool(id);
            }
            self.focus_center();
            return false;
        }
        if let Some(id) = shown.first() {
            return self.show_tool(*id).is_some();
        }
        let mut revealed = Vec::new();
        for first in [true, false] {
            let slot = side.slot(first);
            let candidate = self
                .tools
                .iter()
                .filter(|tool| tool.slot == slot && tool.sidebar && tool.tree.layout.is_some())
                .max_by_key(|tool| (tool.shown_at, std::cmp::Reverse(tool.order)))
                .map(|tool| tool.id);
            if let Some(id) = candidate {
                self.reveal_tool(id);
                revealed.push(id);
            }
        }
        if let Some(id) = revealed.first() {
            self.show_tool(*id);
        }
        !revealed.is_empty()
    }

    /// Move a tool window to `slot`, replacing the window shown there when
    /// this one shows. Its size, share, and tabs go with it.
    pub fn place_tool(&mut self, id: ToolFrameId, slot: Slot) -> bool {
        let Some(tool) = self.tool(id) else {
            return false;
        };
        let shown = tool.shown;
        let tool = self.tool_mut(id);
        tool.shown = false;
        tool.slot = slot;
        if shown {
            self.reveal_tool(id);
        }
        self.settle_focus(None);
        self.debug_assert_invariants();
        true
    }

    /// Return a tool window to the defaults it registered with: its slot,
    /// button position, size, and share, and shown when `visible` (a
    /// container with tabs keeps showing).
    pub fn reset_tool(&mut self, id: ToolFrameId, slot: Slot, order: usize, visible: bool) {
        let Some(tool) = self.tool(id) else {
            return;
        };
        let keep = tool.shown && tool.kind == ToolKind::Container;
        let tool = self.tool_mut(id);
        tool.shown = false;
        tool.slot = slot;
        tool.order = order;
        tool.sidebar = true;
        tool.size = slot.side().default_size();
        tool.share = 0.5;
        if (visible || keep) && !self.tool_is_empty(id) {
            self.reveal_tool(id);
        }
        self.settle_focus(None);
        self.debug_assert_invariants();
    }

    /// Put a tool window's button back on the stripes without showing it.
    pub fn add_to_sidebar(&mut self, id: ToolFrameId) {
        if self.tool(id).is_some() {
            self.tool_mut(id).sidebar = true;
        }
    }

    /// Hide a tool window and take its button off the stripes.
    pub fn remove_from_sidebar(&mut self, id: ToolFrameId) -> bool {
        if self.tool(id).is_none() {
            return false;
        }
        self.hide_tool(id);
        self.tool_mut(id).sidebar = false;
        true
    }

    /// Move every terminal tab of a tool window to the center.
    fn evict_tool_terminals(&mut self, id: ToolFrameId) {
        let area = Area::Tool(id);
        let terminals = self
            .panes
            .iter()
            .filter(|pane| pane.area == area)
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

    /// Capture the tool-only layout record: each tool window's slot, state,
    /// and sizes. Documents, terminals, and live handles never enter it.
    pub fn tool_layout_snapshot(&self) -> ToolLayoutSnapshot {
        ToolLayoutSnapshot {
            version: TOOL_LAYOUT_VERSION,
            tools: self
                .tools
                .iter()
                .map(|tool| StoredTool {
                    id: tool.stable_id.clone(),
                    slot: tool.slot,
                    shown: tool.shown,
                    sidebar: tool.sidebar,
                    size: tool.size,
                    share: tool.share,
                })
                .collect(),
            customized: false,
        }
    }

    /// Set a window's slot, button, and sizes from its record, hidden.
    fn apply_stored(&mut self, id: ToolFrameId, stored: &StoredTool) {
        let tool = self.tool_mut(id);
        tool.shown = false;
        tool.slot = stored.slot;
        tool.sidebar = stored.sidebar;
        tool.size = stored.size.max(DockLocation::MIN_SIZE);
        tool.share = WorkbenchLayout::clamp_ratio(stored.share);
    }

    /// Show a window its record shows, when it has tabs to show.
    fn reveal_stored(&mut self, id: ToolFrameId, stored: &StoredTool) {
        if stored.shown && !self.tool_is_empty(id) {
            self.reveal_tool(id);
        }
    }

    /// Install a whole tool-only record over the registered tools. Tools the
    /// record does not name keep their place.
    pub fn restore_tool_layout(&mut self, snapshot: &ToolLayoutSnapshot) {
        let named = self
            .tools
            .iter()
            .filter_map(|tool| Some((tool.id, snapshot.tool(&tool.stable_id)?.clone())))
            .collect::<Vec<_>>();
        for (id, stored) in &named {
            self.apply_stored(*id, stored);
        }
        for (id, stored) in &named {
            self.reveal_stored(*id, stored);
        }
        self.settle_focus(None);
        self.debug_assert_invariants();
    }

    /// Place a newly registered tool as `snapshot` records it. A tool the
    /// record does not name stays where it registered, and shows when
    /// `visible` and its slot is free. Returns whether the record named the
    /// tool.
    pub fn place_registered_tool(
        &mut self,
        id: ToolFrameId,
        snapshot: Option<&ToolLayoutSnapshot>,
        visible: bool,
    ) -> bool {
        let Some(tool) = self.tool(id) else {
            return false;
        };
        let stored = snapshot.and_then(|snapshot| snapshot.tool(&tool.stable_id).cloned());
        match stored {
            Some(stored) => {
                self.apply_stored(id, &stored);
                self.reveal_stored(id, &stored);
                true
            }
            None => {
                let slot = tool.slot;
                if visible && self.slot_tool(slot).is_none() && !self.tool_is_empty(id) {
                    self.reveal_tool(id);
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(workbench: &mut Workbench, name: &str, slot: Slot) -> ToolFrameId {
        workbench.register_tool(name, name, slot, ToolKind::Fixed)
    }

    #[test]
    fn a_slot_shows_one_window_and_the_replaced_one_keeps_its_tabs() {
        let workbench = &mut Workbench::empty();
        let (center, _) = workbench.open_document_in_center(DocumentId::new(1));
        let explorer = fixed(workbench, "knot.explorer", Slot::LeftTop);
        let projects = fixed(workbench, "knot.projects", Slot::LeftTop);
        let terminal = workbench.register_tool(
            "knot.terminal",
            "Terminal",
            Slot::LeftTop,
            ToolKind::Container,
        );

        // Registered windows start hidden with their tab ready.
        assert!(!workbench.tool_visible(explorer));
        assert!(workbench.tool_tab(explorer).is_some());
        assert_eq!(workbench.focused_pane_id(), Some(center));

        let (pane, _) = workbench.show_tool(explorer).unwrap();
        assert_eq!(workbench.slot_tool(Slot::LeftTop), Some(explorer));
        assert_eq!(workbench.focused_pane_id(), Some(pane));
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Left));

        // Showing another replaces it; the replaced window keeps its tab.
        workbench.show_tool(projects).unwrap();
        assert!(workbench.tool_visible(projects) && !workbench.tool_visible(explorer));
        assert_eq!(workbench.tool_tab(explorer).unwrap().0, pane);
        workbench.show_tool(explorer).unwrap();
        assert!(workbench.tool_visible(explorer) && !workbench.tool_visible(projects));

        // A container with tabs replaces and is replaced in turn, keeping
        // its terminals.
        let (term_pane, _) = workbench
            .open_terminal_in_tool(terminal, TerminalSessionId(1))
            .unwrap();
        assert_eq!(workbench.slot_tool(Slot::LeftTop), Some(terminal));
        assert!(!workbench.tool_visible(explorer));
        workbench.show_tool(explorer).unwrap();
        assert!(workbench.pane(term_pane).is_some());
        assert!(!workbench.tool_is_empty(terminal));
        assert_eq!(
            workbench.focused_pane_id(),
            workbench.tool_tab(explorer).map(|t| t.0)
        );
    }

    #[test]
    fn stripe_clicks_show_hide_and_focus_the_center() {
        let workbench = &mut Workbench::empty();
        let (center, _) = workbench.open_document_in_center(DocumentId::new(1));
        let a = fixed(workbench, "knot.a", Slot::RightTop);
        let b = fixed(workbench, "knot.b", Slot::RightTop);
        let StripeClick::Shown(pane, _) = workbench.stripe_click(a) else {
            panic!("a hidden window shows");
        };
        assert_eq!(workbench.focused_pane_id(), Some(pane));
        // A click on a visible window hides it, focused or not.
        workbench.focus_pane(center);
        assert_eq!(workbench.stripe_click(a), StripeClick::Hidden);
        assert!(!workbench.tool_visible(a));
        assert_eq!(workbench.focused_pane_id(), Some(center));
        // The toggle pattern: show and focus, focus, then hide.
        assert!(workbench.toggle_tool(b));
        workbench.focus_pane(center);
        assert!(workbench.toggle_tool(b));
        assert_eq!(
            workbench.focused_pane_id(),
            workbench.tool_tab(b).map(|t| t.0)
        );
        assert!(!workbench.toggle_tool(b));
        assert_eq!(workbench.focused_pane_id(), Some(center));
        // With no center, hiding leaves nothing focused.
        workbench.show_tool(b);
        workbench.remove_document_tabs();
        workbench.hide_tool(b);
        assert_eq!(workbench.focused_pane_id(), None);
    }

    #[test]
    fn moving_to_a_slot_replaces_there_and_keeps_size_and_state() {
        let workbench = &mut Workbench::empty();
        workbench.open_document_in_center(DocumentId::new(1));
        let a = fixed(workbench, "knot.a", Slot::LeftTop);
        let b = fixed(workbench, "knot.b", Slot::BottomRight);
        workbench.show_tool(a);
        workbench.show_tool(b);
        workbench.set_tool_size(a, 333.);
        assert!(workbench.place_tool(a, Slot::BottomRight));
        assert_eq!(workbench.slot_tool(Slot::BottomRight), Some(a));
        assert!(!workbench.tool_visible(b) && workbench.slot_tool(Slot::LeftTop).is_none());
        assert_eq!(workbench.tool_size(a), Some(333.));
        assert_eq!(
            workbench.dock_location(workbench.tool_tab(a).unwrap().0),
            Some(DockLocation::Bottom)
        );
        // A hidden window moves without showing.
        workbench.hide_tool(a);
        workbench.place_tool(a, Slot::RightBottom);
        assert!(!workbench.tool_visible(a));
        assert_eq!(workbench.tool(a).unwrap().slot(), Slot::RightBottom);
        assert!(!workbench.tool_visible(b));
    }

    #[test]
    fn sizes_floor_and_each_window_remembers_its_own() {
        let workbench = &mut Workbench::empty();
        let a = fixed(workbench, "knot.a", Slot::LeftTop);
        let b = fixed(workbench, "knot.b", Slot::LeftTop);
        workbench.set_tool_size(a, 1.);
        assert_eq!(workbench.tool_size(a), Some(DockLocation::MIN_SIZE));
        workbench.set_tool_size(a, 600.);
        workbench.set_tool_size(b, 150.);
        workbench.show_tool(a);
        assert_eq!(workbench.side_extent(DockLocation::Left), Some(600.));
        workbench.show_tool(b);
        assert_eq!(workbench.side_extent(DockLocation::Left), Some(150.));
        workbench.set_side_extent(DockLocation::Left, 200.);
        assert_eq!(
            (workbench.tool_size(a), workbench.tool_size(b)),
            (Some(600.), Some(200.))
        );
        assert_eq!(workbench.side_extent(DockLocation::Right), None);
    }

    #[test]
    fn the_layout_record_restores_and_waits_for_late_tools() {
        let mut source = Workbench::empty();
        source.open_document_in_center(DocumentId::new(1));
        let a = fixed(&mut source, "knot.a", Slot::LeftTop);
        let b = fixed(&mut source, "knot.b", Slot::LeftTop);
        let c = fixed(&mut source, "knot.c", Slot::BottomLeft);
        source.show_tool(a);
        source.show_tool(b);
        source.place_tool(c, Slot::RightBottom);
        source.show_tool(c);
        source.set_tool_size(b, 280.);
        source.remove_from_sidebar(a);
        let snapshot = source.tool_layout_snapshot();
        assert_eq!(snapshot.tool("knot.b").unwrap().size, 280.);
        assert!(!snapshot.tool("knot.a").unwrap().sidebar);
        let payload = serde_json::to_string(&snapshot).unwrap();
        for forbidden in ["document", "presentation", "terminal"] {
            assert!(
                !payload.contains(forbidden),
                "tool record leaks {forbidden}"
            );
        }
        let decoded: ToolLayoutSnapshot = serde_json::from_str(&payload).unwrap();
        assert_eq!(decoded, snapshot);

        // Tools registering one by one land as the record says, whatever the
        // order; one the record does not name takes its default.
        let workbench = &mut Workbench::empty();
        for name in ["knot.c", "knot.b", "knot.a"] {
            let id = fixed(workbench, name, Slot::LeftTop);
            assert!(workbench.place_registered_tool(id, Some(&decoded), true));
        }
        assert_eq!(workbench.tool_layout_snapshot().tools.len(), 3);
        for name in ["knot.a", "knot.b", "knot.c"] {
            let id = workbench.tool_by_stable_id(name).unwrap();
            let stored = decoded.tool(name).unwrap();
            let tool = workbench.tool(id).unwrap();
            assert_eq!(
                (tool.slot(), tool.is_shown(), tool.on_sidebar(), tool.size()),
                (stored.slot, stored.shown, stored.sidebar, stored.size),
                "{name}"
            );
        }
        let unknown = fixed(workbench, "knot.unknown", Slot::LeftBottom);
        assert!(!workbench.place_registered_tool(unknown, Some(&decoded), true));
        assert_eq!(workbench.slot_tool(Slot::LeftBottom), Some(unknown));
        // A default does not displace a window that is already showing.
        let late = fixed(workbench, "knot.late", Slot::LeftBottom);
        assert!(!workbench.place_registered_tool(late, Some(&decoded), true));
        assert_eq!(workbench.slot_tool(Slot::LeftBottom), Some(unknown));

        // Restoring a whole record over a live workbench follows it.
        workbench.show_tool(late);
        workbench.restore_tool_layout(&decoded);
        assert_eq!(workbench.slot_tool(Slot::LeftBottom), Some(late));
        assert_eq!(
            workbench.tool_layout_snapshot().tool("knot.b"),
            decoded.tool("knot.b")
        );
    }

    #[test]
    fn unregistering_removes_the_window_and_moves_terminals_to_the_center() {
        let workbench = &mut Workbench::new_for_document(DocumentId::new(1));
        let fixed_tool = fixed(workbench, "knot.a", Slot::LeftTop);
        let terminal =
            workbench.register_tool("knot.t", "T", Slot::BottomLeft, ToolKind::Container);
        workbench
            .open_terminal_in_tool(terminal, TerminalSessionId(1))
            .unwrap();
        workbench.unregister_tool(terminal);
        workbench.unregister_tool(fixed_tool);
        assert!(workbench.tools().is_empty());
        assert!(
            workbench
                .tabs()
                .any(|tab| tab.surface_id() == TabSurfaceId::Terminal(TerminalSessionId(1)))
        );
        assert_eq!(workbench.panes().len(), 1);
    }

    #[test]
    fn documents_and_fixed_tabs_stay_in_their_windows() {
        let workbench = &mut Workbench::empty();
        let (center, _) = workbench.open_document_in_center(DocumentId::new(2));
        let a = fixed(workbench, "knot.a", Slot::LeftTop);
        let b = fixed(workbench, "knot.b", Slot::RightTop);
        let (a_pane, a_tab) = workbench.show_tool(a).unwrap();
        assert_eq!(
            workbench.open_tab_for_document(a_pane, DocumentId::new(2)),
            None
        );
        // A document opened from a tool window goes to the center.
        let (pane, _, _) = workbench
            .open_preview_for_document(a_pane, DocumentId::new(3))
            .unwrap();
        assert_eq!(pane, center);
        let tool = workbench.capture_tab_move(a_pane, a_tab).unwrap();
        for target in [
            TabMoveDestination::Area(Area::Center),
            TabMoveDestination::Area(Area::Tool(b)),
        ] {
            assert!(workbench.relocate_tab(tool, target).is_none());
        }
        let doc_tab = workbench.pane(center).unwrap().active_tab_id();
        let doc = workbench.capture_tab_move(center, doc_tab).unwrap();
        assert!(
            workbench
                .relocate_tab(doc, TabMoveDestination::Area(Area::Tool(a)))
                .is_none()
        );
        let mut destination = Workbench::empty();
        assert!(
            workbench
                .transfer_tab_to(&mut destination, tool, TabMoveDestination::Empty)
                .is_none()
        );
    }
}
