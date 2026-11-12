//! Automatic tool-layout memory across windows and restarts.
use super::operations::empty_product_window;
use super::*;
use crate::app::tool_layout::{ApplicationToolLayout, ToolLayoutSnapshot};
use crate::app::workbench::{DockLocation, ToolFrameId, WorkbenchLayout};
use knot_model::workbench::{StoredNode, StoredPlacement};

fn stored(cx: &TestAppContext) -> ToolLayoutSnapshot {
    cx.read(|cx| cx.global::<ApplicationToolLayout>().0.current())
}

fn explorer_placement(cx: &TestAppContext) -> Option<StoredPlacement> {
    stored(cx).placement(crate::app::product::tools::TEST_TOOL_ID)
}

fn explorer_id(window: &Entity<KnotWindow>, cx: &gpui::App) -> ToolFrameId {
    window
        .read(cx)
        .workbench
        .read(cx)
        .tool_by_stable_id(crate::app::product::tools::TEST_TOOL_ID)
        .unwrap()
}

fn open_workspace(window: &Entity<KnotWindow>, cx: &mut TestAppContext) {
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    window.update(cx, |window, cx| {
        window.install_workspace(ResourceUri::parse("mem://product/").unwrap(), cx);
        window.ensure_test_tool(false, cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn user_customization_becomes_the_fresh_window_default(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_product_window(&documents, cx);
    open_workspace(&window, cx);
    // Automatic presentation on workspace open does not publish a new default.
    assert_eq!(stored(cx), ToolLayoutSnapshot::product_default());
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, _| {
            workbench.set_dock_size(DockLocation::Left, 311.);
            workbench.set_dock_size(DockLocation::Bottom, 300.);
        });
    });
    place_explorer(&window, native_window, DockLocation::Bottom, cx);
    let snapshot = stored(cx);
    assert_eq!(
        snapshot.placement(crate::app::product::tools::TEST_TOOL_ID),
        Some(StoredPlacement::Docked(DockLocation::Bottom))
    );
    assert_eq!(snapshot.dock(DockLocation::Left).size, 311.);
    assert_eq!(snapshot.dock(DockLocation::Bottom).size, 300.);

    // A fresh window inherits the customized arrangement.
    let (fresh, _) = empty_product_window(&documents, cx);
    cx.read(|cx| {
        assert!(
            fresh
                .read(cx)
                .workbench
                .read(cx)
                .tool_by_stable_id(crate::app::product::tools::TEST_TOOL_ID)
                .is_none()
        );
    });
    open_workspace(&fresh, cx);
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        let (pane, _) = workbench.tool_tab(explorer_id(&fresh, cx)).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Bottom));
        assert_eq!(workbench.dock_size(DockLocation::Bottom), 300.);
        assert_eq!(workbench.dock_size(DockLocation::Left), 311.);
    });
}

#[gpui::test]
fn existing_windows_keep_live_state_and_activation_does_not_publish(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_product_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, DockLocation::Right, cx);
    let published = stored(cx);

    let (fresh, _) = empty_product_window(&documents, cx);
    open_workspace(&fresh, cx);
    // The older window keeps its live arrangement and the fresh window copies it.
    for (candidate, dock) in [
        (&window, DockLocation::Right),
        (&fresh, DockLocation::Right),
    ] {
        cx.read(|cx| {
            let workbench = candidate.read(cx).workbench.read(cx);
            let (pane, _) = workbench.tool_tab(explorer_id(candidate, cx)).unwrap();
            assert_eq!(workbench.dock_location(pane), Some(dock));
        });
    }
    // Interacting with the older window without a layout change publishes nothing.
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            window.focus_active_surface(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(stored(cx), published);
}

#[gpui::test]
fn hidden_tools_restore_placement_and_size_across_windows(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_product_window(&documents, cx);
    open_workspace(&window, cx);
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, _| {
            workbench.set_dock_size(DockLocation::Bottom, 300.);
        });
    });
    place_explorer(&window, native_window, DockLocation::Bottom, cx);
    toggle_test_tool(&window, cx);
    assert!(matches!(
        explorer_placement(cx),
        Some(StoredPlacement::Hidden {
            dock: DockLocation::Bottom,
            ..
        })
    ));

    // A fresh window remembers the hidden placement without showing the tool.
    let (fresh, _) = empty_product_window(&documents, cx);
    open_workspace(&fresh, cx);
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        assert!(workbench.tool_tab(explorer_id(&fresh, cx)).is_none());
        assert_eq!(workbench.dock_size(DockLocation::Bottom), 300.);
    });
    // Explicitly reopening returns the tool to its remembered dock and size.
    toggle_test_tool(&fresh, cx);
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        let (pane, _) = workbench.tool_tab(explorer_id(&fresh, cx)).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Bottom));
        assert_eq!(workbench.dock_size(DockLocation::Bottom), 300.);
    });
}

