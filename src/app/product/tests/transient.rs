//! Transient extension interactions own one window-local overlay.
use super::operations::empty_product_window;
use super::*;
use crate::app::extensions::host::handlers::TransientError;
use crate::app::transient::{PromptText, TransientOutcome, TransientOwner};
use knot_protocol::{CommandInvocationId, PickerItem};

fn owner(invocation: Option<u64>) -> TransientOwner {
    TransientOwner {
        extension: ExtensionId::new(7),
        lifecycle: ExtensionLifecycleId::new(3),
        invocation: invocation.map(CommandInvocationId::new),
    }
}

fn picker_items() -> Vec<PickerItem> {
    vec![
        PickerItem {
            detail: None,
            hint: None,

            id: "alpha".into(),
            icon: Some(knot_protocol::IconRef::File {
                file: "alpha.rs".into(),
            }),
            label: "Alpha".into(),
            description: Some("first".into()),
        },
        PickerItem {
            detail: None,
            hint: None,
            icon: None,
            id: "beta".into(),
            label: "Beta".into(),
            description: Some("second".into()),
        },
    ]
}

fn settle(
    receiver: &mut tokio::sync::oneshot::Receiver<TransientOutcome>,
    cx: &mut TestAppContext,
) -> TransientOutcome {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        match receiver.try_recv() {
            Ok(outcome) => return outcome,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "transient interaction did not settle"
                );
                std::thread::yield_now();
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                panic!("transient sender dropped without an outcome")
            }
        }
    }
}

#[gpui::test]
fn picker_filters_accepts_keyboard_selection_and_restores_focus(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, window) = empty_product_window(&documents, cx);
    cx.update_window(window, |_, _, cx| {
        shell.update(cx, |shell, cx| shell.focus_active_surface(cx));
    })
    .unwrap();
    let before = cx
        .update_window(window, |_, window, cx| window.focused(cx))
        .unwrap();
    let mut receiver = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.begin_picker(owner(None), "Pick".into(), picker_items(), None, cx)
            })
        })
        .unwrap()
        .unwrap();

    let overlap = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell
                    .begin_input(owner(None), "Name".into(), String::new(), None, None, cx)
                    .map(|(receiver, _)| receiver)
            })
        })
        .unwrap();
    assert!(matches!(overlap, Err(TransientError::OperationInProgress)));

    let mut visual = gpui::VisualTestContext::from_window(window, cx);
    visual.run_until_parked();
    assert!(visual.debug_bounds("picker-icon-0").is_some());
    assert!(visual.debug_bounds("picker-icon-1").is_none());
    cx.simulate_input(window, "beta");
    cx.simulate_keystrokes(window, "enter");
    assert_eq!(
        settle(&mut receiver, cx),
        TransientOutcome::Picker("beta".into())
    );
    let after = cx
        .update_window(window, |_, window, cx| window.focused(cx))
        .unwrap();
    assert_eq!(before, after);
}

#[gpui::test]
fn input_and_native_prompts_settle_once(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, window) = empty_product_window(&documents, cx);

    let mut input = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell
                    .begin_input(
                        owner(None),
                        "Add path".into(),
                        "seed".into(),
                        None,
                        None,
                        cx,
                    )
                    .map(|(receiver, _)| receiver)
            })
        })
        .unwrap()
        .unwrap();
    cx.simulate_keystrokes(window, "cmd-a");
    cx.simulate_input(window, "hello");
    cx.simulate_keystrokes(window, "enter");
    assert_eq!(
        settle(&mut input, cx),
        TransientOutcome::Input("hello".into())
    );

    let mut rejected = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.begin_picker(owner(None), "Pick".into(), picker_items(), None, cx)
            })
        })
        .unwrap()
        .unwrap();
    cx.simulate_keystrokes(window, "escape");
    assert_eq!(settle(&mut rejected, cx), TransientOutcome::Dismissed);

    let mut confirmed = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.begin_confirmation(
                    owner(None),
                    PromptText {
                        title: "Remove?".into(),
                        message: Some("fixture".into()),
                    },
                    "Remove".into(),
                    "Cancel".into(),
                    cx,
                )
            })
        })
        .unwrap()
        .unwrap();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Remove");
    assert_eq!(settle(&mut confirmed, cx), TransientOutcome::Confirmed);

    let mut cancelled = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.begin_confirmation(
                    owner(None),
                    PromptText {
                        title: "Remove?".into(),
                        message: None,
                    },
                    "Remove".into(),
                    "Cancel".into(),
                    cx,
                )
            })
        })
        .unwrap()
        .unwrap();
    cx.simulate_prompt_answer("Cancel");
    assert_eq!(settle(&mut cancelled, cx), TransientOutcome::Dismissed);

    let mut message = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.begin_message(
                    owner(None),
                    PromptText {
                        title: "Notice".into(),
                        message: None,
                    },
                    cx,
                )
            })
        })
        .unwrap()
        .unwrap();
    cx.simulate_prompt_answer("OK");
    assert_eq!(settle(&mut message, cx), TransientOutcome::Dismissed);
}

