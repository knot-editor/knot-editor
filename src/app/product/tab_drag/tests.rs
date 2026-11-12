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
    match id {
        1 => "tab-1",
        2 => "tab-2",
        3 => "tab-3",
        4 => "tab-4",
        _ => panic!("unexpected fixture tab"),
    }
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

fn tool_tab_id(window: &Entity<KnotWindow>, cx: &TestAppContext) -> u64 {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let id = window.read(cx).test_tool_frame(cx).unwrap();
        workbench.tool_tab(id).unwrap().1.value()
    })
}

fn tool_presentation(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> crate::app::workbench::PresentationId {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let id = window.read(cx).test_tool_frame(cx).unwrap();
        workbench.tool(id).unwrap().presentation
    })
}

fn assert_tool_focused(window: &Entity<KnotWindow>, visual: &mut VisualTestContext) {
    let tree = visual.read(|cx| window.read(cx).test_tool_tree(cx));
    visual.update(|window, cx| assert!(tree.focus_handle(cx).is_focused(window)));
}

#[gpui::test]
fn tool_drag_moves_between_docks_and_never_enters_the_center(cx: &mut TestAppContext) {
    let (window, native_window) = tool_fixture(cx);
    let presentation = tool_presentation(&window, cx);
    let mut visual = VisualTestContext::from_window(native_window, cx);

    // Escape cancels without moving.
    let before = cx.read(|cx| window.read(cx).workbench.read(cx).tab_snapshots());
    start_drag(&mut visual, tool_tab_id(&window, cx));
    let center = visual.debug_bounds("pane-1").unwrap().center();
    visual.simulate_mouse_move(center, MouseButton::Left, Modifiers::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    visual.read(|cx| {
        assert_eq!(window.read(cx).workbench.read(cx).tab_snapshots(), before);
        assert!(window.read(cx).tab_drag.borrow().preview.is_none());
    });

    // The center's document pane offers the tool no target.
    start_drag(&mut visual, tool_tab_id(&window, cx));
    visual.simulate_mouse_move(center, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    window.read_with(&visual, |window, _| {
        assert!(window.tab_drag.borrow().preview.is_none());
    });
    visual.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    visual.read(|cx| {
        assert_eq!(window.read(cx).workbench.read(cx).tab_snapshots(), before);
    });

    // Workbench edges move the same frame between docks.
    for (dock, at) in [
        (DockLocation::Right, (1., 0.5)),
        (DockLocation::Bottom, (0.5, 1.)),
        (DockLocation::Left, (0., 0.5)),
    ] {
        start_drag(&mut visual, tool_tab_id(&window, cx));
        let bounds = window.read_with(&visual, |window, _| {
            window.tab_drag.borrow().workbench_bounds().unwrap()
        });
        let target = point(
            (bounds.left() + bounds.size.width * at.0)
                .clamp(bounds.left() + px(2.), bounds.right() - px(2.)),
            (bounds.top() + bounds.size.height * at.1)
                .clamp(bounds.top() + px(2.), bounds.bottom() - px(2.)),
        );
        visual.simulate_mouse_move(target, MouseButton::Left, Modifiers::default());
        visual.run_until_parked();
        window.read_with(&visual, |window, _| {
            assert_eq!(
                window.tab_drag.borrow().preview.unwrap().tab,
                TabMoveDestination::Area(Area::Dock(dock))
            );
        });
        visual.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
        visual.run_until_parked();
        visual.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            let id = window.read(cx).test_tool_frame(cx).unwrap();
            assert_eq!(
                workbench.dock_location(workbench.tool_tab(id).unwrap().0),
                Some(dock)
            );
            assert_eq!(workbench.tool(id).unwrap().presentation, presentation);
        });
        assert_tool_focused(&window, &mut visual);
    }

    // Invalid release above the pane keeps the model unchanged.
    let before = cx.read(|cx| window.read(cx).workbench.read(cx).tab_snapshots());
    start_drag(&mut visual, tool_tab_id(&window, cx));
    let pane = visual.debug_bounds("pane-1").unwrap();
    let invalid = point(pane.center().x, pane.top() - px(5.));
    visual.simulate_mouse_move(invalid, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(invalid, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    visual.read(|cx| {
        assert_eq!(window.read(cx).workbench.read(cx).tab_snapshots(), before);
    });
}

#[gpui::test]
fn hiding_tool_mid_drag_cancels_drop(cx: &mut TestAppContext) {
    let (window, native_window) = tool_fixture(cx);
    let before = cx.read(|cx| window.read(cx).workbench.read(cx).tab_snapshots());
    let mut visual = VisualTestContext::from_window(native_window, cx);
    start_drag(&mut visual, tool_tab_id(&window, cx));
    window.update(&mut visual, |window, cx| {
        let id = window.test_tool_frame(cx).unwrap();
        window.workbench.update(cx, |workbench, _| {
            workbench.hide_tool(id);
        });
        cx.notify();
    });
    visual.run_until_parked();
    let pane = visual.debug_bounds("pane-1").unwrap();
    visual.simulate_mouse_up(pane.center(), MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    visual.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let id = window.read(cx).test_tool_frame(cx).unwrap();
        assert!(workbench.tool_tab(id).is_none());
        assert_eq!(workbench.tab_snapshots().len(), before.len() - 1);
    });
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
                .dock_size(DockLocation::Left),
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
                .dock_size(DockLocation::Left),
            300.
        );
    });
    let explorer = visual.debug_bounds("pane-2").unwrap();
    assert_eq!(explorer.size.width, px(300.));

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
                .dock_size(DockLocation::Left),
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
fn document_drag_offers_no_dock_or_dock_pane_target(cx: &mut TestAppContext) {
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
    // Workbench edges keep the pane-local split, never a dock.
    let bounds = window.read_with(&visual, |window, _| {
        window.tab_drag.borrow().workbench_bounds().unwrap()
    });
    for target in [
        point(bounds.right() - px(2.), bounds.center().y),
        point(bounds.center().x, bounds.bottom() - px(2.)),
    ] {
        start_drag(&mut visual, document_tab);
        visual.simulate_mouse_move(target, MouseButton::Left, Modifiers::default());
        visual.run_until_parked();
        window.read_with(&visual, |window, _| {
            assert!(!matches!(
                window.tab_drag.borrow().preview.map(|preview| preview.tab),
                Some(TabMoveDestination::Area(Area::Dock(_)))
            ));
        });
        visual.simulate_keystrokes("escape");
        visual.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
        visual.run_until_parked();
    }
    // The explorer's dock pane offers no target and refuses the release.
    let explorer = visual
        .debug_bounds(tab_selector(tool_tab_id(&window, cx)))
        .unwrap();
    let target = point(explorer.center().x, explorer.bottom() + px(40.));
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
