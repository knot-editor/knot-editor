//! Public extension API capture, target authority, ownership, and unload.
use super::*;

#[gpui::test]
fn public_api_capture_survives_focus_and_nested_handlers_without_leaking_to_callbacks(
    cx: &mut TestAppContext,
) {
    use knot_protocol::ExtensionKey;
    const SOURCE: &str = r#"
import { commands, editor, workbench } from "knot";
async function expectCode(operation, code) {
  try { await operation(); } catch (error) { if (error.code === code) return; throw error; }
  throw new Error(`expected ${code}`);
}
await expectCode(() => editor.activeBuffer(), "ContextUnavailable");
await expectCode(() => workbench.selectedText(), "ContextUnavailable");
await expectCode(() => commands.invoke("file.new"), "ContextUnavailable");
await workbench.writeClipboardText("activation");
let origin;
let previous;
await workbench.registerView({
  id: "example.outline",
  title: "Context-free",
  content: { kind: "tree", provider: {
async getChildren(parentId) {
  await expectCode(() => editor.activeBuffer(), "ContextUnavailable");
  await expectCode(() => workbench.selectedText(), "ContextUnavailable");
  return parentId === null ? [{ id: "provider", label: "context-free", collapsibleState: "none" }] : [];
},
  } },
});
await editor.registerCompletionProvider("context-free", {
  async provideCompletions(context) {
await expectCode(() => editor.activeBuffer(), "ContextUnavailable");
await expectCode(() => workbench.selectedText(), "ContextUnavailable");
await expectCode(() => editor.activeBuffer(origin.target), "InvalidTarget");
await context.buffer.snapshot();
return [{ label: "context-free", insertText: "context-free" }];
  },
});
await commands.register("api.inline", async (context) => {
  if (context.buffer !== origin.buffer || context.target.window !== origin.target.window) throw new Error("inline capture");
  if (await editor.activeBuffer(origin.target) !== origin.buffer) throw new Error("inherited authority");
});
await commands.register("api.capture", async (context) => {
  if (!Object.isFrozen(context.target) || typeof context.target.window !== "string") throw new Error("capture shape");
  if (previous) await expectCode(() => editor.activeBuffer(previous), "InvalidTarget");
  previous = context.target;
  origin = context;
  await expectCode(() => editor.activeBuffer({ ...context.target }), "InvalidTarget");
  await expectCode(() => workbench.selectedText(context.target), "UnsupportedOperation");
  await workbench.writeClipboardText("awaiting");
  if (await editor.activeBuffer() !== context.buffer || await editor.activeBuffer(context.target) !== context.buffer) throw new Error("focus redirected capture");
  const snapshot = await context.buffer.snapshot();
  if (snapshot.text !== "first") throw new Error(`wrong buffer: ${snapshot.text}`);
  if ((await commands.invoke("api.inline")).kind !== "completed") throw new Error("inline failed");
  if ((await commands.invoke("api.cross", { window: context.target.window })).kind !== "completed") throw new Error("cross failed");
  await context.buffer.onDidChange(async (change) => {
await expectCode(() => editor.activeBuffer(), "ContextUnavailable");
await expectCode(() => editor.activeBuffer(context.target), "InvalidTarget");
await expectCode(() => workbench.selectedText(), "ContextUnavailable");
await workbench.writeClipboardText((await change.buffer.snapshot()).text);
  });
});
"#;
    const CROSS: &str = r#"
import { commands, editor, workbench } from "knot";
await commands.register("api.cross", async (context) => {
  if (context.target.window !== context.arguments.window || await editor.activeBuffer(context.target) !== context.buffer) throw new Error("cross capture");
  await workbench.writeClipboardText((await context.buffer.snapshot()).text);
});
"#;
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let first_document = cx.update(|cx| create_untitled_document(&documents, cx));
    let first_model = cx.read(|cx| {
        documents
            .read(cx)
            .get(first_document)
            .unwrap()
            .model()
            .clone()
    });
    first_model.update(cx, |model, _| model.replace(0..0, "first").unwrap());
    let (first_window, first_native_window) =
        product_window(first_document, first_model.clone(), cx);
    let second_document = cx.update(|cx| create_untitled_document(&documents, cx));
    let second_model = cx.read(|cx| {
        documents
            .read(cx)
            .get(second_document)
            .unwrap()
            .model()
            .clone()
    });
    second_model.update(cx, |model, _| model.replace(0..0, "second").unwrap());
    let (second_window, second_native_window) = product_window(second_document, second_model, cx);
    for (id, source) in [(701, SOURCE), (702, CROSS)] {
        host.update(cx, |host, cx| {
            let execution = host
                .load_test_source(
                    ExtensionKey::new(ExtensionId::new(id), ExtensionLifecycleId::new(1)),
                    source,
                )
                .unwrap();
            cx.spawn(async move |_, _| {
                execution.await.unwrap();
            })
            .detach();
        });
    }
    wait_for_api_condition(cx, |cx| {
        let catalog = cx
            .global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog();
        let catalog = catalog.borrow();
        catalog.resolve("api.capture").is_ok() && catalog.resolve("api.cross").is_ok()
    });
    wait_for_api_condition(cx, |cx| {
        first_window
            .read(cx)
            .contributed_views
            .get("example.outline")
            .is_some_and(|view| view.tree().read(cx).root_labels() == ["context-free"])
    });
    // Capture before the palette takes focus, then admit the saved origin.
    let origin = cx
        .update_window(first_native_window, |_, _, cx| {
            first_window.update(cx, |window, cx| {
                window.focus_active_surface(cx);
                window.open_command_palette(cx);
                window.command_palette.as_ref().unwrap().origin()
            })
        })
        .unwrap();
    for _ in 0..2 {
        host.update(cx, |host, _| host.hold_clipboard_writes());
        let dispatcher = cx.read(|cx| cx.global::<ApplicationProductCommands>().0.clone());
        let execution = dispatcher.update(cx, |dispatcher, cx| {
            dispatcher.dispatch(
                Command {
                    name: "api.capture".into(),
                    arguments: CommandArgumentValue::Null,
                },
                origin.clone(),
                cx,
            )
        });
        wait_for_api_condition(cx, |cx| host.read(cx).pending_view_request_count() == 1);
        cx.update_window(second_native_window, |_, _, cx| {
            second_window.update(cx, |window, cx| window.focus_active_surface(cx))
        })
        .unwrap();
        host.update(cx, |host, cx| host.release_view_requests(cx));
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        cx.read(|cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "first"));
    }
    let editor = cx.read(|cx| {
        first_window
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone()
    });
    let completion = dispatch_product_command(
        &first_window,
        first_native_window,
        super::super::SHOW_COMPLETIONS_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);
    wait_for_api_condition(cx, |cx| {
        editor
            .read(cx)
            .editor()
            .read(cx)
            .completion_state()
            .is_some_and(|(items, pending, failures, _)| {
                items == 1 && pending == 0 && failures == 0
            })
    });
    let (_shared_window, _shared_native_window) =
        product_window(first_document, first_model.clone(), cx);
    cx.update_window(first_native_window, |_, native_window, _| {
        native_window.remove_window()
    })
    .unwrap();
    drop(first_window);
    first_model.update(cx, |model, cx| {
        model.replace(0..5, "shared callback").unwrap();
        cx.notify();
    });
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref()
            == Some("shared callback")
    });
}

