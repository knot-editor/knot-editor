//! Keyboard navigation between tabs and panes.
use super::*;
use crate::app::workbench::TabId;

/// A window whose one pane holds four views of one document, the last active.
fn four_tabs(cx: &mut TestAppContext) -> (Entity<KnotWindow>, gpui::AnyWindowHandle) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model.clone(), cx);
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, cx| {
            let pane = workbench.focused_pane_id().unwrap();
            for _ in 0..3 {
                workbench
                    .open_tab_for_document(pane, document, model.clone(), cx)
                    .unwrap();
            }
        });
    });
    cx.run_until_parked();
    (window, native_window)
}

fn tabs(window: &Entity<KnotWindow>, cx: &TestAppContext) -> Vec<TabId> {
    cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .tabs()
            .iter()
            .map(|tab| tab.id())
            .collect()
    })
}

fn active(window: &Entity<KnotWindow>, cx: &TestAppContext) -> TabId {
    cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab_id()
    })
}

fn recent(window: &Entity<KnotWindow>, cx: &TestAppContext) -> Vec<TabId> {
    cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .recent_tabs()
    })
}

fn run(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    name: &'static str,
    cx: &mut TestAppContext,
) -> CommandOutcome {
    let execution = dispatch_product_command(window, native_window, name, cx);
    wait_for_command(execution, cx)
}

#[gpui::test]
fn ordered_and_positional_tab_commands_wrap_and_select_the_last(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::{
        GO_TO_LAST_TAB_COMMAND, GO_TO_TAB_2_COMMAND, GO_TO_TAB_8_COMMAND, NEXT_TAB_COMMAND,
        PREVIOUS_TAB_COMMAND,
    };
    let (window, native_window) = four_tabs(cx);
    let order = tabs(&window, cx);
    assert_eq!(
        run(&window, native_window, NEXT_TAB_COMMAND, cx),
        CommandOutcome::Completed
    );
    assert_eq!(active(&window, cx), order[0]);
    run(&window, native_window, PREVIOUS_TAB_COMMAND, cx);
    assert_eq!(active(&window, cx), order[3]);
    run(&window, native_window, GO_TO_TAB_2_COMMAND, cx);
    assert_eq!(active(&window, cx), order[1]);
    assert_eq!(
        run(&window, native_window, GO_TO_TAB_8_COMMAND, cx),
        CommandOutcome::Unavailable
    );
    assert_eq!(active(&window, cx), order[1]);
    run(&window, native_window, GO_TO_LAST_TAB_COMMAND, cx);
    assert_eq!(active(&window, cx), order[3]);
}

#[gpui::test]
fn a_recent_tab_walk_commits_one_activation_when_ctrl_is_released(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::{NEXT_RECENT_TAB_COMMAND, PREVIOUS_RECENT_TAB_COMMAND};
    let (window, native_window) = four_tabs(cx);
    let order = tabs(&window, cx);
    // Activate tabs so the recent order differs from tab order.
    for index in [1, 0, 3] {
        window.update(cx, |window, cx| {
            window.workbench.update(cx, |workbench, _| {
                let pane = workbench.focused_pane_id().unwrap();
                workbench.activate_tab(pane, order[index]);
            })
        });
    }
    let before = recent(&window, cx);
    assert_eq!(before, [order[3], order[0], order[1], order[2]]);

    let mut visual = VisualTestContext::from_window(native_window, cx);
    visual.simulate_modifiers_change(Modifiers::control());
    run(&window, native_window, NEXT_RECENT_TAB_COMMAND, cx);
    run(&window, native_window, NEXT_RECENT_TAB_COMMAND, cx);
    run(&window, native_window, NEXT_RECENT_TAB_COMMAND, cx);
    run(&window, native_window, PREVIOUS_RECENT_TAB_COMMAND, cx);
    // The walk shows tabs without reordering them.
    assert_eq!(active(&window, cx), order[1]);
    assert_eq!(recent(&window, cx), before);

    let mut visual = VisualTestContext::from_window(native_window, cx);
    visual.simulate_modifiers_change(Modifiers::none());
    visual.run_until_parked();
    assert_eq!(active(&window, cx), order[1]);
    assert_eq!(
        recent(&window, cx),
        [order[1], order[3], order[0], order[2]]
    );

    // Without ctrl held, as from the palette, a step commits at once and
    // switches back to the previous tab.
    run(&window, native_window, NEXT_RECENT_TAB_COMMAND, cx);
    assert_eq!(active(&window, cx), order[3]);
    assert_eq!(recent(&window, cx)[0], order[3]);
}

