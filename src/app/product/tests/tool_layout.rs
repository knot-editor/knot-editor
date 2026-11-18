//! Automatic tool-layout memory across windows and restarts.
use super::operations::empty_headless_window;
use super::*;
use crate::app::tool_layout::{ApplicationToolLayout, ToolLayoutSnapshot};
use crate::app::workbench::{DockLocation, Slot, ToolFrameId, WorkbenchLayout};
use knot_model::workbench::StoredTool;

fn stored(cx: &TestAppContext) -> ToolLayoutSnapshot {
    cx.read(|cx| cx.global::<ApplicationToolLayout>().0.current())
}

fn explorer_stored(cx: &TestAppContext) -> Option<StoredTool> {
    stored(cx)
        .tool(crate::app::product::tools::TEST_TOOL_ID)
        .cloned()
}

fn explorer_id(window: &Entity<KnotWindow>, cx: &gpui::App) -> ToolFrameId {
    window
        .read(cx)
        .workbench
        .read(cx)
        .tool_by_stable_id(crate::app::product::tools::TEST_TOOL_ID)
        .unwrap()
}

/// Whether the explorer is shown, and the slot it sits in.
fn explorer_state(window: &Entity<KnotWindow>, cx: &TestAppContext) -> (bool, Slot, f32) {
    cx.read(|cx| {
        let id = explorer_id(window, cx);
        let workbench = window.read(cx).workbench.read(cx);
        let tool = workbench.tool(id).unwrap();
        (workbench.tool_visible(id), tool.slot(), tool.size())
    })
}

/// A click on the explorer's stripe button, which hides it while it shows.
fn click_explorer(window: &Entity<KnotWindow>, cx: &mut TestAppContext) {
    window.update(cx, |window, cx| {
        let id = window.test_tool_frame(cx).unwrap();
        window.click_stripe_button(id, cx);
    });
}

fn toggle_explorer(window: &Entity<KnotWindow>, cx: &mut TestAppContext) {
    let stable_id = cx.read(|cx| {
        let id = explorer_id(window, cx);
        window
            .read(cx)
            .workbench
            .read(cx)
            .tool(id)
            .unwrap()
            .stable_id
            .clone()
    });
    window.update(cx, |window, cx| window.toggle_tool(&stable_id, cx));
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
    let (window, native_window) = empty_headless_window(&documents, cx);
    open_workspace(&window, cx);
    // Automatic presentation on workspace open does not publish a new default.
    assert_eq!(stored(cx), ToolLayoutSnapshot::product_default());
    window.update(cx, |window, cx| {
        let id = window.test_tool_frame(cx).unwrap();
        window.workbench.update(cx, |workbench, _| {
            workbench.set_tool_size(id, 311.);
        });
    });
    place_explorer(&window, native_window, Slot::BottomLeft, cx);
    let explorer = explorer_stored(cx).unwrap();
    assert_eq!(
        (explorer.slot, explorer.shown, explorer.size),
        (Slot::BottomLeft, true, 311.)
    );

    // A fresh window inherits the customized arrangement.
    let (fresh, _) = empty_headless_window(&documents, cx);
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
    assert_eq!(explorer_state(&fresh, cx), (true, Slot::BottomLeft, 311.));
}