#[gpui::test]
fn folder_picker_settles_selected_dismissed_and_failed(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, window) = empty_product_window(&documents, cx);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_path_buf();

    cx.set_global(ApplicationOpenDialog(Arc::new(FixedOpenDialog(
        OpenDialogOutcome::Selected(path.clone()),
    ))));
    let mut selected = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.begin_folder_picker(Some(owner(None)), cx)
            })
        })
        .unwrap()
        .unwrap();
    assert_eq!(settle(&mut selected, cx), TransientOutcome::Folder(path));

    cx.set_global(ApplicationOpenDialog(Arc::new(FixedOpenDialog(
        OpenDialogOutcome::Cancelled,
    ))));
    let mut dismissed = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.begin_folder_picker(Some(owner(None)), cx)
            })
        })
        .unwrap()
        .unwrap();
    assert_eq!(settle(&mut dismissed, cx), TransientOutcome::Dismissed);

    cx.set_global(ApplicationOpenDialog(Arc::new(FixedOpenDialog(
        OpenDialogOutcome::Failed("dialog exploded".into()),
    ))));
    let mut failed = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.begin_folder_picker(Some(owner(None)), cx)
            })
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        settle(&mut failed, cx),
        TransientOutcome::Failed("dialog exploded".into())
    );
}

#[gpui::test]
fn cancellation_matches_the_owner_and_two_windows_stay_independent(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (first_shell, first_window) = empty_product_window(&documents, cx);
    let (second_shell, second_window) = empty_product_window(&documents, cx);

    let mut first = cx
        .update_window(first_window, |_, _, cx| {
            first_shell.update(cx, |shell, cx| {
                shell.begin_picker(owner(None), "First".into(), picker_items(), None, cx)
            })
        })
        .unwrap()
        .unwrap();
    let mut second = cx
        .update_window(second_window, |_, _, cx| {
            second_shell.update(cx, |shell, cx| {
                shell.begin_picker(owner(None), "Second".into(), picker_items(), None, cx)
            })
        })
        .unwrap()
        .unwrap();

    // Another lifecycle cannot cancel this window's interaction.
    let other = TransientOwner {
        extension: ExtensionId::new(7),
        lifecycle: ExtensionLifecycleId::new(9),
        invocation: None,
    };
    first_shell.update(cx, |shell, cx| shell.cancel_transient(other, cx));
    cx.run_until_parked();
    assert!(first.try_recv().is_err());

    first_shell.update(cx, |shell, cx| shell.cancel_transient(owner(None), cx));
    assert_eq!(settle(&mut first, cx), TransientOutcome::Cancelled);

    // The second window still owns its own interaction.
    cx.simulate_input(second_window, "alpha");
    cx.simulate_keystrokes(second_window, "enter");
    assert_eq!(
        settle(&mut second, cx),
        TransientOutcome::Picker("alpha".into())
    );
}

#[gpui::test]
fn window_closure_releases_the_pending_interaction(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let (shell, window) = empty_product_window(&documents, cx);
    let mut receiver = cx
        .update_window(window, |_, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.begin_picker(owner(None), "Pick".into(), picker_items(), None, cx)
            })
        })
        .unwrap()
        .unwrap();
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert_eq!(settle(&mut receiver, cx), TransientOutcome::Cancelled);
}

#[gpui::test]
fn unloading_the_extension_settles_its_transient(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    const SOURCE: &str = r#"
import { commands, workbench } from "knot";
await commands.register("transient.await-picker", async () => {
  await workbench.showPicker({ title: "Pick", items: [{ id: "a", label: "A" }] });
});
"#;
    let documents = install_globals(cx);
    let host = cx.update(crate::app::extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(711), ExtensionLifecycleId::new(1));
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
            .resolve("transient.await-picker")
            .is_ok()
    });
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
    let pending = dispatch_product_command(&shell, window, "transient.await-picker", cx);
    wait_for_api_condition(cx, |cx| shell.read(cx).transient.is_some());
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    cx.run_until_parked();
    cx.read(|cx| assert!(shell.read(cx).transient.is_none()));
    assert_ne!(wait_for_command(pending, cx), CommandOutcome::Completed);
}