#[gpui::test]
fn public_api_context_rejects_replaced_closed_and_transferred_presentations(
    cx: &mut TestAppContext,
) {
    use knot_protocol::ExtensionKey;
    const SOURCE: &str = r#"
import { commands, editor, workbench } from "knot";
await commands.register("api.stale", async (context) => {
  await workbench.writeClipboardText("waiting");
  for (const operation of [() => editor.activeBuffer(), () => editor.activeBuffer(context.target), () => workbench.selectedText(context.target)]) {
let rejected = false;
try { await operation(); } catch (error) { if (error.code !== "InvalidTarget") throw error; rejected = true; }
if (!rejected) throw new Error("stale presentation accepted");
  }
  // An explicit live buffer remains usable after its presentation changes.
  await context.buffer.snapshot();
  await workbench.writeClipboardText("InvalidTarget");
});
await commands.register("api.nontext", async (context) => {
  if (context.buffer !== null || await editor.activeBuffer() !== null || await editor.activeBuffer(context.target) !== null) throw new Error("nontext buffer");
  try { await workbench.selectedText(); } catch (error) { if (error.code === "UnsupportedOperation") return; throw error; }
  throw new Error("unsupported selection accepted");
});
"#;
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source(
                ExtensionKey::new(ExtensionId::new(703), ExtensionLifecycleId::new(1)),
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
            .resolve("api.nontext")
            .is_ok()
    });
    for change in ["replace", "close", "transfer"] {
        let document = cx.update(|cx| create_untitled_document(&documents, cx));
        let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
        let (window, native_window) = product_window(document, model.clone(), cx);
        host.update(cx, |host, _| host.hold_clipboard_writes());
        let execution = dispatch_product_command(&window, native_window, "api.stale", cx);
        wait_for_api_condition(cx, |cx| host.read(cx).pending_view_request_count() == 1);
        let target = cx
            .update_window(native_window, |_, _, cx| {
                window.update(cx, |window, cx| window.capture_command_target(cx))
            })
            .unwrap();
        match change {
            "replace" => {
                let workbench = cx.read(|cx| window.read(cx).workbench.clone());
                workbench.update(cx, |workbench, cx| {
                    assert!(workbench.replace_tab_document(
                        target.tab.unwrap().pane,
                        target.tab.unwrap().tab,
                        document,
                        model.clone(),
                        cx
                    ))
                });
            }
            "close" => {
                cx.update_window(native_window, |_, native_window, _| {
                    native_window.remove_window()
                })
                .unwrap();
            }
            "transfer" => {
                let destination_document = cx.update(|cx| create_untitled_document(&documents, cx));
                let destination_model = cx.read(|cx| {
                    documents
                        .read(cx)
                        .get(destination_document)
                        .unwrap()
                        .model()
                        .clone()
                });
                let (destination_window, _) =
                    product_window(destination_document, destination_model, cx);
                let source = cx.read(|cx| {
                    window
                        .read(cx)
                        .capture_tab_relocation(
                            target.tab.unwrap().pane,
                            target.tab.unwrap().tab,
                            cx,
                        )
                        .unwrap()
                });
                let destination = cx.read(|cx| {
                    let workbench = &destination_window.read(cx).workbench;
                    let pane = workbench.read(cx).focused_pane().unwrap();
                    super::super::relocate::TabRelocationDestination {
                        window: destination_window.downgrade(),
                        workbench: workbench.downgrade(),
                        tab: crate::app::workbench::TabMoveDestination::At {
                            anchor: workbench
                                .read(cx)
                                .capture_tab_move(pane.id(), pane.active_tab().id())
                                .unwrap(),
                            placement: crate::app::workbench::TabMovePlacement::End,
                        },
                    }
                });
                let outcome = cx
                    .update_window(native_window, |_, _, cx| {
                        window.update(cx, |window, cx| {
                            window.relocate_tab_to(&source, &destination, cx)
                        })
                    })
                    .unwrap();
                assert_eq!(outcome, CommandOutcome::Completed);
            }
            _ => unreachable!(),
        }
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
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            let (session, view) = window.create_terminal(cx);
            let pane = window.workbench.read(cx).focused_pane_id().unwrap();
            let tab = window.workbench.update(cx, |workbench, _| {
                workbench.open_terminal_tab(pane, session, view).unwrap()
            });
            window.activate_tab(pane, tab, cx);
        })
    })
    .unwrap();
    let execution = dispatch_product_command(&window, native_window, "api.nontext", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
}

