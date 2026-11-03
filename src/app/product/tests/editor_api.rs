//! The extension editor handle and buffer line lookups, through registered
//! extension commands.
use super::*;

const SOURCE: &str = r#"
import { commands, workbench } from "knot";
const check = (condition, message) => { if (!condition) throw new Error(message); };
const expectCode = async (operation, code) => {
  try { await operation(); } catch (error) { if (error.code === code) return; throw error; }
  throw new Error(`expected ${code}`);
};
await commands.register("api.lines", async ({ buffer }) => {
  // "a\r\n" "é😀\n" "third": CRLF and multibyte lines.
  check(await buffer.lineCount() === 3, "line count");
  const range = async (line) => { const value = await buffer.lineRange(line); return [value.startByteOffset, value.endByteOffset]; };
  check(JSON.stringify(await range(0)) === "[0,1]", "CRLF terminator excluded");
  check(JSON.stringify(await range(1)) === "[3,9]", "multibyte line");
  check(JSON.stringify(await range(2)) === "[10,15]", "last line");
  await expectCode(() => buffer.lineRange(3), "InvalidRange");
  check(await buffer.lineAt(0) === 0 && await buffer.lineAt(2) === 0, "line of a terminator");
  check(await buffer.lineAt(3) === 1 && await buffer.lineAt(15) === 2, "line of offsets");
  await expectCode(() => buffer.lineAt(4), "InvalidRange");
  await expectCode(() => buffer.lineAt(16), "InvalidRange");
  const snapshot = await buffer.snapshot(await buffer.lineRange(1));
  check(snapshot.text === "é😀" && snapshot.byteOffsetAtUtf16(1) === 2, "line snapshot");
});
let saved;
await commands.register("api.editor", async ({ editor, buffer }) => {
  const selection = await editor.selection();
  check(selection.anchor === 2 && selection.caret === 6, "initial selection");
  saved = await editor.viewState();
  const target = (await buffer.lineRange(150)).startByteOffset;
  await editor.setSelection({ anchor: target, caret: target }, { reveal: "center" });
  const moved = await editor.selection();
  check(moved.anchor === target && moved.caret === target, "moved selection");
  await expectCode(() => editor.setSelection({ anchor: 0, caret: 1_000_000 }), "InvalidRange");
});
await commands.register("api.restore", async ({ editor }) => {
  await editor.restoreViewState(saved);
});
await commands.register("api.stale", async ({ editor }) => {
  const state = await editor.viewState();
  await workbench.writeClipboardText("waiting");
  for (const operation of [() => editor.selection(), () => editor.viewState(), () => editor.restoreViewState(state), () => editor.setSelection({ anchor: 0, caret: 0 })]) {
    await expectCode(operation, "InvalidTarget");
  }
  await workbench.writeClipboardText("InvalidTarget");
});
await commands.register("api.noeditor", async ({ editor }) => {
  if (editor === null) commands.unavailable();
});
"#;

fn load(cx: &mut TestAppContext) -> Entity<crate::app::extensions::host::ProductExtensionHost> {
    use crate::host::lifecycle::ExtensionKey;
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source(
                ExtensionKey::new(ExtensionId::new(720), ExtensionLifecycleId::new(1)),
                SOURCE,
            )
            .unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
        })
        .detach();
    });
    wait_for_api_condition(cx, |cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog()
            .borrow()
            .resolve("api.noeditor")
            .is_ok()
    });
    host
}

fn active_editor(
    shell: &Entity<ProductShell>,
    cx: &mut TestAppContext,
) -> Entity<crate::app::editor::EditorView> {
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

#[gpui::test]
fn buffer_line_lookups_handle_crlf_and_multibyte_text(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    load(cx);
    let model = cx.new(|_| BufferModel::from_text("a\r\né😀\nthird"));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Lines", model.clone(), cx)
    });
    let (shell, window) = product_window(document, model, cx);
    let execution = dispatch_product_command(&shell, window, "api.lines", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
}

#[gpui::test]
fn editor_handle_reads_moves_and_restores_the_captured_view(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    load(cx);
    let text = (0..200)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let model = cx.new(|_| BufferModel::from_text(text));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Long", model.clone(), cx)
    });
    let (shell, window) = product_window(document, model.clone(), cx);
    cx.run_until_parked();
    let editor = active_editor(&shell, cx);
    let original = crate::app::editor::EditorViewState {
        anchor: 2,
        caret: 6,
        top: 0,
        scroll_x: 0.,
    };
    editor.update(cx, |editor, cx| editor.restore_view_state(original, cx));

    let execution = dispatch_product_command(&shell, window, "api.editor", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    let target = cx.read(|cx| model.read(cx).line_range(150).start);
    cx.read(|cx| {
        let state = editor.read(cx).view_state(cx);
        assert_eq!((state.anchor, state.caret), (target, target));
        // The off-screen caret's line is centered rather than at an edge.
        let top_line = model.read(cx).line_of_offset(state.top);
        assert!(top_line > 100 && top_line < 150, "top line {top_line}");
    });

    let execution = dispatch_product_command(&shell, window, "api.restore", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.read(|cx| assert_eq!(editor.read(cx).view_state(cx), original));
}

#[gpui::test]
fn editor_handle_rejects_after_its_view_closes(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let host = load(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
    host.update(cx, |host, _| host.hold_clipboard_writes());
    let execution = dispatch_product_command(&shell, window, "api.stale", cx);
    wait_for_api_condition(cx, |cx| host.read(cx).pending_view_request_count() == 1);
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    host.update(cx, |host, cx| host.release_view_requests(cx));
    assert_eq!(
        wait_for_command(execution, cx),
        CommandOutcome::InvalidTarget
    );
    cx.read(|cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "InvalidTarget"
        )
    });
}

