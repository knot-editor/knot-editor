//! Window-local workbench layout.
//!
//! A workbench is a center area of documents surrounded by tool windows.
//! Every tool window sits in one of six slots (two per left, right, and bottom
//! side), and a slot shows at most one of its windows. The center and each tool
//! window own a binary split tree whose leaves are tabbed panes. Tabs carry the
//! identity of what they show (a document, a terminal session, or a tool) and a
//! presentation identity; the application owns the views behind
//! presentations. This module models layout and lifecycle transitions
//! independently of native windows, views, and confirmation dialogs.

use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PaneId(u64);

impl PaneId {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TabId(u64);

impl TabId {
    pub fn value(self) -> u64 {
        self.0
    }
}

/// An application document's identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DocumentId(u64);

impl DocumentId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

/// The identity of what a tab presents. A document tab gets a fresh one when
/// it opens, when its document is replaced, and when it moves to another
/// workbench; a tool tab presents its tool's, which outlives the tab. Captured
/// command targets compare it to tell a replaced presentation from the one
/// they admitted.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PresentationId(u64);

impl PresentationId {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchTabSnapshot {
    pub pane_id: PaneId,
    pub tab_id: TabId,
    pub surface: TabSurfaceId,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalSessionId(pub u64);

/// The identity of a composite view, unique across the application.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompositeViewId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabSurfaceId {
    Document(DocumentId),
    Terminal(TerminalSessionId),
    Composite(CompositeViewId),
    Tool(ToolFrameId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitPlacement {
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkbenchLayout {
    Pane(PaneId),
    Split {
        direction: SplitDirection,
        /// The fraction of the split's extent given to `first`.
        ratio: f32,
        first: Box<Self>,
        second: Box<Self>,
    },
}

impl WorkbenchLayout {
    /// Each side of an ordinary split keeps at least this share.
    pub const MIN_RATIO: f32 = 0.1;

    pub fn clamp_ratio(ratio: f32) -> f32 {
        if ratio.is_finite() {
            ratio.clamp(Self::MIN_RATIO, 1. - Self::MIN_RATIO)
        } else {
            0.5
        }
    }

    pub fn split(direction: SplitDirection, first: Self, second: Self) -> Self {
        Self::Split {
            direction,
            ratio: 0.5,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    fn split_pane(
        &mut self,
        target: PaneId,
        new_pane: PaneId,
        direction: SplitDirection,
        placement: SplitPlacement,
    ) -> bool {
        match self {
            Self::Pane(pane_id) if *pane_id == target => {
                let (first, second) = match placement {
                    SplitPlacement::Before => (new_pane, target),
                    SplitPlacement::After => (target, new_pane),
                };
                *self = Self::split(direction, Self::Pane(first), Self::Pane(second));
                true
            }
            Self::Pane(_) => false,
            Self::Split { first, second, .. } => {
                first.split_pane(target, new_pane, direction, placement)
                    || second.split_pane(target, new_pane, direction, placement)
            }
        }
    }

    fn remove_pane(self, target: PaneId) -> Option<Self> {
        match self {
            Self::Pane(pane_id) => (pane_id != target).then_some(Self::Pane(pane_id)),
            Self::Split {
                direction,
                ratio,
                first,
                second,
            } => match (first.remove_pane(target), second.remove_pane(target)) {
                (Some(first), Some(second)) => Some(Self::Split {
                    direction,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (Some(remaining), None) | (None, Some(remaining)) => Some(remaining),
                (None, None) => None,
            },
        }
    }

    fn first_pane(&self) -> PaneId {
        match self {
            Self::Pane(pane_id) => *pane_id,
            Self::Split { first, .. } => first.first_pane(),
        }
    }

    fn contains_pane(&self, target: PaneId) -> bool {
        match self {
            Self::Pane(pane_id) => *pane_id == target,
            Self::Split { first, second, .. } => {
                first.contains_pane(target) || second.contains_pane(target)
            }
        }
    }

    fn pane_ids(&self, result: &mut Vec<PaneId>) {
        match self {
            Self::Pane(pane_id) => result.push(*pane_id),
            Self::Split { first, second, .. } => {
                first.pane_ids(result);
                second.pane_ids(result);
            }
        }
    }

    /// The pane that identifies a split: the first pane of its second child.
    /// Exactly one split has a given boundary pane, so dividers can address
    /// their split without a separate identity.
    pub fn boundary(&self) -> Option<PaneId> {
        match self {
            Self::Pane(_) => None,
            Self::Split { second, .. } => Some(second.first_pane()),
        }
    }

    fn split_with_boundary_mut(&mut self, boundary: PaneId) -> Option<&mut Self> {
        if self.boundary() == Some(boundary) {
            return Some(self);
        }
        match self {
            Self::Pane(_) => None,
            Self::Split { first, second, .. } => first
                .split_with_boundary_mut(boundary)
                .or_else(|| second.split_with_boundary_mut(boundary)),
        }
    }

    /// The splits enclosing `target`, outermost first, each with whether
    /// `target` lies in its second child.
    fn ancestors(&self, target: PaneId) -> Vec<(&Self, bool)> {
        let mut ancestors = Vec::new();
        let mut node = self;
        while let Self::Split { first, second, .. } = node {
            if first.contains_pane(target) {
                ancestors.push((node, false));
                node = first;
            } else if second.contains_pane(target) {
                ancestors.push((node, true));
                node = second;
            } else {
                break;
            }
        }
        ancestors
    }

    /// How many panes lie side by side along `direction`, counting nested
    /// splits of the same direction.
    fn weight(&self, direction: SplitDirection) -> f32 {
        match self {
            Self::Split {
                direction: nested,
                first,
                second,
                ..
            } if *nested == direction => first.weight(direction) + second.weight(direction),
            _ => 1.,
        }
    }
}

/// Where a pane lives: the center document area or a tool window.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Area {
    Center,
    Tool(ToolFrameId),
}

/// What a tab presents, as far as placement cares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabKind {
    Document,
    Terminal,
    Composite,
    Tool,
}

impl TabSurfaceId {
    pub fn kind(self) -> TabKind {
        match self {
            Self::Document(_) => TabKind::Document,
            Self::Terminal(_) => TabKind::Terminal,
            Self::Composite(_) => TabKind::Composite,
            Self::Tool(_) => TabKind::Tool,
        }
    }
}

pub struct WorkbenchTab {
    id: TabId,
    /// Pinned tabs form a prefix of their pane's tabs.
    pinned: bool,
    /// A preview tab, of a document or a composite view, is replaced by the
    /// pane's next preview.
    preview: bool,
    surface: TabSurfaceId,
    presentation: PresentationId,
}

impl WorkbenchTab {
    pub fn id(&self) -> TabId {
        self.id
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    pub fn is_preview(&self) -> bool {
        self.preview
    }

    pub fn surface_id(&self) -> TabSurfaceId {
        self.surface
    }

    pub fn presentation(&self) -> PresentationId {
        self.presentation
    }

    pub fn document_id(&self) -> Option<DocumentId> {
        match self.surface {
            TabSurfaceId::Document(document_id) => Some(document_id),
            TabSurfaceId::Terminal(_) | TabSurfaceId::Composite(_) | TabSurfaceId::Tool(_) => None,
        }
    }
}

pub struct Pane {
    id: PaneId,
    area: Area,
    tabs: Vec<WorkbenchTab>,
    active_tab: TabId,
    /// Tabs most recently activated first. Tabs never activated, such as
    /// restored ones, follow in tab order.
    recent: Vec<TabId>,
}

impl Pane {
    fn new(id: PaneId, area: Area, tabs: Vec<WorkbenchTab>, active_tab: TabId) -> Self {
        Self {
            id,
            area,
            tabs,
            active_tab,
            recent: vec![active_tab],
        }
    }

    /// Make a tab active and the most recently used.
    fn select(&mut self, tab: TabId) {
        self.active_tab = tab;
        self.recent.retain(|id| *id != tab);
        self.recent.insert(0, tab);
    }

    fn pinned_count(&self) -> usize {
        self.tabs.iter().take_while(|tab| tab.pinned).count()
    }

    /// Clamp an insertion index into the pinned or unpinned section.
    fn section_index(&self, pinned: bool, index: usize) -> usize {
        let pinned_count = self.pinned_count();
        if pinned {
            index.min(pinned_count)
        } else {
            index.clamp(pinned_count, self.tabs.len())
        }
    }

    /// The pane's tabs in recently activated order.
    pub fn recent_tabs(&self) -> Vec<TabId> {
        let mut order = self
            .recent
            .iter()
            .copied()
            .filter(|id| self.tabs.iter().any(|tab| tab.id == *id))
            .collect::<Vec<_>>();
        for tab in &self.tabs {
            if !order.contains(&tab.id) {
                order.push(tab.id);
            }
        }
        order
    }

    pub fn id(&self) -> PaneId {
        self.id
    }

    pub fn area(&self) -> Area {
        self.area
    }

    pub fn tabs(&self) -> &[WorkbenchTab] {
        &self.tabs
    }

    pub fn active_tab_id(&self) -> TabId {
        self.active_tab
    }

    pub fn active_tab(&self) -> &WorkbenchTab {
        self.tabs
            .iter()
            .find(|tab| tab.id == self.active_tab)
            .expect("pane active tab must belong to the pane")
    }
}

/// One area's split tree and the pane it last focused.
#[derive(Default)]
pub(super) struct AreaTree {
    pub(super) layout: Option<WorkbenchLayout>,
    pub(super) focused: Option<PaneId>,
}

pub struct Workbench {
    tools: Vec<ToolFrame>,
    center: AreaTree,
    next_pane_id: u64,
    next_tab_id: u64,
    next_presentation_id: u64,
    next_tool_id: u64,
    /// Counts window reveals, for [`ToolFrame::shown_at`].
    reveal_clock: u64,
    /// The tool windows the last Hide All Tool Windows hid.
    hidden_by_hide_all: Vec<ToolFrameId>,
    panes: Vec<Pane>,
    focused_pane: Option<PaneId>,
    /// The pane that had focus before the focused one, for going back.
    previous_pane: Option<PaneId>,
    /// Terminals that rang the bell while their tab was not shown.
    bell_marks: HashSet<TerminalSessionId>,
    composite_documents: HashMap<CompositeViewId, Vec<DocumentId>>,
}

impl Workbench {
    pub fn empty() -> Self {
        Self {
            tools: Vec::new(),
            center: AreaTree::default(),
            next_pane_id: 1,
            next_tab_id: 1,
            next_presentation_id: 1,
            next_tool_id: 1,
            reveal_clock: 0,
            hidden_by_hide_all: Vec::new(),
            panes: Vec::new(),
            focused_pane: None,
            previous_pane: None,
            bell_marks: HashSet::new(),
            composite_documents: HashMap::new(),
        }
    }

    /// A workbench with one center pane showing `document_id`.
    pub fn new_for_document(document_id: DocumentId) -> Self {
        let mut workbench = Self::empty();
        let pane_id = workbench.create_pane(Area::Center);
        let tab = workbench.new_tab(TabSurfaceId::Document(document_id), false);
        let tab_id = tab.id;
        let pane = workbench.panes.last_mut().unwrap();
        pane.tabs.push(tab);
        pane.select(tab_id);
        workbench.set_focus(pane_id);
        workbench.debug_assert_invariants();
        workbench
    }

    fn tree(&self, area: Area) -> &AreaTree {
        match area {
            Area::Center => &self.center,
            Area::Tool(id) => &self.tool(id).expect("areas name registered tools").tree,
        }
    }

    fn tree_mut(&mut self, area: Area) -> &mut AreaTree {
        match area {
            Area::Center => &mut self.center,
            Area::Tool(id) => {
                &mut self
                    .tools
                    .iter_mut()
                    .find(|tool| tool.id == id)
                    .expect("areas name registered tools")
                    .tree
            }
        }
    }

    /// Whether tabs of `surface` may occupy `area`. Documents live in the
    /// center, a fixed tool's tab only in its own window, and a terminal in the
    /// center or a container tool window. Every placement, relocation, and
    /// restore checks this declaration. A diff or a composite view lives only
    /// in the center.
    pub fn allows(&self, surface: TabSurfaceId, area: Area) -> bool {
        match (surface, area) {
            (TabSurfaceId::Document(_) | TabSurfaceId::Composite(_), area) => area == Area::Center,
            (TabSurfaceId::Terminal(_), Area::Center) => true,
            (TabSurfaceId::Terminal(_), Area::Tool(id)) => self
                .tool(id)
                .is_some_and(|tool| tool.kind() == ToolKind::Container),
            (TabSurfaceId::Tool(tool), area) => area == Area::Tool(tool),
        }
    }

    /// The center's split tree, `None` while the center is empty.
    pub fn center(&self) -> Option<&WorkbenchLayout> {
        self.center.layout.as_ref()
    }

    pub fn area_layout(&self, area: Area) -> Option<&WorkbenchLayout> {
        self.tree(area).layout.as_ref()
    }

    /// Whether an area is painted: the center always, a tool window while it
    /// is shown in its slot.
    pub fn area_shown(&self, area: Area) -> bool {
        match area {
            Area::Center => true,
            Area::Tool(id) => self.tool(id).is_some_and(|tool| tool.shown),
        }
    }

    pub fn pane_area(&self, pane: PaneId) -> Option<Area> {
        self.pane(pane).map(|pane| pane.area)
    }

    fn pane_shown(&self, pane: PaneId) -> bool {
        self.pane_area(pane)
            .is_some_and(|area| self.area_shown(area))
    }

    pub fn panes(&self) -> &[Pane] {
        &self.panes
    }

    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.iter().find(|pane| pane.id == id)
    }

    pub fn tab(&self, pane: PaneId, tab: TabId) -> Option<&WorkbenchTab> {
        self.pane(pane)?.tabs.iter().find(|item| item.id == tab)
    }

    /// Every tab, in pane order and then tab order.
    pub fn tabs(&self) -> impl Iterator<Item = &WorkbenchTab> {
        self.panes.iter().flat_map(|pane| &pane.tabs)
    }

    pub fn focused_pane_id(&self) -> Option<PaneId> {
        self.focused_pane
    }

    pub fn focused_pane(&self) -> Option<&Pane> {
        self.focused_pane.and_then(|id| self.pane(id))
    }

    /// An area's panes in layout order.
    pub fn area_panes(&self, area: Area) -> Vec<PaneId> {
        let mut ids = Vec::new();
        if let Some(layout) = &self.tree(area).layout {
            layout.pane_ids(&mut ids);
        }
        ids
    }

    /// Every area in layout order: the center, then the tool windows by slot
    /// and registration, hidden ones included.
    pub fn areas(&self) -> Vec<Area> {
        let mut tools = self.tools.iter().collect::<Vec<_>>();
        tools.sort_by_key(|tool| tool.slot.index());
        std::iter::once(Area::Center)
            .chain(tools.into_iter().map(|tool| Area::Tool(tool.id)))
            .collect()
    }

    /// Every pane in layout order: the center, then the tool windows, hidden
    /// ones included.
    pub fn pane_order(&self) -> Vec<PaneId> {
        self.areas()
            .into_iter()
            .flat_map(|area| self.area_panes(area))
            .collect()
    }

    /// Capture tab identities in layout pane order and then tab order.
    pub fn tab_snapshots(&self) -> Vec<WorkbenchTabSnapshot> {
        self.pane_order()
            .into_iter()
            .flat_map(|pane_id| {
                self.pane(pane_id)
                    .expect("layout pane must belong to the workbench")
                    .tabs
                    .iter()
                    .map(move |tab| WorkbenchTabSnapshot {
                        pane_id,
                        tab_id: tab.id,
                        surface: tab.surface_id(),
                    })
            })
            .collect()
    }

    /// Focus `pane_id`, remembering the pane it leaves and, for its area,
    /// the pane. Focusing a hidden tool window's pane shows the window in its
    /// slot, replacing the one there.
    fn set_focus(&mut self, pane_id: PaneId) {
        if self.focused_pane != Some(pane_id) {
            self.previous_pane = self.focused_pane;
        }
        self.focused_pane = Some(pane_id);
        let area = self.pane_area(pane_id).expect("focused panes are live");
        self.tree_mut(area).focused = Some(pane_id);
        if let Area::Tool(id) = area {
            self.reveal_tool(id);
            self.reveal_clock += 1;
            let clock = self.reveal_clock;
            self.tool_mut(id).active_at = clock;
        }
        self.prune_bell_marks();
    }

    /// The pane an area would focus: the one it last focused, else its first.
    fn area_focus(&self, area: Area) -> Option<PaneId> {
        let tree = self.tree(area);
        tree.focused
            .filter(|pane| self.pane_area(*pane) == Some(area))
            .or_else(|| tree.layout.as_ref().map(WorkbenchLayout::first_pane))
    }

    /// Keep focus on a painted pane: when the focused pane is gone or
    /// hidden, focus returns to the previously focused pane if it is in
    /// `preferred`'s area, else moves to `preferred`'s pane, then the
    /// center's, else to the empty center. Docks take focus only when asked
    /// to.
    fn settle_focus(&mut self, preferred: Option<Area>) {
        if self.focused_pane.is_some_and(|pane| self.pane_shown(pane)) {
            return;
        }
        let previous = self.previous_pane_id().filter(|pane| {
            preferred.is_some_and(|area| self.pane_area(*pane) == Some(area))
                && self.pane_shown(*pane)
        });
        let candidate = previous.or_else(|| {
            preferred
                .into_iter()
                .chain([Area::Center])
                .filter(|area| self.area_shown(*area))
                .find_map(|area| self.area_focus(area))
        });
        self.focused_pane = candidate;
        if let Some(pane) = candidate {
            let area = self.pane_area(pane).unwrap();
            self.tree_mut(area).focused = Some(pane);
        }
    }

    /// The area commands that walk panes stay in: the focused pane's, or the
    /// center.
    fn focused_area(&self) -> Area {
        self.focused_pane()
            .map(|pane| pane.area)
            .unwrap_or(Area::Center)
    }

    /// The first pane of the focused pane's area, in layout order.
    pub fn first_pane_id(&self) -> Option<PaneId> {
        self.area_panes(self.focused_area()).first().copied()
    }

    /// The last pane of the focused pane's area, in layout order.
    pub fn last_pane_id(&self) -> Option<PaneId> {
        self.area_panes(self.focused_area()).last().copied()
    }

    /// The pane after (or before) the focused one in its area's layout
    /// order, wrapping. Without focus, the walk starts at either end of the
    /// center.
    pub fn cycle_pane_id(&self, forward: bool) -> Option<PaneId> {
        let order = self.area_panes(self.focused_area());
        let count = order.len();
        let Some(index) = order
            .iter()
            .position(|pane| Some(*pane) == self.focused_pane)
        else {
            return if forward { order.first() } else { order.last() }.copied();
        };
        Some(order[(index + if forward { 1 } else { count - 1 }) % count])
    }

    /// The pane that had focus before the focused one, while it is open.
    pub fn previous_pane_id(&self) -> Option<PaneId> {
        self.previous_pane.filter(|pane| self.pane(*pane).is_some())
    }

    /// The center pane documents open in: the focused pane when it is in the
    /// center, else the center's last focused pane.
    pub fn center_pane(&self) -> Option<PaneId> {
        self.focused_pane
            .filter(|pane| self.pane_area(*pane) == Some(Area::Center))
            .or_else(|| self.area_focus(Area::Center))
    }

    /// Focus the center's last focused pane, or the empty center.
    fn focus_center(&mut self) {
        match self.area_focus(Area::Center) {
            Some(pane) => self.set_focus(pane),
            None => {
                if self.focused_pane.is_some() {
                    self.previous_pane = self.focused_pane;
                }
                self.focused_pane = None;
            }
        }
        self.debug_assert_invariants();
    }

    pub fn focus_pane(&mut self, pane_id: PaneId) -> bool {
        if self.pane(pane_id).is_none() {
            return false;
        }
        self.set_focus(pane_id);
        self.debug_assert_invariants();
        true
    }

    /// Make a tab active and its pane focused, recording the activation in
    /// the pane's recently used order.
    pub fn activate_tab(&mut self, pane_id: PaneId, tab_id: TabId) -> bool {
        self.show_tab(pane_id, tab_id, true)
    }

    /// Make a tab active and its pane focused. Without `record`, the pane's
    /// recently used order stays as it was, as a recent-tab walk needs.
    pub fn show_tab(&mut self, pane_id: PaneId, tab_id: TabId, record: bool) -> bool {
        let Some(pane) = self.panes.iter_mut().find(|pane| pane.id == pane_id) else {
            return false;
        };
        if !pane.tabs.iter().any(|tab| tab.id == tab_id) {
            return false;
        }
        if record {
            pane.select(tab_id);
        } else {
            pane.active_tab = tab_id;
        }
        self.set_focus(pane_id);
        self.debug_assert_invariants();
        true
    }

    /// The tab after (or before) the active one in tab order, wrapping.
    pub fn adjacent_tab(&self, pane_id: PaneId, forward: bool) -> Option<TabId> {
        let pane = self.pane(pane_id)?;
        let count = pane.tabs.len();
        let index = pane.tabs.iter().position(|tab| tab.id == pane.active_tab)?;
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        Some(pane.tabs[next].id)
    }

    /// Pin a tab at the end of its pane's pinned prefix, or unpin it to the
    /// first unpinned position. Returns whether the tab is now pinned.
    pub fn toggle_pin(&mut self, pane_id: PaneId, tab_id: TabId) -> Option<bool> {
        let pane = self.panes.iter_mut().find(|pane| pane.id == pane_id)?;
        let index = pane.tabs.iter().position(|tab| tab.id == tab_id)?;
        let mut tab = pane.tabs.remove(index);
        tab.pinned = !tab.pinned;
        tab.preview = false;
        let pinned = tab.pinned;
        let destination = pane.pinned_count();
        pane.tabs.insert(destination, tab);
        self.debug_assert_invariants();
        Some(pinned)
    }

    /// Open a document as the pane's preview tab, replacing its current
    /// preview in place; a dock pane opens it in the center instead. Returns
    /// the pane, the tab, and the surface the preview replaced.
    pub fn open_preview_for_document(
        &mut self,
        pane_id: PaneId,
        document_id: DocumentId,
    ) -> Option<(PaneId, TabId, Option<TabSurfaceId>)> {
        self.open_preview(pane_id, TabSurfaceId::Document(document_id))
    }

    /// Open a document as the center pane's preview tab, creating the
    /// center's first pane when the center is empty.
    pub fn open_preview_in_center(
        &mut self,
        document_id: DocumentId,
    ) -> (PaneId, TabId, Option<TabSurfaceId>) {
        let pane = self.ensure_center_pane();
        self.open_preview_for_document(pane, document_id)
            .expect("the center pane is live")
    }

    /// Open a composite view as the center pane's preview tab, replacing the
    /// pane's current preview in place. Returns the pane, the tab, and the
    /// surface the preview replaced.
    pub fn open_preview_composite_in_center(
        &mut self,
        composite: CompositeViewId,
    ) -> Option<(PaneId, TabId, Option<TabSurfaceId>)> {
        if self.has_composite(composite) {
            return None;
        }
        let pane = self.ensure_center_pane();
        self.open_preview(pane, TabSurfaceId::Composite(composite))
    }

    fn open_preview(
        &mut self,
        pane_id: PaneId,
        surface: TabSurfaceId,
    ) -> Option<(PaneId, TabId, Option<TabSurfaceId>)> {
        if self.pane_area(pane_id)? != Area::Center {
            let center = self.ensure_center_pane();
            return self.open_preview(center, surface);
        }
        let preview = self
            .pane(pane_id)?
            .tabs
            .iter()
            .find(|tab| tab.preview)
            .map(|tab| (tab.id, tab.surface));
        let (tab, replaced) = match preview {
            Some((tab, replaced)) => {
                self.replace_tab_surface(pane_id, tab, surface);
                (tab, Some(replaced))
            }
            None => (self.insert_tab(pane_id, surface)?, None),
        };
        let pane = self.panes.iter_mut().find(|pane| pane.id == pane_id)?;
        pane.tabs.iter_mut().find(|item| item.id == tab)?.preview = true;
        Some((pane_id, tab, replaced))
    }

    /// Make a preview tab regular. Returns whether it was a preview.
    pub fn promote_preview(&mut self, pane_id: PaneId, tab_id: TabId) -> bool {
        let Some(tab) = self
            .panes
            .iter_mut()
            .find(|pane| pane.id == pane_id)
            .and_then(|pane| pane.tabs.iter_mut().find(|tab| tab.id == tab_id))
        else {
            return false;
        };
        std::mem::replace(&mut tab.preview, false)
    }

    /// Make every preview tab regular when `edited` reports a document it
    /// shows.
    pub fn promote_edited_previews(&mut self, edited: impl Fn(DocumentId) -> bool) -> bool {
        let mut promoted = false;
        let composite_documents = &self.composite_documents;
        for tab in self.panes.iter_mut().flat_map(|pane| &mut pane.tabs) {
            if !tab.preview {
                continue;
            }
            let shown = match tab.surface {
                TabSurfaceId::Document(document) => std::slice::from_ref(&document).to_vec(),
                TabSurfaceId::Composite(composite) => composite_documents
                    .get(&composite)
                    .cloned()
                    .unwrap_or_default(),
                TabSurfaceId::Terminal(_) | TabSurfaceId::Tool(_) => Vec::new(),
            };
            if shown.into_iter().any(&edited) {
                tab.preview = false;
                promoted = true;
            }
        }
        promoted
    }

    pub fn open_tab_for_document(
        &mut self,
        pane_id: PaneId,
        document_id: DocumentId,
    ) -> Option<TabId> {
        self.insert_tab(pane_id, TabSurfaceId::Document(document_id))
    }

    pub fn open_terminal_tab(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
    ) -> Option<TabId> {
        if self.has_terminal_session(session_id) {
            return None;
        }
        self.insert_tab(pane_id, TabSurfaceId::Terminal(session_id))
    }

    /// Open a composite view in the center pane, creating the center's first
    /// pane when the center is empty.
    pub fn open_composite_in_center(
        &mut self,
        composite: CompositeViewId,
    ) -> Option<(PaneId, TabId)> {
        if self.has_composite(composite) {
            return None;
        }
        let pane = self.ensure_center_pane();
        self.insert_tab(pane, TabSurfaceId::Composite(composite))
            .map(|tab| (pane, tab))
    }

    /// Append a fresh tab to a pane whose area allows its kind, and activate
    /// it.
    fn insert_tab(&mut self, pane_id: PaneId, surface: TabSurfaceId) -> Option<TabId> {
        let pane_index = self.panes.iter().position(|pane| pane.id == pane_id)?;
        if !self.allows(surface, self.panes[pane_index].area) {
            return None;
        }
        let tab = self.new_tab(surface, false);
        let tab_id = tab.id;
        let pane = &mut self.panes[pane_index];
        pane.tabs.push(tab);
        pane.select(tab_id);
        self.set_focus(pane_id);
        self.debug_assert_invariants();
        Some(tab_id)
    }

    /// Open a document in the center pane, creating the center's first pane
    /// when the center is empty.
    pub fn open_document_in_center(&mut self, document: DocumentId) -> (PaneId, TabId) {
        let pane = self.ensure_center_pane();
        let tab = self.open_tab_for_document(pane, document).unwrap();
        (pane, tab)
    }

    pub fn open_terminal_in_center(
        &mut self,
        session: TerminalSessionId,
    ) -> Option<(PaneId, TabId)> {
        if self.has_terminal_session(session) {
            return None;
        }
        let pane = self.ensure_center_pane();
        self.open_terminal_tab(pane, session).map(|tab| (pane, tab))
    }

    /// The center pane, or a fresh empty one when the center is empty. The
    /// caller installs the first tab of a fresh pane before publishing this
    /// transition.
    fn ensure_center_pane(&mut self) -> PaneId {
        self.center_pane()
            .unwrap_or_else(|| self.create_pane(Area::Center))
    }

    /// Install an empty pane as the root of an area that has none.
    pub(super) fn create_pane(&mut self, area: Area) -> PaneId {
        debug_assert!(self.tree(area).layout.is_none());
        let pane = self.allocate_pane_id();
        self.tree_mut(area).layout = Some(WorkbenchLayout::Pane(pane));
        self.panes.push(Pane::new(pane, area, Vec::new(), TabId(0)));
        pane
    }

    /// Show another document in a document tab, as a fresh presentation.
    pub fn replace_tab_document(
        &mut self,
        pane_id: PaneId,
        tab_id: TabId,
        document_id: DocumentId,
    ) -> bool {
        let is_document = self
            .pane(pane_id)
            .and_then(|pane| pane.tabs.iter().find(|tab| tab.id == tab_id))
            .is_some_and(|tab| tab.document_id().is_some());
        is_document
            && self.replace_tab_surface(pane_id, tab_id, TabSurfaceId::Document(document_id))
    }

    /// Show another surface in a tab, as a fresh presentation.
    fn replace_tab_surface(
        &mut self,
        pane_id: PaneId,
        tab_id: TabId,
        surface: TabSurfaceId,
    ) -> bool {
        let presentation = self.allocate_presentation();
        let Some(pane) = self.panes.iter_mut().find(|pane| pane.id == pane_id) else {
            return false;
        };
        let Some(tab) = pane.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return false;
        };
        tab.surface = surface;
        tab.presentation = presentation;
        pane.select(tab_id);
        self.set_focus(pane_id);
        self.debug_assert_invariants();
        true
    }

    #[cfg(test)]
    /// Split the focused pane and show its active document in a fresh tab.
    pub fn split_focused(
        &mut self,
        direction: SplitDirection,
        placement: SplitPlacement,
    ) -> Option<PaneId> {
        let focused_id = self.focused_pane?;
        self.split_pane(focused_id, direction, placement)
    }

    /// Split one captured pane and show its active document in a fresh tab.
    pub fn split_pane(
        &mut self,
        pane_id: PaneId,
        direction: SplitDirection,
        placement: SplitPlacement,
    ) -> Option<PaneId> {
        let document_id = self.pane(pane_id)?.active_tab().document_id()?;
        self.split_with(
            pane_id,
            direction,
            placement,
            TabSurfaceId::Document(document_id),
        )
    }

    /// Split a captured pane: a document tab splits like
    /// [`split_pane`](Self::split_pane), anything else opens the independent
    /// terminal session `new_session_id` in the new pane.
    pub fn split_pane_with_terminal(
        &mut self,
        pane_id: PaneId,
        direction: SplitDirection,
        placement: SplitPlacement,
        new_session_id: TerminalSessionId,
    ) -> Option<PaneId> {
        let active = self.pane(pane_id)?.active_tab();
        if active.document_id().is_some() {
            return self.split_pane(pane_id, direction, placement);
        }
        if self.has_terminal_session(new_session_id) {
            return None;
        }
        self.split_with(
            pane_id,
            direction,
            placement,
            TabSurfaceId::Terminal(new_session_id),
        )
    }

    /// Split a pane within its area, showing `surface` in the new pane when
    /// the area allows its kind.
    fn split_with(
        &mut self,
        pane_id: PaneId,
        direction: SplitDirection,
        placement: SplitPlacement,
        surface: TabSurfaceId,
    ) -> Option<PaneId> {
        let area = self.pane_area(pane_id)?;
        if !self.allows(surface, area) {
            return None;
        }
        let new_pane_id = self.allocate_pane_id();
        let tab = self.new_tab(surface, false);
        let tab_id = tab.id;
        let layout = self.tree_mut(area).layout.as_mut()?;
        assert!(
            layout.split_pane(pane_id, new_pane_id, direction, placement),
            "captured pane must occur exactly once in the layout"
        );
        self.panes
            .push(Pane::new(new_pane_id, area, vec![tab], tab_id));
        self.set_focus(new_pane_id);
        self.debug_assert_invariants();
        Some(new_pane_id)
    }

    /// Remove every document tab without closure decisions, collapsing panes
    /// that become empty. Terminal and tool tabs stay where they are.
    pub fn remove_document_tabs(&mut self) -> Vec<DocumentId> {
        let tabs = self
            .panes
            .iter()
            .flat_map(|pane| {
                pane.tabs
                    .iter()
                    .filter_map(move |tab| Some((pane.id, tab.id, tab.document_id()?)))
            })
            .collect::<Vec<_>>();
        for (pane, tab, _) in &tabs {
            self.remove_tab(*pane, *tab);
        }
        tabs.into_iter().map(|(_, _, document)| document).collect()
    }

    /// Record the documents the editors of `composite` show, so its tab counts
    /// as one view of each, however many editors it holds on it.
    pub fn set_composite_documents(
        &mut self,
        composite: CompositeViewId,
        documents: Vec<DocumentId>,
    ) {
        let mut distinct = Vec::new();
        for document in documents {
            if !distinct.contains(&document) {
                distinct.push(document);
            }
        }
        self.composite_documents.insert(composite, distinct);
    }

    /// The documents `composite` shows, each once.
    pub fn composite_documents(&self, composite: CompositeViewId) -> &[DocumentId] {
        self.composite_documents
            .get(&composite)
            .map_or(&[], Vec::as_slice)
    }

    /// The documents `tab` shows or edits, each once: a document tab's own, or
    /// those of a composite's editors.
    pub fn shown_documents(&self, tab: &WorkbenchTab) -> Vec<DocumentId> {
        match tab.surface {
            TabSurfaceId::Composite(composite) => self.composite_documents(composite).to_vec(),
            _ => tab.document_id().into_iter().collect(),
        }
    }

    pub fn view_count(&self, document_id: DocumentId) -> usize {
        self.panes
            .iter()
            .flat_map(|pane| &pane.tabs)
            .filter(|tab| self.shown_documents(tab).contains(&document_id))
            .count()
    }

    fn has_composite(&self, composite: CompositeViewId) -> bool {
        self.tabs()
            .any(|tab| tab.surface_id() == TabSurfaceId::Composite(composite))
    }

    fn has_terminal_session(&self, session_id: TerminalSessionId) -> bool {
        self.panes
            .iter()
            .flat_map(|pane| &pane.tabs)
            .any(|tab| tab.surface_id() == TabSurfaceId::Terminal(session_id))
    }

    fn allocate_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id = self
            .next_pane_id
            .checked_add(1)
            .expect("pane identity space exhausted");
        id
    }

    fn allocate_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id = self
            .next_tab_id
            .checked_add(1)
            .expect("tab identity space exhausted");
        id
    }

    fn allocate_presentation(&mut self) -> PresentationId {
        let id = PresentationId(self.next_presentation_id);
        self.next_presentation_id = self
            .next_presentation_id
            .checked_add(1)
            .expect("presentation identity space exhausted");
        id
    }

    /// A fresh tab showing `surface`. A tool tab presents its tool.
    fn new_tab(&mut self, surface: TabSurfaceId, pinned: bool) -> WorkbenchTab {
        let presentation = match surface {
            TabSurfaceId::Tool(id) => {
                self.tool(id)
                    .expect("a tool tab shows a registered tool")
                    .presentation
            }
            TabSurfaceId::Document(_) | TabSurfaceId::Terminal(_) | TabSurfaceId::Composite(_) => {
                self.allocate_presentation()
            }
        };
        WorkbenchTab {
            id: self.allocate_tab_id(),
            pinned,
            preview: false,
            surface,
            presentation,
        }
    }

    fn remove_tab(&mut self, pane_id: PaneId, tab_id: TabId) -> Option<RemovedTab> {
        let pane_index = self.panes.iter().position(|pane| pane.id == pane_id)?;
        let tab_index = self.panes[pane_index]
            .tabs
            .iter()
            .position(|tab| tab.id == tab_id)?;
        let tab = self.panes[pane_index].tabs.remove(tab_index);

        if self.panes[pane_index].tabs.is_empty() {
            let area = self.panes.remove(pane_index).area;
            let tree = self.tree_mut(area);
            tree.layout = tree
                .layout
                .take()
                .and_then(|layout| layout.remove_pane(pane_id));
            if let Area::Tool(id) = area
                && self.tree(area).layout.is_none()
            {
                self.tool_mut(id).shown = false;
            }
            self.settle_focus(Some(area));
            let removed = RemovedTab {
                tab,
                removed_pane: Some(pane_id),
            };
            self.prune_bell_marks();
            self.debug_assert_invariants();
            Some(removed)
        } else {
            let pane = &mut self.panes[pane_index];
            pane.recent.retain(|id| *id != tab_id);
            if pane.active_tab == tab_id {
                pane.select(pane.tabs[tab_index.min(pane.tabs.len() - 1)].id);
            }
            let removed = RemovedTab {
                tab,
                removed_pane: None,
            };
            self.prune_bell_marks();
            self.debug_assert_invariants();
            Some(removed)
        }
    }

    pub fn contains_tab(&self, pane_id: PaneId, tab_id: TabId, document_id: DocumentId) -> bool {
        self.pane(pane_id).is_some_and(|pane| {
            pane.tabs
                .iter()
                .any(|tab| tab.id == tab_id && tab.document_id() == Some(document_id))
        })
    }

    pub fn contains_surface(&self, pane_id: PaneId, tab_id: TabId, surface: TabSurfaceId) -> bool {
        self.pane(pane_id).is_some_and(|pane| {
            pane.tabs
                .iter()
                .any(|tab| tab.id == tab_id && tab.surface_id() == surface)
        })
    }

    fn debug_assert_invariants(&self) {
        #[cfg(debug_assertions)]
        {
            let mut layout_panes = Vec::new();
            for area in self.areas() {
                for pane in self.area_panes(area) {
                    debug_assert_eq!(self.pane_area(pane), Some(area));
                    layout_panes.push(pane);
                }
            }
            let layout_pane_set = layout_panes.iter().copied().collect::<HashSet<_>>();
            let pane_set = self.panes.iter().map(Pane::id).collect::<HashSet<_>>();
            debug_assert_eq!(layout_panes.len(), layout_pane_set.len());
            debug_assert_eq!(layout_pane_set, pane_set);
            debug_assert!(self.center.layout.is_none() || self.focused_pane.is_some());
            debug_assert!(
                self.focused_pane
                    .is_none_or(|focused| self.pane_shown(focused))
            );

            let mut slots = HashSet::new();
            for tool in &self.tools {
                debug_assert!(!tool.shown || tool.tree.layout.is_some());
                debug_assert!(!tool.shown || slots.insert(tool.slot));
            }
            let mut tool_ids = HashSet::new();
            let mut presentations = HashSet::new();
            let mut tab_ids = HashSet::new();
            for pane in &self.panes {
                debug_assert!(!pane.tabs.is_empty());
                debug_assert!(pane.tabs.iter().any(|tab| tab.id == pane.active_tab));
                debug_assert!(
                    pane.tabs
                        .iter()
                        .skip_while(|tab| tab.pinned)
                        .all(|tab| !tab.pinned)
                );
                for tab in &pane.tabs {
                    debug_assert!(self.allows(tab.surface, pane.area));
                    debug_assert!(tab_ids.insert(tab.id));
                    debug_assert!(presentations.insert(tab.presentation));
                    if let TabSurfaceId::Tool(id) = tab.surface {
                        debug_assert!(tool_ids.insert(id));
                        debug_assert_eq!(
                            Some(tab.presentation),
                            self.tool(id).map(|tool| tool.presentation)
                        );
                    }
                }
            }
        }
    }
}

struct RemovedTab {
    tab: WorkbenchTab,
    removed_pane: Option<PaneId>,
}

mod stripes;
pub use stripes::Stripes;
mod terminals;
pub use terminals::free_label;
mod tools;
pub use tools::{
    DockLocation, Slot, StoredTool, StripeClick, TOOL_LAYOUT_VERSION, ToolBadge, ToolFrame,
    ToolFrameId, ToolKind, ToolLayoutSnapshot,
};

mod arrange;
mod close;
pub mod closure;
mod recent;
pub use recent::{RecentWalk, RecentlyClosed};
mod relocate;
mod resize;
pub use resize::{BottomDockLayout, Rect, neighbor};

pub use arrange::{Arrangement, PlanNode, ToolArrangement};

pub use relocate::{
    SplitEdge, TabMoveCapture, TabMoveDestination, TabMovePlacement, TabMoveTransition,
};

pub use close::{
    CloseConfirmation, CloseRequestOutcome, CloseTransition, DocumentCloseDisposition, PendingClose,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_walks_stay_in_the_focused_area_and_focus_falls_back_to_the_center() {
        let document = DocumentId::new(20);
        let workbench = &mut Workbench::new_for_document(document);
        let main = workbench.focused_pane_id().unwrap();
        let split = workbench
            .split_focused(SplitDirection::Horizontal, SplitPlacement::After)
            .unwrap();
        let terminal = workbench.register_tool(
            "knot.terminal",
            "Terminal",
            Slot::LeftTop,
            ToolKind::Container,
        );
        let (upper, _) = workbench
            .open_terminal_in_tool(terminal, TerminalSessionId(1))
            .unwrap();
        let lower = workbench
            .split_pane_with_terminal(
                upper,
                SplitDirection::Vertical,
                SplitPlacement::After,
                TerminalSessionId(2),
            )
            .unwrap();
        assert_eq!(workbench.area_panes(Area::Center), [main, split]);
        assert_eq!(workbench.pane_order(), [main, split, upper, lower]);
        assert_eq!(workbench.focused_pane_id(), Some(lower));
        assert_eq!(workbench.cycle_pane_id(true), Some(upper));
        assert_eq!(workbench.first_pane_id(), Some(upper));
        workbench.focus_pane(main);
        assert_eq!(workbench.cycle_pane_id(true), Some(split));
        assert_eq!(workbench.last_pane_id(), Some(split));
        workbench.focus_pane(lower);

        // Hiding the window hands focus to the center pane it last focused.
        workbench.hide_tool(terminal);
        assert_eq!(workbench.focused_pane_id(), Some(main));
        assert_eq!(workbench.center_pane(), Some(main));
        // Emptying the center leaves focus on the empty center, not a window.
        let (upper, _) = workbench.show_tool(terminal).unwrap();
        workbench.focus_pane(main);
        workbench.remove_document_tabs();
        assert_eq!(workbench.center(), None);
        assert_eq!(workbench.center_pane(), None);
        assert_eq!(workbench.focused_pane_id(), None);
        assert!(workbench.focus_pane(upper));
        let (center, _) = workbench.open_document_in_center(document);
        assert_eq!(workbench.pane_area(center), Some(Area::Center));
        assert_eq!(workbench.focused_pane_id(), Some(center));
    }

    #[test]
    fn arranging_installs_the_center_and_containers_and_focuses_the_marked_pane() {
        let workbench = &mut Workbench::empty();
        let tool = workbench.register_tool("knot.tool", "Tool", Slot::RightTop, ToolKind::Fixed);
        let terminal = workbench.register_tool(
            "knot.terminal",
            "Terminal",
            Slot::BottomLeft,
            ToolKind::Container,
        );
        let pane = |tabs, focused| PlanNode::Pane {
            selected: 0,
            focused,
            pinned: 0,
            tabs,
        };
        let arranged = |tool, slot, shown, size, root| ToolArrangement {
            tool,
            slot,
            shown,
            sidebar: true,
            size,
            share: 0.5,
            root,
        };
        let installed = workbench.arrange(Arrangement {
            center: Some(pane(
                vec![(TabSurfaceId::Document(DocumentId::new(21)), 'a')],
                false,
            )),
            tools: vec![
                arranged(tool, Slot::RightTop, true, 300., None),
                arranged(
                    terminal,
                    Slot::BottomLeft,
                    true,
                    150.,
                    Some(pane(
                        vec![(TabSurfaceId::Terminal(TerminalSessionId(7)), 'b')],
                        true,
                    )),
                ),
            ],
        });
        assert_eq!(
            installed
                .iter()
                .map(|(_, value)| *value)
                .collect::<Vec<_>>(),
            ['a', 'b']
        );
        let right = workbench.tool_tab(tool).unwrap().0;
        assert!(workbench.tool_visible(tool) && workbench.tool_visible(terminal));
        assert_eq!(workbench.tool_size(tool), Some(300.));
        assert_eq!(workbench.tool_size(terminal), Some(150.));
        let terminal_pane = workbench.area_panes(Area::Tool(terminal))[0];
        assert_eq!(workbench.focused_pane_id(), Some(terminal_pane));
        // A tab restored into a pane whose area does not allow its kind lands
        // in the center.
        let (center, _) =
            workbench.insert_arranged(right, 0, TabSurfaceId::Document(DocumentId::new(9)), false);
        assert_eq!(workbench.pane_area(center), Some(Area::Center));

        // Arranging again keeps fixed tools' tabs and replaces the rest.
        workbench.arrange(Arrangement {
            center: None,
            tools: vec![arranged(tool, Slot::RightTop, false, 300., None)],
        });
        assert!(!workbench.tool_visible(tool) && !workbench.tool_visible(terminal));
        assert!(workbench.tool_is_empty(terminal));
        assert_eq!(workbench.tool_tab(tool).unwrap().0, right);
    }

    #[test]
    fn composite_tabs_live_only_in_the_center_and_close_by_identity() {
        let document = DocumentId::new(1);
        let workbench = &mut Workbench::new_for_document(document);
        let pane = workbench.focused_pane_id().unwrap();
        let container = workbench.register_tool(
            "terminals",
            "Terminals",
            Slot::BottomLeft,
            ToolKind::Container,
        );
        let composite = CompositeViewId(3);

        assert!(!workbench.allows(TabSurfaceId::Composite(composite), Area::Tool(container)));
        assert!(workbench.allows(TabSurfaceId::Composite(composite), Area::Center));
        assert_eq!(
            TabSurfaceId::Composite(composite).kind(),
            TabKind::Composite
        );

        let (opened_pane, tab) = workbench.open_composite_in_center(composite).unwrap();
        assert_eq!(opened_pane, pane);
        assert!(workbench.open_composite_in_center(composite).is_none());
        assert_eq!(workbench.pane(pane).unwrap().active_tab_id(), tab);
        assert_eq!(workbench.tab(pane, tab).unwrap().document_id(), None);
        assert_eq!(workbench.view_count(document), 1);

        // Editors on a document make the tab one view of it, however many.
        workbench.set_composite_documents(composite, vec![document, document]);
        assert_eq!(workbench.composite_documents(composite), &[document]);
        assert_eq!(workbench.view_count(document), 2);

        assert!(
            workbench
                .request_close_composite_tab(pane, tab, CompositeViewId(4))
                .is_none()
        );
        let transition = workbench
            .request_close_composite_tab(pane, tab, composite)
            .unwrap();
        assert_eq!(transition.document, None);
        assert!(workbench.tab(pane, tab).is_none());
    }

    #[test]
    fn mixed_tabs_keep_identity_activation_and_document_views() {
        let document = DocumentId::new(1);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let pane = workbench.focused_pane_id().unwrap();
            let document_tab = workbench.focused_pane().unwrap().active_tab_id();
            let terminal = TerminalSessionId(41);
            let terminal_tab = workbench.open_terminal_tab(pane, terminal).unwrap();
            assert_eq!(workbench.view_count(document), 1);
            assert_eq!(
                workbench.focused_pane().unwrap().active_tab_id(),
                terminal_tab
            );
            assert_eq!(
                workbench.tab_snapshots(),
                vec![
                    WorkbenchTabSnapshot {
                        pane_id: pane,
                        tab_id: document_tab,
                        surface: TabSurfaceId::Document(document),
                    },
                    WorkbenchTabSnapshot {
                        pane_id: pane,
                        tab_id: terminal_tab,
                        surface: TabSurfaceId::Terminal(terminal),
                    },
                ]
            );
            assert!(!workbench.contains_surface(
                pane,
                document_tab,
                TabSurfaceId::Terminal(terminal)
            ));
            assert!(workbench.activate_tab(pane, document_tab));
            assert_eq!(
                workbench.focused_pane().unwrap().active_tab().document_id(),
                Some(document)
            );
        }
    }

    #[test]
    fn terminal_split_gets_independent_session_and_document_split_keeps_model() {
        let document = DocumentId::new(2);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let document_pane = workbench.focused_pane_id().unwrap();
            let terminal_source = TerminalSessionId(41);
            workbench
                .open_terminal_tab(document_pane, terminal_source)
                .unwrap();
            assert!(
                workbench
                    .split_pane(
                        document_pane,
                        SplitDirection::Horizontal,
                        SplitPlacement::After
                    )
                    .is_none()
            );
            let terminal_pane = workbench
                .split_pane_with_terminal(
                    document_pane,
                    SplitDirection::Horizontal,
                    SplitPlacement::After,
                    TerminalSessionId(42),
                )
                .unwrap();
            assert_eq!(workbench.view_count(document), 1);
            assert_eq!(
                workbench
                    .pane(terminal_pane)
                    .unwrap()
                    .active_tab()
                    .surface_id(),
                TabSurfaceId::Terminal(TerminalSessionId(42))
            );
            assert_eq!(
                workbench
                    .pane(document_pane)
                    .unwrap()
                    .active_tab()
                    .surface_id(),
                TabSurfaceId::Terminal(terminal_source)
            );
            let document_tab = workbench.pane(document_pane).unwrap().tabs()[0].id();
            assert!(workbench.activate_tab(document_pane, document_tab));
            workbench
                .split_pane_with_terminal(
                    document_pane,
                    SplitDirection::Vertical,
                    SplitPlacement::After,
                    TerminalSessionId(43),
                )
                .unwrap();
            assert_eq!(workbench.view_count(document), 2);
        }
    }

