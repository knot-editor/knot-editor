//! The product command catalog, captured targets, palette, and dispatch.
use super::*;

#[gpui::test]
fn product_catalog_registers_every_slice_command(cx: &mut TestAppContext) {
    install_globals(cx);
    cx.read(|cx| {
        let dispatcher = cx.global::<ApplicationProductCommands>().0.read(cx);
        let mut actual = dispatcher
            .definitions()
            .map(|definition| definition.name.to_string())
            .collect::<Vec<_>>();
        let mut expected = super::super::super::commands::dispatch::product_command_names()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    });
}

#[gpui::test]
fn pooled_fixture_commands_and_completions_use_the_product_target(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model.clone(), cx);
    let host = cx.update(|cx| {
        let host = super::super::super::extensions::host::install(cx);
        host.update(cx, |host, cx| host.start_test_fixture("product", cx));
        host
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let registered = cx.read(|cx| {
            cx.global::<ApplicationProductCommands>()
                .0
                .read(cx)
                .definitions()
                .any(|definition| definition.name.as_ref() == "knot.fixture.primary")
        });
        if registered {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "fixture command did not enter the product catalog"
        );
        std::thread::yield_now();
    }

    let mut execution =
        dispatch_product_command(&window, native_window, "knot.fixture.primary", cx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let outcome = loop {
        cx.run_until_parked();
        match execution.completion.try_recv() {
            Ok(outcome) => break outcome,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
            Err(error) => panic!("fixture command completion closed: {error}"),
        }
    };
    assert_eq!(outcome, CommandOutcome::Completed);
    assert_eq!(cx.read(|cx| model.read(cx).text()), "fixture ");

    let mut completion = dispatch_product_command(
        &window,
        native_window,
        super::super::super::commands::dispatch::SHOW_COMPLETIONS_COMMAND,
        cx,
    );
    cx.run_until_parked();
    assert_eq!(
        completion.completion.try_recv().unwrap(),
        CommandOutcome::Completed
    );

    let editor = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone()
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if cx.read(|cx| {
            editor
                .read(cx)
                .editor()
                .read(cx)
                .completion_state()
                .is_some_and(|(items, pending, failures, _)| {
                    items == 2 && pending == 0 && failures == 0
                })
        }) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "fixture completion items did not reach the product editor"
        );
        std::thread::yield_now();
    }

    drop(host);
    cx.update(|cx| {
        drop(cx.remove_global::<super::super::super::extensions::host::ApplicationExtensionHost>())
    });
    cx.run_until_parked();
}

#[gpui::test]
async fn dispatch_completion_preserves_the_captured_pane(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, window_handle) = product_window(document, model, cx);

    cx.update_window(window_handle, |_, _, cx| {
        window.update(cx, |window, cx| {
            window.split(SplitDirection::Horizontal, cx)
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();

    let (first_pane, second_pane, first_editor, second_editor) = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let first = &workbench.panes()[0];
        let second = &workbench.panes()[1];
        (
            first.id(),
            second.id(),
            workbench.editor(first.active_tab()).unwrap().clone(),
            workbench.editor(second.active_tab()).unwrap().clone(),
        )
    });
    let target = cx
        .update_window(window_handle, |_, native_window, cx| {
            focus_view(&first_editor.focus_handle(cx), native_window, cx);
            window.update(cx, |window, cx| window.capture_command_target(cx))
        })
        .unwrap();
    let execution = cx.update(|cx| {
        let dispatcher = cx.global::<ApplicationProductCommands>().0.clone();
        dispatcher.update(cx, |dispatcher, cx| {
            dispatcher.dispatch(
                Command {
                    name: NEW_COMMAND.into(),
                    arguments: CommandArgumentValue::Null,
                },
                target,
                cx,
            )
        })
    });
    cx.update_window(window_handle, |_, native_window, cx| {
        focus_view(&second_editor.focus_handle(cx), native_window, cx)
    })
    .unwrap();

    assert_eq!(
        execution.completion.await.unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.pane(first_pane).unwrap().tabs().len(), 2);
        assert_eq!(workbench.pane(second_pane).unwrap().tabs().len(), 1);
    });
}