#[gpui::test]
fn context_free_transient_requires_a_target(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    const SOURCE: &str = r#"
import { commands, workbench } from "knot";
async function rejects(action, code) {
  try { await action(); } catch (error) { if (error.code === code) return; throw error; }
  throw new Error(`expected ${code}`);
}
await rejects(() => workbench.showPicker({ title: "Pick", items: [] }), "ContextUnavailable");
await rejects(() => workbench.showInput({ title: "Name" }), "ContextUnavailable");
await rejects(() => workbench.showConfirmation({ title: "Sure?" }), "ContextUnavailable");
await rejects(() => workbench.showMessage({ title: "Notice" }), "ContextUnavailable");
await commands.register("transient.context-free-ok", () => {});
"#;
    let _documents = install_globals(cx);
    let host = cx.update(crate::app::extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(712), ExtensionLifecycleId::new(1));
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
            .resolve("transient.context-free-ok")
            .is_ok()
    });
}

fn transient_picker(
    shell: &Entity<KnotWindow>,
    cx: &mut TestAppContext,
) -> Entity<crate::app::picker::Picker> {
    cx.read(
        |cx| match shell.read(cx).transient.as_ref().unwrap().surface.as_ref() {
            Some(crate::app::transient::TransientSurface::Picker(picker)) => picker.clone(),
            _ => panic!("the transient is not a picker"),
        },
    )
}

fn clipboard(cx: &mut TestAppContext) -> Option<String> {
    cx.read(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

const PREVIEW_SOURCE: &str = r#"
import { commands, workbench } from "knot";
await commands.register("transient.preview", async () => {
  const seen = [];
  const picked = await workbench.showPicker({
    title: "Pick",
    items: [
      { id: "a", label: "Alpha" },
      { id: "b", label: "Beta", detail: "second", hint: "B" },
      { id: "c", label: "Gamma" },
    ],
    onHighlight: async (id) => {
      seen.push(id);
      await workbench.writeClipboardText(seen.join(","));
    },
  });
  await workbench.writeClipboardText(`${seen.join(",")}|${JSON.stringify(picked)}`);
});
"#;

fn start_preview(
    cx: &mut TestAppContext,
) -> (
    Entity<crate::app::extensions::host::ProductExtensionHost>,
    knot_protocol::ExtensionKey,
    Entity<KnotWindow>,
    gpui::AnyWindowHandle,
    crate::app::CommandExecution,
) {
    use knot_protocol::ExtensionKey;

    let documents = install_globals(cx);
    let host = cx.update(crate::app::extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(713), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, PREVIEW_SOURCE).unwrap();
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
            .resolve("transient.preview")
            .is_ok()
    });
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
    let pending = dispatch_product_command(&shell, window, "transient.preview", cx);
    // The initial highlight reaches the callback before anything moves.
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref()
            == Some("a")
    });
    (host, key, shell, window, pending)
}

#[gpui::test]
fn picker_previews_report_the_latest_highlight_then_the_selection(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, completion) = start_preview(cx);
    let picker = transient_picker(&shell, cx);
    picker.read_with(cx, |picker, _| {
        assert_eq!(picker.entry(1).detail.as_deref(), Some("second"));
        assert_eq!(picker.entry(1).hint.as_deref(), Some("B"));
    });
    // Two moves before the extension asks again collapse into the latest.
    picker.update(cx, |picker, cx| {
        picker.move_selection(1, cx);
        picker.move_selection(1, cx);
    });
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref()
            == Some("a,c")
    });
    picker.update(cx, |picker, cx| picker.set_query("zzz", cx));
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref()
            == Some("a,c,")
    });
    picker.update(cx, |picker, cx| {
        picker.set_query("beta", cx);
        picker.confirm(cx);
    });
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| text.contains('|'))
    });
    // The confirmation settles the picker; the highlight it raced is dropped.
    assert_eq!(
        clipboard(cx).as_deref(),
        Some(r#"a,c,|{"kind":"selected","id":"b"}"#)
    );
    assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);
}