fn pane_ids(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> Vec<crate::app::workbench::PaneId> {
    cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .panes()
            .iter()
            .map(|pane| pane.id())
            .collect()
    })
}

fn focused(window: &Entity<KnotWindow>, cx: &TestAppContext) -> crate::app::workbench::PaneId {
    cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane_id()
            .unwrap()
    })
}

fn focus(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    pane: crate::app::workbench::PaneId,
    cx: &mut TestAppContext,
) {
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            window
                .workbench
                .update(cx, |workbench, _| workbench.focus_pane(pane));
            window.focus_active_surface(cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
}

/// Whether the last frame painted `pane`.
fn painted(
    window: &Entity<KnotWindow>,
    pane: crate::app::workbench::PaneId,
    cx: &mut TestAppContext,
) -> bool {
    cx.run_until_parked();
    cx.read(|cx| {
        window
            .read(cx)
            .tab_drag
            .borrow()
            .painted_panes()
            .contains(&pane)
    })
}

/// A document pane with the explorer docked on its left and two tabs.
fn docked_window(cx: &mut TestAppContext) -> (Entity<KnotWindow>, gpui::AnyWindowHandle) {
    let (window, native_window) = four_tabs(cx);
    window.update(cx, |window, cx| {
        window.ensure_test_tool(true, cx);
        let pane = window.workbench.read(cx).center_pane().unwrap();
        window
            .workbench
            .update(cx, |workbench, _| workbench.focus_pane(pane));
        cx.notify();
    });
    cx.run_until_parked();
    (window, native_window)
}

fn dock_and_main(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> (crate::app::workbench::PaneId, crate::app::workbench::PaneId) {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let dock = workbench.area_panes(crate::app::workbench::Area::Dock(
            crate::app::workbench::DockLocation::Left,
        ))[0];
        (dock, workbench.center_pane().unwrap())
    })
}

#[gpui::test]
fn moving_tabs_by_direction_splits_at_edges_skips_docks_and_collapses_emptied_panes(
    cx: &mut TestAppContext,
) {
    use crate::app::commands::dispatch::{
        FOCUS_PANE_LEFT_COMMAND, FOCUS_PANE_RIGHT_COMMAND, MOVE_TAB_LEFT_COMMAND,
        MOVE_TAB_RIGHT_COMMAND,
    };
    let (window, native_window) = docked_window(cx);
    let (dock, main) = dock_and_main(&window, cx);
    focus(&window, native_window, main, cx);

    // No pane lies to the right, so the tab splits its pane on that side.
    let moved = active(&window, cx);
    assert_eq!(
        run(&window, native_window, MOVE_TAB_RIGHT_COMMAND, cx),
        CommandOutcome::Completed
    );
    let right = focused(&window, cx);
    assert_ne!(right, main);
    assert_eq!(tabs(&window, cx), [moved]);
    assert_eq!(pane_ids(&window, cx).len(), 3);

    // Moving the only tab back collapses the emptied pane.
    assert_eq!(
        run(&window, native_window, MOVE_TAB_LEFT_COMMAND, cx),
        CommandOutcome::Completed
    );
    assert_eq!(focused(&window, cx), main);
    assert_eq!(dock_and_main(&window, cx), (dock, main));
    assert_eq!(pane_ids(&window, cx).len(), 2);
    assert_eq!(tabs(&window, cx).last(), Some(&moved));

    // Focus and tabs move into the dock on the left.
    assert_eq!(
        run(&window, native_window, FOCUS_PANE_LEFT_COMMAND, cx),
        CommandOutcome::Completed
    );
    assert_eq!(focused(&window, cx), dock);
    assert_eq!(
        run(&window, native_window, FOCUS_PANE_LEFT_COMMAND, cx),
        CommandOutcome::Unavailable
    );
    run(&window, native_window, FOCUS_PANE_RIGHT_COMMAND, cx);
    assert_eq!(focused(&window, cx), main);
    // A document never enters the dock: moving it left splits its pane.
    run(&window, native_window, MOVE_TAB_LEFT_COMMAND, cx);
    let left = focused(&window, cx);
    assert_ne!(left, dock);
    assert_ne!(left, main);
    assert_eq!(tabs(&window, cx), [moved]);
    assert_eq!(dock_and_main(&window, cx).0, dock);
}

