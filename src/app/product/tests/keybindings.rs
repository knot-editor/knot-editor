//! Keymap precedence and script keybindings across config, extensions, and native defaults.
use super::*;

#[gpui::test]
fn script_keybindings_follow_config_phases_and_runtime_replacement(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("pre-init.js"),
        "import * as knot from 'knot'; knot.keymaps.bind('cmd-n', 'file.open');",
    )
    .unwrap();
    std::fs::write(
        config.path().join("post-init.js"),
        "import * as knot from 'knot'; knot.keymaps.bind('cmd-n', 'file.save-as'); knot.keymaps.bind('cmd-x', null); knot.keymaps.bind('cmd-j', { name: 'test.args', arguments: { marker: 42 } });",
    )
    .unwrap();
    install_extension(
        extensions.path(),
        "@example/good",
        &[],
        &[(
            "main.js",
            r#"
                import * as knot from 'knot';
                knot.keymaps.bind('cmd-n', 'file.save');
                knot.keymaps.bind('cmd-r', 'file.open');
                await knot.commands.register('test.remap', () => {
                    knot.keymaps.bind('cmd-r', 'file.new');
                    knot.keymaps.unbind('cmd-n');
                });
                await knot.commands.register('test.unbind', () => knot.keymaps.bind('cmd-r', null, { view: 'editor' }));
                await knot.commands.register('test.restore', () => knot.keymaps.unbind('cmd-r', { view: 'editor' }));
                await knot.commands.register('test.remove', () => knot.keymaps.unbind('cmd-r'));
                await knot.commands.register('test.args', ({ arguments: args }) => {
                    if (args.marker !== 42) throw new Error('wrong binding arguments');
                });
                await knot.commands.register('test.invalid', () => {
                    knot.keymaps.bind('cmd-r', 'file.save');
                    knot.keymaps.bind('bad-modifier-key', 'file.new');
                });
            "#,
        )],
    );
    install_extension(
        extensions.path(),
        "@example/bad",
        &[],
        &[(
            "main.js",
            "import * as knot from 'knot'; knot.keymaps.bind('cmd-l', 'file.new'); await knot.workbench.writeClipboardText('startup'); throw new Error('failed after binding');",
        )],
    );
    install_globals(cx);
    cx.update(super::super::bind_product_keys);
    install_launch_gate(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    cx.read(|cx| {
        assert_eq!(keymap_command(cx, "cmd-n", Some("editor")).as_deref(), Some("file.save-as"));
        assert_eq!(keymap_command(cx, "cmd-r", Some("editor")).as_deref(), Some("file.open"));
        assert_eq!(keymap_command(cx, "cmd-x", Some("editor")).as_deref(), Some("<unbound>"));
        assert_eq!(keymap_command(cx, "cmd-l", Some("editor")), None);
        assert!(matches!(
            cx.global::<ApplicationKeymaps>().0.resolve(&crate::app::keys::Keystroke::parse("cmd-j").unwrap().unparse(), Some("editor")),
            Some(BindingResolution::Command(Command { arguments: CommandArgumentValue::Object(arguments), .. }))
                if arguments.get("marker") == Some(&CommandArgumentValue::Number(42.0))
        ));
    });
    let report = cx
        .read(super::super::super::extensions::host::startup_report)
        .unwrap();
    assert!(report.report.entries.iter().any(|entry| {
        entry.name.as_deref() == Some("@example/bad")
            && matches!(&entry.result, super::super::super::extensions::load::LoadResult::Failed(cause) if cause.contains("failed after binding"))
    }));
    assert!(cx.read(|cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .definitions()
            .any(|definition| definition.name.as_ref() == "test.args")
    }));
    let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let window = headless_window(document, model, cx);
    let native_window = Headless;
    let target = native_window.run(cx, |cx| {
        window.update(cx, |window, cx| {
            window.focus_active_surface(cx);
            window.capture_command_target(cx)
        })
    });
    let command = cx.read(|cx| {
        let Some(BindingResolution::Command(command)) =
            cx.global::<ApplicationKeymaps>().0.resolve(
                &crate::app::keys::Keystroke::parse("cmd-j")
                    .unwrap()
                    .unparse(),
                Some("editor"),
            )
        else {
            panic!("expected command binding");
        };
        command
    });
    let execution = cx.update(|cx| {
        let dispatcher = cx.global::<ApplicationProductCommands>().0.clone();
        dispatcher.update(cx, |dispatcher, cx| {
            dispatcher.dispatch(command, target, cx)
        })
    });
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    let execution = dispatch_product_command(&window, native_window, "test.remap", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.read(|cx| {
        assert_eq!(
            keymap_command(cx, "cmd-r", Some("editor")).as_deref(),
            Some("file.new")
        );
        assert_eq!(
            keymap_command(cx, "cmd-n", Some("editor")).as_deref(),
            Some("file.save-as")
        );
    });
    let invalid = wait_for_command(
        dispatch_product_command(&window, native_window, "test.invalid", cx),
        cx,
    );
    assert!(
        matches!(invalid, CommandOutcome::HandlerFailure { message } if message.contains("keybinding operation 2"))
    );
    cx.read(|cx| {
        assert_eq!(
            keymap_command(cx, "cmd-r", Some("editor")).as_deref(),
            Some("file.new")
        )
    });
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&window, native_window, "test.unbind", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert_eq!(
            keymap_command(cx, "cmd-r", Some("editor")).as_deref(),
            Some("<unbound>")
        );
        assert_eq!(
            keymap_command(cx, "cmd-r", Some("terminal")).as_deref(),
            Some("file.new")
        );
    });
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&window, native_window, "test.restore", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert_eq!(
            keymap_command(cx, "cmd-r", Some("editor")).as_deref(),
            Some("file.new")
        )
    });
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&window, native_window, "test.remove", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    cx.read(|cx| assert_eq!(keymap_command(cx, "cmd-r", Some("editor")), None));
}