#[gpui::test]
fn document_splits_and_dock_visibility_reach_fresh_windows_only_as_dock_state(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_product_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, DockLocation::Right, cx);
    place_explorer(&window, native_window, DockLocation::Left, cx);
    // Document splits are not layout changes and never publish.
    window.update(cx, |window, cx| {
        let pane = window.workbench.read(cx).center_pane().unwrap();
        window
            .workbench
            .update(cx, |workbench, _| workbench.focus_pane(pane));
    });
    let published = stored(cx);
    let split = dispatch_product_command(&window, native_window, "workbench.split-horizontal", cx);
    assert_eq!(wait_for_command(split, cx), CommandOutcome::Completed);
    assert_eq!(stored(cx), published);
    assert_eq!(
        published.dock(DockLocation::Left).root,
        Some(StoredNode::Pane {
            selected: 0,
            tools: vec![crate::app::product::tools::TEST_TOOL_ID.into()],
        })
    );

    // The unfocused dock takes focus, then hides back to the center.
    let center = cx.read(|cx| window.read(cx).workbench.read(cx).focused_pane_id());
    let toggle = dispatch_product_command(&window, native_window, "workbench.toggle-left-dock", cx);
    assert_eq!(wait_for_command(toggle, cx), CommandOutcome::Completed);
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let (pane, _) = workbench.tool_tab(explorer_id(&window, cx)).unwrap();
        assert_eq!(workbench.focused_pane_id(), Some(pane));
    });
    let toggle = dispatch_product_command(&window, native_window, "workbench.toggle-left-dock", cx);
    assert_eq!(wait_for_command(toggle, cx), CommandOutcome::Completed);
    assert_eq!(
        cx.read(|cx| window.read(cx).workbench.read(cx).focused_pane_id()),
        center
    );
    // A hidden dock keeps the explorer in it, and fresh windows inherit both.
    assert!(!stored(cx).dock(DockLocation::Left).visible);
    let (fresh, _) = empty_product_window(&documents, cx);
    open_workspace(&fresh, cx);
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        assert!(matches!(workbench.center(), Some(WorkbenchLayout::Pane(_))));
        let id = explorer_id(&fresh, cx);
        let (pane, _) = workbench.tool_tab(id).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Left));
        assert!(!workbench.tool_visible(id));
    });
}

#[gpui::test]
fn reset_layout_restores_product_defaults(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_product_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, DockLocation::Bottom, cx);
    toggle_test_tool(&window, cx);
    assert_ne!(stored(cx), ToolLayoutSnapshot::product_default());

    let reset = dispatch_product_command(&window, native_window, "workbench.reset-layout", cx);
    assert_eq!(wait_for_command(reset, cx), CommandOutcome::Completed);
    // Reset publishes product defaults as the confirmed customization.
    let mut expected = ToolLayoutSnapshot::product_default();
    expected.customized = true;
    // The tool is not a contributed view, so it keeps its remembered place.
    assert_eq!(stored(cx).docks, expected.docks);
    assert_eq!(stored(cx).terminal_home, expected.terminal_home);
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        for dock in DockLocation::ALL {
            assert_eq!(
                workbench.dock_size(dock),
                ToolLayoutSnapshot::product_default().dock(dock).size
            );
        }
    });
}

#[gpui::test]
fn restore_skips_unknown_tools_without_blank_frames(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, _) = empty_product_window(&documents, cx);
    open_workspace(&window, cx);
    let before = stored(cx);
    let mut snapshot = ToolLayoutSnapshot::product_default();
    let pane = |tool: &str| StoredNode::Pane {
        selected: 0,
        tools: vec![tool.to_owned()],
    };
    snapshot.docks[DockLocation::Left.index()].root = None;
    snapshot.docks[DockLocation::Right.index()].root = Some(pane("knot.missing"));
    snapshot.docks[DockLocation::Bottom.index()].root =
        Some(pane(crate::app::product::tools::TEST_TOOL_ID));
    window.update(cx, |window, cx| {
        window
            .workbench
            .update(cx, |workbench, _| workbench.restore_tool_layout(&snapshot));
    });
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.tools().len(), 1);
        let (pane, _) = workbench.tool_tab(explorer_id(&window, cx)).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Bottom));
        assert_eq!(workbench.panes().len(), 2);
    });
    // Restore is never a customization: the stored default is untouched.
    assert_eq!(stored(cx), before);
}

