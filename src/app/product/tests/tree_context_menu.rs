//! The context menu of an extension tree's row actions, opened by pointer
//! and by command, and the keys that reach it.
use super::*;
use crate::app::tree_view::{SHOW_TREE_CONTEXT_MENU_COMMAND, TreeView};
use knot_protocol::ExtensionKey;

const SOURCE: &str = r#"
import { commands, keymaps, workbench } from "knot";
await workbench.registerView({
  id: "example.actions",
  title: "Actions",
  content: { kind: "tree", provider: {
    getChildren(parentId) {
      return parentId === null ? [
        { id: "with", label: "With actions", collapsibleState: "none", actions: [
          { label: "Copy one", command: "test.copy", arguments: ["one"] },
          { label: "Copy two", command: "test.copy", arguments: ["two"] },
        ] },
        { id: "without", label: "Without actions", collapsibleState: "none" },
      ] : [];
    },
  } },
});
await commands.register("test.copy", async ({ arguments: [text] }) => {
  await workbench.writeClipboardText(`copied ${text}`);
});
await commands.register("test.global", async () => {
  await workbench.writeClipboardText("global");
});
keymaps.bind("f5", "test.global");
"#;

fn load_source(
    host: &Entity<crate::app::extensions::host::ProductExtensionHost>,
    cx: &mut TestAppContext,
) {
    let key = ExtensionKey::new(ExtensionId::new(990), ExtensionLifecycleId::new(1));
    let loaded = Arc::new(Mutex::new(None));
    let result = loaded.clone();
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| {
            *result.lock().unwrap() = Some(execution.await.map(|_| ()));
        })
        .detach();
    });
    wait_for_api_condition(cx, |_| loaded.lock().unwrap().is_some());
    loaded.lock().unwrap().take().unwrap().unwrap();
}

struct Fixture {
    window: Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    tree: Entity<TreeView>,
    editor: gpui::FocusHandle,
}

impl Fixture {
    fn start(cx: &mut TestAppContext) -> Self {
        let documents = install_globals(cx);
        cx.update(super::super::bind_product_keys);
        let host = cx.update(super::super::super::extensions::host::install);
        let document = cx.update(|cx| create_untitled_document(&documents, cx));
        let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
        let (window, native_window) = product_window(document, model, cx);
        load_source(&host, cx);
        let tree = wait_for_contributed_view(&window, "example.actions", cx);
        wait_for_api_condition(cx, |cx| tree.read(cx).root_labels().len() == 2);
        let editor = cx.read(|cx| {
            window
                .read(cx)
                .workbench
                .read(cx)
                .focused_editor()
                .unwrap()
                .focus_handle(cx)
        });
        let fixture = Self {
            native_window,
            window,
            tree,
            editor,
        };
        fixture.focus(
            &fixture.tree.read_with(cx, |tree, cx| tree.focus_handle(cx)),
            cx,
        );
        fixture
    }

    fn focus(&self, handle: &gpui::FocusHandle, cx: &mut TestAppContext) {
        cx.update_window(self.native_window, |_, native_window, cx| {
            handle.focus(native_window, cx);
            focus_view(handle, native_window, cx);
        })
        .unwrap();
        cx.run_until_parked();
    }

    fn row(&self, index: usize, cx: &mut TestAppContext) -> gpui::Point<gpui::Pixels> {
        cx.run_until_parked();
        cx.refresh().unwrap();
        let selector = if index == 0 {
            "extension-tree-row-0"
        } else {
            "extension-tree-row-1"
        };
        VisualTestContext::from_window(self.native_window, cx)
            .debug_bounds(selector)
            .expect("the row is laid out")
            .center()
    }

    fn right_click(&self, index: usize, cx: &mut TestAppContext) {
        let row = self.row(index, cx);
        let mut window_cx = VisualTestContext::from_window(self.native_window, cx);
        window_cx.simulate_mouse_down(row, MouseButton::Right, Modifiers::default());
        window_cx.simulate_mouse_up(row, MouseButton::Right, Modifiers::default());
        cx.run_until_parked();
    }

    fn press(&self, keys: &str, cx: &mut TestAppContext) {
        cx.refresh().unwrap();
        cx.simulate_keystrokes(self.native_window, keys);
        // A key can run a command on the package's worker thread, whose
        // completion settles focus and the menu.
        crate::app::product::driver::settle(&self.window, cx);
    }

    fn menu_open(&self, cx: &TestAppContext) -> bool {
        cx.read(|cx| self.tree.read(cx).context_menu_open())
    }

    fn tree_focused(&self, cx: &mut TestAppContext) -> bool {
        cx.update_window(self.native_window, |_, native_window, cx| {
            self.tree.focus_handle(cx).is_focused(native_window)
        })
        .unwrap()
    }

    fn selected(&self, cx: &TestAppContext) -> Option<String> {
        cx.read(|cx| self.tree.read(cx).selected_label())
    }

    fn wait_for_clipboard(&self, text: &str, cx: &mut TestAppContext) {
        wait_for_api_condition(cx, |cx| {
            cx.read_from_clipboard()
                .is_some_and(|item| item.text().as_deref() == Some(text))
        });
    }
}

#[gpui::test]
fn right_click_opens_a_row_menu_whose_actions_run_and_return_focus(cx: &mut TestAppContext) {
    let fixture = Fixture::start(cx);
    fixture.right_click(0, cx);
    assert!(fixture.menu_open(cx));
    assert_eq!(fixture.selected(cx).as_deref(), Some("With actions"));
    assert!(!fixture.tree_focused(cx));

    // The menu takes the arrows and Enter the keymap leaves; the action runs
    // with its arguments and focus returns to the tree.
    fixture.press("down down enter", cx);
    fixture.wait_for_clipboard("copied two", cx);
    assert!(!fixture.menu_open(cx));
    assert!(fixture.tree_focused(cx));

    // A row without actions is selected and opens no menu.
    fixture.right_click(1, cx);
    assert!(!fixture.menu_open(cx));
    assert_eq!(fixture.selected(cx).as_deref(), Some("Without actions"));
    let execution = dispatch_tree_command(
        &fixture.window,
        fixture.native_window,
        SHOW_TREE_CONTEXT_MENU_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Unavailable);
    assert!(!fixture.menu_open(cx));
}