#[gpui::test]
fn final_document_close_invalidates_js_buffer_even_with_retained_editor(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;
    const SOURCE: &str = r#"
import { commands } from "knot";
let saved;
await commands.register("closure.setup", async ({ buffer }) => {
  saved = buffer;
  const snapshot = await buffer.snapshot();
  await buffer.contributions.replace([{ range: { startByteOffset: 0, endByteOffset: 1 }, decoration: { color: "info", style: "wavy" } }], { ifRevision: snapshot.revision });
  await buffer.onDidChange(() => {});
});
await commands.register("closure.live", async () => {
  if ((await saved.snapshot()).text !== "owned") throw new Error("shared buffer lost");
});
await commands.register("closure.closed", async () => {
  for (const operation of [() => saved.snapshot(), () => saved.onDidChange(() => {}), () => saved.applyEdits([], { ifRevision: 0 }), () => saved.contributions.replace([], { ifRevision: 0 })]) {
try { await operation(); } catch (error) {
  if (error.code === "BufferClosed") continue;
  throw error;
}
throw new Error("closed buffer accepted operation");
  }
});
"#;
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("owned")));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Clean", model.clone(), cx)
    });
    let (first, first_window) = product_window(document, model.clone(), cx);
    let (second, second_window) = product_window(document, model.clone(), cx);
    let retained_editor = cx.read(|cx| {
        first
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone()
    });
    let key = ExtensionKey::new(ExtensionId::new(706), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
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
            .resolve("closure.closed")
            .is_ok()
    });
    let setup = dispatch_product_command(&first, first_window, "closure.setup", cx);
    assert_eq!(wait_for_command(setup, cx), CommandOutcome::Completed);
    let close = dispatch_product_command(&first, first_window, CLOSE_TAB_COMMAND, cx);
    assert_eq!(wait_for_command(close, cx), CommandOutcome::Completed);
    cx.read(|cx| {
        assert!(documents.read(cx).get(document).is_some());
        assert!(model.read(cx).is_open());
        assert_eq!(model.read(cx).resolved_contributions().len(), 1);
    });
    let live = dispatch_product_command(&second, second_window, "closure.live", cx);
    assert_eq!(wait_for_command(live, cx), CommandOutcome::Completed);
    let close = dispatch_product_command(&second, second_window, CLOSE_TAB_COMMAND, cx);
    assert_eq!(wait_for_command(close, cx), CommandOutcome::Completed);
    cx.read(|cx| {
        assert!(documents.read(cx).get(document).is_none());
        assert!(!model.read(cx).is_open());
        assert!(model.read(cx).resolved_contributions().is_empty());
        assert_eq!(retained_editor.read(cx).model(), &model);
    });
    let closed = dispatch_product_command(&second, second_window, "closure.closed", cx);
    assert_eq!(wait_for_command(closed, cx), CommandOutcome::Completed);
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
}