#[gpui::test]
fn config_remaps_product_keys_without_changing_menu_or_palette_dispatch(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("pre-init.js"),
        "import * as knot from 'knot'; knot.keymaps.bind('cmd-n', null); knot.keymaps.bind('cmd-g', 'file.new');",
    )
    .unwrap();
    std::fs::write(
        config.path().join("post-init.js"),
        "import * as knot from 'knot'; knot.keymaps.bind('cmd-r', 'test.mark');",
    )
    .unwrap();
    install_extension(
        extensions.path(),
        "@example/mark",
        &[],
        &[(
            "main.js",
            "import * as knot from 'knot'; await knot.commands.register('test.mark', () => knot.keymaps.bind('cmd-y', 'file.new'));",
        )],
    );
    install_globals(cx);
    cx.update(super::super::bind_product_keys);
    install_launch_gate(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    let native_window = cx.windows()[0];
    let window = cx
        .update_window(native_window, |_, native_window, _| {
            native_window.root::<KnotWindow>().flatten()
        })
        .unwrap()
        .unwrap();
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    cx.refresh().unwrap();

    cx.simulate_keystrokes(native_window, "cmd-n");
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(window.read(cx).workbench.read(cx).tab_snapshots().len(), 0));
    cx.simulate_keystrokes(native_window, "cmd-g");
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(window.read(cx).workbench.read(cx).tab_snapshots().len(), 1));

    cx.simulate_keystrokes(native_window, "cmd-r");
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| keymap_command(cx, "cmd-y", Some("editor")).as_deref() == Some("file.new"))
    });

    cx.dispatch_action(native_window, ProductCommandSource::new(NEW_COMMAND));
    cx.read(|cx| assert_eq!(window.read(cx).workbench.read(cx).tab_snapshots().len(), 2));
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.open_command_palette(cx));
    })
    .unwrap();
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "f i l e . n e w enter");
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(window.read(cx).workbench.read(cx).tab_snapshots().len(), 3));
}

#[gpui::test]
fn config_global_copy_binding_routes_to_editor_and_extension_view(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;

    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("post-init.js"),
        "import * as knot from 'knot'; knot.keymaps.bind('cmd-j', 'copy');",
    )
    .unwrap();
    install_extension(
        extensions.path(),
        "@example/outline-copy",
        &[],
        &[(
            "main.js",
            r#"
                import * as knot from 'knot';
                await knot.workbench.registerView({
                    id: 'example.outline',
                    title: 'Outline',
                    content: { kind: 'tree', provider: {
                        getChildren(parentId) {
                            return parentId === null
                                ? [{ id: 'first', label: 'Tree item', collapsibleState: 'none' }]
                                : [];
                        },
                    } },
                });
                await knot.commands.registerForView('example.outline', 'copy', async () => {
                    const text = await knot.workbench.selectedTreeItem();
                    await knot.workbench.writeClipboardText(text);
                });
            "#,
        )],
    );
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    install_launch_gate(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    cx.read(|cx| {
        assert_eq!(
            keymap_command(cx, "cmd-j", Some("editor")).as_deref(),
            Some("copy")
        );
        assert_eq!(
            keymap_command(cx, "cmd-j", Some("example.outline")).as_deref(),
            Some("copy")
        );
    });
    let native_window = cx.windows()[0];
    let window = cx
        .update_window(native_window, |_, native_window, _| {
            native_window.root::<KnotWindow>().flatten()
        })
        .unwrap()
        .unwrap();
    let tree = wait_for_contributed_view(&window, "example.outline", cx);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| tree.read(cx).root_labels() == ["Tree item"])
    });
    // The launched window starts empty; a new document gives it an editor.
    cx.dispatch_action(native_window, ProductCommandSource::new(NEW_COMMAND));
    cx.run_until_parked();

    cx.update_window(native_window, |_, native_window, cx| {
        let editor = window
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone();
        editor.update(cx, |editor, cx| {
            editor.replace_text_in_range(None, "Editor selection", native_window, cx);
            editor.editor().clone().update(cx, |editor, cx| {
                editor.execute_editing_command("editor.select-all", cx)
            });
        });
        focus_view(&editor.focus_handle(cx), native_window, cx);
    })
    .unwrap();
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "cmd-j");
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("Editor selection")
        );
    });

    cx.update_window(native_window, |_, native_window, cx| {
        tree.update(cx, |tree, cx| assert!(tree.select_item("first", cx)));
        focus_view(&tree.focus_handle(cx), native_window, cx);
    })
    .unwrap();
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("untouched".into())));
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "cmd-j");
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref()
                == Some("first")
        })
    });
    cx.read(|cx| assert_eq!(documents.read(cx).documents().count(), 1));
}

