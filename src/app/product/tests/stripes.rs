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

    // The explorer's button sits in the left stripe, and the terminal's in
    // its lower end, the terminal home being the bottom dock.
    let mut visual = VisualTestContext::from_window(window, cx);
    let left = visual.debug_bounds("stripe-left").unwrap();
    let explorer = visual.debug_bounds(EXPLORER_BUTTON).unwrap();
    let terminal = visual.debug_bounds("stripe-button-terminal").unwrap();
    assert!(left.contains(&explorer.center()));
    assert!(left.contains(&terminal.center()));
    assert!(terminal.top() > explorer.bottom());
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
        assert!(workbench.dock_visible(DockLocation::Bottom));
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

    // Dragging the terminal button to the left stripe moves the dock
    // terminals there and makes it their home.
    let from = visual
        .debug_bounds("stripe-button-terminal")
        .unwrap()
        .center();
    let to = visual.debug_bounds("stripe-group-0").unwrap().center();
    visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(
        from + point(px(0.), px(12.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    drop(visual);
    cx.read(|cx| {
        let workbench = shell.read(cx).workbench.read(cx);
        assert_eq!(workbench.terminal_home(), DockLocation::Left);
        assert!(
            workbench
                .dock_terminal_tabs_outside(DockLocation::Left)
                .is_empty()
        );
        let (pane, _) = workbench.recent_dock_terminal().unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Left));
    });
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

#[gpui::test]
fn a_stripe_click_replaces_the_tool_in_its_pane_and_the_menu_shows_one_beside(
    cx: &mut TestAppContext,
) {
    use knot_protocol::{ExtensionId, ExtensionKey, ExtensionLifecycleId};
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let host = cx.update(crate::app::extensions::host::install);
    let (shell, window) = empty_product_window(&documents, cx);
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source(
                ExtensionKey::new(ExtensionId::new(801), ExtensionLifecycleId::new(1)),
                r#"
import { workbench } from "knot";
await workbench.registerView({
  id: "example.notes",
  title: "Notes",
  content: { kind: "tree", provider: { getChildren: () => [] } },
  placement: { location: "left", visible: false },
});
"#,
            )
            .unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
        })
        .detach();
    });
    wait_for_api_condition(cx, |cx| {
        shell
            .read(cx)
            .workbench
            .read(cx)
            .tool_by_stable_id("example.notes")
            .is_some()
    });
    let toggle = dispatch_product_command(&shell, window, "workbench.toggle-explorer", cx);
    assert_eq!(wait_for_command(toggle, cx), CommandOutcome::Completed);
    cx.run_until_parked();
    let shown = |cx: &TestAppContext| {
        cx.read(|cx| {
            let workbench = shell.read(cx).workbench.read(cx);
            ["knot.explorer", "example.notes"].map(|id| {
                let tool = workbench.tool_by_stable_id(id).unwrap();
                workbench.tool_visible(tool)
            })
        })
    };
    assert_eq!(shown(cx), [true, false]);

    // Clicking the other tool's button replaces the explorer in its pane.
    click(window, "stripe-button-example.notes", cx);
    assert_eq!(shown(cx), [false, true]);
    click(window, EXPLORER_BUTTON, cx);
    assert_eq!(shown(cx), [true, false]);

    // The button's menu opens on a right click; its Show Beside action puts
    // the tool in a pane of its own next to the explorer's.
    let mut visual = VisualTestContext::from_window(window, cx);
    let at = visual
        .debug_bounds("stripe-button-example.notes")
        .unwrap()
        .center();
    visual.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    visual.run_until_parked();
    shell.read_with(&visual, |shell, _| assert!(shell.stripe_menu.is_some()));
    drop(visual);
    shell.update(cx, |shell, cx| {
        shell.show_tool_beside("example.notes", cx);
    });
    assert_eq!(shown(cx), [true, true]);
    cx.read(|cx| {
        let workbench = shell.read(cx).workbench.read(cx);
        let [first, second] = ["knot.explorer", "example.notes"].map(|id| {
            workbench
                .tool_tab(workbench.tool_by_stable_id(id).unwrap())
                .unwrap()
                .0
        });
        assert_ne!(first, second);
    });
}