#[gpui::test]
fn maximize_ends_on_toggle_split_close_and_directional_focus(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::{
        CLOSE_TAB_COMMAND, FOCUS_PANE_LEFT_COMMAND, SPLIT_HORIZONTAL_COMMAND,
        TOGGLE_MAXIMIZE_PANE_COMMAND,
    };
    let (window, native_window) = docked_window(cx);
    let (dock, main) = dock_and_main(&window, cx);
    focus(&window, native_window, main, cx);
    let maximize = |cx: &mut TestAppContext| {
        assert_eq!(
            run(&window, native_window, TOGGLE_MAXIMIZE_PANE_COMMAND, cx),
            CommandOutcome::Completed
        );
    };

    maximize(cx);
    assert!(painted(&window, main, cx));
    assert!(!painted(&window, dock, cx));
    maximize(cx);
    assert!(painted(&window, dock, cx));

    // Splitting focuses the new pane, which ends the maximized state.
    maximize(cx);
    run(&window, native_window, SPLIT_HORIZONTAL_COMMAND, cx);
    let split = focused(&window, cx);
    assert!(painted(&window, dock, cx));
    assert!(painted(&window, main, cx));

    // Closing the maximized pane restores the rest.
    maximize(cx);
    assert!(!painted(&window, main, cx));
    run(&window, native_window, CLOSE_TAB_COMMAND, cx);
    assert!(!pane_ids(&window, cx).contains(&split));
    assert!(painted(&window, main, cx));
    assert!(painted(&window, dock, cx));

    // Moving focus by direction restores the layout too.
    focus(&window, native_window, main, cx);
    maximize(cx);
    assert!(!painted(&window, dock, cx));
    run(&window, native_window, FOCUS_PANE_LEFT_COMMAND, cx);
    assert_eq!(focused(&window, cx), dock);
    assert!(painted(&window, main, cx));
    assert!(painted(&window, dock, cx));
}

#[gpui::test]
fn pane_cycling_stays_in_the_focused_area(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::{
        FOCUS_FIRST_PANE_COMMAND, FOCUS_LAST_PANE_COMMAND, FOCUS_NEXT_PANE_COMMAND,
        FOCUS_PANE_LEFT_COMMAND, FOCUS_PREVIOUS_FOCUSED_PANE_COMMAND, FOCUS_PREVIOUS_PANE_COMMAND,
        SPLIT_HORIZONTAL_COMMAND,
    };
    let (window, native_window) = docked_window(cx);
    let (dock, main) = dock_and_main(&window, cx);
    focus(&window, native_window, main, cx);
    run(&window, native_window, SPLIT_HORIZONTAL_COMMAND, cx);
    let split = focused(&window, cx);

    // The walk wraps over the center without entering the dock.
    run(&window, native_window, FOCUS_NEXT_PANE_COMMAND, cx);
    assert_eq!(focused(&window, cx), main);
    run(&window, native_window, FOCUS_NEXT_PANE_COMMAND, cx);
    assert_eq!(focused(&window, cx), split);
    run(&window, native_window, FOCUS_PREVIOUS_PANE_COMMAND, cx);
    assert_eq!(focused(&window, cx), main);
    run(&window, native_window, FOCUS_LAST_PANE_COMMAND, cx);
    assert_eq!(focused(&window, cx), split);
    run(&window, native_window, FOCUS_FIRST_PANE_COMMAND, cx);
    assert_eq!(focused(&window, cx), main);

    // Directional focus reaches the dock, a walk stays in it, and the
    // previously focused pane goes back to the center.
    run(&window, native_window, FOCUS_PANE_LEFT_COMMAND, cx);
    assert_eq!(focused(&window, cx), dock);
    run(&window, native_window, FOCUS_NEXT_PANE_COMMAND, cx);
    assert_eq!(focused(&window, cx), dock);
    run(&window, native_window, FOCUS_LAST_PANE_COMMAND, cx);
    assert_eq!(focused(&window, cx), dock);
    run(
        &window,
        native_window,
        FOCUS_PREVIOUS_FOCUSED_PANE_COMMAND,
        cx,
    );
    assert_eq!(focused(&window, cx), main);
}