#[gpui::test]
fn picker_previews_settle_on_dismissal_and_unload(cx: &mut TestAppContext) {
    let (host, key, shell, window, completion) = start_preview(cx);
    let picker = transient_picker(&shell, cx);
    cx.update_window(window, |_, _, cx| {
        picker.update(cx, |_, cx| {
            cx.emit(crate::app::picker::PickerEvent::Dismissed)
        });
    })
    .unwrap();
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| text.contains('|'))
    });
    assert_eq!(clipboard(cx).as_deref(), Some(r#"a|{"kind":"dismissed"}"#));
    assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);

    // Unloading while the extension awaits the next highlight settles it.
    let pending = dispatch_product_command(&shell, window, "transient.preview", cx);
    wait_for_api_condition(cx, |cx| shell.read(cx).transient.is_some());
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    cx.run_until_parked();
    cx.read(|cx| assert!(shell.read(cx).transient.is_none()));
    assert_ne!(wait_for_command(pending, cx), CommandOutcome::Completed);
}

fn transient_input(
    shell: &Entity<KnotWindow>,
    cx: &mut TestAppContext,
) -> Entity<crate::app::transient::InputView> {
    cx.read(
        |cx| match shell.read(cx).transient.as_ref().unwrap().surface.as_ref() {
            Some(crate::app::transient::TransientSurface::Input(input)) => input.clone(),
            _ => panic!("the transient is not an input"),
        },
    )
}

fn type_into(
    input: &Entity<crate::app::transient::InputView>,
    texts: &[&str],
    cx: &mut TestAppContext,
) {
    input.update(cx, |input, cx| {
        let field = input.input().clone();
        for text in texts {
            field.update(cx, |field, cx| field.set_text(*text, cx));
        }
    });
}

fn input_message(
    input: &Entity<crate::app::transient::InputView>,
    cx: &gpui::App,
) -> Option<String> {
    input.read(cx).message().map(str::to_string)
}

fn completion_labels(
    input: &Entity<crate::app::transient::InputView>,
    cx: &gpui::App,
) -> Vec<String> {
    input
        .read(cx)
        .items()
        .iter()
        .map(|item| item.label.clone())
        .collect()
}

fn press(
    input: &Entity<crate::app::transient::InputView>,
    key: &str,
    cx: &mut TestAppContext,
) -> bool {
    let keystroke = crate::app::keys::Keystroke::parse(key).unwrap();
    input.update(cx, |input, cx| input.key_down(&keystroke, cx))
}

fn field_text(input: &Entity<crate::app::transient::InputView>, cx: &gpui::App) -> String {
    input.read(cx).text(cx)
}

const LIVE_INPUT_SOURCE: &str = r#"
import { commands, completion, history, workbench } from "knot";
await commands.register("transient.live-input", async () => {
  const seen = [];
  const result = await workbench.showInput({
    title: "Line",
    value: "1",
    onChange: async (value) => {
      seen.push(value);
      await workbench.writeClipboardText(seen.join(","));
      return value === "" ? null : `message ${value}`;
    },
  });
  await workbench.writeClipboardText(`${seen.join(",")}|${JSON.stringify(result)}`);
});
await commands.register("transient.failing-input", async () => {
  const seen = [];
  try {
    await workbench.showInput({
      title: "Line",
      onChange: async (value) => {
        seen.push(value);
        await workbench.writeClipboardText(seen.join(","));
        if (value === "x") throw new Error("boom");
        return `ok ${value}`;
      },
    });
  } catch (error) {
    await workbench.writeClipboardText(`caught ${error.message}|${seen.join(",")}`);
    return;
  }
  await workbench.writeClipboardText("not rethrown");
});
const WORDS = ["alpha", "alps", "beta"];
await commands.register("transient.completing-input", async () => {
  const result = await workbench.showInput({
    title: "Word",
    onChange: async (value) => {
      const found = await completion.match(value, WORDS, { styles: ["prefix"] });
      return {
        items: found.map(({ index, ranges }) => ({
          label: WORDS[index],
          value: WORDS[index],
          detail: "word",
          icon: WORDS[index] === "beta" ? { folder: "beta" } : { file: WORDS[index] },
          matches: ranges,
        })),
      };
    },
  });
  await workbench.writeClipboardText(JSON.stringify(result));
});
await commands.register("transient.match", async () => {
  const found = await completion.match("set", ["xset", "set", "résetté"]);
  await workbench.writeClipboardText(JSON.stringify(found));
});
await commands.register("transient.history", async () => {
  for (const id of ["c", "b", "c"]) await history.record("things", id);
  const ordered = await history.order("things", ["a", "b", "c", "d"].map((id) => ({ id })));
  await workbench.writeClipboardText(
    `${(await history.recent("things")).join(",")}|${ordered.map((item) => item.id).join(",")}`,
  );
});
await commands.register("transient.malformed-input", async () => {
  try {
    await workbench.showInput({ title: "Word", onChange: () => ({ items: "nope" }) });
  } catch (error) {
    await workbench.writeClipboardText(`caught ${error.message}`);
    return;
  }
  await workbench.writeClipboardText("not rethrown");
});
await commands.register("transient.query", async () => {
  const seen = [];
  const result = await workbench.showPicker({
    title: "Query",
    onQuery: async (query) => {
      seen.push(query);
      await workbench.writeClipboardText(seen.join(","));
      return [
        { id: `${query}-1`, label: `one ${query}`, description: "dir", labelMatches: [[0, 3]] },
        { id: `${query}-2`, label: `two ${query}`, descriptionMatches: [[0, 1]] },
      ];
    },
  });
  await workbench.writeClipboardText(`${seen.join(",")}|${JSON.stringify(result)}`);
});
await commands.register("transient.failing-query", async () => {
  const seen = [];
  try {
    await workbench.showPicker({
      title: "Query",
      onQuery: async (query) => {
        seen.push(query);
        await workbench.writeClipboardText(seen.join(","));
        if (query === "x") throw new Error("boom");
        return [{ id: query, label: `row ${query}` }];
      },
    });
  } catch (error) {
    await workbench.writeClipboardText(`caught ${error.message}|${seen.join(",")}`);
    return;
  }
  await workbench.writeClipboardText("not rethrown");
});
await commands.register("transient.stale-query", async () => {
  await workbench.showPicker({
    title: "Query",
    onQuery: async (query) => {
      await workbench.writeClipboardText(`waiting ${query}`);
      while ((await workbench.readClipboardText()) !== `go ${query}`) {}
      return [{ id: query, label: `row ${query}` }];
    },
  });
});
await commands.register("transient.stale-input", async () => {
  await workbench.showInput({
    title: "Word",
    onChange: async (value) => {
      await workbench.writeClipboardText(`waiting ${value}`);
      while ((await workbench.readClipboardText()) !== `go ${value}`) {}
      return { message: `for ${value}`, items: [{ label: value, value }] };
    },
  });
});
"#;

