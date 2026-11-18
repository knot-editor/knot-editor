use super::super::{KnotWindow, create_untitled_document};
use crate::app::{
    product::tests::{install_globals, product_window},
    workbench::{
        Area, DockLocation, SplitEdge, TabMoveDestination, TabMovePlacement, TabSurfaceId,
    },
};
use gpui::{
    AnyWindowHandle, AppContext, Entity, Focusable, Modifiers, MouseButton, TestAppContext,
    VisualTestContext, point, px,
};

fn fixture(cx: &mut TestAppContext) -> (Entity<KnotWindow>, AnyWindowHandle) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model.clone(), cx);
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, cx| {
            let pane = workbench.focused_pane_id().unwrap();
            for _ in 0..2 {
                workbench
                    .open_tab_for_document(pane, document, model.clone(), cx)
                    .unwrap();
            }
            cx.notify();
        });
        cx.notify();
    });
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_surface(cx));
    })
    .unwrap();
    cx.run_until_parked();
    (window, native_window)
}

pub(super) fn tab_selector(id: u64) -> &'static str {
    Box::leak(format!("tab-{id}").into_boxed_str())
}

pub(super) fn start_drag(cx: &mut VisualTestContext, id: u64) {
    let from = cx.debug_bounds(tab_selector(id)).unwrap().center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        from + point(px(12.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.read(|cx| assert!(cx.has_active_drag()));
}

#[gpui::test]
fn tab_bar_reorder_and_release_position_drive_product_transition(cx: &mut TestAppContext) {
    let (window, native_window) = fixture(cx);
    let before = cx.read(|cx| window.read(cx).workbench.read(cx).tab_snapshots());
    let mut visual = VisualTestContext::from_window(native_window, cx);
    start_drag(&mut visual, before[0].tab_id.value());
    let tab = visual
        .debug_bounds(tab_selector(before[2].tab_id.value()))
        .unwrap();
    let hover = tab.origin + point(px(1.), px(5.));
    visual.simulate_mouse_move(hover, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    window.read_with(&visual, |window, _| {
        let state = window.tab_drag.borrow();
        assert!(matches!(
            state.preview.unwrap().tab,
            TabMoveDestination::At {
                placement: TabMovePlacement::Before,
                ..
            }
        ));
        assert_eq!(state.preview.unwrap().bounds.size.width, px(2.));
    });
    // Release beyond the last midpoint appends, even without another move event.
    visual.simulate_mouse_up(
        point(tab.right() + px(20.), tab.center().y),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.run_until_parked();
    visual.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        assert_eq!(
            workbench.tab_snapshots(),
            vec![before[1], before[2], before[0]]
        );
        assert_eq!(
            workbench.focused_pane().unwrap().active_tab_id(),
            before[0].tab_id
        );
        assert!(
            cx.global::<super::ApplicationTabDrag>()
                .0
                .borrow()
                .active
                .is_none()
        );
        assert!(!cx.has_active_drag());
    });
}

#[gpui::test]
fn all_edges_split_then_center_move_collapses_source_and_focuses_editor(cx: &mut TestAppContext) {
    for edge in [
        SplitEdge::Left,
        SplitEdge::Right,
        SplitEdge::Top,
        SplitEdge::Bottom,
    ] {
        let (window, native_window) = fixture(cx);
        let before = cx.read(|cx| window.read(cx).workbench.read(cx).tab_snapshots());
        let mut visual = VisualTestContext::from_window(native_window, cx);
        start_drag(&mut visual, before[0].tab_id.value());
        let pane = visual.debug_bounds("pane-1").unwrap();
        let body_top = visual.debug_bounds("tab-bar-1").unwrap().bottom();
        let destination = match edge {
            SplitEdge::Left => point(pane.left() + px(5.), pane.center().y),
            SplitEdge::Right => point(pane.right() - px(5.), pane.center().y),
            SplitEdge::Top => point(pane.center().x, body_top + px(5.)),
            SplitEdge::Bottom => point(pane.center().x, pane.bottom() - px(5.)),
        };
        visual.simulate_mouse_move(destination, MouseButton::Left, Modifiers::default());
        visual.run_until_parked();
        visual.read(|cx| {
            let window = window.read(cx);
            let preview = window.tab_drag.borrow().preview.unwrap();
            assert!(matches!(preview.tab, TabMoveDestination::At { placement: TabMovePlacement::Split(actual), .. } if actual == edge));
        });
        visual.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
        visual.run_until_parked();
        let moved = visual.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            assert_eq!(workbench.panes().len(), 2);
            assert_eq!(workbench.tab_snapshots().len(), 3);
            workbench.focused_pane_id().unwrap()
        });
        assert_ne!(moved, before[0].pane_id);
        start_drag(&mut visual, before[0].tab_id.value());
        let center = visual.debug_bounds("pane-1").unwrap().center();
        visual.simulate_mouse_move(center, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
        visual.run_until_parked();
        let editor = visual.read(|cx| {
            let window = window.read(cx);
            let workbench = window.workbench.read(cx);
            assert_eq!(
                workbench
                    .panes()
                    .iter()
                    .map(|pane| pane.id())
                    .collect::<Vec<_>>(),
                vec![before[0].pane_id]
            );
            assert_eq!(
                workbench.tab_snapshots(),
                vec![before[1], before[2], before[0]]
            );
            workbench.focused_editor().unwrap().clone()
        });
        visual.update(|window, cx| assert!(editor.focus_handle(cx).is_focused(window)));
    }
}

#[gpui::test]
fn escape_and_stale_source_leave_model_unchanged(cx: &mut TestAppContext) {
    let (window, native_window) = fixture(cx);
    let before = cx.read(|cx| window.read(cx).workbench.read(cx).tab_snapshots());
    let mut visual = VisualTestContext::from_window(native_window, cx);
    start_drag(&mut visual, before[0].tab_id.value());
    let pane = visual.debug_bounds("pane-1").unwrap();
    visual.simulate_mouse_move(pane.center(), MouseButton::Left, Modifiers::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(pane.center(), MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    visual.read(|cx| {
        let window = window.read(cx);
        assert_eq!(window.workbench.read(cx).tab_snapshots(), before);
        assert!(
            cx.global::<super::ApplicationTabDrag>()
                .0
                .borrow()
                .active
                .is_none()
        );
        assert!(window.tab_drag.borrow().preview.is_none());
        assert!(!cx.has_active_drag());
    });
    start_drag(&mut visual, before[0].tab_id.value());
    // Replacing the workbench invalidates the captured source despite matching local IDs.
    window.update(&mut visual, |window, cx| {
        let old = window.workbench.read(cx);
        let document = old
            .focused_pane()
            .unwrap()
            .active_tab()
            .document_id()
            .unwrap();
        let model = KnotWindow::documents(cx)
            .read(cx)
            .get(document)
            .unwrap()
            .model()
            .clone();
        window.workbench =
            cx.new(|cx| crate::app::workbench::Workbench::new_for_document(document, model, cx));
        cx.notify();
    });
    visual.run_until_parked();
    let pane = visual.debug_bounds("pane-1").unwrap();
    visual.simulate_mouse_up(
        point(pane.left() + px(5.), pane.center().y),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.run_until_parked();
    visual.read(|cx| assert_eq!(window.read(cx).workbench.read(cx).tab_snapshots().len(), 1));
}

#[gpui::test]
fn terminal_drag_preserves_live_view_session_and_attachment(cx: &mut TestAppContext) {
    let (window, native_window) = fixture(cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            let pane = window.workbench.read(cx).focused_pane_id().unwrap();
            assert!(
                window
                    .new_terminal_in_pane(
                        Some(pane),
                        crate::app::product::operations::TerminalLocation::Center,
                        None,
                        cx
                    )
                    .is_some()
            );
        });
    })
    .unwrap();
    cx.run_until_parked();
    let (tab, view, session) = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        let tab = pane.active_tab();
        let TabSurfaceId::Terminal(id) = tab.surface_id() else {
            panic!("expected terminal")
        };
        (
            tab.id(),
            workbench.terminal_view(tab).unwrap().clone(),
            cx.global::<super::super::ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions[&id]
                .clone(),
        )
    });
    let mut visual = VisualTestContext::from_window(native_window, cx);
    start_drag(&mut visual, tab.value());
    let pane = visual.debug_bounds("pane-1").unwrap();
    let destination = point(pane.right() - px(5.), pane.center().y);
    visual.simulate_mouse_move(destination, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    visual.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.panes().len(), 2);
        assert_eq!(workbench.focused_terminal_view(), Some(&view));
        assert!(view.read(cx).is_attached_to(&session));
    });
    visual.update(|window, cx| assert!(view.focus_handle(cx).is_focused(window)));
}