#[gpui::test]
fn invalid_config_keybinding_batch_rolls_back_before_product_readiness(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("pre-init.js"),
        "import * as knot from 'knot'; knot.keymaps.bind('cmd-r', 'file.open'); knot.keymaps.bind('bad-modifier-key', 'file.new');",
    )
    .unwrap();
    install_globals(cx);
    cx.update(super::super::bind_product_keys);
    install_launch_gate(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(!wait_for_product_startup(cx));
    cx.read(|cx| {
        assert_eq!(keymap_command(cx, "cmd-r", Some("editor")), None);
        assert_eq!(host.read(cx).lifecycle_count(), 0);
        assert!(cx.global::<ApplicationWindows>().0.borrow().is_empty());
    });
    assert_eq!(cx.windows().len(), 1);
}

#[gpui::test]
fn invalid_extension_keybinding_batch_does_not_block_other_extensions(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    install_extension(
        extensions.path(),
        "@example/good",
        &[],
        &[(
            "main.js",
            "import * as knot from 'knot'; knot.keymaps.bind('cmd-r', 'file.new');",
        )],
    );
    install_extension(
        extensions.path(),
        "@example/bad",
        &[],
        &[(
            "main.js",
            "import * as knot from 'knot'; knot.keymaps.bind('cmd-l', 'file.new'); knot.keymaps.bind('bad-modifier-key', 'file.open');",
        )],
    );
    install_globals(cx);
    cx.update(super::super::bind_product_keys);
    install_launch_gate(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    let report = cx
        .read(super::super::super::extensions::host::startup_report)
        .unwrap();
    assert!(report.report.entries.iter().any(|entry| {
        entry.name.as_deref() == Some("@example/good")
            && entry.result == super::super::super::extensions::load::LoadResult::Loaded
    }));
    assert!(report.report.entries.iter().any(|entry| {
        entry.name.as_deref() == Some("@example/bad")
            && matches!(&entry.result, super::super::super::extensions::load::LoadResult::Failed(cause) if cause.contains("keybinding operation 2"))
    }));
    cx.read(|cx| {
        assert_eq!(
            keymap_command(cx, "cmd-r", Some("editor")).as_deref(),
            Some("file.new")
        );
        assert_eq!(keymap_command(cx, "cmd-l", Some("editor")), None);
    });
}

#[gpui::test]
fn unloading_a_script_lifecycle_removes_its_keybindings(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;

    install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(super::super::super::extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(91), ExtensionLifecycleId::new(1));
    let loaded = Arc::new(Mutex::new(false));
    let done = loaded.clone();
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source(
                key,
                "import * as knot from 'knot'; knot.keymaps.bind('cmd-r', 'file.open');",
            )
            .unwrap();
        cx.spawn(async move |_, _| {
            execution.await.unwrap();
            *done.lock().unwrap() = true;
        })
        .detach();
    });
    crate::test_support::wait_until(cx, |_| *loaded.lock().unwrap());
    cx.read(|cx| {
        assert_eq!(
            keymap_command(cx, "cmd-r", None).as_deref(),
            Some("file.open")
        )
    });
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    cx.read(|cx| assert_eq!(keymap_command(cx, "cmd-r", None), None));
}

#[gpui::test]
async fn keybinding_adapter_enters_the_product_dispatcher(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, window_handle) = product_window(document, model, cx);
    cx.update_window(window_handle, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();

    cx.simulate_keystrokes(window_handle, "secondary-n");
    cx.run_until_parked();

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
            Some(&CommandOutcome::Completed)
        );
    });
}

