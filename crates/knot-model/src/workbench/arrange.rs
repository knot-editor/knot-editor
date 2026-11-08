//! Installing a restored arrangement into a fresh workbench.

use super::*;

/// A pruned arrangement whose tabs are materialized per kind before the
/// workbench installs it.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanNode<T> {
    Pane {
        dock: Option<DockLocation>,
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
                dock,
                selected,
                focused,
                pinned,
                tabs,
            } => PlanNode::Pane {
                dock,
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
}

impl Workbench {
    /// Install one restored tab at `index` of `pane`, or at the end of the
    /// normal group when that pane is gone, and activate it.
    pub fn insert_arranged(
        &mut self,
        pane: PaneId,
        index: usize,
        surface: TabSurfaceId,
        pinned: bool,
    ) -> (PaneId, TabId) {
        let (pane, index) = if self.pane(pane).is_some() {
            (pane, index)
        } else {
            let pane = self
                .normal_pane()
                .unwrap_or_else(|| self.create_group(None));
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
        self.focused_pane = Some(pane);
        self.debug_assert_invariants();
        (pane, id)
    }

    /// Replace every tab with a restored arrangement. Visible tools are
    /// hidden first, keeping their remembered placement, so tools the
    /// arrangement does not name stay hidden. Each arranged tab carries a value
    /// the caller gets back with the tab's presentation, in plan order. The
    /// focused pane is the marked one, else the first ordinary group, else the
    /// first pane.
    pub fn arrange<T>(
        &mut self,
        root: Option<PlanNode<(TabSurfaceId, T)>>,
        dock_sizes: [f32; 3],
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
        self.docks.clear();
        let mut focused = None;
        let mut installed = Vec::new();
        self.layout = root.map(|root| self.install(root, &mut focused, &mut installed));
        self.focused_pane = focused.or_else(|| {
            self.panes
                .iter()
                .find(|pane| self.dock_location(pane.id).is_none())
                .or(self.panes.first())
                .map(|pane| pane.id)
        });
        self.set_dock_sizes(dock_sizes);
        installed
    }

    fn install<T>(
        &mut self,
        node: PlanNode<(TabSurfaceId, T)>,
        focused: &mut Option<PaneId>,
        installed: &mut Vec<(PresentationId, T)>,
    ) -> WorkbenchLayout {
        match node {
            PlanNode::Pane {
                dock,
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
                self.panes.push(Pane::new(pane, tabs, active_tab));
                if let Some(dock) = dock {
                    self.docks.push((pane, dock));
                }
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
                first: Box::new(self.install(*first, focused, installed)),
                second: Box::new(self.install(*second, focused, installed)),
            },
        }
    }
}