    #[test]
    fn panes_keep_ordered_tabs_and_stable_focus() {
        let first_document = DocumentId::new(3);
        let second_document = DocumentId::new(4);
        let workbench = &mut Workbench::new_for_document(first_document);

        {
            let pane_id = workbench.focused_pane_id().unwrap();
            let first_tab = workbench.focused_pane().unwrap().active_tab_id();
            let second_tab = workbench
                .open_tab_for_document(pane_id, second_document)
                .unwrap();

            let pane = workbench.focused_pane().unwrap();
            assert_eq!(
                pane.tabs().iter().map(WorkbenchTab::id).collect::<Vec<_>>(),
                vec![first_tab, second_tab]
            );
            assert_eq!(pane.active_tab_id(), second_tab);
            assert!(workbench.activate_tab(pane_id, first_tab));
            assert_eq!(workbench.focused_pane().unwrap().active_tab_id(), first_tab);
            assert!(!workbench.activate_tab(PaneId(u64::MAX), first_tab));
        }
    }

    #[test]
    fn tab_order_wraps_and_recent_order_follows_activation_closure_and_moves() {
        let document = DocumentId::new(5);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let pane = workbench.focused_pane_id().unwrap();
            let first = workbench.focused_pane().unwrap().active_tab_id();
            let second = workbench.open_tab_for_document(pane, document).unwrap();
            let third = workbench.open_tab_for_document(pane, document).unwrap();
            let recent = |workbench: &Workbench| workbench.pane(pane).unwrap().recent_tabs();

            assert_eq!(workbench.adjacent_tab(pane, true), Some(first));
            assert_eq!(workbench.adjacent_tab(pane, false), Some(second));
            assert_eq!(recent(workbench), [third, second, first]);
            assert!(workbench.activate_tab(pane, first));
            assert_eq!(workbench.adjacent_tab(pane, false), Some(third));
            assert_eq!(recent(workbench), [first, third, second]);

            // Showing without recording leaves the order for a walk.
            assert!(workbench.show_tab(pane, second, false));
            assert_eq!(recent(workbench), [first, third, second]);
            assert!(workbench.activate_tab(pane, second));
            assert_eq!(recent(workbench), [second, first, third]);

            // Closing the active tab activates its neighbor, which becomes
            // the most recent.
            workbench.remove_tab(pane, second).unwrap();
            assert_eq!(workbench.pane(pane).unwrap().active_tab_id(), third);
            assert_eq!(recent(workbench), [third, first]);

            // A tab moved into another pane is that pane's most recent.
            let other = workbench
                .split_pane(pane, SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();
            let source = workbench.capture_tab_move(pane, first).unwrap();
            let anchor = workbench
                .capture_tab_move(other, workbench.pane(other).unwrap().active_tab_id())
                .unwrap();
            let moved = workbench
                .relocate_tab(
                    source,
                    TabMoveDestination::At {
                        anchor,
                        placement: TabMovePlacement::Before,
                    },
                )
                .unwrap();
            assert_eq!(recent(workbench), [third]);
            assert_eq!(
                workbench.pane(other).unwrap().recent_tabs()[0],
                moved.destination.tab_id
            );
        }
    }

