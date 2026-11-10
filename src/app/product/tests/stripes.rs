//! The tool stripes: buttons by home, toggling by click, and rehoming by drag.
use super::operations::empty_product_window;
use super::*;
use crate::app::workbench::DockLocation;
use gpui::{Modifiers, MouseButton, VisualTestContext, point, px};

const EXPLORER_BUTTON: &str = "stripe-button-knot.explorer";

fn explorer_shown(shell: &Entity<ProductShell>, cx: &TestAppContext) -> bool {
    cx.read(|cx| {
        let workbench = shell.read(cx).workbench.read(cx);
        let explorer = workbench
            .tool_by_stable_id(crate::app::tool_layout::EXPLORER_TOOL_ID)
            .unwrap();
        workbench.tool_visible(explorer)
    })
}

fn click(window: gpui::AnyWindowHandle, selector: &'static str, cx: &mut TestAppContext) {
    let mut visual = VisualTestContext::from_window(window, cx);
    let center = visual.debug_bounds(selector).unwrap().center();
    visual.simulate_click(center, Modifiers::default());
    visual.run_until_parked();
}

#[gpui::test]
fn stripes_list_tools_by_home_toggle_on_click_and_rehome_by_drag(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (shell, window) = empty_product_window(&documents, cx);
    let toggle = dispatch_product_command(&shell, window, "workbench.toggle-explorer", cx);
    assert_eq!(wait_for_command(toggle, cx), CommandOutcome::Completed);
    cx.run_until_parked();
    assert!(explorer_shown(&shell, cx));

    // The explorer's button sits in the left stripe and the terminal's in
    // the right one, the terminal home.
    let mut visual = VisualTestContext::from_window(window, cx);
    let left = visual.debug_bounds("stripe-left").unwrap();
    let right = visual.debug_bounds("stripe-right").unwrap();
    let explorer = visual.debug_bounds(EXPLORER_BUTTON).unwrap();
    let terminal = visual.debug_bounds("stripe-button-terminal").unwrap();
    assert!(left.contains(&explorer.center()));
    assert!(right.contains(&terminal.center()));
    drop(visual);

    // A click toggles: the focused explorer hides, then shows again.
    click(window, EXPLORER_BUTTON, cx);
    assert!(!explorer_shown(&shell, cx));
    click(window, EXPLORER_BUTTON, cx);
    assert!(explorer_shown(&shell, cx));

    // The terminal button opens a dock terminal in its home, then hides it.
    click(window, "stripe-button-terminal", cx);
    cx.read(|cx| {
        let workbench = shell.read(cx).workbench.read(cx);
        assert!(workbench.dock_terminals_visible());
        assert!(workbench.dock_visible(DockLocation::Right));
    });
    click(window, "stripe-button-terminal", cx);
    cx.read(|cx| {
        assert!(!shell.read(cx).workbench.read(cx).dock_terminals_visible());
    });

    // Dragging the explorer's button to the right stripe moves the tool and
    // rehomes it.
    let mut visual = VisualTestContext::from_window(window, cx);
    let from = visual.debug_bounds(EXPLORER_BUTTON).unwrap().center();
    let to = visual.debug_bounds("stripe-group-1").unwrap().center();
    visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(
        from + point(px(12.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    drop(visual);
    cx.read(|cx| {
        let workbench = shell.read(cx).workbench.read(cx);
        let explorer = workbench
            .tool_by_stable_id(crate::app::tool_layout::EXPLORER_TOOL_ID)
            .unwrap();
        assert_eq!(workbench.tool_home(explorer), Some(DockLocation::Right));
        let (pane, _) = workbench.tool_tab(explorer).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Right));
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    let right = visual.debug_bounds("stripe-right").unwrap();
    let explorer = visual.debug_bounds(EXPLORER_BUTTON).unwrap();
    assert!(right.contains(&explorer.center()));
}

#[gpui::test]
fn the_tool_stripes_setting_hides_both_stripes(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (_, window) = empty_product_window(&documents, cx);
    let mut visual = VisualTestContext::from_window(window, cx);
    assert!(visual.debug_bounds("stripe-left").is_some());
    visual.update(|_, cx| {
        crate::app::settings::set_boolean(crate::app::settings::WORKBENCH_TOOL_STRIPES, false, cx);
    });
    visual.run_until_parked();
    assert!(visual.debug_bounds("stripe-left").is_none());
    assert!(visual.debug_bounds("stripe-right").is_none());
}
