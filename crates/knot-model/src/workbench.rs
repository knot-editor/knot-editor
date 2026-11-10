//! Window-local workbench layout.
//!
//! A workbench is a center area of documents surrounded by left, right, and
//! bottom docks of tools, each a binary split tree whose leaves are tabbed
//! panes, with native tool definitions. Tabs carry the identity of what they
//! show (a document, a terminal session, or a tool) and a presentation
//! identity; the application owns the views behind presentations. This module
//! models layout and lifecycle transitions independently of native windows,
//! views, and confirmation dialogs.

#[cfg(debug_assertions)]
use std::collections::HashSet;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabSurfaceId {
    Document(DocumentId),
    Terminal(TerminalSessionId),
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

/// Where a pane lives: the center document area or one of the docks.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Area {
    Center,
    Dock(DockLocation),
}

impl Area {
    /// Every area, in layout order.
    pub const ALL: [Self; 4] = [
        Self::Center,
        Self::Dock(DockLocation::Left),
        Self::Dock(DockLocation::Right),
        Self::Dock(DockLocation::Bottom),
    ];
}

/// What a tab presents, as far as placement cares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabKind {
    Document,
    Terminal,
    Tool,
}

impl TabKind {
    /// Whether tabs of this kind may occupy `area`. Every placement,
    /// relocation, and restore checks this declaration.
    pub fn allows(self, area: Area) -> bool {
        match self {
            Self::Document => area == Area::Center,
            Self::Terminal => true,
            Self::Tool => area != Area::Center,
        }
    }
}

impl TabSurfaceId {
    pub fn kind(self) -> TabKind {
        match self {
            Self::Document(_) => TabKind::Document,
            Self::Terminal(_) => TabKind::Terminal,
            Self::Tool(_) => TabKind::Tool,
        }
    }
}

pub struct WorkbenchTab {
    id: TabId,
    /// Pinned tabs form a prefix of their pane's tabs.
    pinned: bool,
    /// A preview document tab is replaced by the pane's next preview.
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
            TabSurfaceId::Terminal(_) | TabSurfaceId::Tool(_) => None,
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
struct AreaTree {
    layout: Option<WorkbenchLayout>,
    focused: Option<PaneId>,
}

/// A dock keeps its arrangement while hidden; its size is absolute.
struct DockState {
    tree: AreaTree,
    size: f32,
    visible: bool,
}

pub struct Workbench {
    tools: Vec<ToolFrame>,
    center: AreaTree,
    docks: [DockState; 3],
    next_pane_id: u64,
    next_tab_id: u64,
    next_presentation_id: u64,
    next_tool_id: u64,
    /// The dock new terminals open in when no dock terminal exists.
    terminal_home: DockLocation,
    /// Dock terminal tabs, most recently focused first.
    terminal_recent: Vec<TabId>,
    panes: Vec<Pane>,
    focused_pane: Option<PaneId>,
    /// The pane that had focus before the focused one, for going back.
    previous_pane: Option<PaneId>,
}

impl Workbench {
    pub fn empty() -> Self {
        Self {
            tools: Vec::new(),
            center: AreaTree::default(),
            docks: DockLocation::ALL.map(|dock| DockState {
                tree: AreaTree::default(),
                size: dock.default_size(),
                visible: true,
            }),
            next_pane_id: 1,
            next_tab_id: 1,
            next_presentation_id: 1,
            next_tool_id: 1,
            terminal_home: DEFAULT_TERMINAL_HOME,
            terminal_recent: Vec::new(),
            panes: Vec::new(),
            focused_pane: None,
            previous_pane: None,
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
            Area::Dock(dock) => &self.docks[dock.index()].tree,
        }
    }

    fn tree_mut(&mut self, area: Area) -> &mut AreaTree {
        match area {
            Area::Center => &mut self.center,
            Area::Dock(dock) => &mut self.docks[dock.index()].tree,
        }
    }

    /// The center's split tree, `None` while the center is empty.
    pub fn center(&self) -> Option<&WorkbenchLayout> {
        self.center.layout.as_ref()
    }

    pub fn area_layout(&self, area: Area) -> Option<&WorkbenchLayout> {
        self.tree(area).layout.as_ref()
    }

