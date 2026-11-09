//! Automatic tool-layout memory across windows and restarts.
use super::operations::empty_product_window;
use super::*;
use crate::app::tool_layout::{ApplicationToolLayout, EXPLORER_TOOL_ID, ToolLayoutSnapshot};
use crate::app::workbench::{DockLocation, ToolFrameId, WorkbenchLayout};
use knot_model::workbench::{StoredNode, StoredPlacement};

fn stored(cx: &TestAppContext) -> ToolLayoutSnapshot {
    cx.read(|cx| cx.global::<ApplicationToolLayout>().0.current())
}

fn explorer_placement(cx: &TestAppContext) -> Option<StoredPlacement> {
    stored(cx).placement(EXPLORER_TOOL_ID)
}

fn explorer_id(shell: &Entity<ProductShell>, cx: &gpui::App) -> ToolFrameId {
    shell
        .read(cx)
        .workbench
        .read(cx)
        .tool_by_stable_id(EXPLORER_TOOL_ID)
        .unwrap()
}

fn open_workspace(shell: &Entity<ProductShell>, cx: &mut TestAppContext) {
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    shell.update(cx, |shell, cx| {
        shell.install_workspace(ResourceUri::parse("mem://product/").unwrap(), cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn user_customization_becomes_the_fresh_window_default(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, window) = empty_product_window(&documents, cx);
    open_workspace(&shell, cx);
    // Automatic presentation on workspace open does not publish a new default.
    assert_eq!(stored(cx), ToolLayoutSnapshot::product_default());
    shell.update(cx, |shell, cx| {
        shell.workbench.update(cx, |workbench, _| {
            workbench.set_dock_size(DockLocation::Left, 311.);
            workbench.set_dock_size(DockLocation::Bottom, 300.);
        });
    });
    place_explorer(&shell, window, DockLocation::Bottom, cx);
    let snapshot = stored(cx);
    assert_eq!(
        snapshot.placement(EXPLORER_TOOL_ID),
        Some(StoredPlacement::Docked(DockLocation::Bottom))
    );
    assert_eq!(snapshot.dock(DockLocation::Left).size, 311.);
    assert_eq!(snapshot.dock(DockLocation::Bottom).size, 300.);

    // A fresh window inherits the customized arrangement.
    let (fresh, fresh_window) = empty_product_window(&documents, cx);
    cx.read(|cx| {
        assert!(
            fresh
                .read(cx)
                .workbench
                .read(cx)
                .tool_by_stable_id(EXPLORER_TOOL_ID)
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
    let _ = fresh_window;
}

#[gpui::test]
fn existing_windows_keep_live_state_and_activation_does_not_publish(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, window) = empty_product_window(&documents, cx);
    open_workspace(&shell, cx);
    place_explorer(&shell, window, DockLocation::Right, cx);
    let published = stored(cx);

    let (fresh, _) = empty_product_window(&documents, cx);
    open_workspace(&fresh, cx);
    // The older window keeps its live arrangement and the fresh window copies it.
    for (candidate, dock) in [(&shell, DockLocation::Right), (&fresh, DockLocation::Right)] {
        cx.read(|cx| {
            let workbench = candidate.read(cx).workbench.read(cx);
            let (pane, _) = workbench.tool_tab(explorer_id(candidate, cx)).unwrap();
            assert_eq!(workbench.dock_location(pane), Some(dock));
        });
    }
    // Interacting with the older window without a layout change publishes nothing.
    cx.update_window(window, |_, _, cx| {
        shell.update(cx, |shell, cx| {
            shell.focus_active_surface(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(stored(cx), published);
}

#[gpui::test]
fn hidden_tools_restore_placement_and_size_across_windows(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, window) = empty_product_window(&documents, cx);
    open_workspace(&shell, cx);
    shell.update(cx, |shell, cx| {
        shell.workbench.update(cx, |workbench, _| {
            workbench.set_dock_size(DockLocation::Bottom, 300.);
        });
    });
    place_explorer(&shell, window, DockLocation::Bottom, cx);
    let hidden = dispatch_product_command(&shell, window, "workbench.toggle-explorer", cx);
    assert_eq!(wait_for_command(hidden, cx), CommandOutcome::Completed);
    assert!(matches!(
        explorer_placement(cx),
        Some(StoredPlacement::Hidden {
            dock: DockLocation::Bottom,
            ..
        })
    ));

    // A fresh window remembers the hidden placement without showing the tool.
    let (fresh, fresh_window) = empty_product_window(&documents, cx);
    open_workspace(&fresh, cx);
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        assert!(workbench.tool_tab(explorer_id(&fresh, cx)).is_none());
        assert_eq!(workbench.dock_size(DockLocation::Bottom), 300.);
    });
    // Explicitly reopening returns the tool to its remembered dock and size.
    let reopened = dispatch_product_command(&fresh, fresh_window, "workbench.toggle-explorer", cx);
    assert_eq!(wait_for_command(reopened, cx), CommandOutcome::Completed);
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
    let (shell, window) = empty_product_window(&documents, cx);
    open_workspace(&shell, cx);
    place_explorer(&shell, window, DockLocation::Left, cx);
    // Document splits are not layout changes and never publish.
    shell.update(cx, |shell, cx| {
        let pane = shell.workbench.read(cx).center_pane().unwrap();
        shell
            .workbench
            .update(cx, |workbench, _| workbench.focus_pane(pane));
    });
    let published = stored(cx);
    let split = dispatch_product_command(&shell, window, "workbench.split-horizontal", cx);
    assert_eq!(wait_for_command(split, cx), CommandOutcome::Completed);
    assert_eq!(stored(cx), published);
    assert_eq!(
        published.dock(DockLocation::Left).root,
        Some(StoredNode::Pane {
            selected: 0,
            tools: vec![EXPLORER_TOOL_ID.into()],
        })
    );

    // A hidden dock keeps the explorer in it, and fresh windows inherit both.
    let toggle = dispatch_product_command(&shell, window, "workbench.toggle-left-sidebar", cx);
    assert_eq!(wait_for_command(toggle, cx), CommandOutcome::Completed);
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
    let (shell, window) = empty_product_window(&documents, cx);
    open_workspace(&shell, cx);
    place_explorer(&shell, window, DockLocation::Bottom, cx);
    let hidden = dispatch_product_command(&shell, window, "workbench.toggle-explorer", cx);
    assert_eq!(wait_for_command(hidden, cx), CommandOutcome::Completed);
    assert_ne!(stored(cx), ToolLayoutSnapshot::product_default());

    let reset = dispatch_product_command(&shell, window, "workbench.reset-layout", cx);
    assert_eq!(wait_for_command(reset, cx), CommandOutcome::Completed);
    // Reset publishes product defaults as the confirmed customization.
    let mut expected = ToolLayoutSnapshot::product_default();
    expected.customized = true;
    assert_eq!(stored(cx), expected);
    cx.read(|cx| {
        let workbench = shell.read(cx).workbench.read(cx);
        let (pane, _) = workbench.tool_tab(explorer_id(&shell, cx)).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Left));
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
    let (shell, _) = empty_product_window(&documents, cx);
    open_workspace(&shell, cx);
    let before = stored(cx);
    let mut snapshot = ToolLayoutSnapshot::product_default();
    let pane = |tool: &str| StoredNode::Pane {
        selected: 0,
        tools: vec![tool.to_owned()],
    };
    snapshot.docks[DockLocation::Left.index()].root = None;
    snapshot.docks[DockLocation::Right.index()].root = Some(pane("knot.missing"));
    snapshot.docks[DockLocation::Bottom.index()].root = Some(pane(EXPLORER_TOOL_ID));
    shell.update(cx, |shell, cx| {
        shell
            .workbench
            .update(cx, |workbench, _| workbench.restore_tool_layout(&snapshot));
    });
    cx.read(|cx| {
        let workbench = shell.read(cx).workbench.read(cx);
        assert_eq!(workbench.tools().len(), 1);
        let (pane, _) = workbench.tool_tab(explorer_id(&shell, cx)).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Bottom));
        assert_eq!(workbench.panes().len(), 2);
    });
    // Restore is never a customization: the stored default is untouched.
    assert_eq!(stored(cx), before);
}

#[gpui::test]
fn fresh_window_restores_visible_tools_without_folder(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, window) = empty_product_window(&documents, cx);
    open_workspace(&shell, cx);
    place_explorer(&shell, window, DockLocation::Right, cx);
    // A fresh window with no folder inherits the visible right dock.
    let (fresh, _) = empty_product_window(&documents, cx);
    fresh.update(cx, |fresh, cx| fresh.restore_fresh_window_layout(cx));
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        let (pane, _) = workbench.tool_tab(explorer_id(&fresh, cx)).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Right));
    });
}