fn tool_fixture(cx: &mut TestAppContext) -> (Entity<KnotWindow>, AnyWindowHandle) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    window.update(cx, |window, cx| {
        window.ensure_test_tool(false, cx);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_surface(cx));
    })
    .unwrap();
    (window, native_window)
}

/// The pane of the explorer test tool.
fn tool_pane(window: &Entity<KnotWindow>, cx: &TestAppContext) -> u64 {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let id = window.read(cx).test_tool_frame(cx).unwrap();
        workbench.tool_tab(id).unwrap().0.value()
    })
}

#[gpui::test]
fn a_terminal_drags_between_the_center_and_its_window_and_a_tool_has_no_tab_to_drag(
    cx: &mut TestAppContext,
) {
    let (window, native_window) = tool_fixture(cx);
    let terminal_window = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .tool_by_stable_id(crate::app::terminal::TERMINAL_TOOL_ID)
            .unwrap()
    });
    // A terminal in the center and one in the Terminal window.
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            let center = window.workbench.read(cx).center_pane().unwrap();
            window.new_terminal_in_pane(
                Some(center),
                crate::app::product::operations::TerminalLocation::Center,
                None,
                cx,
            );
            window.new_terminal_in_pane(
                None,
                crate::app::product::operations::TerminalLocation::Dock,
                None,
                cx,
            );
        });
    })
    .unwrap();
    cx.run_until_parked();
    let (center_tab, window_tab, window_pane) = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let center = workbench.center_pane().unwrap();
        let center_tab = workbench
            .pane(center)
            .unwrap()
            .tabs()
            .iter()
            .find(|tab| matches!(tab.surface_id(), TabSurfaceId::Terminal(_)))
            .unwrap()
            .id()
            .value();
        let pane = workbench.area_panes(Area::Tool(terminal_window))[0];
        let tab = workbench.pane(pane).unwrap().active_tab_id().value();
        (center_tab, tab, pane)
    });
    let mut visual = VisualTestContext::from_window(native_window, cx);

    // A fixed tool window shows no tab to drag.
    let tool_tab = cx.read(|cx| {
        let id = window.read(cx).test_tool_frame(cx).unwrap();
        window
            .read(cx)
            .workbench
            .read(cx)
            .tool_tab(id)
            .unwrap()
            .1
            .value()
    });
    assert!(visual.debug_bounds(tab_selector(tool_tab)).is_none());

    // Dropping the center terminal on the Terminal window's strip moves it in.
    start_drag(&mut visual, center_tab);
    let strip = visual
        .debug_bounds(Box::leak(
            format!("tab-bar-{}", window_pane.value()).into_boxed_str(),
        ))
        .unwrap();
    let target = point(strip.right() - px(60.), strip.center().y);
    visual.simulate_mouse_move(target, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    window.read_with(&visual, |window, _| {
        assert!(window.tab_drag.borrow().preview.is_some());
    });
    visual.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    visual.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.pane(window_pane).unwrap().tabs().len(), 2);
    });

    // And back: dropping the window's terminal on the center pane.
    start_drag(&mut visual, window_tab);
    let center = visual.debug_bounds("pane-1").unwrap();
    let target = center.center();
    visual.simulate_mouse_move(target, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    visual.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let centre = workbench.center_pane().unwrap();
        assert!(
            workbench
                .pane(centre)
                .unwrap()
                .tabs()
                .iter()
                .any(|tab| tab.id().value() == window_tab)
        );
    });
}

