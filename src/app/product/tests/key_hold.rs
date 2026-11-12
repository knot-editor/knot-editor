//! Ordered key delivery around key-invoked extension commands.
use super::*;
use crate::app::editor::{EditorView, Reveal};
use gpui::EntityInputHandler;
use knot_protocol::ExtensionKey;

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
    await commands.register("hold.ask", async () => {
      const input = await workbench.showInput({ title: "Ask" });
      await workbench.writeClipboardText(input.kind === "accepted" ? input.value : "dismissed");
    });
    keymaps.bind("f6", "hold.ask");
    keymaps.bind("f8", "hold.home");
    keymaps.bind("f7", "hold.enter");
    keymaps.bind("h", "hold.left", { view: "editor", context: "modal" });
    const typed = [];
    // Typing "i" leaves the context, so later keys type natively.
    await commands.register("hold.text", async ({ editor, arguments: args }) => {
      typed.push(args.text);
      if (args.text === "i") await editor.setKeyContext("modal", false);
    });
    await commands.register("hold.report", () => workbench.writeClipboardText(typed.join(",")));
    keymaps.bindText("hold.text", { view: "editor", context: "modal" });
"#;

const KEY: ExtensionKey = ExtensionKey::new(ExtensionId::new(732), ExtensionLifecycleId::new(1));

struct Fixture {
    host: Entity<crate::app::extensions::host::ProductExtensionHost>,
    window: Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    model: Entity<Buffer>,
    editor: Entity<EditorView>,
}

/// One editor showing `text` with the caret at its end, focused, with the
/// fixture package loaded.
fn fixture(text: &str, cx: &mut TestAppContext) -> Fixture {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(KEY, SOURCE).unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    wait_for_api_condition(cx, |cx| {
        keymap_command(cx, "f8", Some("editor")).as_deref() == Some("hold.home")
    });
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Held", model.clone(), cx)
    });
    let (window, native_window) = product_window(document, model.clone(), cx);
    let editor = focused_editor(&window, cx);
    let end = text.len();
    focus_editor(&editor, end, native_window, cx);
    Fixture {
        host,
        native_window,
        window,
        model,
        editor,
    }
}

fn focused_editor(window: &Entity<KnotWindow>, cx: &mut TestAppContext) -> Entity<EditorView> {
    cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
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
    cx.update_window(window, |_, native_window, cx| {
        editor.update(cx, |editor, cx| {
            editor.editor().clone().update(cx, |editor, cx| {
                editor.set_selection(caret, caret, Reveal::Nearest, cx)
            });
        });
        focus_view(&editor.focus_handle(cx), native_window, cx);
    })
    .unwrap();
    cx.refresh().unwrap();
}

impl Fixture {
    fn text(&self, cx: &mut TestAppContext) -> String {
        cx.read(|cx| self.model.read(cx).text())
    }

    fn holds_keys(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| self.window.read(cx).holds_keys())
    }

    fn settle(&self, cx: &mut TestAppContext) {
        wait_for_api_condition(cx, |cx| !self.window.read(cx).holds_keys());
    }
}

#[gpui::test]
fn keys_and_text_after_an_extension_command_apply_after_it_in_order(cx: &mut TestAppContext) {
    let fixture = fixture("xyz", cx);
    // The second f8 is itself held, and holds what follows it in turn.
    cx.simulate_keystrokes(fixture.native_window, "f8 a f8 b");
    cx.simulate_input(fixture.native_window, "c");
    fixture.settle(cx);
    assert_eq!(fixture.text(cx), "bcaxyz");
}

#[gpui::test]
fn a_held_key_resolves_in_the_context_the_command_set(cx: &mut TestAppContext) {
    let fixture = fixture("abc", cx);
    cx.simulate_keystrokes(fixture.native_window, "f7 h");
    fixture.settle(cx);
    assert_eq!(fixture.text(cx), "abc");
    cx.read(|cx| {
        assert_eq!(
            fixture.editor.read(cx).editor().read(cx).selection(cx),
            (2, 2)
        )
    });
}

#[gpui::test]
fn native_commands_do_not_hold_keys(cx: &mut TestAppContext) {
    let fixture = fixture("abc", cx);
    cx.simulate_keystrokes(fixture.native_window, "home");
    assert!(!fixture.holds_keys(cx));
    cx.read(|cx| {
        assert_eq!(
            fixture.editor.read(cx).editor().read(cx).selection(cx),
            (0, 0)
        )
    });
}