#[gpui::test]
fn existing_windows_keep_live_state_and_activation_does_not_publish(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_headless_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, Slot::RightTop, cx);
    let published = stored(cx);

    let (fresh, _) = empty_headless_window(&documents, cx);
    open_workspace(&fresh, cx);
    // The older window keeps its live arrangement and the fresh window copies it.
    for candidate in [&window, &fresh] {
        assert_eq!(explorer_state(candidate, cx).1, Slot::RightTop);
    }
    // Interacting with the older window without a layout change publishes nothing.
    native_window.run(cx, |cx| {
        window.update(cx, |window, cx| {
            window.focus_active_surface(cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(stored(cx), published);
}

#[gpui::test]
fn hidden_tools_restore_slot_and_size_across_windows(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_headless_window(&documents, cx);
    open_workspace(&window, cx);
    window.update(cx, |window, cx| {
        let id = window.test_tool_frame(cx).unwrap();
        window.workbench.update(cx, |workbench, _| {
            workbench.set_tool_size(id, 300.);
        });
    });
    place_explorer(&window, native_window, Slot::BottomLeft, cx);
    click_explorer(&window, cx);
    let explorer = explorer_stored(cx).unwrap();
    assert_eq!((explorer.slot, explorer.shown), (Slot::BottomLeft, false));

    // A fresh window remembers the hidden window without showing it.
    let (fresh, _) = empty_headless_window(&documents, cx);
    open_workspace(&fresh, cx);
    assert_eq!(explorer_state(&fresh, cx), (false, Slot::BottomLeft, 300.));
    // Explicitly reopening returns it to its remembered slot and size.
    toggle_test_tool(&fresh, cx);
    assert_eq!(explorer_state(&fresh, cx), (true, Slot::BottomLeft, 300.));
}

#[gpui::test]
fn document_splits_and_side_toggles_reach_fresh_windows_only_as_tool_state(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_headless_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, Slot::RightTop, cx);
    place_explorer(&window, native_window, Slot::LeftTop, cx);
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
    assert_eq!(explorer_stored(cx).unwrap().slot, Slot::LeftTop);

    // The unfocused side takes focus, then hides back to the center.
    let center = cx.read(|cx| window.read(cx).workbench.read(cx).focused_pane_id());
    toggle_explorer(&window, cx);
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let (pane, _) = workbench.tool_tab(explorer_id(&window, cx)).unwrap();
        assert_eq!(workbench.focused_pane_id(), Some(pane));
    });
    toggle_explorer(&window, cx);
    assert_eq!(
        cx.read(|cx| window.read(cx).workbench.read(cx).focused_pane_id()),
        center
    );
    // A hidden window keeps its slot, and fresh windows inherit both.
    assert!(!explorer_stored(cx).unwrap().shown);
    let (fresh, _) = empty_headless_window(&documents, cx);
    open_workspace(&fresh, cx);
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        assert!(matches!(workbench.center(), Some(WorkbenchLayout::Pane(_))));
    });
    assert!(!explorer_state(&fresh, cx).0);
    // Toggling the window again shows it.
    toggle_explorer(&window, cx);
    assert!(explorer_state(&window, cx).0);
}

#[gpui::test]
fn reset_layout_restores_product_defaults(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_headless_window(&documents, cx);
    open_workspace(&window, cx);
    window.update(cx, |window, cx| {
        let terminal = window
            .workbench
            .read(cx)
            .tool_by_stable_id(crate::app::terminal::TERMINAL_TOOL_ID)
            .unwrap();
        window.move_tool_to_slot(terminal, Slot::RightBottom, cx);
        window.workbench.update(cx, |workbench, _| {
            workbench.set_tool_size(terminal, 333.);
        });
    });
    place_explorer(&window, native_window, Slot::BottomLeft, cx);
    click_explorer(&window, cx);
    assert_ne!(stored(cx), ToolLayoutSnapshot::product_default());

    let reset = dispatch_product_command(&window, native_window, "workbench.reset-layout", cx);
    assert_eq!(wait_for_command(reset, cx), CommandOutcome::Completed);
    // The reset is published as the confirmed customization. The test tool
    // and the test Terminal window are not contributed views, so they keep
    // their places.
    assert!(stored(cx).customized);
    assert_eq!(explorer_stored(cx).unwrap().slot, Slot::BottomLeft);
}