#[gpui::test]
fn personal_global_binding_overrides_editor_default_in_every_window(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let mut windows = Vec::new();
    for _ in 0..2 {
        let document = cx.update(|cx| create_untitled_document(&documents, cx));
        let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
        let (window, native_window) = product_window(document, model.clone(), cx);
        cx.update_window(native_window, |_, _, cx| {
            window.update(cx, |window, cx| window.focus_active_editor(cx))
        })
        .unwrap();
        windows.push((native_window, model));
    }
    cx.update(|cx| {
        let changed = cx
            .global_mut::<ApplicationKeymaps>()
            .0
            .set(
                BindingOwner::Extension(ExtensionId::new(0), ExtensionLifecycleId::new(1)),
                "Enter",
                None,
                Some(Command {
                    name: super::super::super::commands::dispatch::INSERT_TAB_COMMAND.into(),
                    arguments: CommandArgumentValue::Null,
                }),
            )
            .unwrap();
        assert!(changed);
        super::super::super::commands::keymaps::rebuild(cx);
    });
    cx.refresh().unwrap();
    for (native_window, _) in &windows {
        cx.simulate_keystrokes(*native_window, "enter");
    }
    cx.run_until_parked();
    cx.read(|cx| {
        for (_, model) in &windows {
            assert_eq!(model.read(cx).text(), "    ");
        }
    });
}

#[gpui::test]
fn unbinding_an_unmapped_letter_preserves_editor_input(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let window = headless_window(document, model.clone(), cx);
    let native_window = Headless;
    native_window.run(cx, |cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    });
    cx.update(|cx| {
        let owner = BindingOwner::Extension(ExtensionId::new(0), ExtensionLifecycleId::new(1));
        assert!(
            cx.global_mut::<ApplicationKeymaps>()
                .0
                .set(owner, "a", Some("editor"), None)
                .unwrap()
        );
        super::super::super::commands::keymaps::rebuild(cx);
    });
    press_keys(&window, "a", cx);
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(model.read(cx).text(), "a"));
}

#[gpui::test]
async fn editor_unbind_leaves_global_binding_available_in_terminal(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    // A named key, because Windows and Linux terminals leave ctrl with a
    // printable character to the window.
    cx.update(|cx| {
        let registry = &mut cx.global_mut::<ApplicationKeymaps>().0;
        let owner = BindingOwner::Extension(ExtensionId::new(0), ExtensionLifecycleId::new(1));
        registry
            .set(
                owner,
                "f9",
                None,
                Some(Command {
                    name: NEW_COMMAND.into(),
                    arguments: CommandArgumentValue::Null,
                }),
            )
            .unwrap();
        assert!(registry.set(owner, "f9", Some("editor"), None).unwrap());
        super::super::super::commands::keymaps::rebuild(cx);
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "f9");
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(
            window.read(cx).workbench.read(cx).panes()[0].tabs().len(),
            1
        )
    });

    let terminal =
        dispatch_product_command(&window, native_window, NEW_TERMINAL_IN_CENTER_COMMAND, cx);
    assert_eq!(
        terminal.completion.await.unwrap(),
        CommandOutcome::Completed
    );
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "f9");
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(
            window.read(cx).workbench.read(cx).panes()[0].tabs().len(),
            3
        )
    });
}

#[gpui::test]
fn sequence_prefix_dispatches_the_product_command(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "secondary-k right");
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(window.read(cx).workbench.read(cx).panes().len(), 2));
}

#[gpui::test]
fn unavailable_high_priority_binding_does_not_invoke_native_default(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    cx.update(|cx| {
        assert!(
            cx.global_mut::<ApplicationKeymaps>()
                .0
                .set(
                    BindingOwner::Extension(ExtensionId::new(0), ExtensionLifecycleId::new(1)),
                    "secondary-n",
                    None,
                    Some(Command {
                        name: "missing.command".into(),
                        arguments: CommandArgumentValue::Null,
                    }),
                )
                .unwrap()
        );
        super::super::super::commands::keymaps::rebuild(cx);
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "secondary-n");
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(
            window.read(cx).workbench.read(cx).panes()[0].tabs().len(),
            1
        );
        assert_eq!(
            cx.global::<ApplicationProductCommands>()
                .0
                .read(cx)
                .last_outcome(),
            Some(&CommandOutcome::Unavailable),
        );
    });
}

#[cfg(windows)]
#[gpui::test]
fn windows_native_ctrl_shift_p_opens_command_palette(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "ctrl-shift-p");
    cx.run_until_parked();
    cx.read(|cx| assert!(window.read(cx).command_palette.is_some()));
}

#[gpui::test]
fn personal_config_ctrl_shift_p_opens_command_palette(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("post-init.js"),
        "import * as knot from 'knot'; knot.keymaps.bind('ctrl-shift-p', 'workbench.show-command-palette');",
    )
    .unwrap();
    install_globals(cx);
    cx.update(super::super::bind_product_keys);
    install_launch_gate(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    let native_window = cx.windows()[0];
    let window = cx
        .update_window(native_window, |_, native_window, _| {
            native_window.root::<KnotWindow>().flatten()
        })
        .unwrap()
        .unwrap();
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx));
    })
    .unwrap();
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "ctrl-shift-p");
    cx.run_until_parked();
    cx.read(|cx| assert!(window.read(cx).command_palette.is_some()));
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn macos_emacs_keys_edit_through_the_knot_preset(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    model.update(cx, |model, _| model.replace(0..0, "hello world").unwrap());
    let window = headless_window(document, model.clone(), cx);
    let native_window = Headless;
    native_window.run(cx, |cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    });
    press_keys(
        &window,
        "ctrl-e ctrl-b ctrl-b ctrl-h ctrl-a ctrl-f ctrl-k",
        cx,
    );
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(model.read(cx).text(), "h"));
}

