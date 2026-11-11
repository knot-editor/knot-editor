//! Editor bars through registered extension commands: stacking and updates
//! across lifecycles, field focus, typing reaching the edit listener, and
//! teardown.
use super::*;
use crate::app::editor::{Editor, EditorView};
use crate::app::keys::Keystroke;
use knot_model::editor::bars::{BarField, BarFieldRef, BarSpec};
use knot_protocol::ExtensionKey;

/// A package whose commands, named after `LABEL`, drive one bar named
/// "find" through a long-lived editor handle that reports edits on the
/// clipboard.
const SOURCE: &str = r#"
import { commands, workbench } from "knot";
const check = (condition, message) => { if (!condition) throw new Error(message); };
const expectError = async (operation, test, message) => {
  try { await operation(); } catch (error) { if (test(error)) return; throw error; }
  throw new Error(message);
};
let handle = null;
let edits = 0;
const editor = async () => {
  if (!handle) {
    const { snapshot } = await workbench.observe(() => {});
    handle = await workbench.editor(snapshot.windows[0].panes[0].selected);
    await handle.onDidEditBar(async ({ bar, fields }) => {
      edits += 1;
      await workbench.writeClipboardText(`LABEL:${bar}:${fields.query}:${edits}`);
    });
  }
  return handle;
};
const query = { id: "query", placeholder: "Find" };
await commands.register("LABEL.open", async () => {
  await (await editor()).openBar("find", { fields: [query] });
});
await commands.register("LABEL.update", async () => {
  await (await editor()).openBar("find", { fields: [query], message: "3 of 17" });
});
await commands.register("LABEL.focus", async () => {
  await (await editor()).focusBarField("find", "query", { text: "seed\n" });
});
await commands.register("LABEL.close", async () => { await (await editor()).closeBar("find"); });
await commands.register("LABEL.read", async () => {
  const fields = await (await editor()).barFields("find");
  await workbench.writeClipboardText(`read:${fields.query}`);
});
await commands.register("LABEL.validate", async ({ editor: captured }) => {
  await expectError(() => captured.openBar("", {}), (e) => e instanceof TypeError, "empty id");
  await expectError(() => captured.openBar("x", { fields: [{ id: 1 }] }), (e) => e instanceof TypeError, "field id type");
  await expectError(() => captured.openBar("x", { fields: [{ id: "a" }, { id: "a" }] }),
    (e) => e instanceof RangeError, "repeated field ids");
  await expectError(() => captured.openBar("x", { fields: [{ id: "" }] }), (e) => e instanceof RangeError, "empty field id");
  await expectError(() => captured.focusBarField("missing", "query"), (e) => e.code === "BarNotFound", "missing bar");
  await expectError(() => captured.barFields("missing"), (e) => e.code === "BarNotFound", "missing bar fields");
  await captured.closeBar("missing");
  // A command's editor opens bars too; they outlive the command.
  await captured.openBar("other", { message: "from a command" });
  await expectError(() => captured.focusBarField("other", "query"), (e) => e.code === "BarNotFound", "missing field");
  check(Object.keys(await captured.barFields("other")).length === 0, "no fields");
});
"#;

fn load(label: &str, id: u64, cx: &mut TestAppContext) -> ExtensionKey {
    let host = cx.read(|cx| crate::app::extensions::host::entity(cx).unwrap());
    let key = ExtensionKey::new(ExtensionId::new(id), ExtensionLifecycleId::new(1));
    let source = SOURCE.replace("LABEL", label);
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, &source).unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    let command = format!("{label}.validate");
    wait_for_api_condition(cx, |cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog()
            .borrow()
            .resolve(&command)
            .is_ok()
    });
    key
}

struct Fixture {
    shell: Entity<ProductShell>,
    window: gpui::AnyWindowHandle,
    editor: Entity<Editor>,
    model: Entity<Buffer>,
}

impl Fixture {
    fn new(cx: &mut TestAppContext) -> Self {
        let documents = install_globals(cx);
        cx.update(super::super::bind_product_keys);
        let document = cx.update(|cx| create_untitled_document(&documents, cx));
        let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
        model.update(cx, |model, _| model.replace(0..0, "text").unwrap());
        cx.update(super::super::super::extensions::host::install);
        let (shell, window) = product_window(document, model.clone(), cx);
        cx.run_until_parked();
        let editor = super::editor_api::active_editor(&shell, cx);
        Self {
            shell,
            window,
            editor,
            model,
        }
    }

