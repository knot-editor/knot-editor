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
    shell: Entity<ProductShell>,
    window: gpui::AnyWindowHandle,
    tree: Entity<TreeView>,
}

impl Fixture {
    fn start(cx: &mut TestAppContext) -> Self {
        let documents = install_globals(cx);
        cx.update(super::super::bind_product_keys);
        let host = cx.update(super::super::super::extensions::host::install);
        let document = cx.update(|cx| create_untitled_document(&documents, cx));
        let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
        let (shell, window) = product_window(document, model, cx);
        load_source(&host, cx);
        let tree = wait_for_contributed_view(&shell, "example.actions", cx);
        wait_for_api_condition(cx, |cx| tree.read(cx).root_labels().len() == 2);
        cx.update_window(window, |_, window, cx| {
            let handle = tree.focus_handle(cx);
            handle.focus(window, cx);
            focus_view(&handle, window, cx);
        })
        .unwrap();
        Self {
            shell,
            window,
            tree,
        }
    }

    fn row(&self, index: usize, cx: &mut TestAppContext) -> gpui::Point<gpui::Pixels> {
        cx.run_until_parked();
        cx.refresh().unwrap();
        let selector = if index == 0 {
            "extension-tree-row-0"
        } else {
            "extension-tree-row-1"
        };
        VisualTestContext::from_window(self.window, cx)
            .debug_bounds(selector)
            .expect("the row is laid out")
            .center()
    }

    fn right_click(&self, index: usize, cx: &mut TestAppContext) {
        let row = self.row(index, cx);
        let mut window_cx = VisualTestContext::from_window(self.window, cx);
        window_cx.simulate_mouse_down(row, MouseButton::Right, Modifiers::default());
        window_cx.simulate_mouse_up(row, MouseButton::Right, Modifiers::default());
        cx.run_until_parked();
    }

    fn press(&self, keys: &str, cx: &mut TestAppContext) {
        cx.refresh().unwrap();
        cx.simulate_keystrokes(self.window, keys);
        // A key can run a command on the package's worker thread, whose
        // completion settles focus and the menu.
        crate::app::product::driver::settle(&self.shell, cx);
    }

    fn menu_open(&self, cx: &TestAppContext) -> bool {
        cx.read(|cx| self.tree.read(cx).context_menu_open())
    }

    fn tree_focused(&self, cx: &mut TestAppContext) -> bool {
        cx.update_window(self.window, |_, window, cx| {
            self.tree.focus_handle(cx).is_focused(window)
        })
        .unwrap()
    }

    fn selected(&self, cx: &TestAppContext) -> Option<String> {
        cx.read(|cx| self.tree.read(cx).selected_text())
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
        &fixture.shell,
        fixture.window,
        SHOW_TREE_CONTEXT_MENU_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Unavailable);
    assert!(!fixture.menu_open(cx));
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
        &fixture.shell,
        fixture.window,
        SHOW_TREE_CONTEXT_MENU_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    assert!(fixture.menu_open(cx));
    fixture.press("down enter", cx);
    fixture.wait_for_clipboard("copied one", cx);
}

#[gpui::test]
fn a_session_without_a_window_opens_no_menu(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let shell = cx.update(|cx| {
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model, cx));
        cx.new(|cx| ProductShell::new(workbench, cx))
    });
    load_source(&host, cx);
    shell.update(cx, |shell, cx| shell.observe_application(cx));
    let tree = wait_for_contributed_view(&shell, "example.actions", cx);
    wait_for_api_condition(cx, |cx| tree.read(cx).root_labels().len() == 2);
    cx.update(|cx| {
        assert!(tree.update(cx, |tree, cx| tree.select_item("with", cx)));
        let handle = tree.focus_handle(cx);
        shell.update(cx, |shell, cx| shell.focus_view(&handle, cx));
    });

    let target = shell.update(cx, |shell, cx| shell.capture_command_target(cx));
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
    shell.update(cx, |shell, cx| shell.press_key(keystroke, cx));
    cx.run_until_parked();
    assert!(!cx.read(|cx| tree.read(cx).context_menu_open()));
}
