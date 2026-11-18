//! The tool window header: its title, actions, options and hide buttons, and
//! a container's new-tab button.
use super::operations::empty_product_window;
use super::*;
use crate::app::tree_view::{TreeProviderIdentity, TreeView, TreeViewEvent};
use crate::app::workbench::{Area, SplitPlacement, ToolContent};
use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, VisualTestContext, point};
use knot_protocol::{
    CommandArgumentValue, ExtensionId, ExtensionLifecycleId, TreeAction, TreeProviderRegistrationId,
};
use std::cell::RefCell;
use std::rc::Rc;

fn test_tool_pane(window: &Entity<KnotWindow>, cx: &TestAppContext) -> (u64, u64, TreeView2) {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tool = workbench
            .tool_by_stable_id(crate::app::product::tools::TEST_TOOL_ID)
            .unwrap();
        let (pane, tab) = workbench.tool_tab(tool).unwrap();
        let Some(ToolContent::Extension(crate::app::workbench::ExtensionView::Tree(tree))) =
            workbench.tool_content(tool)
        else {
            panic!("the test tool is a tree");
        };
        (pane.value(), tab.value(), tree.clone())
    })
}

type TreeView2 = Entity<TreeView>;

/// The Terminal window's first pane and its tool window.
fn terminal_pane(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> (
    crate::app::workbench::PaneId,
    crate::app::workbench::ToolFrameId,
) {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tool = workbench
            .tool_by_stable_id(crate::app::terminal::TERMINAL_TOOL_ID)
            .unwrap();
        (workbench.area_panes(Area::Tool(tool))[0], tool)
    })
}

#[gpui::test]
fn the_header_hides_its_tool_and_shows_the_tree_frame_actions(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    toggle_test_tool(&window, cx);
    let (pane, tab, tree) = test_tool_pane(&window, cx);

    let commands = Rc::new(RefCell::new(Vec::new()));
    let log = commands.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&tree, move |_, event: &TreeViewEvent, _| {
            if let TreeViewEvent::InvokeCommand { command, .. } = event {
                log.borrow_mut().push(command.clone());
            }
        })
    });
    tree.update(cx, |tree, cx| {
        tree.register_provider(
            crate::app::product::tools::TEST_TOOL_ID,
            TreeProviderIdentity {
                extension: ExtensionId::new(7),
                lifecycle: ExtensionLifecycleId::new(1),
                registration: TreeProviderRegistrationId::new(1),
            },
            vec![TreeAction {
                label: "New File".into(),
                command: "example.new-file".into(),
                arguments: CommandArgumentValue::Null,
                icon: None,
                menu_only: false,
                show_on_hover: false,
            }],
            cx,
        )
        .unwrap();
    });
    cx.run_until_parked();

    // The action renders in the pane's header, at its right.
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let header = visual
        .debug_bounds(Box::leak(format!("tab-bar-{pane}").into_boxed_str()))
        .unwrap();
    let action = visual
        .debug_bounds("extension-tree-frame-action-0")
        .unwrap();
    let hide_selector: &'static str = Box::leak(format!("hide-pane-{pane}").into_boxed_str());
    let hide = visual.debug_bounds(hide_selector).unwrap();
    assert!(header.contains(&action.center()));
    assert!(action.right() <= hide.left());
    // The title leads the header, the options button precedes hide, and a
    // fixed window shows no tab strip.
    let title = visual
        .debug_bounds(Box::leak(format!("tool-title-{pane}").into_boxed_str()))
        .unwrap();
    let tool_id = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .tool_by_stable_id(crate::app::product::tools::TEST_TOOL_ID)
            .unwrap()
            .value()
    });
    let options = visual
        .debug_bounds(Box::leak(
            format!("tool-options-{tool_id}").into_boxed_str(),
        ))
        .unwrap();
    assert!(title.right() < action.left());
    assert!(options.right() <= hide.left());
    assert!(
        visual
            .debug_bounds(Box::leak(format!("tab-{tab}").into_boxed_str()))
            .is_none()
    );
    assert!(
        visual
            .debug_bounds(Box::leak(format!("close-tab-{tab}").into_boxed_str()))
            .is_none(),
        "a tool tab has no close button"
    );

    // A double click on the header maximizes the window's pane.
    let at = point((title.right() + action.left()) / 2., header.center().y);
    for click_count in [1, 2] {
        visual.simulate_event(MouseDownEvent {
            position: at,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count,
            first_mouse: false,
        });
        visual.simulate_event(MouseUpEvent {
            position: at,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count,
        });
    }
    visual.run_until_parked();
    assert_eq!(
        window.read_with(&visual, |window, _| window
            .maximized
            .map(|pane| pane.value())),
        Some(pane)
    );
    drop(visual);
    window.update(cx, |window, cx| {
        let pane = window.maximized.unwrap();
        window.toggle_maximize(pane, cx);
    });
    let mut visual = VisualTestContext::from_window(native_window, cx);

    // Clicking it runs the command and leaves keyboard focus in the tree.
    visual.simulate_click(action.center(), Modifiers::default());
    visual.run_until_parked();
    assert_eq!(*commands.borrow(), ["example.new-file"]);
    let focus = cx.read(|cx| tree.read(cx).focus_handle(cx));
    assert!(
        VisualTestContext::from_window(native_window, cx)
            .update(|window, _| focus.is_focused(window))
    );

    // The hide button hides the tool.
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let hide = visual.debug_bounds(hide_selector).unwrap();
    visual.simulate_click(hide.center(), Modifiers::default());
    visual.run_until_parked();
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let tool = workbench
            .tool_by_stable_id(crate::app::product::tools::TEST_TOOL_ID)
            .unwrap();
        assert!(!workbench.tool_visible(tool));
    });
}

