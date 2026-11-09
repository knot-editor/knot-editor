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

/// One dock of a restored arrangement.
#[derive(Clone, Debug, PartialEq)]
pub struct DockArrangement<T> {
    pub root: Option<PlanNode<T>>,
    pub size: f32,
    pub visible: bool,
}

/// A restored arrangement of the center and the left, right, and bottom
/// docks.
#[derive(Clone, Debug, PartialEq)]
pub struct Arrangement<T> {
    pub center: Option<PlanNode<T>>,
    pub docks: [DockArrangement<T>; 3],
}

impl<T> Arrangement<T> {
    pub fn map<U>(self, step: &mut impl FnMut(T) -> U) -> Arrangement<U> {
        let [left, right, bottom] = self.docks;
        let mut dock = |dock: DockArrangement<T>| DockArrangement {
            root: dock.root.map(|root| root.map(step)),
            size: dock.size,
            visible: dock.visible,
        };
        let docks = [dock(left), dock(right), dock(bottom)];
        Arrangement {
            center: self.center.map(|root| root.map(step)),
            docks,
        }
    }
}

impl Workbench {
    /// Install one restored tab at `index` of `pane`, or, when that pane is
    /// gone or its area does not allow the tab's kind, at the end of the
    /// center pane or the tool's dock; and activate it.
    pub fn insert_arranged(
        &mut self,
        pane: PaneId,
        index: usize,
        surface: TabSurfaceId,
        pinned: bool,
    ) -> (PaneId, TabId) {
        let fits = self
            .pane_area(pane)
            .is_some_and(|area| surface.kind().allows(area));
        let (pane, index) = if fits {
            (pane, index)
        } else {
            let pane = match surface {
                TabSurfaceId::Tool(id) => {
                    let area =
                        Area::Dock(self.tool(id).expect("registered tool").remembered_dock());
                    self.area_focus(area)
                        .unwrap_or_else(|| self.create_pane(area))
                }
                TabSurfaceId::Document(_) | TabSurfaceId::Terminal(_) => self.ensure_center_pane(),
            };
            (pane, usize::MAX)
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

    /// Replace every tab with a restored arrangement. Visible tools are
    /// hidden first, keeping their remembered placement, so tools the
    /// arrangement does not name stay hidden. Each arranged tab carries a value
    /// the caller gets back with the tab's presentation, in plan order. The
    /// focused pane is the marked one, else the center's first, else the
    /// first painted dock's.
    pub fn arrange<T>(
        &mut self,
        arrangement: Arrangement<(TabSurfaceId, T)>,
    ) -> Vec<(PresentationId, T)> {
        let visible = self
            .tools
            .iter()
            .map(|tool| tool.id)
            .filter(|id| self.tool_tab(*id).is_some())
            .collect::<Vec<_>>();
        for id in visible {
            self.hide_tool(id);
        }
        self.remove_document_tabs();
        debug_assert!(self.panes.is_empty(), "arranged workbenches start empty");
        self.panes.clear();
        self.focused_pane = None;
        let mut focused = None;
        let mut installed = Vec::new();
        let Arrangement { center, docks } = arrangement;
        self.center = AreaTree {
            layout: center
                .map(|root| self.install(root, Area::Center, &mut focused, &mut installed)),
            focused: None,
        };
        for (dock, arranged) in DockLocation::ALL.into_iter().zip(docks) {
            let area = Area::Dock(dock);
            let layout = arranged
                .root
                .map(|root| self.install(root, area, &mut focused, &mut installed));
            let state = &mut self.docks[dock.index()];
            state.tree = AreaTree {
                layout,
                focused: None,
            };
            state.visible = arranged.visible;
            self.set_dock_size(dock, arranged.size);
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