#[gpui::test]
fn the_dock_divider_is_the_gap_between_the_cards(cx: &mut TestAppContext) {
    let (_, native_window) = tool_fixture(cx);
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let dock = visual.debug_bounds("dock-0").unwrap();
    let divider = visual.debug_bounds("dock-divider-0").unwrap();
    assert_eq!(
        divider.size.width,
        px(crate::app::theme::Chrome::default().card_gap)
    );
    assert_eq!(divider.left(), dock.right());
    assert_eq!(dock.top(), divider.top());
    assert_eq!(dock.bottom(), divider.bottom());
}

#[gpui::test]
fn the_toolbar_row_sits_above_the_workbench_on_the_window_frame(cx: &mut TestAppContext) {
    let (_, native_window) = tool_fixture(cx);
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let dock = visual.debug_bounds("dock-0").unwrap();
    let toolbar = visual.debug_bounds("toolbar");
    assert_eq!(toolbar.is_some(), cfg!(target_os = "macos"));
    if let Some(toolbar) = toolbar {
        assert_eq!(
            toolbar.size.height,
            px(crate::app::tokens::chrome::TOOLBAR_HEIGHT)
        );
        assert!(dock.top() > toolbar.bottom());
    }
}

#[gpui::test]
fn dock_divider_drag_resizes_and_escape_reverts(cx: &mut TestAppContext) {
    let (window, native_window) = tool_fixture(cx);
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let divider = visual.debug_bounds("dock-divider-0").unwrap().center();
    visual.simulate_mouse_down(divider, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(
        divider + point(px(60.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.run_until_parked();
    visual.read(|cx| {
        assert_eq!(
            window
                .read(cx)
                .workbench
                .read(cx)
                .side_extent(DockLocation::Left)
                .unwrap(),
            300.
        );
    });
    visual.simulate_mouse_up(
        divider + point(px(60.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.run_until_parked();
    visual.read(|cx| {
        assert_eq!(
            window
                .read(cx)
                .workbench
                .read(cx)
                .side_extent(DockLocation::Left)
                .unwrap(),
            300.
        );
    });
    let explorer = visual.debug_bounds("pane-2").unwrap();
    // The dock's size includes its card border on both sides.
    let border = crate::app::theme::Chrome::default().card_border;
    assert_eq!(explorer.size.width, px(300. - 2. * border));

    // Escape reverts an in-progress resize.
    let divider = visual.debug_bounds("dock-divider-0").unwrap().center();
    visual.simulate_mouse_down(divider, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(
        divider + point(px(40.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(
        divider + point(px(40.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.run_until_parked();
    visual.read(|cx| {
        assert_eq!(
            window
                .read(cx)
                .workbench
                .read(cx)
                .side_extent(DockLocation::Left)
                .unwrap(),
            300.
        );
    });
}

#[gpui::test]
fn ordinary_divider_drag_changes_the_split_ratio_and_escape_reverts(cx: &mut TestAppContext) {
    let (window, native_window) = fixture(cx);
    let split = window.update(cx, |window, cx| {
        let split = window.workbench.update(cx, |workbench, cx| {
            let pane = workbench.focused_pane_id().unwrap();
            workbench
                .split_pane(
                    pane,
                    crate::app::workbench::SplitDirection::Horizontal,
                    crate::app::workbench::SplitPlacement::After,
                    cx,
                )
                .unwrap()
        });
        cx.notify();
        split
    });
    let mut visual = VisualTestContext::from_window(native_window, cx);
    visual.run_until_parked();
    let selector = format!("split-divider-{}", split.value());
    let selector: &'static str = Box::leak(selector.into_boxed_str());
    let left = visual.debug_bounds("pane-1").unwrap();
    let divider = visual.debug_bounds(selector).unwrap().center();
    let ratio = |visual: &mut VisualTestContext| {
        visual.read(|cx| {
            window
                .read(cx)
                .workbench
                .read(cx)
                .split_ratio(split)
                .unwrap()
        })
    };
    let shift = left.size.width / 5.;
    visual.simulate_mouse_down(divider, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(
        divider - point(shift, px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.simulate_mouse_up(
        divider - point(shift, px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.run_until_parked();
    let dragged = ratio(&mut visual);
    assert!(dragged < 0.5, "{dragged}");
    let resized = visual.debug_bounds("pane-1").unwrap();
    assert!(resized.size.width < left.size.width);

    // Escape restores the ratio the drag started from.
    let divider = visual.debug_bounds(selector).unwrap().center();
    visual.simulate_mouse_down(divider, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(
        divider + point(shift, px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    assert!(ratio(&mut visual) > dragged);
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(
        divider + point(shift, px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.run_until_parked();
    assert_eq!(ratio(&mut visual), dragged);
}

#[gpui::test]
fn document_drag_offers_no_tool_window_target(cx: &mut TestAppContext) {
    let (window, native_window) = tool_fixture(cx);
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let before = cx.read(|cx| window.read(cx).workbench.read(cx).tab_snapshots());
    let document_tab = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        workbench
            .pane(workbench.center_pane().unwrap())
            .unwrap()
            .tabs()[0]
            .id()
            .value()
    });
    // The explorer's pane offers no target and refuses the release.
    let explorer = visual
        .debug_bounds(Box::leak(
            format!("pane-{}", tool_pane(&window, cx)).into_boxed_str(),
        ))
        .unwrap();
    let target = explorer.center();
    start_drag(&mut visual, document_tab);
    visual.simulate_mouse_move(target, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    window.read_with(&visual, |window, _| {
        assert!(window.tab_drag.borrow().preview.is_none());
    });
    visual.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    visual.read(|cx| {
        assert_eq!(window.read(cx).workbench.read(cx).tab_snapshots(), before);
    });
}