#[gpui::test]
fn commands_on_other_surfaces_have_no_editor(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    load(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
    cx.update_window(window, |_, window, cx| {
        shell.update(cx, |shell, cx| {
            let (session, view) = shell.create_terminal(cx);
            let pane = shell.workbench.read(cx).focused_pane_id().unwrap();
            let tab = shell.workbench.update(cx, |workbench, _| {
                workbench.open_terminal_tab(pane, session, view).unwrap()
            });
            shell.activate_tab(pane, tab, window, cx);
        })
    })
    .unwrap();
    let execution = dispatch_product_command(&shell, window, "api.noeditor", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Unavailable);
}

const HANDLE_SOURCE: &str = r#"
import { commands, workbench } from "knot";
let handle;
let closed = 0;
await commands.register("handle.attach", async () => {
  const { snapshot } = await workbench.observe(() => {});
  handle = await workbench.editor(snapshot.windows[0].panes[0].selected);
  handle.onDidClose(() => { closed += 1; });
  await handle.setKeyContext("held", true);
});
await commands.register("handle.move", async () => {
  await handle.setSelection({ anchor: 1, caret: 1 });
});
await commands.register("handle.report", async () => {
  let code = "ok";
  try { await handle.selection(); } catch (error) { code = error.code; }
  let other = "ok";
  try { await workbench.editor({}); } catch (error) { other = error.code; }
  await workbench.writeClipboardText(`${code}:${closed}:${other}`);
});
"#;

fn load_handle_extension(
    cx: &mut TestAppContext,
) -> (
    Entity<crate::app::extensions::host::ProductExtensionHost>,
    crate::host::lifecycle::ExtensionKey,
) {
    use crate::host::lifecycle::ExtensionKey;
    let host = cx.update(super::super::super::extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(721), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, HANDLE_SOURCE).unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    wait_for_api_condition(cx, |cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog()
            .borrow()
            .resolve("handle.report")
            .is_ok()
    });
    (host, key)
}

fn handle_report(
    shell: &Entity<ProductShell>,
    window: gpui::AnyWindowHandle,
    cx: &mut TestAppContext,
) -> String {
    let execution = dispatch_product_command(shell, window, "handle.report", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap())
}

#[gpui::test]
fn editor_handle_outlives_its_command_and_rejects_once_its_tab_is_gone(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (_host, _) = load_handle_extension(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
    // A second tab, so that moving the active one leaves its pane occupied.
    let second_model = cx.new(|_| BufferModel::from_text("abc"));
    let second = documents.update(cx, |documents, cx| {
        documents.create_untitled("Second", second_model.clone(), cx)
    });
    shell.update(cx, |shell, cx| {
        let pane = shell.workbench.read(cx).focused_pane_id().unwrap();
        shell.workbench.update(cx, |workbench, cx| {
            workbench
                .open_tab_for_document(pane, second, second_model, cx)
                .unwrap();
        });
    });
    cx.run_until_parked();
    let editor = active_editor(&shell, cx);

    let execution = dispatch_product_command(&shell, window, "handle.attach", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.read(|cx| assert_eq!(editor.read(cx).key_contexts(), ["held"]));

    // A later command acts through the handle; the target is not rejected
    // for lacking a capture, and a non-target is an InvalidTarget.
    let execution = dispatch_product_command(&shell, window, "handle.move", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.read(|cx| assert_eq!(editor.read(cx).selection(cx), (1, 1)));
    assert_eq!(handle_report(&shell, window, cx), "ok:0:InvalidTarget");

    // Moving the tab to another pane invalidates the handle, once.
    let execution = dispatch_product_command(&shell, window, "workbench.move-tab-right", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.run_until_parked();
    assert_eq!(
        handle_report(&shell, window, cx),
        "InvalidTarget:1:InvalidTarget"
    );
    assert_eq!(
        handle_report(&shell, window, cx),
        "InvalidTarget:1:InvalidTarget"
    );
}

#[gpui::test]
fn editor_handle_rejects_after_closure_replaces_its_tab(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (_host, _) = load_handle_extension(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
    cx.run_until_parked();
    let execution = dispatch_product_command(&shell, window, "handle.attach", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);

    // Closing the only tab makes the shell present a fresh untitled one; the
    // handle never follows it.
    let execution = dispatch_product_command(&shell, window, "workbench.close-tab", cx);
    wait_for_command(execution, cx);
    cx.run_until_parked();
    assert_eq!(
        handle_report(&shell, window, cx),
        "InvalidTarget:1:InvalidTarget"
    );
}

#[gpui::test]
fn editor_handle_key_contexts_end_with_their_lifecycle(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (host, key) = load_handle_extension(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
    cx.run_until_parked();
    let editor = active_editor(&shell, cx);
    let execution = dispatch_product_command(&shell, window, "handle.attach", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.read(|cx| assert_eq!(editor.read(cx).key_contexts(), ["held"]));
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    cx.read(|cx| assert!(editor.read(cx).key_contexts().is_empty()));
}
