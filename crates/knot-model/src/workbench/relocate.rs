//! Atomic relocation of one tab without document or session closure.

use super::{
    Area, DockLocation, Pane, PaneId, PresentationId, SplitDirection, SplitPlacement, TabId,
    TabKind, Workbench, WorkbenchLayout, WorkbenchTab, WorkbenchTabSnapshot,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabMoveCapture {
    pub tab: WorkbenchTabSnapshot,
    pub presentation: PresentationId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitEdge {
    Left,
    Right,
    Top,
    Bottom,
}

impl SplitEdge {
    fn split(self) -> (SplitDirection, SplitPlacement) {
        match self {
            Self::Left => (SplitDirection::Horizontal, SplitPlacement::Before),
            Self::Right => (SplitDirection::Horizontal, SplitPlacement::After),
            Self::Top => (SplitDirection::Vertical, SplitPlacement::Before),
            Self::Bottom => (SplitDirection::Vertical, SplitPlacement::After),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabMovePlacement {
    /// Insert immediately before the captured destination tab.
    Before,
    /// Append to the captured tab's pane.
    End,
    Split(SplitEdge),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabMoveDestination {
    At {
        anchor: TabMoveCapture,
        placement: TabMovePlacement,
    },
    /// Move to the end of a dock's focused pane, creating the dock's first
    /// pane when it has none. Produced by workbench-edge drops; pane-edge
    /// splits keep using `At` with `Split`.
    Dock(DockLocation),
    /// Install the first pane of an empty center.
    Empty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabMoveTransition {
    pub destination: WorkbenchTabSnapshot,
    pub removed_pane: Option<PaneId>,
}

struct PreparedDestination {
    area: Area,
    pane: PaneId,
    tab: TabId,
    index: usize,
    new_pane: bool,
    new_tab: bool,
    layout: WorkbenchLayout,
}

impl Workbench {
    pub fn capture_tab_move(&self, pane: PaneId, tab: TabId) -> Option<TabMoveCapture> {
        let tab = self
            .pane(pane)?
            .tabs
            .iter()
            .find(|candidate| candidate.id == tab)?;
        let presentation = tab.presentation;
        Some(TabMoveCapture {
            tab: WorkbenchTabSnapshot {
                pane_id: pane,
                tab_id: tab.id,
                surface: tab.surface_id(),
            },
            presentation,
        })
    }

    pub fn contains_tab_move(&self, captured: TabMoveCapture) -> bool {
        self.capture_tab_move(captured.tab.pane_id, captured.tab.tab_id) == Some(captured)
    }

    /// The area a destination puts a tab in, while the destination is live.
    fn destination_area(&self, target: TabMoveDestination) -> Option<Area> {
        match target {
            TabMoveDestination::Empty => self.center.layout.is_none().then_some(Area::Center),
            TabMoveDestination::Dock(dock) => Some(Area::Dock(dock)),
            TabMoveDestination::At { anchor, .. } => self
                .contains_tab_move(anchor)
                .then(|| self.pane_area(anchor.tab.pane_id))
                .flatten(),
        }
    }

    /// Whether a tab of `kind` may move to `target`.
    pub fn accepts_tab_move(&self, kind: TabKind, target: TabMoveDestination) -> bool {
        self.destination_area(target)
            .is_some_and(|area| kind.allows(area))
    }

    fn prepare_destination(
        &self,
        kind: TabKind,
        target: TabMoveDestination,
        reused_tab: Option<TabId>,
    ) -> Option<PreparedDestination> {
        if !self.accepts_tab_move(kind, target) {
            return None;
        }
        let area = self.destination_area(target)?;
        let tab = reused_tab.unwrap_or(TabId(self.next_tab_id));
        let fresh = || {
            let pane = PaneId(self.next_pane_id);
            (pane, 0, true, WorkbenchLayout::Pane(pane))
        };
        let (pane, index, new_pane, layout) = match target {
            TabMoveDestination::Empty => fresh(),
            TabMoveDestination::Dock(_) => match self.area_focus(area) {
                Some(pane) => (
                    pane,
                    self.pane(pane)?.tabs.len(),
                    false,
                    self.tree(area).layout.clone()?,
                ),
                None => fresh(),
            },
            TabMoveDestination::At { anchor, placement } => {
                let pane = self.pane(anchor.tab.pane_id)?;
                let mut layout = self.tree(area).layout.clone()?;
                match placement {
                    TabMovePlacement::Before => (
                        pane.id,
                        pane.tabs
                            .iter()
                            .position(|tab| tab.id == anchor.tab.tab_id)?,
                        false,
                        layout,
                    ),
                    TabMovePlacement::End => (pane.id, pane.tabs.len(), false, layout),
                    TabMovePlacement::Split(edge) => {
                        let new_pane = PaneId(self.next_pane_id);
                        let (direction, placement) = edge.split();
                        assert!(layout.split_pane(pane.id, new_pane, direction, placement));
                        (new_pane, 0, true, layout)
                    }
                }
            }
        };
        Some(PreparedDestination {
            area,
            pane,
            tab,
            index,
            new_pane,
            new_tab: reused_tab.is_none(),
            layout,
        })
    }

    fn install_moved_tab(
        &mut self,
        mut tab: WorkbenchTab,
        prepared: PreparedDestination,
    ) -> WorkbenchTabSnapshot {
        tab.id = prepared.tab;
        tab.preview = false;
        if prepared.new_tab {
            tab.presentation = self.allocate_presentation();
        }
        let snapshot = WorkbenchTabSnapshot {
            pane_id: prepared.pane,
            tab_id: tab.id,
            surface: tab.surface_id(),
        };
        if prepared.new_tab {
            assert_eq!(self.allocate_tab_id(), tab.id);
        }
        if prepared.new_pane {
            assert_eq!(self.allocate_pane_id(), prepared.pane);
            self.panes.push(Pane::new(
                prepared.pane,
                prepared.area,
                vec![tab],
                snapshot.tab_id,
            ));
        } else {
            let pane = self
                .panes
                .iter_mut()
                .find(|pane| pane.id == prepared.pane)
                .expect("validated destination pane");
            let index = pane.section_index(tab.pinned, prepared.index);
            pane.tabs.insert(index, tab);
            pane.select(snapshot.tab_id);
        }
        self.tree_mut(prepared.area).layout = Some(prepared.layout);
        self.set_focus(prepared.pane);
        self.debug_assert_invariants();
        snapshot
    }

    pub fn relocate_tab(
        &mut self,
        source: TabMoveCapture,
        target: TabMoveDestination,
    ) -> Option<TabMoveTransition> {
        if !self.contains_tab_move(source) {
            return None;
        }
        let kind = source.tab.surface.kind();
        let mut prepared = self.prepare_destination(kind, target, Some(source.tab.tab_id))?;
        if prepared.pane == source.tab.pane_id {
            let pane = self
                .panes
                .iter_mut()
                .find(|pane| pane.id == prepared.pane)?;
            let index = pane
                .tabs
                .iter()
                .position(|tab| tab.id == source.tab.tab_id)?;
            let tab = pane.tabs.remove(index);
            if index < prepared.index {
                prepared.index -= 1;
            }
            let destination = self.install_moved_tab(tab, prepared);
            return Some(TabMoveTransition {
                destination,
                removed_pane: None,
            });
        }
        if self.pane(source.tab.pane_id)?.tabs.len() == 1
            && self.pane_area(source.tab.pane_id) == Some(prepared.area)
        {
            prepared.layout = prepared
                .layout
                .remove_pane(source.tab.pane_id)
                .expect("destination survives source pane collapse");
        }
        let removed = self
            .remove_tab(source.tab.pane_id, source.tab.tab_id)
            .expect("validated source tab");
        let destination = self.install_moved_tab(removed.tab, prepared);
        Some(TabMoveTransition {
            destination,
            removed_pane: removed.removed_pane,
        })
    }

    /// Move a document or terminal tab into another workbench, where it
    /// becomes a fresh tab and presentation. Both layouts are validated before
    /// anything changes.
    pub fn transfer_tab_to(
        &mut self,
        destination: &mut Workbench,
        source: TabMoveCapture,
        target: TabMoveDestination,
    ) -> Option<TabMoveTransition> {
        if !self.contains_tab_move(source)
            || matches!(source.tab.surface, super::TabSurfaceId::Tool(_))
        {
            return None;
        }
        if let super::TabSurfaceId::Terminal(session) = source.tab.surface
            && destination.has_terminal_session(session)
        {
            return None;
        }
        let prepared = destination.prepare_destination(source.tab.surface.kind(), target, None)?;
        let removed = self
            .remove_tab(source.tab.pane_id, source.tab.tab_id)
            .expect("validated source tab");
        let snapshot = destination.install_moved_tab(removed.tab, prepared);
        self.debug_assert_invariants();
        Some(TabMoveTransition {
            destination: snapshot,
            removed_pane: removed.removed_pane,
        })
    }
}

#[cfg(test)]
mod tests;