fn start_live_input(
    name: &'static str,
    cx: &mut TestAppContext,
) -> (
    Entity<crate::app::extensions::host::ProductExtensionHost>,
    knot_protocol::ExtensionKey,
    Entity<KnotWindow>,
    gpui::AnyWindowHandle,
    crate::app::CommandExecution,
) {
    let (host, key, shell, window) = live_shell(cx);
    let pending = dispatch_product_command(&shell, window, name, cx);
    wait_for_api_condition(cx, |cx| shell.read(cx).transient.is_some());
    (host, key, shell, window, pending)
}

thread_local! {
    static STORAGE: std::cell::RefCell<Vec<tempfile::TempDir>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn live_shell(
    cx: &mut TestAppContext,
) -> (
    Entity<crate::app::extensions::host::ProductExtensionHost>,
    knot_protocol::ExtensionKey,
    Entity<KnotWindow>,
    gpui::AnyWindowHandle,
) {
    use knot_protocol::ExtensionKey;

    let documents = install_globals(cx);
    let host = cx.update(crate::app::extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(714), ExtensionLifecycleId::new(1));
    let storage = tempfile::tempdir().unwrap();
    let root = storage.path().to_owned();
    // The directory lives as long as the test's thread.
    STORAGE.with(|kept| kept.borrow_mut().push(storage));
    host.update(cx, |host, cx| host.set_test_storage_root(root, cx));
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source_with_storage(key, LIVE_INPUT_SOURCE, "@test/live-input")
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
            .resolve("transient.stale-input")
            .is_ok()
    });
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
    (host, key, shell, window)
}

#[gpui::test]
fn live_input_reports_the_latest_value_and_shows_returned_messages(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, completion) = start_live_input("transient.live-input", cx);
    let input = transient_input(&shell, cx);
    // The initial value reaches the callback, and its message shows once the
    // extension pulls again.
    wait_for_api_condition(cx, |cx| {
        input_message(&input, cx).as_deref() == Some("message 1")
    });
    assert_eq!(clipboard(cx).as_deref(), Some("1"));
    // Two edits before the extension asks again collapse into the latest.
    type_into(&input, &["12", "123"], cx);
    wait_for_api_condition(cx, |cx| {
        input_message(&input, cx).as_deref() == Some("message 123")
    });
    assert_eq!(clipboard(cx).as_deref(), Some("1,123"));
    // Returning nothing clears the message.
    type_into(&input, &[""], cx);
    wait_for_api_condition(cx, |cx| input_message(&input, cx).is_none());
    type_into(&input, &["42"], cx);
    wait_for_api_condition(cx, |cx| {
        input_message(&input, cx).as_deref() == Some("message 42")
    });
    input.update(cx, |_, cx| {
        cx.emit(crate::app::transient::InputEvent::Accepted("42".into()))
    });
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| text.contains('|'))
    });
    assert_eq!(
        clipboard(cx).as_deref(),
        Some(r#"1,123,,42|{"kind":"accepted","value":"42"}"#)
    );
    assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);
}

