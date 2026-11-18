//! The tool stripes: buttons by slot, showing and hiding by click, moving by
//! drag, and the sidebar menus.
use super::operations::empty_product_window;
use super::*;
use crate::app::product::Focus;
use crate::app::workbench::{DockLocation, Slot};
use gpui::{Modifiers, MouseButton, VisualTestContext, point, px};

const EXPLORER_BUTTON: &str = "stripe-button-test.tool";
const TERMINAL_BUTTON: &str = "stripe-button-knot.terminal";

fn explorer_shown(window: &Entity<KnotWindow>, cx: &TestAppContext) -> bool {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let explorer = workbench
            .tool_by_stable_id(crate::app::product::tools::TEST_TOOL_ID)
            .unwrap();
        workbench.tool_visible(explorer)
    })
}

fn click(native_window: gpui::AnyWindowHandle, selector: &'static str, cx: &mut TestAppContext) {
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let center = visual.debug_bounds(selector).unwrap().center();
    visual.simulate_click(center, Modifiers::default());
    visual.run_until_parked();
}

#[gpui::test]
fn stripes_group_tools_by_slot_toggle_on_click_and_move_by_drag(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    toggle_test_tool(&window, cx);
    cx.run_until_parked();
    assert!(explorer_shown(&window, cx));

    // The explorer's button sits in the left stripe's top group, and the
    // Terminal window's in its lower end.
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let left = visual.debug_bounds("stripe-left").unwrap();
    let explorer = visual.debug_bounds(EXPLORER_BUTTON).unwrap();
    let terminal = visual.debug_bounds(TERMINAL_BUTTON).unwrap();
    assert!(left.contains(&explorer.center()));
    assert!(left.contains(&terminal.center()));
    assert!(terminal.top() > explorer.bottom());
    drop(visual);

    // A click on a visible tool window hides it, and on a hidden one shows it.
    click(native_window, EXPLORER_BUTTON, cx);
    assert!(!explorer_shown(&window, cx));
    click(native_window, EXPLORER_BUTTON, cx);
    assert!(explorer_shown(&window, cx));

    // The Terminal button opens a terminal in its window, then hides it,
    // keeping the terminal.
    let terminal_state = |cx: &TestAppContext| {
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            let tool = workbench
                .tool_by_stable_id(crate::app::terminal::TERMINAL_TOOL_ID)
                .unwrap();
            (workbench.tool_visible(tool), workbench.tool_is_empty(tool))
        })
    };
    assert_eq!(terminal_state(cx), (false, true));
    click(native_window, TERMINAL_BUTTON, cx);
    assert_eq!(terminal_state(cx), (true, false));
    click(native_window, TERMINAL_BUTTON, cx);
    assert_eq!(terminal_state(cx), (false, false));

    // Dragging the explorer's button to the right stripe moves its window.
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let from = visual.debug_bounds(EXPLORER_BUTTON).unwrap().center();
    assert!(visual.debug_bounds("stripe-right").is_none());
    visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(
        from + point(px(12.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    // The empty right stripe appears while a button is dragged.
    let to = visual
        .debug_bounds(Box::leak(
            format!("stripe-group-{}", Slot::RightTop.index()).into_boxed_str(),
        ))
        .unwrap()
        .center();
    visual.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    drop(visual);
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let explorer = workbench
            .tool_by_stable_id(crate::app::product::tools::TEST_TOOL_ID)
            .unwrap();
        assert_eq!(workbench.tool(explorer).unwrap().slot(), Slot::RightTop);
        assert!(workbench.tool_visible(explorer));
        let (pane, _) = workbench.tool_tab(explorer).unwrap();
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Right));
    });
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let right = visual.debug_bounds("stripe-right").unwrap();
    let explorer = visual.debug_bounds(EXPLORER_BUTTON).unwrap();
    assert!(right.contains(&explorer.center()));

    // Dragging the Terminal button to the left top group moves its window.
    let from = visual.debug_bounds(TERMINAL_BUTTON).unwrap().center();
    let to = visual
        .debug_bounds(Box::leak(
            format!("stripe-group-{}", Slot::LeftTop.index()).into_boxed_str(),
        ))
        .unwrap()
        .center();
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
        let workbench = window.read(cx).workbench.read(cx);
        let terminal = workbench
            .tool_by_stable_id(crate::app::terminal::TERMINAL_TOOL_ID)
            .unwrap();
        assert_eq!(workbench.tool(terminal).unwrap().slot(), Slot::LeftTop);
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
fn a_stripe_click_replaces_the_window_in_its_slot_and_the_menu_removes_it_from_the_sidebar(
    cx: &mut TestAppContext,
) {
    use knot_protocol::{ExtensionId, ExtensionKey, ExtensionLifecycleId};
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let host = cx.update(crate::app::extensions::host::install);
    let (window, native_window) = empty_product_window(&documents, cx);
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
  placement: { location: "leftTop", visible: false },
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
        window
            .read(cx)
            .workbench
            .read(cx)
            .tool_by_stable_id("example.notes")
            .is_some()
    });
    toggle_test_tool(&window, cx);
    cx.run_until_parked();
    let shown = |cx: &TestAppContext| {
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            [crate::app::product::tools::TEST_TOOL_ID, "example.notes"].map(|id| {
                let tool = workbench.tool_by_stable_id(id).unwrap();
                workbench.tool_visible(tool)
            })
        })
    };
    assert_eq!(shown(cx), [true, false]);

    // Clicking the other window's button replaces the explorer in its slot,
    // which keeps its tab.
    click(native_window, "stripe-button-example.notes", cx);
    assert_eq!(shown(cx), [false, true]);
    click(native_window, EXPLORER_BUTTON, cx);
    assert_eq!(shown(cx), [true, false]);

    // The button's menu opens on a right click and takes focus.
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let at = visual
        .debug_bounds("stripe-button-example.notes")
        .unwrap()
        .center();
    visual.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    visual.run_until_parked();
    window.read_with(&visual, |window, _| {
        assert!(window.stripe_menu.is_some());
        assert_eq!(window.focus, Focus::Popup);
    });
    drop(visual);

    // Removing a window from the sidebar hides it and its button; the More
    // tool windows button lists it, and picking it shows the window and
    // returns its button.
    let notes = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .tool_by_stable_id("example.notes")
            .unwrap()
    });
    window.update(cx, |window, cx| window.remove_tool_from_sidebar(notes, cx));
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(native_window, cx);
    assert!(visual.debug_bounds("stripe-button-example.notes").is_none());
    assert!(visual.debug_bounds("stripe-more").is_some());
    drop(visual);
    click(native_window, "stripe-more", cx);
    window.read_with(cx, |window, _| assert!(window.stripe_menu.is_some()));
    window.update(cx, |window, cx| window.restore_tool_to_sidebar(notes, cx));
    cx.run_until_parked();
    assert_eq!(shown(cx), [false, true]);
    let mut visual = VisualTestContext::from_window(native_window, cx);
    assert!(visual.debug_bounds("stripe-button-example.notes").is_some());
    assert!(visual.debug_bounds("stripe-more").is_none());
}