#[gpui::test]
fn public_api_disposal_and_unload_release_foreground_ownership(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;
    const SOURCE: &str = r#"
import { commands, editor, workbench, keymaps } from "knot";
let owned;
keymaps.bind("cmd-alt-o", "api.owned");
await commands.register("api.owned-setup", async ({ buffer }) => {
  const snapshot = await buffer.snapshot();
  await buffer.contributions.replace([{ range: { startByteOffset: 0, endByteOffset: 1 }, decoration: { color: "info", style: "wavy" } }], { ifRevision: snapshot.revision });
  owned = [
buffer.contributions,
await buffer.onDidChange(() => workbench.writeClipboardText("leaked listener")),
await commands.register("api.owned", () => {}),
await workbench.registerView({
  id: "example.owned",
  title: "Owned",
  content: { kind: "tree", provider: {
    getChildren() { return [{ id: "owned", label: "owned", collapsibleState: "none" }]; },
  } },
}),
await editor.registerCompletionProvider("owned", { provideCompletions() { return [{ label: "owned", insertText: "owned" }]; } }),
  ];
});
await commands.register("api.owned-cleanup", async () => {
  const cleanup = owned.map(owner => owner.dispose());
  for (let index = 0; index < owned.length; index++) if (cleanup[index] !== owned[index].dispose()) throw new Error("cleanup promise changed");
  await Promise.all(cleanup);
});
"#;
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    model.update(cx, |model, _| model.replace(0..0, "owned").unwrap());
    let (window, native_window) = product_window(document, model.clone(), cx);
    let editor = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone()
    });
    for explicit_cleanup in [true, false] {
        let key = ExtensionKey::new(
            ExtensionId::new(if explicit_cleanup { 704 } else { 705 }),
            ExtensionLifecycleId::new(1),
        );
        host.update(cx, |host, cx| {
            let execution = host.load_test_source(key, SOURCE).unwrap();
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
                .resolve("api.owned-cleanup")
                .is_ok()
        });
        let setup = dispatch_product_command(&window, native_window, "api.owned-setup", cx);
        assert_eq!(wait_for_command(setup, cx), CommandOutcome::Completed);
        wait_for_api_condition(cx, |cx| {
            window
                .read(cx)
                .contributed_views
                .get("example.owned")
                .is_some_and(|view| view.tree().read(cx).root_labels() == ["owned"])
        });
        cx.read(|cx| assert_eq!(model.read(cx).resolved_contributions().len(), 1));
        let completion = dispatch_product_command(
            &window,
            native_window,
            super::super::SHOW_COMPLETIONS_COMMAND,
            cx,
        );
        assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);
        wait_for_api_condition(cx, |cx| {
            editor
                .read(cx)
                .editor()
                .read(cx)
                .completion_state()
                .is_some_and(|(items, pending, _, _)| items == 1 && pending == 0)
        });
        if explicit_cleanup {
            let cleanup = dispatch_product_command(&window, native_window, "api.owned-cleanup", cx);
            assert_eq!(wait_for_command(cleanup, cx), CommandOutcome::Completed);
        } else {
            host.update(cx, |host, cx| host.unload(key, cx).unwrap());
        }
        cx.read(|cx| {
            assert!(model.read(cx).resolved_contributions().is_empty());
            assert!(window.read(cx).contributed_views.is_empty());
            assert!(
                cx.global::<ApplicationProductCommands>()
                    .0
                    .read(cx)
                    .catalog()
                    .borrow()
                    .resolve("api.owned")
                    .is_err()
            );
            let (items, pending, _, _) = editor
                .read(cx)
                .editor()
                .read(cx)
                .completion_state()
                .unwrap();
            assert_eq!((items, pending), (0, 0));
        });
        if explicit_cleanup {
            host.update(cx, |host, cx| host.unload(key, cx).unwrap());
        }
        cx.read(|cx| {
            assert!(keymap_command(cx, "cmd-alt-o", Some("editor")).is_none());
            assert!(
                cx.global::<ApplicationProductCommands>()
                    .0
                    .read(cx)
                    .catalog()
                    .borrow()
                    .resolve("api.owned-setup")
                    .is_err()
            );
        });
    }
    let key = ExtensionKey::new(ExtensionId::new(706), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
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
            .resolve("api.owned-cleanup")
            .is_ok()
    });
    let setup = dispatch_product_command(&window, native_window, "api.owned-setup", cx);
    assert_eq!(wait_for_command(setup, cx), CommandOutcome::Completed);
    cx.read(|cx| assert_eq!(model.read(cx).resolved_contributions().len(), 1));
    cx.update(|cx| {
        drop(cx.remove_global::<super::super::super::extensions::host::ApplicationExtensionHost>())
    });
    cx.update(move |_| drop(host));
    cx.run_until_parked();
    cx.read(|cx| {
        assert!(model.read(cx).resolved_contributions().is_empty());
        assert!(window.read(cx).contributed_views.is_empty());
        assert!(
            cx.global::<ApplicationProductCommands>()
                .0
                .read(cx)
                .catalog()
                .borrow()
                .resolve("api.owned")
                .is_err()
        );
        assert!(keymap_command(cx, "cmd-alt-o", Some("editor")).is_none());
    });
}

#[gpui::test]
fn contributed_view_registered_before_the_window_attaches_on_open(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    const SOURCE: &str = r#"
import { workbench } from "knot";
await workbench.registerView({
  id: "example.deferred",
  title: "Deferred",
  content: { kind: "tree", provider: {
getChildren() { return [{ id: "root", label: "Deferred", collapsibleState: "none" }]; },
  } },
});
"#;

    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(90), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
        })
        .detach();
    });
    wait_for_api_condition(cx, |cx| {
        super::super::super::extensions::host::view_definitions(cx)
            .iter()
            .any(|definition| definition.id == "example.deferred")
    });

    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, _window) = product_window(document, model, cx);
    let tree = wait_for_contributed_view(&window, "example.deferred", cx);
    wait_for_api_condition(cx, |cx| tree.read(cx).root_labels() == ["Deferred"]);
}