#[gpui::test]
fn a_throwing_change_callback_stops_and_rethrows_once_the_input_settles(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, completion) = start_live_input("transient.failing-input", cx);
    let input = transient_input(&shell, cx);
    wait_for_api_condition(cx, |cx| input_message(&input, cx).as_deref() == Some("ok "));
    type_into(&input, &["x"], cx);
    wait_for_api_condition(cx, |cx| input_message(&input, cx).is_none());
    // The input stays open and later values reach no callback.
    cx.read(|cx| assert!(shell.read(cx).transient.is_some()));
    type_into(&input, &["y"], cx);
    cx.run_until_parked();
    input.update(cx, |_, cx| {
        cx.emit(crate::app::transient::InputEvent::Dismissed)
    });
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| text.contains('|'))
    });
    assert_eq!(clipboard(cx).as_deref(), Some("caught boom|,x"));
    assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);
}

#[gpui::test]
fn live_input_settles_on_dismissal_and_unload(cx: &mut TestAppContext) {
    let (host, key, shell, window, completion) = start_live_input("transient.live-input", cx);
    let input = transient_input(&shell, cx);
    wait_for_api_condition(cx, |cx| {
        input_message(&input, cx).as_deref() == Some("message 1")
    });
    input.update(cx, |_, cx| {
        cx.emit(crate::app::transient::InputEvent::Dismissed)
    });
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| text.contains('|'))
    });
    assert_eq!(clipboard(cx).as_deref(), Some(r#"1|{"kind":"dismissed"}"#));
    assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);

    // Unloading while the extension awaits the next value settles it.
    let pending = dispatch_product_command(&shell, window, "transient.live-input", cx);
    wait_for_api_condition(cx, |cx| shell.read(cx).transient.is_some());
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    cx.run_until_parked();
    cx.read(|cx| assert!(shell.read(cx).transient.is_none()));
    assert_ne!(wait_for_command(pending, cx), CommandOutcome::Completed);
}

#[gpui::test]
fn input_completions_show_and_an_empty_answer_hides_them(cx: &mut TestAppContext) {
    let (_host, _key, shell, window, _completion) =
        start_live_input("transient.completing-input", cx);
    let input = transient_input(&shell, cx);
    wait_for_api_condition(cx, |cx| completion_labels(&input, cx).len() == 3);
    cx.read(|cx| assert_eq!(input.read(cx).selected(), 0));
    type_into(&input, &["al"], cx);
    wait_for_api_condition(cx, |cx| completion_labels(&input, cx) == ["alpha", "alps"]);
    cx.read(|cx| {
        let items = input.read(cx).items();
        assert_eq!(items[0].matches, Some(vec![[0, 2]]));
        assert_eq!(
            items[0].icon,
            Some(knot_protocol::IconRef::File {
                file: "alpha".into()
            })
        );
    });
    let mut visual = gpui::VisualTestContext::from_window(window, cx);
    visual.run_until_parked();
    for index in 0..2 {
        assert!(
            visual
                .debug_bounds(format!("completion-icon-{index}").leak())
                .is_some(),
            "completion {index} paints its icon"
        );
    }
    type_into(&input, &["zz"], cx);
    wait_for_api_condition(cx, |cx| completion_labels(&input, cx).is_empty());
}

#[gpui::test]
fn the_completion_highlight_moves_and_wraps(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, _completion) =
        start_live_input("transient.completing-input", cx);
    let input = transient_input(&shell, cx);
    wait_for_api_condition(cx, |cx| completion_labels(&input, cx).len() == 3);
    let selected = |cx: &mut TestAppContext| cx.read(|cx| input.read(cx).selected());
    assert!(press(&input, "up", cx));
    assert_eq!(selected(cx), 2);
    assert!(press(&input, "down", cx));
    assert_eq!(selected(cx), 0);
    assert!(press(&input, "ctrl-n", cx));
    assert_eq!(selected(cx), 1);
    assert!(press(&input, "ctrl-p", cx));
    assert_eq!(selected(cx), 0);
}