    /// Whether an area is painted: the center always, a dock while it is
    /// visible and holds panes.
    pub fn area_shown(&self, area: Area) -> bool {
        match area {
            Area::Center => true,
            Area::Dock(dock) => {
                let state = &self.docks[dock.index()];
                state.visible && state.tree.layout.is_some()
            }
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

    /// Every pane in layout order: the center, then the left, right, and
    /// bottom docks, hidden ones included.
    pub fn pane_order(&self) -> Vec<PaneId> {
        Area::ALL
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
    /// the pane. Focusing a hidden dock's pane shows the dock.
    fn set_focus(&mut self, pane_id: PaneId) {
        if self.focused_pane != Some(pane_id) {
            self.previous_pane = self.focused_pane;
        }
        self.focused_pane = Some(pane_id);
        let area = self.pane_area(pane_id).expect("focused panes are live");
        self.tree_mut(area).focused = Some(pane_id);
        if let Area::Dock(dock) = area {
            self.docks[dock.index()].visible = true;
            let active = self.pane(pane_id).and_then(|pane| {
                pane.tabs
                    .iter()
                    .find(|tab| tab.id == pane.active_tab)
                    .filter(|tab| tab.surface.kind() == TabKind::Terminal)
                    .map(|tab| tab.id)
            });
            if let Some(id) = active {
                self.terminal_recent.retain(|recent| *recent != id);
                self.terminal_recent.insert(0, id);
            }
        }
    }

    /// The pane an area would focus: the one it last focused, else its first.
    fn area_focus(&self, area: Area) -> Option<PaneId> {
        let tree = self.tree(area);
        tree.focused
            .filter(|pane| self.pane_area(*pane) == Some(area))
            .or_else(|| tree.layout.as_ref().map(WorkbenchLayout::first_pane))
    }

    /// Keep focus on a painted pane: when the focused pane is gone or
    /// hidden, focus moves to `preferred`'s pane, then the center's, else to
    /// the empty center. Docks take focus only when asked to.
    fn settle_focus(&mut self, preferred: Option<Area>) {
        if self.focused_pane.is_some_and(|pane| self.pane_shown(pane)) {
            return;
        }
        let candidate = preferred
            .into_iter()
            .chain([Area::Center])
            .filter(|area| self.area_shown(*area))
            .find_map(|area| self.area_focus(area));
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
    /// the pane, the tab, and the document the preview replaced.
    pub fn open_preview_for_document(
        &mut self,
        pane_id: PaneId,
        document_id: DocumentId,
    ) -> Option<(PaneId, TabId, Option<DocumentId>)> {
        if self.pane_area(pane_id)? != Area::Center {
            let center = self.ensure_center_pane();
            return self.open_preview_for_document(center, document_id);
        }
        let preview = self
            .pane(pane_id)?
            .tabs
            .iter()
            .find(|tab| tab.preview)
            .map(|tab| (tab.id, tab.document_id()));
        let (tab, replaced) = match preview {
            Some((tab, replaced)) => {
                self.replace_tab_document(pane_id, tab, document_id);
                (tab, replaced)
            }
            None => (self.open_tab_for_document(pane_id, document_id)?, None),
        };
        let pane = self.panes.iter_mut().find(|pane| pane.id == pane_id)?;
        pane.tabs.iter_mut().find(|item| item.id == tab)?.preview = true;
        Some((pane_id, tab, replaced))
    }

    /// Open a document as the center pane's preview tab, creating the
    /// center's first pane when the center is empty.
    pub fn open_preview_in_center(
        &mut self,
        document_id: DocumentId,
    ) -> (PaneId, TabId, Option<DocumentId>) {
        let pane = self.ensure_center_pane();
        self.open_preview_for_document(pane, document_id)
            .expect("the center pane is live")
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

    /// Make every preview tab whose document `edited` reports regular.
    pub fn promote_edited_previews(&mut self, edited: impl Fn(DocumentId) -> bool) -> bool {
        let mut promoted = false;
        for tab in self.panes.iter_mut().flat_map(|pane| &mut pane.tabs) {
            if tab.preview && tab.document_id().is_some_and(&edited) {
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

    /// Append a fresh tab to a pane whose area allows its kind, and activate
    /// it.
    fn insert_tab(&mut self, pane_id: PaneId, surface: TabSurfaceId) -> Option<TabId> {
        let pane_index = self.panes.iter().position(|pane| pane.id == pane_id)?;
        if !surface.kind().allows(self.panes[pane_index].area) {
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
        let presentation = self.allocate_presentation();
        let Some(pane) = self.panes.iter_mut().find(|pane| pane.id == pane_id) else {
            return false;
        };
        let Some(tab) = pane.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return false;
        };
        if tab.document_id().is_none() {
            return false;
        }
        tab.surface = TabSurfaceId::Document(document_id);
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
        if !surface.kind().allows(area) {
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

    pub fn view_count(&self, document_id: DocumentId) -> usize {
        self.panes
            .iter()
            .flat_map(|pane| &pane.tabs)
            .filter(|tab| tab.document_id() == Some(document_id))
            .count()
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
            TabSurfaceId::Document(_) | TabSurfaceId::Terminal(_) => self.allocate_presentation(),
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
            self.settle_focus(Some(area));
            let removed = RemovedTab {
                tab,
                removed_pane: Some(pane_id),
            };
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
            for area in Area::ALL {
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
                    debug_assert!(tab.surface.kind().allows(pane.area));
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
pub use stripes::{StripeButton, Stripes};
mod terminals;
pub use terminals::{DEFAULT_TERMINAL_HOME, TerminalToggle};
mod tools;
pub use tools::{
    DockLocation, EXPLORER_TOOL_ID, HiddenTool, StoredDock, StoredNode, StoredPlacement,
    TOOL_LAYOUT_VERSION, ToolFrame, ToolFrameId, ToolLayoutSnapshot,
};

mod arrange;
mod close;
pub mod closure;
mod recent;
pub use recent::{RecentWalk, RecentlyClosed};
mod relocate;
mod resize;
pub use resize::{BottomDockLayout, Rect, neighbor};

pub use arrange::{Arrangement, DockArrangement, PlanNode};

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
        let [first, second] = ["knot.first", "knot.second"]
            .map(|stable| workbench.register_tool(stable, stable, DockLocation::Left));
        let (upper, tab) = workbench.show_tool(first).unwrap();
        workbench.show_tool(second);
        let source = workbench.capture_tab_move(upper, tab).unwrap();
        let lower = workbench
            .relocate_tab(
                source,
                TabMoveDestination::At {
                    anchor: source,
                    placement: TabMovePlacement::Split(SplitEdge::Bottom),
                },
            )
            .unwrap()
            .destination
            .pane_id;
        assert_eq!(workbench.area_panes(Area::Center), [main, split]);
        assert_eq!(workbench.pane_order(), [main, split, upper, lower]);
        assert_eq!(workbench.cycle_pane_id(true), Some(upper));
        assert_eq!(workbench.first_pane_id(), Some(upper));
        workbench.focus_pane(main);
        assert_eq!(workbench.cycle_pane_id(true), Some(split));
        assert_eq!(workbench.last_pane_id(), Some(split));
        workbench.focus_pane(lower);

        // A collapsed pane hands focus to its area, and the dock's last pane
        // to the center pane it last focused.
        workbench.hide_tool(first);
        assert_eq!(workbench.focused_pane_id(), Some(upper));
        workbench.hide_tool(second);
        assert_eq!(workbench.focused_pane_id(), Some(main));
        assert_eq!(workbench.center_pane(), Some(main));
        // Emptying the center leaves focus on the empty center, not a dock.
        let (upper, _) = workbench.show_tool(second).unwrap();
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
    fn arranging_installs_each_area_and_focuses_the_marked_pane() {
        let workbench = &mut Workbench::empty();
        let tool = workbench.register_tool("knot.tool", "Tool", DockLocation::Right);
        let pane = |tabs, focused| PlanNode::Pane {
            selected: 0,
            focused,
            pinned: 0,
            tabs,
        };
        let dock = |root, visible| DockArrangement {
            root,
            size: 300.,
            visible,
        };
        let installed = workbench.arrange(Arrangement {
            center: Some(pane(
                vec![(TabSurfaceId::Document(DocumentId::new(21)), 'a')],
                false,
            )),
            docks: [
                dock(None, true),
                dock(
                    Some(pane(vec![(TabSurfaceId::Tool(tool), 'b')], true)),
                    true,
                ),
                dock(None, false),
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
        assert_eq!(workbench.dock_location(right), Some(DockLocation::Right));
        assert_eq!(workbench.focused_pane_id(), Some(right));
        assert_eq!(workbench.dock_size(DockLocation::Right), 300.);
        assert!(!workbench.dock_visible(DockLocation::Bottom));
        // A tab restored into a pane whose area does not allow its kind lands
        // in the center.
        let (center, _) =
            workbench.insert_arranged(right, 0, TabSurfaceId::Document(DocumentId::new(9)), false);
        assert_eq!(workbench.pane_area(center), Some(Area::Center));
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
            assert_eq!((again, replaced), (preview, Some(second)));
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