#[gpui::test]
async fn linux_preset_leaves_plain_control_keys_to_the_terminal(cx: &mut TestAppContext) {
    use super::super::super::commands::keymaps::{BindingRegistry, Platform, knot_preset};
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update(|cx| {
        let mut registry = BindingRegistry::new();
        registry.replace_preset(&knot_preset(Platform::Linux));
        cx.global_mut::<ApplicationKeymaps>().0 = registry;
        super::super::super::commands::keymaps::rebuild(cx);
    });
    let terminal = dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx);
    assert_eq!(
        terminal.completion.await.unwrap(),
        CommandOutcome::Completed
    );
    cx.refresh().unwrap();
    let tabs = |cx: &mut TestAppContext| {
        cx.read(|cx| window.read(cx).workbench.read(cx).tab_snapshots().len())
    };
    assert_eq!(tabs(cx), 2);
    cx.simulate_keystrokes(native_window, "ctrl-w");
    cx.run_until_parked();
    assert_eq!(tabs(cx), 2);

    cx.simulate_keystrokes(native_window, "ctrl-shift-p");
    cx.run_until_parked();
    cx.read(|cx| assert!(window.read(cx).command_palette.is_some()));
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn selected_contributed_preset_replaces_knot_beneath_extension_bindings(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("post-init.js"),
        r#"import * as knot from 'knot';
        await knot.keymaps.registerPreset({ name: 'test', title: 'Test', bindings: [
            { key: 'cmd-r', command: 'file.new' },
            { key: 'cmd-j', command: 'file.open' },
        ] });
        knot.keymaps.bind('cmd-j', 'file.save-as');
        await knot.settings.set('keymap.preset', 'test');
        await knot.commands.register('test.knot', () => knot.settings.set('keymap.preset', 'knot'));"#,
    )
    .unwrap();
    install_globals(cx);
    cx.update(super::super::bind_product_keys);
    install_launch_gate(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    cx.read(|cx| {
        assert_eq!(
            keymap_command(cx, "cmd-r", None).as_deref(),
            Some("file.new")
        );
        assert_eq!(keymap_command(cx, "cmd-s", None), None);
        assert_eq!(
            keymap_command(cx, "cmd-j", None).as_deref(),
            Some("file.save-as")
        );
    });
    let native_window = cx.windows()[0];
    let window = cx
        .update_window(native_window, |_, native_window, _| {
            native_window.root::<KnotWindow>().flatten()
        })
        .unwrap()
        .unwrap();
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx));
    })
    .unwrap();
    let tabs = |cx: &mut TestAppContext| {
        cx.read(|cx| window.read(cx).workbench.read(cx).tab_snapshots().len())
    };
    let before = tabs(cx);
    cx.refresh().unwrap();
    cx.simulate_keystrokes(native_window, "cmd-r");
    cx.run_until_parked();
    assert_eq!(tabs(cx), before + 1);

    drop(dispatch_product_command(
        &window,
        native_window,
        "test.knot",
        cx,
    ));
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| keymap_command(cx, "cmd-s", None).is_some())
    });
    cx.read(|cx| {
        assert_eq!(keymap_command(cx, "cmd-r", None), None);
        assert_eq!(
            keymap_command(cx, "cmd-s", None).as_deref(),
            Some("file.save")
        );
        assert_eq!(
            keymap_command(cx, "cmd-j", None).as_deref(),
            Some("file.save-as")
        );
    });
}