#[gpui::test]
fn tab_accepts_the_highlighted_completion_and_the_next_answer_replaces_the_list(
    cx: &mut TestAppContext,
) {
    let (_host, _key, shell, _window, _completion) =
        start_live_input("transient.completing-input", cx);
    let input = transient_input(&shell, cx);
    type_into(&input, &["a"], cx);
    wait_for_api_condition(cx, |cx| completion_labels(&input, cx) == ["alpha", "alps"]);
    assert!(press(&input, "down", cx));
    assert!(press(&input, "tab", cx));
    cx.read(|cx| assert_eq!(field_text(&input, cx), "alps"));
    wait_for_api_condition(cx, |cx| completion_labels(&input, cx) == ["alps"]);
    cx.read(|cx| assert_eq!(input.read(cx).selected(), 0));
}

#[gpui::test]
fn enter_submits_the_typed_text_with_a_row_highlighted(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, completion) =
        start_live_input("transient.completing-input", cx);
    let input = transient_input(&shell, cx);
    type_into(&input, &["al"], cx);
    wait_for_api_condition(cx, |cx| completion_labels(&input, cx) == ["alpha", "alps"]);
    assert!(press(&input, "down", cx));
    assert!(press(&input, "enter", cx));
    assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);
    assert_eq!(
        clipboard(cx).as_deref(),
        Some(r#"{"kind":"accepted","value":"al"}"#)
    );
}

#[gpui::test]
fn tab_without_a_list_does_nothing(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, _completion) =
        start_live_input("transient.completing-input", cx);
    let input = transient_input(&shell, cx);
    type_into(&input, &["zz"], cx);
    wait_for_api_condition(cx, |cx| completion_labels(&input, cx).is_empty());
    assert!(press(&input, "tab", cx));
    cx.read(|cx| assert_eq!(field_text(&input, cx), "zz"));
}

#[gpui::test]
fn feedback_for_older_text_is_dropped(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, _completion) = start_live_input("transient.stale-input", cx);
    let input = transient_input(&shell, cx);
    wait_for_api_condition(cx, |cx| clipboard_is(cx, "waiting "));
    type_into(&input, &["al"], cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("go ".into()));
    // The answer for the empty field is delivered while the field says "al".
    wait_for_api_condition(cx, |cx| clipboard_is(cx, "waiting al"));
    cx.read(|cx| {
        assert!(completion_labels(&input, cx).is_empty());
        assert_eq!(input.read(cx).message(), None);
    });
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("go al".into()));
    wait_for_api_condition(cx, |cx| completion_labels(&input, cx) == ["al"]);
    cx.read(|cx| assert_eq!(input.read(cx).message(), Some("for al")));
}

fn picker_labels(picker: &Entity<crate::app::picker::Picker>, cx: &gpui::App) -> Vec<String> {
    let picker = picker.read(cx);
    picker
        .visible_entries()
        .into_iter()
        .map(|entry| picker.entry(entry).label.clone())
        .collect()
}

fn clipboard_has_outcome(cx: &gpui::App) -> bool {
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .is_some_and(|text| text.contains('|'))
}

#[gpui::test]
fn a_queried_picker_shows_the_rows_answering_each_query(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, completion) = start_live_input("transient.query", cx);
    let picker = transient_picker(&shell, cx);
    // The empty query is answered first, and the rows keep their ranges.
    wait_for_api_condition(cx, |cx| picker_labels(&picker, cx) == ["one ", "two "]);
    picker.read_with(cx, |picker, _| {
        #[allow(clippy::single_range_in_vec_init)]
        let expected = vec![0..3];
        assert_eq!(picker.entry(0).label_matches, expected);
        assert_eq!(
            picker.entry(1).description_matches,
            Vec::<std::ops::Range<usize>>::new()
        );
    });
    // Queries typed before the extension asks again collapse into the latest.
    picker.update(cx, |picker, cx| {
        picker.set_query("a", cx);
        picker.set_query("ab", cx);
    });
    wait_for_api_condition(cx, |cx| picker_labels(&picker, cx) == ["one ab", "two ab"]);
    // Confirming names the supplied row.
    picker.update(cx, |picker, cx| {
        picker.move_selection(1, cx);
        picker.confirm(cx);
    });
    wait_for_api_condition(cx, clipboard_has_outcome);
    let text = clipboard(cx).unwrap();
    assert!(
        text.ends_with(r#"|{"kind":"selected","id":"ab-2"}"#),
        "{text}"
    );
    assert!(text.starts_with(',') && text.contains("ab|"), "{text}");
    assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);
}