#[gpui::test]
fn the_timeout_releases_keys_while_the_command_still_runs(cx: &mut TestAppContext) {
    let fixture = fixture("xyz", cx);
    fixture
        .host
        .update(cx, |host, _| host.hold_clipboard_writes());
    cx.simulate_keystrokes(fixture.native_window, "f8 a");
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
    let split = dispatch_product_command(
        &fixture.window,
        fixture.native_window,
        SPLIT_HORIZONTAL_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(split, cx), CommandOutcome::Completed);
    let other = focused_editor(&fixture.window, cx);
    assert_ne!(other, fixture.editor);
    focus_editor(&fixture.editor, 3, fixture.native_window, cx);
    fixture
        .host
        .update(cx, |host, _| host.hold_clipboard_writes());
    cx.simulate_keystrokes(fixture.native_window, "f8 a");
    wait_for_api_condition(cx, |cx| {
        fixture.host.read(cx).pending_view_request_count() == 1
    });
    assert_eq!(fixture.text(cx), "xyz");

    // The held key and the new one both go where focus now is.
    focus_editor(&other, 0, fixture.native_window, cx);
    cx.simulate_keystrokes(fixture.native_window, "b");
    cx.run_until_parked();
    assert!(!fixture.holds_keys(cx));
    assert_eq!(fixture.text(cx), "abxyz");
    fixture
        .host
        .update(cx, |host, cx| host.release_view_requests(cx));
    cx.run_until_parked();
}

#[gpui::test]
fn keys_typed_ahead_of_an_input_reach_it(cx: &mut TestAppContext) {
    let fixture = fixture("xyz", cx);
    cx.simulate_keystrokes(fixture.native_window, "f6 a b enter");
    wait_for_api_condition(cx, |cx| cx.read_from_clipboard().is_some());
    assert_eq!(fixture.text(cx), "xyz");
    assert_eq!(
        cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        "ab"
    );
}

/// Named keys and text held behind a text-bound command replay in the order
/// they were pressed, once the command has left the context that claimed them.
#[gpui::test]
fn held_keys_and_text_after_a_text_command_replay_in_order(cx: &mut TestAppContext) {
    let fixture = fixture("", cx);
    cx.simulate_keystrokes(fixture.native_window, "f7");
    fixture.settle(cx);
    cx.simulate_input(fixture.native_window, "ix");
    cx.simulate_keystrokes(fixture.native_window, "enter");
    cx.simulate_input(fixture.native_window, "y");
    cx.simulate_keystrokes(fixture.native_window, "backspace");
    cx.simulate_input(fixture.native_window, "z");
    fixture.settle(cx);
    assert_eq!(fixture.text(cx), "x\nz");
}

#[gpui::test]
fn a_text_binding_receives_typed_and_composed_text_in_order(cx: &mut TestAppContext) {
    let fixture = fixture("abc", cx);
    let typed = |cx: &mut TestAppContext| {
        let report =
            dispatch_product_command(&fixture.window, fixture.native_window, "hold.report", cx);
        assert_eq!(wait_for_command(report, cx), CommandOutcome::Completed);
        cx.read(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .unwrap_or_default()
    };
    // Without the context nothing claims the text.
    cx.simulate_keystrokes(fixture.native_window, "x");
    fixture.settle(cx);
    assert_eq!(fixture.text(cx), "abcx");

    cx.simulate_keystrokes(fixture.native_window, "f7 a b");
    cx.simulate_input(fixture.native_window, "c");
    // A key bound in the context still wins over the text binding.
    cx.simulate_keystrokes(fixture.native_window, "h");
    cx.simulate_input(fixture.native_window, "e");
    fixture.settle(cx);
    assert_eq!(fixture.text(cx), "abcx");
    cx.read(|cx| {
        assert_eq!(
            fixture.editor.read(cx).editor().read(cx).selection(cx),
            (3, 3)
        )
    });
    assert_eq!(typed(cx), "a,b,c,e");

    // The preedit is not shown; the commit arrives as text.
    cx.update_window(fixture.native_window, |_, native_window, cx| {
        fixture.editor.update(cx, |editor, cx| {
            editor.replace_and_mark_text_in_range(None, "k", None, native_window, cx);
            assert_eq!(editor.marked_text_range(native_window, cx), None);
            editor.replace_text_in_range(None, "か", native_window, cx);
        })
    })
    .unwrap();
    fixture.settle(cx);
    assert_eq!(fixture.text(cx), "abcx");
    assert_eq!(typed(cx), "a,b,c,e,か");

    // Teardown restores typing.
    fixture
        .host
        .update(cx, |host, cx| host.unload(KEY, cx).unwrap());
    cx.simulate_keystrokes(fixture.native_window, "z");
    fixture.settle(cx);
    assert_eq!(fixture.text(cx), "abczx");
}