#[gpui::test]
fn show_keybindings_lists_owners_and_overrides_and_runs_the_accepted_command(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    let save = crate::app::keys::Keystroke::parse("secondary-s")
        .unwrap()
        .unparse();
    cx.update(|cx| {
        cx.global_mut::<ApplicationKeymaps>()
            .0
            .set(
                BindingOwner::Extension(ExtensionId::new(0), ExtensionLifecycleId::new(1)),
                &save,
                None,
                Some(Command {
                    name: NEW_COMMAND.into(),
                    arguments: CommandArgumentValue::Null,
                }),
            )
            .unwrap();
    });
    let execution = dispatch_product_command(
        &window,
        native_window,
        super::super::commands::SHOW_KEYBINDINGS_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.run_until_parked();
    cx.read(|cx| {
        let picker = match window.read(cx).transient.as_ref().unwrap().surface.as_ref() {
            Some(crate::app::transient::TransientSurface::Picker(picker)) => picker.read(cx),
            _ => panic!("the transient is not a picker"),
        };
        let rows = picker
            .visible_entries()
            .into_iter()
            .map(|entry| picker.entry(entry).clone())
            .collect::<Vec<_>>();
        let row = |label: &str, owner: &str| {
            rows.iter()
                .find(|row| {
                    row.label == label
                        && row.description.as_deref() == Some(&format!("{save} · {owner}"))
                })
                .unwrap_or_else(|| panic!("no {label} row from {owner}"))
        };
        let overriding = row("New Document", "extension");
        assert_eq!(overriding.detail, None);
        assert_eq!(
            overriding.hint,
            Some(crate::app::commands::keymaps::display_key(&save))
        );
        assert_eq!(
            row("Save", "Knot preset").detail.as_deref(),
            Some("Overridden by extension")
        );
        assert!(rows.iter().any(|row| {
            row.label == "Move left"
                && row
                    .description
                    .as_deref()
                    .is_some_and(|description| description.ends_with(" · in editor"))
        }));
    });
    let tabs = |cx: &mut TestAppContext| {
        cx.read(|cx| window.read(cx).workbench.read(cx).tab_snapshots().len())
    };
    let before = tabs(cx);
    cx.simulate_input(native_window, "New Document");
    cx.simulate_keystrokes(native_window, "enter");
    cx.run_until_parked();
    assert_eq!(tabs(cx), before + 1);
}

/// Key-to-commit latency of a key bound to an extension command that moves
/// the caret, and how often text typed right after such a key lands before
/// the command's commit.
#[gpui::test]
#[ignore = "measurement: run in release with --ignored --nocapture"]
fn measure_extension_key_latency(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;
    use std::time::{Duration, Instant};
    const KEYS: usize = 1000;
    const RACES: usize = 200;
    const SOURCE: &str = r#"
        import { commands } from "knot";
        let next = 0;
        await commands.register("probe.move", async ({ editor }) => {
          next = (next + 1) % 10;
          await editor.setSelection({ anchor: next, caret: next });
        });
        await commands.register("probe.home", async ({ editor }) => {
          await editor.setSelection({ anchor: 0, caret: 0 });
        });
        await commands.register("probe.noop", () => {});
    "#;

    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source(
                ExtensionKey::new(ExtensionId::new(730), ExtensionLifecycleId::new(1)),
                SOURCE,
            )
            .unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    wait_for_api_condition(cx, |cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog()
            .borrow()
            .resolve("probe.noop")
            .is_ok()
    });
    cx.update(|cx| {
        let registry = &mut cx.global_mut::<ApplicationKeymaps>().0;
        let owner = BindingOwner::Extension(ExtensionId::new(730), ExtensionLifecycleId::new(1));
        for (key, name) in [("f9", "probe.move"), ("f8", "probe.home")] {
            registry
                .set(
                    owner,
                    key,
                    None,
                    Some(Command {
                        name: name.into(),
                        arguments: CommandArgumentValue::Null,
                    }),
                )
                .unwrap();
        }
        super::super::super::commands::keymaps::rebuild(cx);
    });
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("0123456789")));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Probe", model.clone(), cx)
    });
    let (window, native_window) = product_window(document, model.clone(), cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    cx.refresh().unwrap();
    let editor = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone()
    });
    let wait = |cx: &mut TestAppContext, mut done: Box<dyn FnMut(&gpui::App) -> bool>| {
        crate::test_support::wait_until(cx, |cx| cx.read(|cx| done(cx)));
    };
    let report = |name: &str, mut samples: Vec<Duration>| {
        samples.sort_unstable();
        let at = |fraction: f64| {
            samples[((samples.len() as f64 * fraction) as usize).min(samples.len() - 1)]
        };
        println!(
            "{name}, {} samples: p50 {:?}, p90 {:?}, p99 {:?}, max {:?}",
            samples.len(),
            at(0.5),
            at(0.9),
            at(0.99),
            samples[samples.len() - 1]
        );
    };

    // Warm the isolate and the dispatch path.
    for _ in 0..50 {
        let execution = dispatch_product_command(&window, native_window, "probe.noop", cx);
        wait_for_command(execution, cx);
    }
    let mut round_trips = Vec::with_capacity(KEYS);
    for _ in 0..KEYS {
        let start = Instant::now();
        let execution = dispatch_product_command(&window, native_window, "probe.noop", cx);
        wait_for_command(execution, cx);
        round_trips.push(start.elapsed());
    }
    report("dispatch to completion, no-op handler", round_trips);

    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    cx.refresh().unwrap();
    // What simulating and settling one keystroke costs with no binding.
    let mut unbound = Vec::with_capacity(KEYS);
    for _ in 0..KEYS {
        let start = Instant::now();
        cx.simulate_keystrokes(native_window, "f7");
        cx.run_until_parked();
        unbound.push(start.elapsed());
    }
    report("unbound key, test harness baseline", unbound);
    let mut latencies = Vec::with_capacity(KEYS);
    for index in 0..KEYS + 50 {
        let expected = (index + 1) % 10;
        let start = Instant::now();
        cx.simulate_keystrokes(native_window, "f9");
        let editor = editor.clone();
        wait(
            cx,
            Box::new(move |cx| editor.read(cx).editor().read(cx).view_state(cx).caret == expected),
        );
        if index >= 50 {
            latencies.push(start.elapsed());
        }
    }
    report("key to committed selection", latencies);

    // Text typed right after the key: ordered delivery would insert it at
    // the caret the command placed.
    let mut text_first = 0;
    for _ in 0..RACES {
        model.update(cx, |model, _| {
            let length = model.text().len();
            model.replace(0..length, "abc").unwrap();
        });
        editor.update(cx, |editor, cx| {
            editor.editor().clone().update(cx, |editor, cx| {
                editor.restore_view_state(
                    crate::app::editor::EditorViewState {
                        anchor: 3,
                        caret: 3,
                        top: 0,
                        scroll_x: 0.,
                        folds: Vec::new(),
                    },
                    cx,
                )
            })
        });
        cx.simulate_keystrokes(native_window, "f8");
        cx.simulate_input(native_window, "x");
        let (model_ref, editor_ref) = (model.clone(), editor.clone());
        wait(
            cx,
            Box::new(move |cx| {
                let text = model_ref.read(cx).text();
                let caret = editor_ref.read(cx).editor().read(cx).view_state(cx).caret;
                (text == "abcx" && caret == 0) || (text == "xabc" && caret == 1)
            }),
        );
        if cx.read(|cx| model.read(cx).text() == "abcx") {
            text_first += 1;
        }
    }
    println!("text typed after the key landed before its commit in {text_first} of {RACES} trials");
}