#[gpui::test]
fn fresh_window_without_customization_keeps_first_run(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (fresh, _) = empty_product_window(&documents, cx);
    fresh.update(cx, |fresh, cx| fresh.restore_fresh_window_layout(cx));
    cx.read(|cx| {
        assert!(
            fresh
                .read(cx)
                .workbench
                .read(cx)
                .tool_by_stable_id(EXPLORER_TOOL_ID)
                .is_none()
        );
    });
}

#[gpui::test]
fn fresh_window_with_hidden_tool_restores_nothing(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, window) = empty_product_window(&documents, cx);
    open_workspace(&shell, cx);
    place_explorer(&shell, window, DockLocation::Right, cx);
    let hidden = dispatch_product_command(&shell, window, "workbench.toggle-explorer", cx);
    assert_eq!(wait_for_command(hidden, cx), CommandOutcome::Completed);
    let (fresh, _) = empty_product_window(&documents, cx);
    fresh.update(cx, |fresh, cx| fresh.restore_fresh_window_layout(cx));
    cx.read(|cx| {
        assert!(
            fresh
                .read(cx)
                .workbench
                .read(cx)
                .tool_by_stable_id(EXPLORER_TOOL_ID)
                .is_none()
        );
    });
}

#[gpui::test]
fn move_tool_to_dock_places_the_focused_tool_and_publishes_the_default(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::MOVE_TOOL_TO_DOCK_COMMAND;
    let documents = install_globals(cx);
    let (shell, window) = empty_product_window(&documents, cx);
    // A document has focus, so there is no tool to move.
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&shell, window, MOVE_TOOL_TO_DOCK_COMMAND, cx),
            cx
        ),
        CommandOutcome::Unavailable
    );
    open_workspace(&shell, cx);
    for (label, dock) in [
        ("Right", DockLocation::Right),
        ("Bottom", DockLocation::Bottom),
        ("Left", DockLocation::Left),
    ] {
        let id = cx.read(|cx| explorer_id(&shell, cx));
        shell.update(cx, |shell, cx| {
            shell.workbench.update(cx, |workbench, _| {
                let (pane, tab) = workbench.tool_tab(id).unwrap();
                workbench.activate_tab(pane, tab);
            });
        });
        let execution = dispatch_product_command(&shell, window, MOVE_TOOL_TO_DOCK_COMMAND, cx);
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        wait_for_api_condition(cx, |cx| shell.read(cx).transient.is_some());
        cx.simulate_input(window, label);
        cx.simulate_keystrokes(window, "enter");
        cx.run_until_parked();
        assert_eq!(
            explorer_placement(cx),
            Some(StoredPlacement::Docked(dock)),
            "{label}"
        );
        cx.read(|cx| {
            let workbench = shell.read(cx).workbench.read(cx);
            let (pane, _) = workbench.tool_tab(explorer_id(&shell, cx)).unwrap();
            assert_eq!(workbench.focused_pane_id(), Some(pane));
        });
    }
}
