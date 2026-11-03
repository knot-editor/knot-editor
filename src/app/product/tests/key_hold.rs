//! Ordered key delivery around key-invoked extension commands.
use super::*;
use crate::app::editor::{EditorView, Reveal};
use crate::host::lifecycle::ExtensionKey;

const SOURCE: &str = r#"
    import { commands, keymaps, workbench } from "knot";
    await commands.register("hold.home", async ({ editor }) => {
      await workbench.writeClipboardText("home");
      await editor.setSelection({ anchor: 0, caret: 0 });
    });
    await commands.register("hold.enter", ({ editor }) => editor.setKeyContext("modal", true));
    await commands.register("hold.left", async ({ editor }) => {
      const { caret } = await editor.selection();
      await editor.setSelection({ anchor: caret - 1, caret: caret - 1 });
    });
    keymaps.bind("f8", "hold.home");
    keymaps.bind("f7", "hold.enter");
    keymaps.bind("h", "hold.left", { view: "editor", context: "modal" });
"#;

struct Fixture {
    host: Entity<crate::app::extensions::host::ProductExtensionHost>,
    shell: Entity<ProductShell>,
    window: gpui::AnyWindowHandle,
    model: Entity<BufferModel>,
    editor: Entity<EditorView>,
}

/// One editor showing `text` with the caret at its end, focused, with the
/// fixture package loaded.
fn fixture(text: &str, cx: &mut TestAppContext) -> Fixture {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        let key = ExtensionKey::new(ExtensionId::new(732), ExtensionLifecycleId::new(1));
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    wait_for_api_condition(cx, |cx| {
        keymap_command(cx, "f8", Some("editor")).as_deref() == Some("hold.home")
    });
    let model = cx.new(|_| BufferModel::from_text(text));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Held", model.clone(), cx)
    });
    let (shell, window) = product_window(document, model.clone(), cx);
    let editor = focused_editor(&shell, cx);
    let end = text.len();
    focus_editor(&editor, end, window, cx);
    Fixture {
        host,
        shell,
        window,
        model,
        editor,
    }
}

fn focused_editor(shell: &Entity<ProductShell>, cx: &mut TestAppContext) -> Entity<EditorView> {
    cx.read(|cx| {
        shell
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .editor()
            .unwrap()
            .clone()
    })
}

fn focus_editor(
    editor: &Entity<EditorView>,
    caret: usize,
    window: gpui::AnyWindowHandle,
    cx: &mut TestAppContext,
) {
    cx.update_window(window, |_, window, cx| {
        editor.update(cx, |editor, cx| {
            editor.set_selection(caret, caret, Reveal::Nearest, cx);
            editor.focus_handle(cx).focus(window);
        })
    })
    .unwrap();
    cx.refresh().unwrap();
}

impl Fixture {
    fn text(&self, cx: &mut TestAppContext) -> String {
        cx.read(|cx| self.model.read(cx).text())
    }

    fn holds_keys(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| self.shell.read(cx).holds_keys())
    }

    fn settle(&self, cx: &mut TestAppContext) {
        wait_for_api_condition(cx, |cx| !self.shell.read(cx).holds_keys());
    }
}

#[gpui::test]
fn keys_and_text_after_an_extension_command_apply_after_it_in_order(cx: &mut TestAppContext) {
    let fixture = fixture("xyz", cx);
    // The second f8 is itself held, and holds what follows it in turn.
    cx.simulate_keystrokes(fixture.window, "f8 a f8 b");
    cx.simulate_input(fixture.window, "c");
    fixture.settle(cx);
    assert_eq!(fixture.text(cx), "bcaxyz");
}

#[gpui::test]
fn a_held_key_resolves_in_the_context_the_command_set(cx: &mut TestAppContext) {
    let fixture = fixture("abc", cx);
    cx.simulate_keystrokes(fixture.window, "f7 h x");
    fixture.settle(cx);
    assert_eq!(fixture.text(cx), "abxc");
}

#[gpui::test]
fn native_commands_do_not_hold_keys(cx: &mut TestAppContext) {
    let fixture = fixture("abc", cx);
    cx.simulate_keystrokes(fixture.window, "home");
    assert!(!fixture.holds_keys(cx));
    cx.read(|cx| assert_eq!(fixture.editor.read(cx).selection(cx), (0, 0)));
}

#[gpui::test]
fn the_timeout_releases_keys_while_the_command_still_runs(cx: &mut TestAppContext) {
    let fixture = fixture("xyz", cx);
    fixture
        .host
        .update(cx, |host, _| host.hold_clipboard_writes());
    cx.simulate_keystrokes(fixture.window, "f8 a");
    wait_for_api_condition(cx, |cx| {
        fixture.host.read(cx).pending_view_request_count() == 1
    });
    assert!(fixture.holds_keys(cx));
    assert_eq!(fixture.text(cx), "xyz");
    cx.executor()
        .advance_clock(super::super::key_hold::KEY_HOLD_TIMEOUT);
    cx.run_until_parked();
    assert!(!fixture.holds_keys(cx));
    assert_eq!(fixture.text(cx), "xyza");
    fixture
        .host
        .update(cx, |host, cx| host.release_view_requests(cx));
    cx.run_until_parked();
}

#[gpui::test]
fn a_key_after_focus_leaves_the_view_releases_the_held_keys(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::SPLIT_HORIZONTAL_COMMAND;
    let fixture = fixture("xyz", cx);
    let split =
        dispatch_product_command(&fixture.shell, fixture.window, SPLIT_HORIZONTAL_COMMAND, cx);
    assert_eq!(wait_for_command(split, cx), CommandOutcome::Completed);
    let other = focused_editor(&fixture.shell, cx);
    assert_ne!(other, fixture.editor);
    focus_editor(&fixture.editor, 3, fixture.window, cx);
    fixture
        .host
        .update(cx, |host, _| host.hold_clipboard_writes());
    cx.simulate_keystrokes(fixture.window, "f8 a");
    wait_for_api_condition(cx, |cx| {
        fixture.host.read(cx).pending_view_request_count() == 1
    });
    assert_eq!(fixture.text(cx), "xyz");

    // The held key and the new one both go where focus now is.
    focus_editor(&other, 0, fixture.window, cx);
    cx.simulate_keystrokes(fixture.window, "b");
    cx.run_until_parked();
    assert!(!fixture.holds_keys(cx));
    assert_eq!(fixture.text(cx), "abxyz");
    fixture
        .host
        .update(cx, |host, cx| host.release_view_requests(cx));
    cx.run_until_parked();
}