/// A key resolves against the context an extension set on the focused editor
/// as soon as the command that set it commits, without waiting for a frame;
/// keys the context leaves unbound still type, and teardown clears it.
#[gpui::test]
fn editor_key_contexts_resolve_live_and_end_with_their_lifecycle(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;
    const SOURCE: &str = r#"
        import { commands, keymaps } from "knot";
        await commands.register("modal.enter", ({ editor }) => editor.setKeyContext("modal", true));
        await commands.register("modal.leave", ({ editor }) => editor.setKeyContext("modal", false));
        await commands.register("modal.left", async ({ editor }) => {
          const { caret } = await editor.selection();
          await editor.setSelection({ anchor: caret - 1, caret: caret - 1 });
        });
        keymaps.bind("f8", "modal.enter");
        keymaps.bind("h", "modal.left", { view: "editor", context: "modal" });
        keymaps.bind("escape", "modal.leave", { view: "editor", context: "modal" });
    "#;
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let key = ExtensionKey::new(ExtensionId::new(731), ExtensionLifecycleId::new(1));
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    wait_for_api_condition(cx, |cx| {
        keymap_command(cx, "f8", Some("editor")).as_deref() == Some("modal.enter")
    });
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("abc")));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Modal", model.clone(), cx)
    });
    let window = headless_window(document, model.clone(), cx);
    let native_window = Headless;
    native_window.run(cx, |cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    });
    let editor = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone()
    });
    editor.update(cx, |editor, cx| {
        editor.editor().clone().update(cx, |editor, cx| {
            editor.set_selection(3, 3, crate::app::editor::Reveal::Nearest, cx)
        })
    });
    let in_modal = |cx: &gpui::App| editor.read(cx).editor().read(cx).key_contexts() == ["modal"];

    press_keys(&window, "f8", cx);
    wait_for_api_condition(cx, in_modal);
    // No refresh: the context is not in any painted frame yet.
    press_keys(&window, "h", cx);
    wait_for_api_condition(cx, |cx| {
        editor.read(cx).editor().read(cx).selection(cx) == (2, 2)
    });
    press_keys(&window, "x", cx);
    wait_for_api_condition(cx, |cx| model.read(cx).text() == "abxc");

    press_keys(&window, "escape", cx);
    wait_for_api_condition(cx, |cx| {
        editor.read(cx).editor().read(cx).key_contexts().is_empty()
    });
    press_keys(&window, "h", cx);
    wait_for_api_condition(cx, |cx| model.read(cx).text() == "abxhc");

    press_keys(&window, "f8", cx);
    wait_for_api_condition(cx, in_modal);
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    cx.read(|cx| assert!(editor.read(cx).editor().read(cx).key_contexts().is_empty()));
    press_keys(&window, "h", cx);
    wait_for_api_condition(cx, |cx| model.read(cx).text() == "abxhhc");
}

/// One window over "abc" with the editor focused and the caret at the end.
fn focused_editor_window(
    documents: &Entity<DocumentCollection>,
    cx: &mut TestAppContext,
) -> (Entity<KnotWindow>, Entity<Buffer>) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("abc")));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Keys", model.clone(), cx)
    });
    let window = headless_window(document, model.clone(), cx);
    Headless.run(cx, |cx| {
        let editor = window
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone();
        editor.update(cx, |editor, cx| {
            editor.editor().clone().update(cx, |editor, cx| {
                editor.set_selection(3, 3, crate::app::editor::Reveal::Nearest, cx)
            })
        });
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    });
    (window, model)
}