#[gpui::test]
fn focused_extension_tree_claims_copy_and_unload_removes_its_handler(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    const SOURCE: &str = r#"
import * as knot from "knot"; const { commands, workbench } = knot;
await workbench.registerView({
  id: "example.outline",
  title: "Outline",
  content: { kind: "tree", provider: {
getChildren(parentId) {
  return parentId === null ? [
    { id: "first", label: "Selected tree item", collapsibleState: "none", command: "test.view-first" },
    { id: "failure", label: "Fail", collapsibleState: "none" },
  ] : [];
},
  } },
});
const viewCopy = await commands.registerForView("example.outline", "copy", async (context) => {
  if (context.buffer !== null || await knot.editor.activeBuffer(context.target) !== null) throw new Error("outline has a text buffer");
  const text = await workbench.selectedTreeItem(context.target);
  if (text === "failure") throw new Error("copy failed");
  await workbench.writeClipboardText(text);
});
await commands.register("test.view-first", async () => {
  throw new Error("global handler ran");
});
await commands.registerForView("example.outline", "test.view-first", async () => {
  await workbench.writeClipboardText("tree item");
});
await commands.register("test.view-null", () => { throw new Error("global null handler"); });
await commands.registerForView("example.outline", "test.view-null", async (context) => {
  if (context.target.view === null || await workbench.selectedTreeItem(context.target) !== null) throw new Error("missing selection is not null");
});
await commands.register("test.nested-view-copy", async () => {
  const outcome = await commands.invoke("copy");
  if (outcome.kind !== "completed") throw new Error(`nested copy: ${outcome.kind}`);
});
await commands.register("test.nested-new", async () => {
  const outcome = await commands.invoke("file.new");
  if (outcome.kind !== "completed") throw new Error(`nested new: ${outcome.kind}`);
});
await commands.register("test.dispose-view-copy", async () => viewCopy.dispose());
"#;

    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model.clone(), cx);
    let key = ExtensionKey::new(ExtensionId::new(87), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
        })
        .detach();
    });

    let tree = wait_for_contributed_view(&window, "example.outline", cx);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            tree.read(cx).root_labels().len() == 2
                && cx
                    .global::<ApplicationProductCommands>()
                    .0
                    .read(cx)
                    .catalog()
                    .borrow()
                    .resolve_view(tree.read(cx).instance_id(), "copy")
                    .is_some()
        })
    });

    cx.update_window(native_window, |_, native_window, cx| {
        focus_view(&tree.focus_handle(cx), native_window, cx);
        let target = window.update(cx, |window, cx| window.capture_command_target(cx));
        assert_eq!(target.view, Some(tree.read(cx).instance_id()));
    })
    .unwrap();
    let declined = dispatch_tree_copy(&window, native_window, cx);
    assert_eq!(wait_for_command(declined, cx), CommandOutcome::Unavailable);
    let empty_selection = dispatch_tree_command(&window, native_window, "test.view-null", cx);
    assert_eq!(
        wait_for_command(empty_selection, cx),
        CommandOutcome::Completed
    );
    let view_first = dispatch_tree_command(&window, native_window, "test.view-first", cx);
    assert_eq!(wait_for_command(view_first, cx), CommandOutcome::Completed);
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("untouched".into())));
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "enter");
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            cx.read_from_clipboard()
                .is_some_and(|item| item.text().as_deref() == Some("tree item"))
        })
    });
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("untouched".into())));
    cx.refresh().unwrap();
    {
        let mut window_cx = VisualTestContext::from_window(native_window, cx);
        let row = window_cx
            .debug_bounds("extension-tree-row-0")
            .expect("contributed tree row is laid out")
            .center();
        window_cx.simulate_mouse_down(row, MouseButton::Left, Modifiers::default());
        window_cx.simulate_mouse_up(row, MouseButton::Left, Modifiers::default());
    }
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            cx.read_from_clipboard()
                .is_some_and(|item| item.text().as_deref() == Some("tree item"))
        })
    });
    cx.update(|cx| tree.update(cx, |tree, cx| assert!(tree.select_item("first", cx))));
    let copy = dispatch_tree_copy(&window, native_window, cx);
    assert_eq!(wait_for_command(copy, cx), CommandOutcome::Completed);
    cx.update(|cx| {
        assert!(
            cx.global_mut::<ApplicationKeymaps>()
                .0
                .set(
                    BindingOwner::Extension(key.extension, key.lifecycle),
                    "cmd-alt-y",
                    Some("example.outline"),
                    Some(Command {
                        name: super::super::COPY_COMMAND.into(),
                        arguments: CommandArgumentValue::Null,
                    }),
                )
                .unwrap()
        );
        super::super::super::commands::keymaps::rebuild(cx);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("untouched".into()));
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "cmd-alt-y");
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            cx.read_from_clipboard()
                .is_some_and(|item| item.text().as_deref() == Some("first"))
        })
    });
    let nested = dispatch_tree_command(&window, native_window, "test.nested-view-copy", cx);
    assert_eq!(wait_for_command(nested, cx), CommandOutcome::Completed);
    let nested_native = dispatch_tree_command(&window, native_window, "test.nested-new", cx);
    assert_eq!(
        wait_for_command(nested_native, cx),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert_eq!(
            window
                .read(cx)
                .workbench
                .read(cx)
                .focused_pane()
                .unwrap()
                .tabs()
                .len(),
            2
        );
    });
    cx.update_window(native_window, |_, native_window, cx| {
        focus_view(&tree.focus_handle(cx), native_window, cx)
    })
    .unwrap();
    cx.read(|cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "first"));

    let (second_window, second_native_window) = product_window(document, model.clone(), cx);
    let second_tree = wait_for_contributed_view(&second_window, "example.outline", cx);
    assert_ne!(
        cx.read(|cx| tree.read(cx).instance_id()),
        cx.read(|cx| second_tree.read(cx).instance_id())
    );
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| second_tree.read(cx).root_labels().len() == 2)
    });
    cx.update_window(second_native_window, |_, native_window, cx| {
        second_tree.update(cx, |tree, cx| assert!(tree.select_item("first", cx)));
        focus_view(&second_tree.focus_handle(cx), native_window, cx);
    })
    .unwrap();
    assert_eq!(
        wait_for_command(
            dispatch_tree_copy(&second_window, second_native_window, cx),
            cx
        ),
        CommandOutcome::Completed
    );

    cx.update(|cx| tree.update(cx, |tree, cx| assert!(tree.select_item("failure", cx))));
    let failure = dispatch_tree_copy(&window, native_window, cx);
    assert!(matches!(
        wait_for_command(failure, cx),
        CommandOutcome::HandlerFailure { .. }
    ));
    cx.update(|cx| tree.update(cx, |tree, cx| assert!(tree.select_item("first", cx))));
    let recovery = dispatch_tree_copy(&window, native_window, cx);
    assert_eq!(wait_for_command(recovery, cx), CommandOutcome::Completed);

    let dispose = dispatch_product_command(&window, native_window, "test.dispose-view-copy", cx);
    assert_eq!(wait_for_command(dispose, cx), CommandOutcome::Completed);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            cx.global::<ApplicationProductCommands>()
                .0
                .read(cx)
                .catalog()
                .borrow()
                .resolve_view(tree.read(cx).instance_id(), "copy")
                .is_none()
        })
    });
    let disposed = dispatch_tree_copy(&second_window, second_native_window, cx);
    assert_eq!(wait_for_command(disposed, cx), CommandOutcome::Unavailable);

    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    cx.read(|cx| {
        assert!(
            cx.global::<ApplicationKeymaps>()
                .0
                .resolve(
                    &crate::app::keys::Keystroke::parse("cmd-alt-y")
                        .unwrap()
                        .unparse(),
                    Some("example.outline")
                )
                .is_none()
        );
    });
    cx.read(|cx| {
        assert!(window.read(cx).contributed_views.is_empty());
        assert!(second_window.read(cx).contributed_views.is_empty());
        assert!(!tree.read(cx).lifecycle_state().0);
        assert!(!second_tree.read(cx).lifecycle_state().0);
    });
}