#[gpui::test]
fn a_throwing_query_callback_stops_and_rethrows_once_the_picker_settles(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, completion) = start_live_input("transient.failing-query", cx);
    let picker = transient_picker(&shell, cx);
    wait_for_api_condition(cx, |cx| picker_labels(&picker, cx) == ["row "]);
    picker.update(cx, |picker, cx| picker.set_query("x", cx));
    wait_for_api_condition(cx, |cx| clipboard_is(cx, ",x"));
    // The picker stays open and later queries reach no callback.
    cx.read(|cx| assert!(shell.read(cx).transient.is_some()));
    picker.update(cx, |picker, cx| picker.set_query("y", cx));
    cx.run_until_parked();
    picker.update(cx, |_, cx| {
        cx.emit(crate::app::picker::PickerEvent::Dismissed)
    });
    wait_for_api_condition(cx, clipboard_has_outcome);
    assert_eq!(clipboard(cx).as_deref(), Some("caught boom|,x"));
    assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);
}

#[gpui::test]
fn rows_for_an_older_query_are_dropped(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, _completion) = start_live_input("transient.stale-query", cx);
    let picker = transient_picker(&shell, cx);
    wait_for_api_condition(cx, |cx| clipboard_is(cx, "waiting "));
    picker.update(cx, |picker, cx| picker.set_query("al", cx));
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("go ".into()));
    // The answer for the empty query is delivered while the field says "al".
    wait_for_api_condition(cx, |cx| clipboard_is(cx, "waiting al"));
    assert!(cx.read(|cx| picker_labels(&picker, cx)).is_empty());
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("go al".into()));
    wait_for_api_condition(cx, |cx| picker_labels(&picker, cx) == ["row al"]);
}

fn clipboard_is(cx: &gpui::App, expected: &str) -> bool {
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .is_some_and(|text| text == expected)
}

#[gpui::test]
fn a_malformed_completion_answer_rejects_the_call(cx: &mut TestAppContext) {
    let (_host, _key, shell, _window, completion) =
        start_live_input("transient.malformed-input", cx);
    let input = transient_input(&shell, cx);
    cx.run_until_parked();
    input.update(cx, |_, cx| {
        cx.emit(crate::app::transient::InputEvent::Dismissed)
    });
    assert_eq!(wait_for_command(completion, cx), CommandOutcome::Completed);
    assert!(
        clipboard(cx)
            .as_deref()
            .is_some_and(|text| text.starts_with("caught "))
    );
}

#[gpui::test]
fn completion_match_follows_the_styles_setting_and_reports_utf16_ranges(cx: &mut TestAppContext) {
    let (_host, _key, shell, window) = live_shell(cx);
    let run = |cx: &mut TestAppContext| {
        let pending = dispatch_product_command(&shell, window, "transient.match", cx);
        assert_eq!(wait_for_command(pending, cx), CommandOutcome::Completed);
        clipboard(cx).unwrap()
    };
    // The default flex style matches "set" as a subsequence anywhere.
    let flex = run(cx);
    assert!(
        flex.contains(r#""index":0"#) && flex.contains(r#""index":2"#),
        "{flex}"
    );
    cx.update(|cx| {
        cx.default_global::<crate::app::settings::ApplicationSettings>()
            .0
            .set(
                crate::app::settings::COMPLETION_STYLES,
                serde_json::json!(["prefix"]),
                Default::default(),
            )
            .unwrap();
    });
    assert_eq!(run(cx), r#"[{"index":1,"ranges":[[0,3]]}]"#);
    cx.update(|cx| {
        cx.default_global::<crate::app::settings::ApplicationSettings>()
            .0
            .set(
                crate::app::settings::COMPLETION_STYLES,
                serde_json::json!(["substring"]),
                Default::default(),
            )
            .unwrap();
    });
    // "résetté" holds "set" after a two-byte letter: UTF-16 offsets, not bytes.
    assert_eq!(
        run(cx),
        r#"[{"index":1,"ranges":[[0,3]]},{"index":0,"ranges":[[1,4]]},{"index":2,"ranges":[[2,5]]}]"#
    );
}

#[gpui::test]
fn history_orders_recent_items_first_and_persists_in_package_storage(cx: &mut TestAppContext) {
    let (_host, _key, shell, window) = live_shell(cx);
    let pending = dispatch_product_command(&shell, window, "transient.history", cx);
    assert_eq!(wait_for_command(pending, cx), CommandOutcome::Completed);
    assert_eq!(clipboard(cx).as_deref(), Some("c,b|c,b,a,d"));
}