/// A sequence another package binds in a context that is off holds no key:
/// a modal package's text binding receives both keys, and nothing is left
/// pending. Once that context is on, the sequence runs its command.
#[gpui::test]
fn a_sequence_bound_in_another_context_leaves_keys_to_a_modal_package(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;
    const MODAL: &str = r#"
        import { commands, keymaps, workbench } from "knot";
        const typed = [];
        await commands.register("a.enter", ({ editor }) => editor.setKeyContext("a.mode", true));
        await commands.register("a.type", ({ arguments: { text } }) => { typed.push(text); });
        await commands.register("a.report", () => workbench.writeClipboardText(typed.join("")));
        keymaps.bind("f8", "a.enter");
        keymaps.bind("f9", "a.report");
        keymaps.bindText("a.type", { view: "editor", context: "a.mode" });
    "#;
    const CHORDS: &str = r#"
        import { commands, keymaps, workbench } from "knot";
        await commands.register("b.enter", ({ editor }) => editor.setKeyContext("b.mode", true));
        await commands.register("b.bottom", () => workbench.writeClipboardText("bottom"));
        await commands.register("b.top", () => workbench.writeClipboardText("top"));
        keymaps.bind("f7", "b.enter");
        keymaps.bind("z b", "b.bottom", { view: "editor", context: "b.mode" });
        keymaps.bind("z t", "b.top", { view: "editor", context: "b.mode" });
    "#;
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(super::super::super::extensions::host::install);
    for (id, source) in [(741, MODAL), (742, CHORDS)] {
        let key = ExtensionKey::new(ExtensionId::new(id), ExtensionLifecycleId::new(1));
        host.update(cx, |host, cx| {
            let execution = host.load_test_source(key, source).unwrap();
            cx.spawn(async move |_, _| execution.await.unwrap())
                .detach();
        });
    }
    wait_for_api_condition(cx, |cx| {
        keymap_command(cx, "f7", None).as_deref() == Some("b.enter")
            && keymap_command(cx, "f9", None).as_deref() == Some("a.report")
    });
    let (window, model) = focused_editor_window(&documents, cx);
    let clipboard = |cx: &gpui::App| cx.read_from_clipboard().and_then(|item| item.text());

    press_keys(&window, "f8", cx);
    wait_for_api_condition(cx, |cx| !window.read(cx).holds_keys());
    press_keys(&window, "z b", cx);
    press_keys(&window, "f9", cx);
    wait_for_api_condition(cx, |cx| clipboard(cx).as_deref() == Some("zb"));
    cx.read(|cx| {
        assert!(!window.read(cx).has_pending_keys());
        assert_eq!(model.read(cx).text(), "abc");
    });

    press_keys(&window, "f7", cx);
    wait_for_api_condition(cx, |cx| !window.read(cx).holds_keys());
    press_keys(&window, "z t", cx);
    wait_for_api_condition(cx, |cx| clipboard(cx).as_deref() == Some("top"));
    press_keys(&window, "f9", cx);
    wait_for_api_condition(cx, |cx| clipboard(cx).as_deref() == Some("zb"));
}

/// The keys of a sequence that stops matching are delivered in order, as
/// if unbound, and a prefix left alone is delivered after a second.
#[gpui::test]
fn an_unfinished_sequence_delivers_its_keys(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let (window, model) = focused_editor_window(&documents, cx);

    press_keys(&window, "secondary-k", cx);
    cx.read(|cx| assert!(window.read(cx).has_pending_keys()));
    press_keys(&window, "x y", cx);
    cx.run_until_parked();
    cx.read(|cx| {
        assert!(!window.read(cx).has_pending_keys());
        assert_eq!(model.read(cx).text(), "abcxy");
    });

    press_keys(&window, "secondary-k", cx);
    cx.executor()
        .advance_clock(crate::app::commands::key_router::SEQUENCE_TIMEOUT);
    cx.run_until_parked();
    cx.read(|cx| assert!(!window.read(cx).has_pending_keys()));
    press_keys(&window, "z", cx);
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(model.read(cx).text(), "abcxyz"));
}

#[gpui::test]
fn about_shows_the_compiled_build_identity(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let execution = dispatch_product_command(
        &window,
        native_window,
        super::super::commands::ABOUT_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.run_until_parked();
    cx.read(|cx| {
        let picker = match window.read(cx).transient.as_ref().unwrap().surface.as_ref() {
            Some(crate::app::transient::TransientSurface::Picker(picker)) => picker.read(cx),
            _ => panic!("the transient is not a picker"),
        };
        let descriptions = picker
            .visible_entries()
            .into_iter()
            .map(|entry| picker.entry(entry).description.clone().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            descriptions,
            [
                crate::app::build_info::VERSION,
                crate::app::build_info::COMMIT
            ]
        );
    });
}