#[gpui::test]
async fn destroyed_product_targets_are_not_retargeted(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, window_handle) = product_window(document, model, cx);
    cx.refresh().unwrap();

    let target = cx
        .update_window(window_handle, |_, _, cx| {
            window.update(cx, |window, cx| {
                window.focus_active_editor(cx);
                window.capture_command_target(cx)
            })
        })
        .unwrap();
    cx.update_window(window_handle, |_, native_window, cx| {
        window.update(cx, |window, cx| window.close_active_tab(native_window, cx));
    })
    .unwrap();
    let execution = cx.update(|cx| {
        let dispatcher = cx.global::<ApplicationProductCommands>().0.clone();
        dispatcher.update(cx, |dispatcher, cx| {
            dispatcher.dispatch(
                Command {
                    name: SPLIT_HORIZONTAL_COMMAND.into(),
                    arguments: CommandArgumentValue::Null,
                },
                target,
                cx,
            )
        })
    });

    assert_eq!(
        execution.completion.await.unwrap(),
        CommandOutcome::InvalidTarget
    );
    cx.read(|cx| assert!(window.read(cx).workbench.read(cx).panes().is_empty()));
}

#[gpui::test]
fn menu_action_reaches_the_captured_window_in_its_dispatch_turn(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    cx.refresh().unwrap();

    cx.dispatch_action(native_window, ProductCommandSource::new(NEW_COMMAND));

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
        assert_eq!(
            cx.global::<ApplicationProductCommands>()
                .0
                .read(cx)
                .last_outcome(),
            Some(&CommandOutcome::Completed),
        );
    });
}

#[gpui::test]
async fn palette_discovers_commands_and_keeps_its_opening_target(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, window_handle) = product_window(document, model, cx);
    cx.update_window(window_handle, |_, _, cx| {
        window.update(cx, |window, cx| {
            window.split(SplitDirection::Horizontal, cx)
        });
    })
    .unwrap();
    cx.refresh().unwrap();

    let (first_pane, second_pane, first_editor) = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let first = &workbench.panes()[0];
        (
            first.id(),
            workbench.panes()[1].id(),
            workbench.editor(first.active_tab()).unwrap().clone(),
        )
    });
    cx.update_window(window_handle, |_, native_window, cx| {
        focus_view(&first_editor.focus_handle(cx), native_window, cx);
    })
    .unwrap();
    cx.simulate_keystrokes(window_handle, "secondary-shift-p");
    cx.run_until_parked();
    cx.update_window(window_handle, |_, _, cx| {
        assert!(window.read(cx).command_palette.is_some());
        let workbench = window.read(cx).workbench.clone();
        workbench.update(cx, |workbench, _| {
            workbench.focus_pane(second_pane);
        });
    })
    .unwrap();
    cx.refresh().unwrap();

    cx.simulate_keystrokes(window_handle, "f i l e . n e w enter");

    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.pane(first_pane).unwrap().tabs().len(), 2);
        assert_eq!(workbench.pane(second_pane).unwrap().tabs().len(), 1);
    });
}

#[gpui::test]
async fn native_participants_reject_arguments_before_mutation(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model.clone(), cx);
    let target = cx
        .update_window(native_window, |_, _, cx| {
            window.update(cx, |window, cx| window.focus_active_editor(cx));
            window.update(cx, |window, cx| window.capture_command_target(cx))
        })
        .unwrap();
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("ignored".into())));
    let dispatcher = cx.read(|cx| cx.global::<ApplicationProductCommands>().0.clone());
    for name in [
        super::super::COPY_COMMAND,
        super::super::PASTE_COMMAND,
        NEW_COMMAND,
        super::super::NEW_WINDOW_COMMAND,
        SHOW_EXTENSION_REPORT_COMMAND,
    ] {
        let execution = dispatcher.update(cx, |dispatcher, cx| {
            dispatcher.dispatch(
                Command {
                    name: name.into(),
                    arguments: CommandArgumentValue::String("unexpected".into()),
                },
                target.clone(),
                cx,
            )
        });
        assert!(
            matches!(
                execution.completion.await.unwrap(),
                CommandOutcome::InvalidArgument { .. }
            ),
            "{name}"
        );
    }
    cx.read(|cx| {
        assert_eq!(model.read(cx).text(), "");
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
        assert_eq!(cx.windows().len(), 1);
    });
}