#[gpui::test]
fn fresh_window_restores_visible_tools_without_folder(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_product_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, DockLocation::Right, cx);
    // A fresh window with no folder inherits the visible right dock.
    let (fresh, _) = empty_product_window(&documents, cx);
    fresh.update(cx, |fresh, cx| {
        fresh.restore_fresh_window_layout(cx);
        fresh.ensure_test_tool(false, cx);
    });
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        let (pane, _) = workbench.tool_tab(explorer_id(&fresh, cx)).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Right));
    });
}

#[gpui::test]
fn fresh_window_with_hidden_tool_restores_nothing(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_product_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, DockLocation::Right, cx);
    toggle_test_tool(&window, cx);
    let (fresh, _) = empty_product_window(&documents, cx);
    fresh.update(cx, |fresh, cx| {
        fresh.restore_fresh_window_layout(cx);
        fresh.ensure_test_tool(false, cx);
    });
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        assert!(workbench.tool_tab(explorer_id(&fresh, cx)).is_none());
    });
}

#[gpui::test]
fn move_tab_commands_follow_the_kind_and_rehome_tools(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_product_window(&documents, cx);
    let run = |name: &'static str, cx: &mut TestAppContext| {
        wait_for_command(
            dispatch_product_command(&window, native_window, name, cx),
            cx,
        )
    };
    // A document stays in the center.
    for name in [
        "workbench.move-tab-to-left-dock",
        "workbench.move-tab-to-right-dock",
        "workbench.move-tab-to-bottom-dock",
        "workbench.move-tab-to-center",
    ] {
        assert_eq!(run(name, cx), CommandOutcome::Unavailable, "{name}");
    }
    open_workspace(&window, cx);
    let published = stored(cx);
    let focus_explorer = |cx: &mut TestAppContext| {
        window.update(cx, |window, cx| {
            let id = window.test_tool_frame(cx).unwrap();
            window.workbench.update(cx, |workbench, _| {
                let (pane, tab) = workbench.tool_tab(id).unwrap();
                workbench.activate_tab(pane, tab);
            });
        });
    };
    // A tool never enters the center, nor moves to the dock it is in.
    focus_explorer(cx);
    for name in [
        "workbench.move-tab-to-center",
        "workbench.move-tab-to-left-dock",
    ] {
        assert_eq!(run(name, cx), CommandOutcome::Unavailable, "{name}");
    }
    assert_eq!(stored(cx), published);
    // Moving a tool to another dock rehomes it, focused, and publishes.
    for (name, dock) in [
        ("workbench.move-tab-to-right-dock", DockLocation::Right),
        ("workbench.move-tab-to-bottom-dock", DockLocation::Bottom),
        ("workbench.move-tab-to-left-dock", DockLocation::Left),
    ] {
        focus_explorer(cx);
        assert_eq!(run(name, cx), CommandOutcome::Completed, "{name}");
        assert_eq!(explorer_placement(cx), Some(StoredPlacement::Docked(dock)));
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            let (pane, _) = workbench.tool_tab(explorer_id(&window, cx)).unwrap();
            assert_eq!(workbench.dock_location(pane), Some(dock));
            assert_eq!(workbench.focused_pane_id(), Some(pane));
        });
    }
}

#[gpui::test]
fn moved_tools_return_to_their_new_home_when_hidden_and_in_fresh_windows(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_product_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, DockLocation::Right, cx);
    // Hiding the moved tool and showing it again returns it to its new dock.
    for expected in [false, true] {
        toggle_test_tool(&window, cx);
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            let id = explorer_id(&window, cx);
            assert_eq!(workbench.tool_visible(id), expected);
            if expected {
                let (pane, _) = workbench.tool_tab(id).unwrap();
                assert_eq!(workbench.dock_location(pane), Some(DockLocation::Right));
            }
        });
    }
    // A fresh window inherits the home: hidden there, the explorer still
    // first shows in the right dock.
    toggle_test_tool(&window, cx);
    let (fresh, _) = empty_product_window(&documents, cx);
    toggle_test_tool(&fresh, cx);
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        let (pane, _) = workbench.tool_tab(explorer_id(&fresh, cx)).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Right));
        assert_eq!(workbench.focused_pane_id(), Some(pane));
    });
}