#[gpui::test]
fn a_menu_action_runs_when_another_view_had_focus(cx: &mut TestAppContext) {
    let fixture = Fixture::start(cx);
    // Focus events reach the window only in an active native window.
    cx.update_window(fixture.native_window, |_, native_window, _| {
        native_window.activate_window()
    })
    .unwrap();
    let editor = fixture.editor.clone();
    fixture.focus(&editor, cx);
    fixture.right_click(0, cx);
    assert!(fixture.menu_open(cx));
    fixture.press("down down enter", cx);
    fixture.wait_for_clipboard("copied two", cx);
    assert!(fixture.tree_focused(cx));
}

#[gpui::test]
fn shift_f10_opens_the_selected_row_menu_and_bindings_still_win(cx: &mut TestAppContext) {
    let fixture = Fixture::start(cx);
    fixture.press("down", cx);
    assert_eq!(fixture.selected(cx).as_deref(), Some("With actions"));
    fixture.press("shift-f10", cx);
    assert!(fixture.menu_open(cx));

    // A global binding runs while the menu holds focus, and leaves it open.
    fixture.press("f5", cx);
    fixture.wait_for_clipboard("global", cx);
    assert!(fixture.menu_open(cx));
    // An arrow the keymap leaves moves through the menu, not the tree.
    fixture.press("down", cx);
    assert_eq!(fixture.selected(cx).as_deref(), Some("With actions"));

    // Escape dismisses the menu and returns focus to the tree, whose own
    // keys work again.
    fixture.press("escape", cx);
    assert!(!fixture.menu_open(cx));
    assert!(fixture.tree_focused(cx));
    fixture.press("down", cx);
    assert_eq!(fixture.selected(cx).as_deref(), Some("Without actions"));

    // The command opens the menu at the selection, and runs its first action.
    fixture.press("up", cx);
    let execution = dispatch_tree_command(
        &fixture.window,
        fixture.native_window,
        SHOW_TREE_CONTEXT_MENU_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    assert!(fixture.menu_open(cx));
    fixture.press("down enter", cx);
    fixture.wait_for_clipboard("copied one", cx);
}

#[gpui::test]
fn a_window_without_a_native_window_opens_no_menu(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let window = cx.update(|cx| {
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model, cx));
        cx.new(|cx| KnotWindow::new(workbench, cx))
    });
    load_source(&host, cx);
    window.update(cx, |window, cx| window.observe_application(cx));
    let tree = wait_for_contributed_view(&window, "example.actions", cx);
    wait_for_api_condition(cx, |cx| tree.read(cx).root_labels().len() == 2);
    cx.update(|cx| {
        assert!(tree.update(cx, |tree, cx| tree.select_item("with", cx)));
        let handle = tree.focus_handle(cx);
        window.update(cx, |window, cx| window.focus_view(&handle, cx));
    });

    let target = window.update(cx, |window, cx| window.capture_command_target(cx));
    let dispatcher = cx.read(|cx| cx.global::<ApplicationProductCommands>().0.clone());
    let execution = dispatcher.update(cx, |dispatcher, cx| {
        dispatcher.dispatch(
            Command {
                name: SHOW_TREE_CONTEXT_MENU_COMMAND.into(),
                arguments: CommandArgumentValue::Null,
            },
            target,
            cx,
        )
    });
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Unavailable);
    let keystroke = crate::app::keys::Keystroke::parse("shift-f10").unwrap();
    window.update(cx, |window, cx| window.press_key(keystroke, cx));
    cx.run_until_parked();
    assert!(!cx.read(|cx| tree.read(cx).context_menu_open()));
}

#[gpui::test]
fn a_long_error_row_keeps_the_height_of_the_rows_around_it(cx: &mut TestAppContext) {
    const FAILING: &str = r#"
import { workbench } from "knot";
await workbench.registerView({
  id: "example.failing",
  title: "Failing",
  content: { kind: "tree", provider: {
    getChildren(parentId) {
      if (parentId === "bad") throw new Error("a very long failure message ".repeat(20));
      return parentId === null ? [
        { id: "bad", label: "Bad", collapsibleState: "collapsed" },
        { id: "next", label: "Next", collapsibleState: "none" },
      ] : [];
    },
  } },
});
"#;
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(991), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, FAILING).unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
        })
        .detach();
    });
    let tree = wait_for_contributed_view(&window, "example.failing", cx);
    wait_for_api_condition(cx, |cx| tree.read(cx).root_labels() == ["Bad", "Next"]);
    cx.update_window(native_window, |_, native_window, cx| {
        let handle = tree.focus_handle(cx);
        handle.focus(native_window, cx);
        focus_view(&handle, native_window, cx);
    })
    .unwrap();
    cx.update(|cx| tree.update(cx, |tree, cx| tree.activate_row_for_test("bad", cx)));
    wait_for_api_condition(cx, |cx| !tree.read(cx).is_loading());
    cx.refresh().unwrap();
    let mut visual = VisualTestContext::from_window(native_window, cx);
    let row = visual.debug_bounds("extension-tree-row-0").unwrap();
    let status = visual.debug_bounds("extension-tree-status-1").unwrap();
    assert_eq!(status.size.height, row.size.height);
}