#[gpui::test]
fn palette_shows_titles_descriptions_and_bindings_and_hides_opted_out_commands(
    cx: &mut TestAppContext,
) {
    use knot_protocol::ExtensionKey;
    use knot_protocol::{ExtensionId, ExtensionLifecycleId};

    const SOURCE: &str = r#"
import { commands, keymaps } from "knot";
await commands.register("fixture.described", async () => {}, {
  title: "Run Fixture",
  description: "Runs the fixture.",
});
await commands.register("fixture.hidden", async () => {}, { palette: false });
await commands.register("fixture.plain", async () => {});
keymaps.bind("ctrl-alt-f", "fixture.described");
"#;
    let documents = install_globals(cx);
    cx.update(crate::app::commands::keymaps::install);
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
            .resolve("fixture.plain")
            .is_ok()
            && cx
                .global::<crate::app::commands::keymaps::ApplicationKeymaps>()
                .0
                .shown_bindings(None)
                .contains_key("fixture.described")
    });
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            window.focus_active_surface(cx);
            window.open_command_palette(cx);
        });
    })
    .unwrap();
    cx.update(|cx| {
        let picker = window
            .read(cx)
            .command_palette
            .as_ref()
            .unwrap()
            .picker()
            .clone();
        let picker = picker.read(cx);
        let rows = picker
            .visible_entries()
            .into_iter()
            .map(|entry| picker.entry(entry).clone())
            .collect::<Vec<_>>();
        let described = rows.iter().find(|row| row.label == "Run Fixture").unwrap();
        assert_eq!(described.description.as_deref(), Some("Runs the fixture."));
        assert_eq!(
            described.hint,
            Some(crate::app::commands::keymaps::display_key("ctrl-alt-f"))
        );
        assert!(rows.iter().any(|row| row.label == "fixture.plain"));
        assert!(!rows.iter().any(|row| row.label == "fixture.hidden"));
        assert!(!rows.iter().any(|row| row.label == "Move left"));
        let save = rows.iter().find(|row| row.label == "Save").unwrap();
        assert!(save.hint.is_some());
    });
}

#[gpui::test]
fn confirming_a_palette_command_ranks_it_first_next_time(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let state_dir = tempfile::tempdir().unwrap();
    let database = crate::app::record_store::StateDatabase::open(
        state_dir.path().join("state.sqlite3"),
        cx.background_executor.clone(),
    );
    cx.update(|cx| {
        cx.set_global(crate::app::command_history::ApplicationCommandHistory(
            crate::app::command_history::CommandHistory::open(&database),
        ))
    });
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let first_command = |cx: &mut TestAppContext| {
        cx.update_window(native_window, |_, _, cx| {
            window.update(cx, |window, cx| {
                window.focus_active_surface(cx);
                window.open_command_palette(cx);
                let palette = window.command_palette.as_ref().unwrap();
                let entry = palette.picker().read(cx).selected_entry().unwrap();
                palette.command(entry).name.to_string()
            })
        })
        .unwrap()
    };
    assert_ne!(first_command(cx), "workbench.split-horizontal");
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "s p l i t r i g h t enter");
    cx.run_until_parked();
    cx.read(|cx| assert!(window.read(cx).command_palette.is_none()));
    assert_eq!(first_command(cx), "workbench.split-horizontal");
    cx.read(|cx| {
        assert_eq!(
            cx.global::<crate::app::command_history::ApplicationCommandHistory>()
                .0
                .recent(),
            ["workbench.split-horizontal"]
        );
    });
}

#[gpui::test]
fn closing_the_palette_returns_focus_to_its_origin(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model.clone(), cx);
    let editor = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        workbench
            .editor(workbench.panes()[0].active_tab())
            .unwrap()
            .clone()
    });
    cx.update_window(native_window, |_, native_window, cx| {
        focus_view(&editor.focus_handle(cx), native_window, cx);
    })
    .unwrap();
    cx.refresh().unwrap();
    let palette_key = "secondary-shift-p";
    let editor_focused = |cx: &mut TestAppContext| {
        cx.update_window(native_window, |_, native_window, cx| {
            editor.focus_handle(cx).is_focused(native_window)
        })
        .unwrap()
    };

    cx.simulate_keystrokes(native_window, palette_key);
    cx.run_until_parked();
    cx.simulate_keystrokes(native_window, "escape");
    cx.run_until_parked();
    cx.read(|cx| assert!(window.read(cx).command_palette.is_none()));
    assert!(editor_focused(cx));

    cx.simulate_keystrokes(native_window, palette_key);
    cx.run_until_parked();
    cx.read(|cx| assert!(window.read(cx).command_palette.is_some()));
    cx.simulate_keystrokes(native_window, "t o g g l e p i n enter");
    cx.run_until_parked();
    cx.read(|cx| assert!(window.read(cx).command_palette.is_none()));
    assert!(editor_focused(cx));

    cx.simulate_keystrokes(native_window, "a");
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(model.read(cx).text(), "a"));
}