#[gpui::test]
fn a_full_width_bottom_dock_runs_under_the_side_docks(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::FOCUS_PANE_DOWN_COMMAND;
    use crate::app::workbench::{DockLocation, ToolContent};
    let (window, native_window) = docked_window(cx);
    let (dock, _) = dock_and_main(&window, cx);
    let bottom = window.update(cx, |window, cx| {
        let tree = cx.new(|cx| crate::app::tree_view::TreeView::new("test.bottom", "1", cx));
        let bottom = window.workbench.update(cx, |workbench, _| {
            let id = workbench.register_tool(
                "test.bottom",
                "Bottom",
                ToolContent::Extension(crate::app::workbench::ExtensionView::Tree(tree)),
                DockLocation::Bottom,
            );
            workbench.show_tool(id).unwrap().0
        });
        cx.notify();
        bottom
    });
    cx.run_until_parked();
    let bounds = |cx: &mut TestAppContext| {
        let mut visual = VisualTestContext::from_window(native_window, cx);
        let side = visual.debug_bounds("dock-0").unwrap();
        let bottom = visual.debug_bounds("dock-2").unwrap();
        (side, bottom)
    };

    // Contained: the side dock spans the height and nothing lies below it.
    let (side, under) = bounds(cx);
    assert!(side.bottom() > under.top(), "{side:?} {under:?}");
    assert!(under.left() >= side.right());
    focus(&window, native_window, dock, cx);
    assert_eq!(
        run(&window, native_window, FOCUS_PANE_DOWN_COMMAND, cx),
        CommandOutcome::Unavailable
    );

    // Full: the side dock stops above the bottom dock, which spans the width.
    cx.update(|cx| {
        cx.default_global::<crate::app::settings::ApplicationSettings>()
            .0
            .set(
                crate::app::settings::WORKBENCH_BOTTOM_DOCK_LAYOUT,
                "full".into(),
                Default::default(),
            )
            .unwrap();
    });
    window.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    let (side, under) = bounds(cx);
    assert!(side.bottom() <= under.top(), "{side:?} {under:?}");
    assert!(under.left() <= side.left());
    assert_eq!(
        run(&window, native_window, FOCUS_PANE_DOWN_COMMAND, cx),
        CommandOutcome::Completed
    );
    assert_eq!(focused(&window, cx), bottom);
}

#[gpui::test]
fn hiding_a_dock_keeps_its_maximized_pane_for_the_next_show(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::{TOGGLE_LEFT_DOCK_COMMAND, TOGGLE_MAXIMIZE_PANE_COMMAND};
    let (window, native_window) = docked_window(cx);
    let (dock, main) = dock_and_main(&window, cx);
    focus(&window, native_window, dock, cx);
    run(&window, native_window, TOGGLE_MAXIMIZE_PANE_COMMAND, cx);
    assert!(painted(&window, dock, cx));
    assert!(!painted(&window, main, cx));

    // Hiding the focused dock returns focus to the center.
    run(&window, native_window, TOGGLE_LEFT_DOCK_COMMAND, cx);
    assert_eq!(focused(&window, cx), main);
    assert!(painted(&window, main, cx));

    // Showing it focuses the dock pane and maximizes it again.
    run(&window, native_window, TOGGLE_LEFT_DOCK_COMMAND, cx);
    assert_eq!(focused(&window, cx), dock);
    assert!(painted(&window, dock, cx));
    assert!(!painted(&window, main, cx));

    // The memory is spent: hide, show, and a plain focus change restore nothing.
    run(&window, native_window, TOGGLE_MAXIMIZE_PANE_COMMAND, cx);
    run(&window, native_window, TOGGLE_LEFT_DOCK_COMMAND, cx);
    run(&window, native_window, TOGGLE_LEFT_DOCK_COMMAND, cx);
    assert!(painted(&window, main, cx));
    assert!(painted(&window, dock, cx));
}
