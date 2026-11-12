//! Keyboard navigation between tabs and panes, and the maximized pane.
//!
//! A recent-tab walk shows tabs in the pane's recently used order without
//! changing that order. Releasing ctrl commits the walk as one activation of
//! the tab it ended on; a walk started without ctrl held commits at once.
//!
//! Directional commands find a neighbor from pane rectangles laid out by
//! split ratios and dock sizes over the painted workbench, so they work while
//! a pane is maximized. Maximize is window-local render state that lasts only
//! while its pane stays focused, except that a dock remembers its maximized
//! pane while hidden.

use super::relocate::TabRelocationDestination;
use super::*;
use crate::app::window_layout::Rect;
use crate::app::workbench::{SplitEdge, TabMoveDestination, TabMovePlacement};
use knot_model::workbench::RecentWalk;
use knot_protocol::CommandOutcome;

impl KnotWindow {
    /// Activate the next or previous tab of a pane in tab order, wrapping.
    pub(super) fn cycle_tab(
        &mut self,
        pane: PaneId,
        forward: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        let tab = self
            .workbench
            .read(cx)
            .adjacent_tab(pane, forward)
            .ok_or(CommandOutcome::InvalidTarget)?;
        self.activate_tab(pane, tab, cx);
        Ok(())
    }

    /// Activate a pane's tab by position, or its last tab for `None`.
    pub(super) fn go_to_tab(
        &mut self,
        pane: PaneId,
        index: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        let tabs = self
            .workbench
            .read(cx)
            .pane(pane)
            .ok_or(CommandOutcome::InvalidTarget)?
            .tabs();
        let tab = match index {
            Some(index) => tabs.get(index),
            None => tabs.last(),
        }
        .map(|tab| tab.id())
        .ok_or(CommandOutcome::Unavailable)?;
        self.activate_tab(pane, tab, cx);
        Ok(())
    }

    /// Step a recent-tab walk of the pane forward or back, starting one.
    pub(super) fn walk_recent_tabs(
        &mut self,
        pane: PaneId,
        forward: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        if self
            .recent_walk
            .as_ref()
            .is_some_and(|walk| walk.pane() != pane)
        {
            self.commit_recent_walk(cx);
        }
        let order = self
            .workbench
            .read(cx)
            .pane(pane)
            .ok_or(CommandOutcome::InvalidTarget)?
            .recent_tabs();
        if order.len() < 2 {
            return Err(CommandOutcome::Unavailable);
        }
        let tab = self
            .recent_walk
            .get_or_insert_with(|| RecentWalk::new(pane, order))
            .step(forward);
        let shown = self
            .workbench
            .update(cx, |workbench, _| workbench.show_tab(pane, tab, false));
        if !shown {
            self.recent_walk = None;
            return Err(CommandOutcome::InvalidTarget);
        }
        self.focus_active_surface(cx);
        if !self.modifiers(cx).control {
            self.commit_recent_walk(cx);
        }
        cx.notify();
        Ok(())
    }

    /// End a recent-tab walk, recording the tab it ended on as activated.
    pub(super) fn commit_recent_walk(&mut self, cx: &mut Context<Self>) {
        let Some(walk) = self.recent_walk.take() else {
            return;
        };
        let (walk_pane, tab) = (walk.pane(), walk.current());
        self.workbench.update(cx, |workbench, _| {
            let pane = workbench.pane(walk_pane);
            if pane.is_some_and(|pane| pane.active_tab_id() == tab) {
                workbench.activate_tab(walk_pane, tab);
            }
        });
    }

    pub(super) fn modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.modifiers.control {
            self.commit_recent_walk(cx);
        }
    }
}

impl KnotWindow {
    /// The pane beside `pane` toward `edge`, among panes `accepts` admits.
    fn neighbor(
        &self,
        pane: PaneId,
        edge: SplitEdge,
        accepts: impl Fn(Area) -> bool,
        cx: &App,
    ) -> Option<PaneId> {
        let area = self
            .tab_drag
            .borrow()
            .workbench_bounds()
            .map(|bounds| Rect {
                x: 0.,
                y: 0.,
                width: f32::from(bounds.size.width),
                height: f32::from(bounds.size.height),
            })
            .unwrap_or(Rect {
                x: 0.,
                y: 0.,
                width: 1200.,
                height: 800.,
            });
        let workbench = self.workbench.read(cx);
        let mut rects = workbench.pane_rects(area, crate::app::settings::bottom_dock_layout(cx));
        rects.retain(|(candidate, _)| {
            *candidate == pane || workbench.pane_area(*candidate).is_some_and(&accepts)
        });
        crate::app::workbench::neighbor(&rects, pane, edge)
    }