#[gpui::test]
fn startup_script_command_cannot_borrow_the_focused_product_window(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx));
    })
    .unwrap();

    let key = ExtensionKey::new(ExtensionId::new(89), ExtensionLifecycleId::new(1));
    let (done, mut result) = tokio::sync::oneshot::channel();
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source(
                key,
                r#"
import * as knot from "knot"; const { commands } = knot;
let rejected = false;
try { await commands.invoke("file.new"); }
catch (error) { if (error.code !== "ContextUnavailable") throw error; rejected = true; }
if (!rejected) throw new Error("startup borrowed focus");
"#,
            )
            .unwrap();
        cx.spawn(async move |_, _| {
            let _ = done.send(execution.await);
        })
        .detach();
    });
    let mut script = None;
    crate::test_support::wait_until(cx, |_| {
        match result.try_recv() {
            Ok(outcome) => script = Some(outcome),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            Err(error) => panic!("script completion closed: {error}"),
        }
        script.is_some()
    });
    script.unwrap().unwrap();
    cx.read(|cx| {
        assert_eq!(
            window
                .read(cx)
                .workbench
                .read(cx)
                .focused_pane()
                .unwrap()
                .tabs()
                .len(),
            1
        );
    });
}

#[gpui::test]
fn extension_command_with_editor_prefix_keeps_its_arguments(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(91), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source(
                key,
                r#"
import * as knot from "knot"; const { commands } = knot;
await commands.register("editor.custom", async ({ arguments: args }) => {
  if (args?.value !== 42) throw new Error("missing command arguments");
});
"#,
            )
            .unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
        })
        .detach();
    });
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            cx.global::<ApplicationProductCommands>()
                .0
                .read(cx)
                .catalog()
                .borrow()
                .resolve("editor.custom")
                .is_ok()
        })
    });
    let target = cx
        .update_window(native_window, |_, _, cx| {
            window.update(cx, |window, cx| window.focus_active_editor(cx));
            window.update(cx, |window, cx| window.capture_command_target(cx))
        })
        .unwrap();
    let dispatcher = cx.read(|cx| cx.global::<ApplicationProductCommands>().0.clone());
    let execution = dispatcher.update(cx, |dispatcher, cx| {
        dispatcher.dispatch(
            Command {
                name: "editor.custom".into(),
                arguments: CommandArgumentValue::Object(std::collections::BTreeMap::from([(
                    "value".into(),
                    CommandArgumentValue::Number(42.),
                )])),
            },
            target,
            cx,
        )
    });
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
}

#[gpui::test]
fn nested_script_waits_for_a_pending_native_command(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    let documents = install_globals(cx);
    cx.set_global(ApplicationOpenDialog(Arc::new(FixedOpenDialog(
        OpenDialogOutcome::Cancelled,
    ))));
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(90), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source(
                key,
                r#"
import * as knot from "knot"; const { commands } = knot;
await commands.register("test.nested-open", async () => {
  const outcome = await commands.invoke("file.open");
  if (outcome.kind !== "cancelled") throw new Error(`open: ${outcome.kind}`);
});
"#,
            )
            .unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
        })
        .detach();
    });
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            cx.global::<ApplicationProductCommands>()
                .0
                .read(cx)
                .catalog()
                .borrow()
                .resolve("test.nested-open")
                .is_ok()
        })
    });
    let execution = dispatch_product_command(&window, native_window, "test.nested-open", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.read(|cx| assert_eq!(documents.read(cx).documents().count(), 1));
}