#[gpui::test]
fn a_stripe_without_buttons_is_not_painted(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    toggle_test_tool(&window, cx);
    let mut visual = VisualTestContext::from_window(native_window, cx);
    assert!(visual.debug_bounds("stripe-left").is_some());
    assert!(visual.debug_bounds("stripe-right").is_none());
}

#[gpui::test]
fn a_container_shows_its_tab_strip_title_and_new_tab_button(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    window.update(cx, |window, cx| window.toggle_terminal(cx).unwrap());
    cx.run_until_parked();
    let (pane, tool) = terminal_pane(&window, cx);
    let count = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .pane(pane)
            .unwrap()
            .tabs()
            .len()
    });
    assert_eq!(count, 1);
    let mut visual = VisualTestContext::from_window(native_window, cx);
    // Even one tab shows its strip, led by the window's title.
    let tab = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .pane(pane)
            .unwrap()
            .active_tab_id()
            .value()
    });
    assert!(
        visual
            .debug_bounds(Box::leak(format!("tab-{tab}").into_boxed_str()))
            .is_some()
    );
    let title = visual
        .debug_bounds(Box::leak(
            format!("tool-title-{}", pane.value()).into_boxed_str(),
        ))
        .unwrap();
    assert!(
        title.right()
            <= visual
                .debug_bounds(Box::leak(format!("tab-{tab}").into_boxed_str()))
                .unwrap()
                .left()
    );
    let button = visual
        .debug_bounds(Box::leak(
            format!("new-terminal-{}", pane.value()).into_boxed_str(),
        ))
        .unwrap();
    visual.simulate_click(button.center(), Modifiers::default());
    visual.run_until_parked();
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.pane(pane).unwrap().tabs().len(), 2);
        assert!(workbench.tool_visible(tool));
    });

    // A second pane has its strip and `+` but not the title.
    window.update(cx, |window, cx| {
        window.split_pane(pane, SplitDirection::Horizontal, SplitPlacement::After, cx);
    });
    cx.run_until_parked();
    let second = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .area_panes(Area::Tool(tool))[1]
    });
    let mut visual = VisualTestContext::from_window(native_window, cx);
    assert!(
        visual
            .debug_bounds(Box::leak(
                format!("new-terminal-{}", second.value()).into_boxed_str()
            ))
            .is_some()
    );
    assert!(
        visual
            .debug_bounds(Box::leak(
                format!("tool-title-{}", second.value()).into_boxed_str()
            ))
            .is_none()
    );
}

#[gpui::test]
fn a_terminal_tab_has_a_close_button_and_a_shell_label(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    window.update(cx, |window, cx| window.toggle_terminal(cx).unwrap());
    cx.run_until_parked();
    let (pane, _) = terminal_pane(&window, cx);
    let tab = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .pane(pane)
            .unwrap()
            .active_tab_id()
            .value()
    });
    let mut visual = VisualTestContext::from_window(native_window, cx);
    assert!(
        visual
            .debug_bounds(Box::leak(format!("close-tab-{tab}").into_boxed_str()))
            .is_some()
    );
}

#[gpui::test]
fn the_options_button_opens_the_windows_menu(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    toggle_test_tool(&window, cx);
    let tool = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .tool_by_stable_id(crate::app::product::tools::TEST_TOOL_ID)
            .unwrap()
    });
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let options = visual
        .debug_bounds(Box::leak(
            format!("tool-options-{}", tool.value()).into_boxed_str(),
        ))
        .unwrap();
    visual.simulate_click(options.center(), Modifiers::default());
    visual.run_until_parked();
    window.read_with(&visual, |window, _| assert!(window.stripe_menu.is_some()));
}

#[gpui::test]
fn two_windows_share_a_side_and_their_divider_sets_the_share(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    toggle_test_tool(&window, cx);
    // The Terminal window takes the left bottom slot beside the explorer.
    window.update(cx, |window, cx| {
        let terminal = window
            .workbench
            .read(cx)
            .tool_by_stable_id(crate::app::terminal::TERMINAL_TOOL_ID)
            .unwrap();
        window.move_tool_to_slot(terminal, crate::app::workbench::Slot::LeftBottom, cx);
        window.toggle_terminal(cx).unwrap();
    });
    cx.run_until_parked();
    let share = |cx: &gpui::App| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .side_share(crate::app::workbench::DockLocation::Left)
    };
    assert_eq!(cx.read(share), Some(0.5));

    let mut visual = VisualTestContext::from_window(native_window, cx);
    let divider = visual.debug_bounds("slot-divider-0").unwrap().center();
    visual.simulate_mouse_down(divider, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(
        divider + point(gpui::px(0.), gpui::px(60.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.run_until_parked();
    let dragged = visual.read(share);
    assert!(dragged.unwrap() > 0.5, "{dragged:?}");
    // Escape reverts an in-progress drag.
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(divider, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    assert_eq!(visual.read(share), Some(0.5));
}