    /// Focus the pane beside `pane` toward `edge`, in its area when
    /// `within_area`, restoring the layout.
    pub(super) fn focus_pane_toward(
        &mut self,
        pane: PaneId,
        edge: SplitEdge,
        within_area: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        self.maximized = None;
        cx.notify();
        let own = self.workbench.read(cx).pane_area(pane);
        let neighbor = self
            .neighbor(pane, edge, |area| !within_area || Some(area) == own, cx)
            .ok_or(CommandOutcome::Unavailable)?;
        self.workbench
            .update(cx, |workbench, _| workbench.focus_pane(neighbor));
        self.focus_active_surface(cx);
        Ok(())
    }

    /// Focus the pane `choose` picks, restoring the layout.
    pub(super) fn focus_chosen_pane(
        &mut self,
        choose: impl FnOnce(&Workbench) -> Option<PaneId>,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        self.maximized = None;
        cx.notify();
        let pane = choose(self.workbench.read(cx)).ok_or(CommandOutcome::Unavailable)?;
        self.workbench
            .update(cx, |workbench, _| workbench.focus_pane(pane));
        self.focus_active_surface(cx);
        Ok(())
    }

    /// Move a tab into the pane beside its own toward `edge` whose area
    /// allows the tab's kind, or split its pane on that side when no such
    /// pane lies there.
    pub(super) fn move_tab_toward(
        &mut self,
        pane: PaneId,
        tab: TabId,
        edge: SplitEdge,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        self.maximized = None;
        cx.notify();
        let source = self
            .capture_tab_relocation(pane, tab, cx)
            .ok_or(CommandOutcome::InvalidTarget)?;
        let kind = source.tab.tab.surface.kind();
        let workbench = self.workbench.read(cx);
        let target = match self.neighbor(pane, edge, |area| kind.allows(area), cx) {
            Some(neighbor) => TabMoveDestination::At {
                anchor: workbench
                    .capture_tab_move(neighbor, workbench.pane(neighbor).unwrap().active_tab_id())
                    .ok_or(CommandOutcome::InvalidTarget)?,
                placement: TabMovePlacement::End,
            },
            None if workbench
                .pane(pane)
                .is_some_and(|pane| pane.tabs().len() > 1) =>
            {
                TabMoveDestination::At {
                    anchor: source.tab,
                    placement: TabMovePlacement::Split(edge),
                }
            }
            None => return Err(CommandOutcome::Unavailable),
        };
        let destination = TabRelocationDestination {
            window: cx.weak_entity(),
            workbench: self.workbench.downgrade(),
            tab: target,
        };
        match self.relocate_tab_to(&source, &destination, cx) {
            CommandOutcome::Completed => Ok(()),
            outcome => Err(outcome),
        }
    }

    /// Render only `pane`, or the whole layout again.
    pub(super) fn toggle_maximize(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        self.maximized = if self.maximized == Some(pane) {
            None
        } else {
            Some(pane)
        };
        cx.notify();
    }

    /// The maximized pane while it is still focused; anything else ends it.
    /// Hiding a dock remembers its maximized pane, and showing the dock with
    /// that pane focused maximizes it again.
    pub(super) fn maximized_pane(&mut self, cx: &App) -> Option<PaneId> {
        let workbench = self.workbench.read(cx);
        if let Some(pane) = self.maximized
            && let Some(Area::Dock(dock)) = workbench.pane_area(pane)
            && !workbench.area_shown(Area::Dock(dock))
        {
            self.hidden_maximized[dock.index()] = Some(pane);
            self.maximized = None;
        }
        let focused = workbench.focused_pane_id();
        for dock in DockLocation::ALL {
            if workbench.area_shown(Area::Dock(dock))
                && let Some(pane) = self.hidden_maximized[dock.index()].take()
                && focused == Some(pane)
            {
                self.maximized = Some(pane);
            }
        }
        self.maximized = self.maximized.filter(|pane| focused == Some(*pane));
        self.maximized
    }
}