    fn run(&self, name: &'static str, cx: &mut TestAppContext) {
        let execution = dispatch_product_command(&self.shell, self.window, name, cx);
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        cx.run_until_parked();
    }

    fn view(&self, cx: &TestAppContext) -> Entity<EditorView> {
        cx.read(|cx| {
            self.shell
                .read(cx)
                .workbench
                .read(cx)
                .focused_editor()
                .unwrap()
                .clone()
        })
    }

    /// Each bar's owner, ID, and message, top to bottom.
    fn bars(&self, cx: &TestAppContext) -> Vec<(ExtensionKey, String, Option<String>)> {
        cx.read(|cx| {
            self.editor
                .read(cx)
                .bars()
                .bars()
                .iter()
                .map(|bar| (bar.owner, bar.id.clone(), bar.spec.message.clone()))
                .collect()
        })
    }

    fn field_text(&self, owner: ExtensionKey, cx: &TestAppContext) -> String {
        cx.read(|cx| {
            self.editor
                .read(cx)
                .bar_input(&BarFieldRef::new(owner, "find", "query"))
                .unwrap()
                .read(cx)
                .text()
                .to_owned()
        })
    }

    /// Whether the window focuses `owner`'s query field, and whether the
    /// editor's focus is on it.
    fn field_focus(&self, owner: ExtensionKey, cx: &mut TestAppContext) -> (bool, bool) {
        let field = BarFieldRef::new(owner, "find", "query");
        let handle = cx.read(|cx| {
            self.editor
                .read(cx)
                .bar_input(&field)
                .map(|input| input.read(cx).focus_handle())
        });
        let window_focus = cx
            .update_window(self.window, |_, window, _| {
                handle.is_some_and(|handle| handle.is_focused(window))
            })
            .unwrap();
        let editor_focus = cx.read(|cx| self.editor.read(cx).bars().focused() == Some(&field));
        (window_focus, editor_focus)
    }

    fn text_focused(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view(cx);
        let handle = cx.read(|cx| view.read(cx).focus_handle(cx));
        cx.update_window(self.window, |_, window, _| handle.is_focused(window))
            .unwrap()
    }
}

fn clipboard(cx: &TestAppContext) -> Option<String> {
    cx.read(|cx| cx.read_from_clipboard()?.text())
}

#[gpui::test]
fn bars_from_two_lifecycles_stack_update_and_end_with_their_lifecycle(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    let first = load("first", 801, cx);
    let second = load("second", 802, cx);
    fixture.run("first.open", cx);
    fixture.run("second.open", cx);
    fixture.run("first.update", cx);
    assert_eq!(
        fixture.bars(cx),
        [
            (first, "find".to_owned(), Some("3 of 17".to_owned())),
            (second, "find".to_owned(), None),
        ]
    );

    // The bars paint above the text, which shrinks to fit below them.
    let view = fixture.view(cx);
    let window_height = cx
        .update_window(fixture.window, |_, window, _| window.viewport_size().height)
        .unwrap();
    let text = cx.read(|cx| view.read(cx).interaction_bounds());
    let field = cx.read(|cx| {
        fixture
            .editor
            .read(cx)
            .bar_input(&BarFieldRef::new(second, "find", "query"))
            .unwrap()
            .read(cx)
            .painted_bounds()
            .unwrap()
    });
    assert!(field.bottom() <= text.top(), "{field:?} above {text:?}");
    assert!(text.bottom() <= window_height);

    fixture.run("first.validate", cx);
    assert_eq!(fixture.bars(cx).len(), 3);
    let host = cx.read(|cx| crate::app::extensions::host::entity(cx).unwrap());
    host.update(cx, |host, cx| host.unload(first, cx).unwrap());
    assert_eq!(fixture.bars(cx), [(second, "find".to_owned(), None)]);
    fixture.run("second.close", cx);
    assert!(fixture.bars(cx).is_empty());
}

