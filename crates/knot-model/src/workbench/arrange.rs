//! Installing a restored arrangement into a fresh workbench.

use super::*;

/// A pruned split tree whose tabs are materialized per kind before the
/// workbench installs it.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanNode<T> {
    Pane {
        selected: usize,
        focused: bool,
        /// How many of the leading tabs are pinned.
        pinned: usize,
        tabs: Vec<T>,
    },
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<PlanNode<T>>,
        second: Box<PlanNode<T>>,
    },
}

impl<T> PlanNode<T> {
    pub fn map<U>(self, step: &mut impl FnMut(T) -> U) -> PlanNode<U> {
        match self {
            Self::Pane {
                selected,
                focused,
                pinned,
                tabs,
            } => PlanNode::Pane {
                selected,
                focused,
                pinned,
                tabs: tabs.into_iter().map(&mut *step).collect(),
            },
            Self::Split {
                direction,
                ratio,
                first,
                second,
            } => PlanNode::Split {
                direction,
                ratio,
                first: Box::new(first.map(step)),
                second: Box::new(second.map(step)),
            },
        }
    }

    pub fn for_each(&self, visit: &mut impl FnMut(&T)) {
        match self {
            Self::Pane { tabs, .. } => tabs.iter().for_each(&mut *visit),
            Self::Split { first, second, .. } => {
                first.for_each(visit);
                second.for_each(visit);
            }
        }
    }
}

/// One tool window of a restored arrangement. A fixed window keeps its own
/// tab, so only a container's `root` is installed.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolArrangement<T> {
    pub tool: ToolFrameId,
    pub slot: Slot,
    pub shown: bool,
    pub sidebar: bool,
    pub size: f32,
    pub share: f32,
    pub root: Option<PlanNode<T>>,
}

/// A restored arrangement of the center and the tool windows.
#[derive(Clone, Debug, PartialEq)]
pub struct Arrangement<T> {
    pub center: Option<PlanNode<T>>,
    pub tools: Vec<ToolArrangement<T>>,
}

impl<T> Arrangement<T> {
    pub fn map<U>(self, step: &mut impl FnMut(T) -> U) -> Arrangement<U> {
        let tools = self
            .tools
            .into_iter()
            .map(|tool| ToolArrangement {
                tool: tool.tool,
                slot: tool.slot,
                shown: tool.shown,
                sidebar: tool.sidebar,
                size: tool.size,
                share: tool.share,
                root: tool.root.map(|root| root.map(&mut *step)),
            })
            .collect();
        Arrangement {
            center: self.center.map(|root| root.map(step)),
            tools,
        }
    }
}

impl Workbench {
    /// Install one restored tab at `index` of `pane`, or, when that pane is
    /// gone or its area does not allow the tab's kind, at the end of the
    /// center pane; and activate it.
    pub fn insert_arranged(
        &mut self,
        pane: PaneId,
        index: usize,
        surface: TabSurfaceId,
        pinned: bool,
    ) -> (PaneId, TabId) {
        let fits = self
            .pane_area(pane)
            .is_some_and(|area| self.allows(surface, area));
        let (pane, index) = if fits {
            (pane, index)
        } else {
            (self.ensure_center_pane(), usize::MAX)
        };
        let tab = self.new_tab(surface, pinned);
        let id = tab.id;
        let group = self
            .panes
            .iter_mut()
            .find(|group| group.id == pane)
            .expect("destination pane is live");
        let index = group.section_index(pinned, index);
        group.tabs.insert(index, tab);
        group.select(id);
        self.set_focus(pane);
        self.debug_assert_invariants();
        (pane, id)
    }

    /// Replace every tab with a restored arrangement. Tool windows hide
    /// first, so those the arrangement does not name stay hidden, and fixed
    /// ones keep their own tab. Each arranged tab carries a value the caller
    /// gets back with the tab's presentation, in plan order. The focused pane
    /// is the marked one, else the center's first, else the first shown tool
    /// window's.
    pub fn arrange<T>(
        &mut self,
        arrangement: Arrangement<(TabSurfaceId, T)>,
    ) -> Vec<(PresentationId, T)> {
        self.remove_document_tabs();
        let fixed = |tool: &ToolFrame| tool.kind() == ToolKind::Fixed;
        let kept = self
            .tools
            .iter()
            .filter(|tool| fixed(tool))
            .map(|tool| Area::Tool(tool.id))
            .collect::<Vec<_>>();
        self.panes.retain(|pane| kept.contains(&pane.area));
        self.focused_pane = None;
        self.center = AreaTree::default();
        for tool in &mut self.tools {
            tool.shown = false;
            if !fixed(tool) {
                tool.tree = AreaTree::default();
            }
        }
        let mut focused = None;
        let mut installed = Vec::new();
        let Arrangement { center, tools } = arrangement;
        self.center.layout =
            center.map(|root| self.install(root, Area::Center, &mut focused, &mut installed));
        for arranged in &tools {
            let tool = self.tool_mut(arranged.tool);
            tool.slot = arranged.slot;
            tool.sidebar = arranged.sidebar;
            tool.size = arranged.size.max(DockLocation::MIN_SIZE);
            tool.share = WorkbenchLayout::clamp_ratio(arranged.share);
        }
        for arranged in tools {
            if self.tool(arranged.tool).unwrap().kind() == ToolKind::Container {
                let area = Area::Tool(arranged.tool);
                let layout = arranged
                    .root
                    .map(|root| self.install(root, area, &mut focused, &mut installed));
                self.tool_mut(arranged.tool).tree.layout = layout;
            }
            if arranged.shown {
                self.reveal_tool(arranged.tool);
            }
        }
        match focused.filter(|pane| self.pane_shown(*pane)) {
            Some(pane) => self.set_focus(pane),
            None => self.settle_focus(None),
        }
        self.debug_assert_invariants();
        installed
    }

    pub(super) fn install<T>(
        &mut self,
        node: PlanNode<(TabSurfaceId, T)>,
        area: Area,
        focused: &mut Option<PaneId>,
        installed: &mut Vec<(PresentationId, T)>,
    ) -> WorkbenchLayout {
        match node {
            PlanNode::Pane {
                selected,
                focused: is_focused,
                pinned,
                tabs,
            } => {
                let pane = self.allocate_pane_id();
                let tabs = tabs
                    .into_iter()
                    .enumerate()
                    .map(|(index, (surface, value))| {
                        let tab = self.new_tab(surface, index < pinned);
                        installed.push((tab.presentation, value));
                        tab
                    })
                    .collect::<Vec<_>>();
                let active_tab = tabs[selected.min(tabs.len() - 1)].id;
                self.panes.push(Pane::new(pane, area, tabs, active_tab));
                if is_focused {
                    *focused = Some(pane);
                }
                WorkbenchLayout::Pane(pane)
            }
            PlanNode::Split {
                direction,
                ratio,
                first,
                second,
            } => WorkbenchLayout::Split {
                direction,
                ratio: WorkbenchLayout::clamp_ratio(ratio),
                first: Box::new(self.install(*first, area, focused, installed)),
                second: Box::new(self.install(*second, area, focused, installed)),
            },
        }
    }
}