    #[test]
    fn pinned_tabs_form_a_prefix_and_moves_stay_in_their_section() {
        let document = DocumentId::new(6);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let pane = workbench.focused_pane_id().unwrap();
            let a = workbench.focused_pane().unwrap().active_tab_id();
            let [b, c, d] =
                [(); 3].map(|_| workbench.open_tab_for_document(pane, document).unwrap());
            let order = |workbench: &Workbench, pane| {
                workbench
                    .pane(pane)
                    .unwrap()
                    .tabs()
                    .iter()
                    .map(|tab| (tab.id(), tab.is_pinned()))
                    .collect::<Vec<_>>()
            };

            assert_eq!(workbench.toggle_pin(pane, c), Some(true));
            assert_eq!(workbench.toggle_pin(pane, d), Some(true));
            assert_eq!(
                order(workbench, pane),
                [(c, true), (d, true), (a, false), (b, false)]
            );
            // Unpinning returns a tab to the first unpinned position.
            assert_eq!(workbench.toggle_pin(pane, c), Some(false));
            assert_eq!(
                order(workbench, pane),
                [(d, true), (c, false), (a, false), (b, false)]
            );
            assert_eq!(workbench.toggle_pin(pane, c), Some(true));

            // A move within the pane stays in its section.
            let capture =
                |workbench: &Workbench, pane, tab| workbench.capture_tab_move(pane, tab).unwrap();
            let source = capture(workbench, pane, b);
            let anchor = capture(workbench, pane, d);
            workbench
                .relocate_tab(
                    source,
                    TabMoveDestination::At {
                        anchor,
                        placement: TabMovePlacement::Before,
                    },
                )
                .unwrap();
            assert_eq!(
                order(workbench, pane),
                [(d, true), (c, true), (b, false), (a, false)]
            );
            let source = capture(workbench, pane, d);
            let anchor = capture(workbench, pane, a);
            workbench
                .relocate_tab(
                    source,
                    TabMoveDestination::At {
                        anchor,
                        placement: TabMovePlacement::End,
                    },
                )
                .unwrap();
            assert_eq!(
                order(workbench, pane),
                [(c, true), (d, true), (b, false), (a, false)]
            );

            // Another pane takes a pinned tab at the end of its pinned prefix.
            let other = workbench
                .split_pane(pane, SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();
            let first = workbench.pane(other).unwrap().active_tab_id();
            let source = capture(workbench, pane, c);
            let anchor = capture(workbench, other, first);
            let moved = workbench
                .relocate_tab(
                    source,
                    TabMoveDestination::At {
                        anchor,
                        placement: TabMovePlacement::End,
                    },
                )
                .unwrap();
            assert_eq!(
                order(workbench, other),
                [(moved.destination.tab_id, true), (first, false)]
            );
        }
    }