#[gpui::test]
fn typing_in_a_focused_field_reaches_the_listener_and_clicks_move_focus(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    let owner = load("find", 803, cx);
    fixture.run("find.open", cx);
    assert!(fixture.text_focused(cx));

    // Replacing the field's text focuses it, selected, and is no edit.
    fixture.run("find.focus", cx);
    assert_eq!(fixture.field_text(owner, cx), "seed");
    assert_eq!(fixture.field_focus(owner, cx), (true, true));
    assert_eq!(clipboard(cx), None);

    cx.simulate_input(fixture.window, "a");
    assert_eq!(fixture.field_text(owner, cx), "a");
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard().and_then(|item| item.text()) == Some("find:find:a:1".to_owned())
    });
    assert_eq!(cx.read(|cx| fixture.model.read(cx).text()), "text");

    // While a listener turn runs, more edits join one further notification,
    // which reads the final text.
    let host = cx.read(|cx| crate::app::extensions::host::entity(cx).unwrap());
    host.update(cx, |host, _| host.hold_clipboard_writes());
    cx.simulate_input(fixture.window, "b");
    wait_for_api_condition(cx, |cx| host.read(cx).pending_view_request_count() == 1);
    cx.simulate_input(fixture.window, "c");
    cx.simulate_input(fixture.window, "d");
    cx.run_until_parked();
    host.update(cx, |host, cx| host.release_view_requests(cx));
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard().and_then(|item| item.text()) == Some("find:find:abcd:3".to_owned())
    });
    fixture.run("find.read", cx);
    assert_eq!(clipboard(cx).as_deref(), Some("read:abcd"));

    // Clicking the text gives it focus, and the bar stays open; clicking
    // the field takes focus back.
    let view = fixture.view(cx);
    let text = cx.read(|cx| view.read(cx).text_bounds());
    let mut visual = VisualTestContext::from_window(fixture.window, cx);
    visual.simulate_click(text.center(), Modifiers::default());
    visual.run_until_parked();
    assert_eq!(fixture.field_focus(owner, cx), (false, false));
    assert!(fixture.text_focused(cx));
    assert_eq!(fixture.bars(cx).len(), 1);
    let field = cx.read(|cx| {
        fixture
            .editor
            .read(cx)
            .bar_input(&BarFieldRef::new(owner, "find", "query"))
            .unwrap()
            .read(cx)
            .painted_bounds()
            .unwrap()
    });
    let mut visual = VisualTestContext::from_window(fixture.window, cx);
    visual.simulate_click(field.center(), Modifiers::default());
    visual.run_until_parked();
    assert_eq!(fixture.field_focus(owner, cx), (true, true));

    // Closing the bar returns focus to the text, where the click left the caret.
    fixture.run("find.close", cx);
    assert!(fixture.text_focused(cx));
    cx.simulate_input(fixture.window, "!");
    assert_eq!(cx.read(|cx| fixture.model.read(cx).text()), "text!");
}

#[gpui::test]
fn a_session_without_a_window_types_into_the_focused_field(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let shell = cx.update(|cx| {
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model.clone(), cx));
        cx.new(|cx| ProductShell::new(workbench, cx))
    });
    let editor = super::editor_api::active_editor(&shell, cx);
    let owner = ExtensionKey::new(ExtensionId::new(804), ExtensionLifecycleId::new(1));
    let field = BarFieldRef::new(owner, "find", "query");
    editor.update(cx, |editor, cx| {
        let spec = BarSpec {
            fields: vec![BarField {
                id: "query".into(),
                placeholder: String::new(),
            }],
            message: None,
        };
        editor.open_bar(owner, "find", spec, cx);
        assert!(editor.focus_bar_field(field.clone(), None, cx));
    });
    for key in ["x", "y", "z"] {
        let keystroke = Keystroke::parse(key).unwrap();
        shell.update(cx, |shell, cx| shell.press_key(keystroke, cx));
    }
    let text = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            editor
                .read(cx)
                .bar_input(&field)
                .unwrap()
                .read(cx)
                .text()
                .to_owned()
        })
    };
    assert_eq!(text(cx), "xyz");
    assert_eq!(cx.read(|cx| model.read(cx).text()), "");

    // With focus back on the text, keys type there.
    editor.update(cx, |editor, cx| editor.focus_text(cx));
    shell.update(cx, |shell, cx| {
        shell.press_key(Keystroke::parse("q").unwrap(), cx)
    });
    assert_eq!(text(cx), "xyz");
    assert_eq!(cx.read(|cx| model.read(cx).text()), "q");
}

