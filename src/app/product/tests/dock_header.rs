//! The dock pane header: its hide button and the tree frame actions.
use super::operations::empty_product_window;
use super::*;
use crate::app::tree_view::{TreeProviderIdentity, TreeView, TreeViewEvent};
use crate::app::workbench::ToolContent;
use gpui::{Modifiers, VisualTestContext};
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
    assert!(
        visual
            .debug_bounds(Box::leak(format!("close-tab-{tab}").into_boxed_str()))
            .is_none(),
        "a tool tab has no close button"
    );

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
fn a_terminal_header_opens_another_terminal_beside_it(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    window.update(cx, |window, cx| window.toggle_terminal(cx).unwrap());
    cx.run_until_parked();
    let (pane, count) = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let (pane, _) = workbench.recent_dock_terminal().unwrap();
        (pane, workbench.pane(pane).unwrap().tabs().len())
    });
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let button = visual
        .debug_bounds(Box::leak(
            format!("new-terminal-{}", pane.value()).into_boxed_str(),
        ))
        .unwrap();
    visual.simulate_click(button.center(), Modifiers::default());
    visual.run_until_parked();
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.pane(pane).unwrap().tabs().len(), count + 1);
    });
}