#[gpui::test]
fn restore_skips_unknown_tools_without_blank_frames(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, _) = empty_headless_window(&documents, cx);
    open_workspace(&window, cx);
    let before = stored(cx);
    let mut snapshot = ToolLayoutSnapshot::product_default();
    let record = |id: &str, slot| StoredTool {
        id: id.to_owned(),
        slot,
        shown: true,
        sidebar: true,
        size: 250.,
        share: 0.5,
    };
    snapshot.tools = vec![
        record("knot.missing", Slot::RightTop),
        record(crate::app::product::tools::TEST_TOOL_ID, Slot::BottomLeft),
    ];
    window.update(cx, |window, cx| {
        window
            .workbench
            .update(cx, |workbench, _| workbench.restore_tool_layout(&snapshot));
    });
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        // The test tool and the Terminal window; nothing for the missing one.
        assert_eq!(workbench.tools().len(), 2);
        let (pane, _) = workbench.tool_tab(explorer_id(&window, cx)).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Bottom));
        assert_eq!(workbench.panes().len(), 2);
    });
    // Restore is never a customization: the stored default is untouched.
    assert_eq!(stored(cx), before);
}

#[gpui::test]
fn fresh_window_restores_shown_tools_without_folder(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_headless_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, Slot::RightTop, cx);
    // A fresh window with no folder inherits the shown right window.
    let (fresh, _) = empty_headless_window(&documents, cx);
    fresh.update(cx, |fresh, cx| {
        fresh.ensure_test_tool(false, cx);
    });
    assert!(explorer_state(&fresh, cx).0);
    assert_eq!(explorer_state(&fresh, cx).1, Slot::RightTop);
}

#[gpui::test]
fn fresh_window_with_hidden_tool_restores_nothing(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_headless_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, Slot::RightTop, cx);
    click_explorer(&window, cx);
    let (fresh, _) = empty_headless_window(&documents, cx);
    fresh.update(cx, |fresh, cx| {
        fresh.ensure_test_tool(false, cx);
    });
    assert!(!explorer_state(&fresh, cx).0);
}

#[gpui::test]
fn open_as_editor_tab_follows_the_kind(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_headless_window(&documents, cx);
    let run = |name: &'static str, cx: &mut TestAppContext| {
        wait_for_command(
            dispatch_product_command(&window, native_window, name, cx),
            cx,
        )
    };
    // A document stays in the center, and the dock moves are gone.
    assert_eq!(
        run("workbench.open-as-editor-tab", cx),
        CommandOutcome::Unavailable
    );
    for name in [
        "workbench.move-tab-to-left-dock",
        "workbench.move-tab-to-right-dock",
        "workbench.move-tab-to-bottom-dock",
    ] {
        assert_eq!(run(name, cx), CommandOutcome::Unavailable, "{name}");
    }
    open_workspace(&window, cx);
    let published = stored(cx);
    // A tool's tab never enters the center.
    window.update(cx, |window, cx| {
        let id = window.test_tool_frame(cx).unwrap();
        window.workbench.update(cx, |workbench, _| {
            let (pane, tab) = workbench.tool_tab(id).unwrap();
            workbench.activate_tab(pane, tab);
        });
    });
    assert_eq!(
        run("workbench.open-as-editor-tab", cx),
        CommandOutcome::Unavailable
    );
    assert_eq!(stored(cx), published);
}

#[gpui::test]
fn moved_tools_return_to_their_new_slot_when_hidden_and_in_fresh_windows(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_headless_window(&documents, cx);
    open_workspace(&window, cx);
    place_explorer(&window, native_window, Slot::RightTop, cx);
    // Hiding the moved tool and showing it again returns it to its new slot.
    for expected in [false, true] {
        click_explorer(&window, cx);
        assert_eq!(explorer_state(&window, cx).0, expected,);
        assert_eq!(explorer_state(&window, cx).1, Slot::RightTop);
    }
    // A fresh window inherits the slot: hidden there, the explorer still
    // first shows in the right top slot.
    click_explorer(&window, cx);
    let (fresh, _) = empty_headless_window(&documents, cx);
    toggle_test_tool(&fresh, cx);
    assert!(explorer_state(&fresh, cx).0);
    assert_eq!(explorer_state(&fresh, cx).1, Slot::RightTop);
    cx.read(|cx| {
        let workbench = fresh.read(cx).workbench.read(cx);
        let (pane, _) = workbench.tool_tab(explorer_id(&fresh, cx)).unwrap();
        assert_eq!(workbench.focused_pane_id(), Some(pane));
    });
}