#[gpui::test]
fn commands_run_in_a_window_without_a_native_window(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let window = cx.update(|cx| {
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model.clone(), cx));
        cx.new(|cx| KnotWindow::new(workbench, cx))
    });
    let run = |name: &'static str, cx: &mut TestAppContext| {
        let execution = cx.update(|cx| {
            let target = window.update(cx, |window, cx| window.capture_command_target(cx));
            KnotWindow::command_dispatcher(cx).update(cx, |dispatcher, cx| {
                dispatcher.dispatch(
                    Command {
                        name: name.into(),
                        arguments: CommandArgumentValue::Null,
                    },
                    target,
                    cx,
                )
            })
        });
        wait_for_command(execution, cx)
    };

    // Native editing and movement need no window either.
    model.update(cx, |model, cx| {
        model.replace(0..0, "one\ntwo").unwrap();
        cx.notify();
    });
    for name in [
        "editor.move-down",
        "editor.move-line-end",
        "editor.delete-backward",
        "editor.move-up",
        "editor.select-line-start",
    ] {
        assert_eq!(run(name, cx), CommandOutcome::Completed, "{name}");
    }
    cx.read(|cx| {
        assert_eq!(model.read(cx).text(), "one\ntw");
        let workbench = window.read(cx).workbench.read(cx);
        let view = workbench.focused_editor().unwrap();
        assert_eq!(view.read(cx).editor().read(cx).selection(cx), (2, 0));
    });

    assert_eq!(run(SPLIT_HORIZONTAL_COMMAND, cx), CommandOutcome::Completed);
    assert_eq!(run(NEW_COMMAND, cx), CommandOutcome::Completed);
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.panes().len(), 2);
        assert_eq!(workbench.focused_pane().unwrap().tabs().len(), 2);
    });
    // Closing the last tab of a clean document needs no prompt.
    assert_eq!(run(CLOSE_TAB_COMMAND, cx), CommandOutcome::Completed);
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.focused_pane().unwrap().tabs().len(), 1);
    });
}

struct CountingOpenDialog(Arc<std::sync::atomic::AtomicUsize>);

impl ProductOpenDialog for CountingOpenDialog {
    fn select(&self) -> OpenDialogFuture {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async { OpenDialogOutcome::Cancelled })
    }
}

#[gpui::test]
async fn open_runs_in_a_new_blank_window_without_a_focused_tab(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let opened = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    cx.set_global(ApplicationOpenDialog(Arc::new(CountingOpenDialog(
        opened.clone(),
    ))));
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, first) = product_window(document, model, cx);
    cx.update_window(first, |_, native_window, cx| {
        native_window.activate_window();
        window.update(cx, |window, cx| window.focus_active_editor(cx));
    })
    .unwrap();
    cx.run_until_parked();

    cx.simulate_keystrokes(first, "secondary-shift-p");
    cx.run_until_parked();
    cx.simulate_keystrokes(first, "w i n d o w . n e w enter");
    cx.run_until_parked();
    let second = cx
        .windows()
        .into_iter()
        .find(|window| *window != first)
        .expect("a new window");
    cx.update_window(second, |_, native_window, _| {
        native_window.activate_window()
    })
    .unwrap();
    cx.run_until_parked();

    cx.simulate_keystrokes(second, "secondary-o");
    cx.run_until_parked();
    assert_eq!(opened.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[gpui::test]
async fn windowless_menu_commands_run_after_the_last_window_closes(cx: &mut TestAppContext) {
    use super::super::super::commands::dispatch::{
        ApplicationCommandSource, NEW_WINDOW_COMMAND, OPEN_COMMAND,
    };
    use super::super::ApplicationLaunchGate;
    let opened = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    install_globals(cx);
    cx.set_global(ApplicationOpenDialog(Arc::new(CountingOpenDialog(
        opened.clone(),
    ))));
    cx.update(super::super::bind_product_keys);
    cx.update(super::super::launch::install_menus);
    install_launch_gate(cx);
    cx.update(|cx| {
        cx.global::<ApplicationLaunchGate>()
            .0
            .borrow_mut()
            .complete();
    });
    let menu = |command: &'static str| ApplicationCommandSource {
        name: command.into(),
    };
    let close_all = |cx: &mut TestAppContext| {
        for window in cx.windows() {
            cx.update_window(window, |_, native_window, _| native_window.remove_window())
                .unwrap();
        }
        cx.run_until_parked();
        assert!(cx.windows().is_empty());
    };

    cx.update(|cx| {
        assert!(cx.is_action_available(&menu(NEW_WINDOW_COMMAND)));
        assert!(!cx.is_action_available(&ProductCommandSource::new(SAVE_COMMAND)));
    });

    cx.update(|cx| cx.dispatch_action(&menu(NEW_WINDOW_COMMAND)));
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    close_all(cx);

    cx.update(|cx| cx.dispatch_action(&menu(NEW_COMMAND)));
    cx.run_until_parked();
    let window = cx.windows()[0].downcast::<KnotWindow>().unwrap();
    cx.read(|cx| {
        let window = window.entity(cx).unwrap();
        let workbench = window.read(cx).workbench.read(cx);
        assert!(workbench.focused_editor().is_some());
    });
    close_all(cx);

    cx.update(|cx| cx.dispatch_action(&menu(OPEN_COMMAND)));
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(opened.load(std::sync::atomic::Ordering::SeqCst), 1);
}