    #[test]
    fn a_preview_is_replaced_in_place_until_pinning_promotion_or_a_move() {
        let [first, second, third, fourth] = [7, 8, 9, 10].map(DocumentId::new);
        let workbench = &mut Workbench::new_for_document(first);

        {
            let pane = workbench.focused_pane_id().unwrap();
            let (_, preview, replaced) = workbench.open_preview_for_document(pane, second).unwrap();
            assert_eq!(replaced, None);
            let (_, again, replaced) = workbench.open_preview_for_document(pane, third).unwrap();
            assert_eq!(
                (again, replaced),
                (preview, Some(TabSurfaceId::Document(second)))
            );
            let tabs = |workbench: &Workbench| {
                workbench
                    .pane(pane)
                    .unwrap()
                    .tabs()
                    .iter()
                    .map(|tab| (tab.document_id().unwrap(), tab.is_preview()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(tabs(workbench), [(first, false), (third, true)]);

            // Promotion keeps the tab, and the next preview opens beside it.
            assert!(workbench.promote_preview(pane, preview));
            assert!(!workbench.promote_preview(pane, preview));
            workbench.open_preview_for_document(pane, fourth);
            assert_eq!(
                tabs(workbench),
                [(first, false), (third, false), (fourth, true)]
            );

            // Edits, pinning, and moves promote too.
            assert!(workbench.promote_edited_previews(|document| document == fourth));
            assert_eq!(tabs(workbench)[2], (fourth, false));
            let (_, pinned, _) = workbench.open_preview_for_document(pane, second).unwrap();
            workbench.toggle_pin(pane, pinned);
            assert_eq!(tabs(workbench)[0], (second, false));
            let (_, moved, _) = workbench.open_preview_for_document(pane, second).unwrap();
            let source = workbench.capture_tab_move(pane, moved).unwrap();
            let anchor = workbench
                .capture_tab_move(pane, workbench.pane(pane).unwrap().tabs()[1].id())
                .unwrap();
            let moved = workbench
                .relocate_tab(
                    source,
                    TabMoveDestination::At {
                        anchor,
                        placement: TabMovePlacement::Before,
                    },
                )
                .unwrap();
            assert!(
                workbench
                    .pane(pane)
                    .unwrap()
                    .tabs()
                    .iter()
                    .all(|tab| tab.id() != moved.destination.tab_id || !tab.is_preview())
            );
        }
    }

    #[test]
    fn a_preview_slot_is_shared_by_documents_and_composite_views() {
        let [first, second, third] = [1, 2, 3].map(DocumentId::new);
        let [one, two] = [1, 2].map(CompositeViewId);
        let workbench = &mut Workbench::new_for_document(first);
        let pane = workbench.focused_pane_id().unwrap();
        let surfaces = |workbench: &Workbench| {
            workbench
                .pane(pane)
                .unwrap()
                .tabs()
                .iter()
                .map(|tab| (tab.surface_id(), tab.is_preview()))
                .collect::<Vec<_>>()
        };

        let (_, tab, replaced) = workbench.open_preview_composite_in_center(one).unwrap();
        assert_eq!(replaced, None);
        workbench.set_composite_documents(one, vec![second]);

        // A document preview replaces a composite preview, and back.
        let (_, same, replaced) = workbench.open_preview_for_document(pane, third).unwrap();
        assert_eq!((same, replaced), (tab, Some(TabSurfaceId::Composite(one))));
        assert_eq!(workbench.composite_documents(one), &[second]);
        let (_, same, replaced) = workbench.open_preview_composite_in_center(two).unwrap();
        assert_eq!((same, replaced), (tab, Some(TabSurfaceId::Document(third))));
        assert_eq!(
            surfaces(workbench),
            [
                (TabSurfaceId::Document(first), false),
                (TabSurfaceId::Composite(two), true)
            ]
        );

        // An edit to a document the composite shows promotes it.
        workbench.set_composite_documents(two, vec![second]);
        assert!(!workbench.promote_edited_previews(|document| document == third));
        assert!(workbench.promote_edited_previews(|document| document == second));
        assert!(!surfaces(workbench)[1].1);

        // Pinning promotes a composite preview and keeps the pinned prefix.
        let (_, tab, _) = workbench
            .open_preview_composite_in_center(CompositeViewId(3))
            .unwrap();
        assert_eq!(workbench.toggle_pin(pane, tab), Some(true));
        assert_eq!(
            surfaces(workbench)[0],
            (TabSurfaceId::Composite(CompositeViewId(3)), false)
        );
    }

    #[test]
    fn tab_snapshots_follow_layout_and_tab_order() {
        let first_document = DocumentId::new(8);
        let second_document = DocumentId::new(9);
        let third_document = DocumentId::new(10);
        let workbench = &mut Workbench::new_for_document(first_document);

        {
            let first_pane = workbench.focused_pane_id().unwrap();
            let first_tab = workbench.focused_pane().unwrap().active_tab_id();
            let second_tab = workbench
                .open_tab_for_document(first_pane, second_document)
                .unwrap();
            assert!(workbench.activate_tab(first_pane, first_tab));
            let preceding_pane = workbench
                .split_focused(SplitDirection::Vertical, SplitPlacement::Before)
                .unwrap();
            let preceding_first_tab = workbench.focused_pane().unwrap().active_tab_id();
            let preceding_second_tab = workbench
                .open_tab_for_document(preceding_pane, third_document)
                .unwrap();

            assert_eq!(
                workbench.tab_snapshots(),
                vec![
                    WorkbenchTabSnapshot {
                        pane_id: preceding_pane,
                        tab_id: preceding_first_tab,
                        surface: TabSurfaceId::Document(first_document),
                    },
                    WorkbenchTabSnapshot {
                        pane_id: preceding_pane,
                        tab_id: preceding_second_tab,
                        surface: TabSurfaceId::Document(third_document),
                    },
                    WorkbenchTabSnapshot {
                        pane_id: first_pane,
                        tab_id: first_tab,
                        surface: TabSurfaceId::Document(first_document),
                    },
                    WorkbenchTabSnapshot {
                        pane_id: first_pane,
                        tab_id: second_tab,
                        surface: TabSurfaceId::Document(second_document),
                    },
                ]
            );
        }
    }

    #[test]
    fn nested_splits_preserve_tree_order_and_focus() {
        let document = DocumentId::new(11);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let first = workbench.focused_pane_id().unwrap();
            let second = workbench
                .split_focused(SplitDirection::Horizontal, SplitPlacement::After)
                .unwrap();
            let third = workbench
                .split_focused(SplitDirection::Vertical, SplitPlacement::Before)
                .unwrap();

            let mut pane_ids = Vec::new();
            workbench.center().unwrap().pane_ids(&mut pane_ids);
            assert_eq!(pane_ids, vec![first, third, second]);
            assert_eq!(workbench.focused_pane_id(), Some(third));
            assert_eq!(workbench.panes().len(), 3);
            assert_eq!(workbench.view_count(document), 3);
            assert!(matches!(
                workbench.center(),
                Some(WorkbenchLayout::Split {
                    direction: SplitDirection::Horizontal,
                    second,
                    ..
                }) if matches!(
                    second.as_ref(),
                    WorkbenchLayout::Split {
                        direction: SplitDirection::Vertical,
                        ..
                    }
                )
            ));
        }
    }

    #[test]
    fn split_placement_orders_the_new_pane_without_changing_its_focus() {
        let document = DocumentId::new(12);
        let workbench = &mut Workbench::new_for_document(document);

        {
            let existing = workbench.focused_pane_id().unwrap();
            let new = workbench
                .split_focused(SplitDirection::Horizontal, SplitPlacement::Before)
                .unwrap();
            let mut pane_ids = Vec::new();
            workbench.center().unwrap().pane_ids(&mut pane_ids);

            assert_eq!(pane_ids, vec![new, existing]);
            assert_eq!(workbench.focused_pane_id(), Some(new));
        }
    }

    #[test]
    fn tabs_and_splits_get_distinct_presentations_of_one_document() {
        let document = DocumentId::new(13);
        let workbench = &mut Workbench::new_for_document(document);
        let first_pane = workbench.focused_pane_id().unwrap();
        let first = workbench
            .focused_pane()
            .unwrap()
            .active_tab()
            .presentation();
        let second_tab = workbench
            .open_tab_for_document(first_pane, document)
            .unwrap();
        let second = workbench
            .tab(first_pane, second_tab)
            .unwrap()
            .presentation();
        let split_pane = workbench
            .split_focused(SplitDirection::Vertical, SplitPlacement::After)
            .unwrap();
        let third = workbench
            .pane(split_pane)
            .unwrap()
            .active_tab()
            .presentation();

        assert_ne!(first, second);
        assert_ne!(second, third);
        assert_ne!(first, third);
        assert_eq!(workbench.view_count(document), 3);

        // Replacing a tab's document presents it anew in the same tab.
        let other = DocumentId::new(14);
        assert!(workbench.replace_tab_document(first_pane, second_tab, other));
        let replaced = workbench.tab(first_pane, second_tab).unwrap();
        assert_eq!(replaced.document_id(), Some(other));
        assert_ne!(replaced.presentation(), second);
    }
}