#[gpui::test]
fn delayed_clipboard_write_survives_presentation_closure_but_not_unload(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    const SOURCE: &str = r#"
import * as knot from "knot"; const { commands, workbench } = knot;
await workbench.registerView({
  id: "example.outline",
  title: "Outline",
  content: { kind: "tree", provider: {
getChildren(parentId) {
  return parentId === null
    ? [{ id: "held", label: "Held selection", collapsibleState: "none" }]
    : [];
},
  } },
});
await commands.registerForView("example.outline", "copy", async () => {
  const text = await workbench.selectedTreeItem();
  await workbench.writeClipboardText(text);
});
"#;

    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model.clone(), cx);
    let key = ExtensionKey::new(ExtensionId::new(88), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
        })
        .detach();
    });
    let tree = wait_for_contributed_view(&window, "example.outline", cx);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            tree.read(cx).root_labels().len() == 1
                && cx
                    .global::<ApplicationProductCommands>()
                    .0
                    .read(cx)
                    .catalog()
                    .borrow()
                    .resolve_view(tree.read(cx).instance_id(), "copy")
                    .is_some()
        })
    });
    cx.update_window(native_window, |_, native_window, cx| {
        tree.update(cx, |tree, cx| assert!(tree.select_item("held", cx)));
        focus_view(&tree.focus_handle(cx), native_window, cx);
    })
    .unwrap();
    host.update(cx, |host, _| host.hold_clipboard_writes());
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("untouched".into())));
    let execution = dispatch_tree_copy(&window, native_window, cx);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| host.read(cx).pending_view_request_count() == 1)
    });
    cx.update_window(native_window, |_, native_window, _| {
        native_window.remove_window()
    })
    .unwrap();
    host.update(cx, |host, cx| host.release_view_requests(cx));
    assert_eq!(
        wait_for_command(execution, cx),
        CommandOutcome::InvalidTarget
    );
    cx.read(|cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "held"));
    let closed_view = cx.read(|cx| tree.read(cx).instance_id());
    drop(window);
    cx.run_until_parked();
    cx.read(|cx| {
        assert!(
            cx.global::<ApplicationProductCommands>()
                .0
                .read(cx)
                .catalog()
                .borrow()
                .resolve_view(closed_view, "copy")
                .is_none()
        )
    });

    let (second_window, second_native_window) = product_window(document, model, cx);
    let second_tree = wait_for_contributed_view(&second_window, "example.outline", cx);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| second_tree.read(cx).root_labels().len() == 1)
    });
    cx.update_window(second_native_window, |_, native_window, cx| {
        second_tree.update(cx, |tree, cx| assert!(tree.select_item("held", cx)));
        focus_view(&second_tree.focus_handle(cx), native_window, cx);
    })
    .unwrap();
    host.update(cx, |host, _| host.hold_clipboard_writes());
    let interrupted = dispatch_tree_copy(&second_window, second_native_window, cx);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| host.read(cx).pending_view_request_count() == 1)
    });
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    host.update(cx, |host, cx| host.release_view_requests(cx));
    assert_eq!(wait_for_command(interrupted, cx), CommandOutcome::Cancelled);
    cx.read(|cx| {
        assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "held");
        assert!(!second_tree.read(cx).lifecycle_state().0);
    });
}

#[gpui::test]
fn tree_rows_report_gestures_and_reveal_selects_through_unloaded_folders(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    const SOURCE: &str = r#"
import * as knot from "knot"; const { commands, workbench } = knot;
const rows = {
  null: [
    { id: "dir", label: "dir", collapsibleState: "collapsed" },
    { id: "file", label: "file", collapsibleState: "none", command: "test.row", arguments: { n: 1 } },
  ],
  dir: [{ id: "dir/inner", label: "inner", collapsibleState: "collapsed" }],
  "dir/inner": [{ id: "dir/inner/leaf", label: "leaf", collapsibleState: "none", command: "test.row" }],
};
const view = await workbench.registerView({
  id: "example.reveal",
  title: "Reveal",
  content: { kind: "tree", provider: { getChildren: (parentId) => rows[parentId] ?? [] } },
});
await commands.register("test.row", () => { throw new Error("global handler ran"); });
await commands.registerForView("example.reveal", "test.row", async (context) => {
  await workbench.writeClipboardText(
    `${JSON.stringify(context.arguments)}|${await workbench.selectedTreeItem(context.target)}`,
  );
});
await commands.register("test.reveal", async () => { await view.reveal(["dir", "dir/inner", "dir/inner/leaf"]); });
"#;

    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(88), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
        })
        .detach();
    });
    let tree = wait_for_contributed_view(&window, "example.reveal", cx);
    wait_for_api_condition(cx, |cx| tree.read(cx).root_labels() == ["dir", "file"]);
    cx.update_window(native_window, |_, native_window, cx| {
        focus_view(&tree.focus_handle(cx), native_window, cx)
    })
    .unwrap();

    let click = |count: usize, cx: &mut TestAppContext| {
        let mut visual = VisualTestContext::from_window(native_window, cx);
        let row = visual
            .debug_bounds("extension-tree-row-1")
            .expect("file row is laid out")
            .center();
        visual.simulate_event(gpui::MouseDownEvent {
            position: row,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: count,
            first_mouse: false,
        });
        visual.simulate_event(gpui::MouseUpEvent {
            position: row,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: count,
        });
    };
    click(1, cx);
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref()
            == Some(r#"{"arguments":{"n":1},"gesture":"click"}|file"#)
    });
    click(2, cx);
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref()
            == Some(r#"{"arguments":{"n":1},"gesture":"doubleClick"}|file"#)
    });

    let reveal = dispatch_product_command(&window, native_window, "test.reveal", cx);
    assert_eq!(wait_for_command(reveal, cx), CommandOutcome::Completed);
    wait_for_api_condition(cx, |cx| {
        tree.read(cx).selected_item_id().as_deref() == Some("dir/inner/leaf")
    });
    cx.read(|cx| {
        assert_eq!(
            tree.read(cx).visible_labels(),
            ["dir", "inner", "leaf", "file"]
        );
    });
}