/// Two bars with three fields, and bindings scoped to the `bar` kind, the
/// bar's context, the editor kind, and everywhere.
const KEYS_SOURCE: &str = r#"
import { commands, keymaps, workbench } from "knot";
let handle = null;
const editor = async () => {
  if (!handle) {
    const { snapshot } = await workbench.observe(() => {});
    handle = await workbench.editor(snapshot.windows[0].panes[0].selected);
  }
  return handle;
};
const note = (name) => async ({ editor: captured, buffer }) => {
  const { text } = await buffer.snapshot();
  await workbench.writeClipboardText(`${name}:${text}:${captured ? "editor" : "none"}`);
};
await commands.register("keys.open", async () => {
  const handle = await editor();
  await handle.openBar("find", { fields: [{ id: "query" }, { id: "other" }] });
  await handle.openBar("goto", { fields: [{ id: "line" }] });
  await handle.focusBarField("find", "query");
});
await commands.register("keys.enter", note("enter"));
await commands.register("keys.editor", note("editor"));
await commands.register("keys.global", note("global"));
await commands.register("keys.wrong", note("wrong"));
keymaps.bind("enter", "keys.enter", { view: "bar", context: "find" });
keymaps.bind("f6", "keys.wrong", { view: "bar", context: "goto" });
keymaps.bind("f7", "keys.editor", { view: "editor" });
keymaps.bind("f9", "keys.global");
"#;

#[gpui::test]
fn bar_fields_resolve_keys_against_the_bar_kind_and_cycle_with_tab(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    let host = cx.read(|cx| crate::app::extensions::host::entity(cx).unwrap());
    let owner = ExtensionKey::new(ExtensionId::new(805), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(owner, KEYS_SOURCE).unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    wait_for_api_condition(cx, |cx| {
        keymap_command(cx, "f9", Some("editor")).as_deref() == Some("keys.global")
    });
    wait_for_api_condition(cx, |cx| {
        keymap_command(cx, "f7", Some("editor")).as_deref() == Some("keys.editor")
    });
    fixture.run("keys.open", cx);
    let focused = |cx: &TestAppContext| {
        cx.read(|cx| {
            fixture
                .editor
                .read(cx)
                .bars()
                .focused()
                .map(|field| (field.bar.clone(), field.field.clone()))
        })
    };
    assert_eq!(focused(cx), Some(("find".into(), "query".into())));

    // The editor kind stops applying, and a bar's context-bound key runs its
    // command in the bar's editor.
    cx.simulate_keystrokes(fixture.window, "f7");
    cx.run_until_parked();
    assert_eq!(clipboard(cx), None);
    cx.simulate_keystrokes(fixture.window, "enter");
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard().and_then(|item| item.text()) == Some("enter:text:editor".into())
    });
    assert_eq!(cx.read(|cx| fixture.model.read(cx).text()), "text");

    // A global binding applies, and unbound keys reach the field.
    cx.simulate_keystrokes(fixture.window, "f9");
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard().and_then(|item| item.text()) == Some("global:text:editor".into())
    });
    // Later input waits for the key's command to complete.
    wait_for_api_condition(cx, |cx| fixture.shell.read(cx).key_hold.is_none());
    assert_eq!(fixture.field_focus(owner, cx), (true, true));
    cx.simulate_input(fixture.window, "ab");
    assert_eq!(
        fixture.field_text(owner, cx),
        "ab",
        "typed into the query field"
    );
    assert_eq!(cx.read(|cx| fixture.model.read(cx).text()), "text");

    // Tab and shift-tab cycle the fields across both bars, wrapping, and
    // never indent; another bar's context does not apply.
    for expected in ["other", "line", "query"] {
        cx.simulate_keystrokes(fixture.window, "tab");
        assert_eq!(focused(cx).unwrap().1, expected);
    }
    cx.simulate_keystrokes(fixture.window, "shift-tab");
    assert_eq!(focused(cx), Some(("goto".into(), "line".into())));
    cx.simulate_keystrokes(fixture.window, "shift-tab");
    assert_eq!(focused(cx), Some(("find".into(), "other".into())));
    assert_eq!(cx.read(|cx| fixture.model.read(cx).text()), "text");
    cx.simulate_keystrokes(fixture.window, "f6");
    cx.run_until_parked();
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("global:text:editor".into()),
        "the goto bar's binding does not apply in the find bar"
    );
}