#[gpui::test]
fn inline_edits_validate_asynchronously_and_settle_with_the_typed_value(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    const SOURCE: &str = r#"
import * as knot from "knot"; const { commands, workbench } = knot;
const view = await workbench.registerView({
  id: "example.edit",
  title: "Edit",
  content: { kind: "tree", provider: { getChildren: (parentId) => parentId === null ? [
    { id: "dir", label: "dir", collapsibleState: "collapsed" },
    { id: "file", label: "file.txt", collapsibleState: "none" },
  ] : [] } },
});
const report = (result) => workbench.writeClipboardText(JSON.stringify(result));
await commands.register("test.rename", async () => {
  await report(await view.editLabel("file", {
    value: "file.txt",
    selection: [0, 4],
    validate: (value) => value.includes("/") ? "no slashes" : null,
  }));
});
await commands.register("test.create", async () => {
  await report(await view.editNewItem("dir", { icon: "file" }));
});
await commands.register("test.missing", async () => {
  try { await view.editLabel("absent"); } catch (error) { await report({ error: error.code }); }
});
"#;

    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(89), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
        })
        .detach();
    });
    let tree = wait_for_contributed_view(&window, "example.edit", cx);
    wait_for_api_condition(cx, |cx| tree.read(cx).root_labels() == ["dir", "file.txt"]);
    cx.update_window(native_window, |_, native_window, cx| {
        focus_view(&tree.focus_handle(cx), native_window, cx)
    })
    .unwrap();
    wait_for_api_condition(cx, |cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog()
            .borrow()
            .resolve("test.rename")
            .is_ok()
    });
    let clipboard = |expected: &'static str, cx: &mut TestAppContext| {
        wait_for_api_condition(cx, |cx| {
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref()
                == Some(expected)
        });
    };

    // A rename selects the stem, so typing replaces it; a message blocks Enter.
    let mut rename = dispatch_product_command(&window, native_window, "test.rename", cx);
    wait_for_api_condition(cx, |cx| {
        if let Ok(outcome) = rename.completion.try_recv() {
            panic!("rename finished early: {outcome:?}");
        }
        tree.read(cx).is_editing()
    });
    cx.simulate_input(native_window, "a/");
    wait_for_api_condition(cx, |cx| {
        tree.read(cx).edit_message_for_test().as_deref() == Some("no slashes")
    });
    cx.simulate_keystrokes(native_window, "enter");
    cx.run_until_parked();
    assert!(
        cx.read(|cx| tree.read(cx).is_editing()),
        "the message blocks the commit"
    );
    cx.simulate_keystrokes(native_window, "backspace");
    cx.simulate_keystrokes(native_window, "enter");
    assert_eq!(wait_for_command(rename, cx), CommandOutcome::Completed);
    clipboard(r#"{"kind":"accepted","value":"a.txt"}"#, cx);
    assert!(cx.read(|cx| !tree.read(cx).is_editing()));

    // A new row shows first among the expanded parent's children.
    let create = dispatch_product_command(&window, native_window, "test.create", cx);
    wait_for_api_condition(cx, |cx| tree.read(cx).is_editing());
    cx.simulate_keystrokes(native_window, "escape");
    assert_eq!(wait_for_command(create, cx), CommandOutcome::Completed);
    clipboard(r#"{"kind":"dismissed"}"#, cx);

    let missing = dispatch_product_command(&window, native_window, "test.missing", cx);
    assert_eq!(wait_for_command(missing, cx), CommandOutcome::Completed);
    clipboard(r#"{"error":"TreeItemNotFound"}"#, cx);
}

#[gpui::test]
fn extension_timers_fire_on_the_executor_clock_and_stop_at_unload(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;
    const SOURCE: &str = r#"
import { commands, workbench } from "knot";
const fired = [];
setTimeout(() => fired.push("timeout"), 75);
await commands.register("timers.report", async () => {
  await workbench.writeClipboardText(fired.join(" ") || "none");
});
await commands.register("timers.arm", async () => {
  setTimeout(() => workbench.writeClipboardText("after unload"), 10);
});
"#;
    let documents = install_globals(cx);
    let host = cx.update(crate::app::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(971), ExtensionLifecycleId::new(1));
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

    // A command queues behind every expiry delivered before it, so its report
    // shows whether the timer has fired.
    let report = |cx: &mut TestAppContext| {
        cx.run_until_parked();
        let execution = dispatch_product_command(&window, native_window, "timers.report", cx);
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        cx.read_from_clipboard().and_then(|item| item.text())
    };
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(74));
    assert_eq!(report(cx).as_deref(), Some("none"));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1));
    assert_eq!(report(cx).as_deref(), Some("timeout"));

    let execution = dispatch_product_command(&window, native_window, "timers.arm", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    host.update(cx, |host, cx| host.unload(key, cx)).unwrap();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(10));
    cx.run_until_parked();
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("timeout")
    );
}
