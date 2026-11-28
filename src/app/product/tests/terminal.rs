//! Terminal tabs, sessions, closure, and cross-window transfer.
use super::*;
use crate::app::documents::DocumentDirty;
use crate::app::product::relocate::TabRelocationSource;

fn capture_active_tab(window: &Entity<KnotWindow>, cx: &TestAppContext) -> TabRelocationSource {
    cx.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        window
            .capture_tab_relocation(pane.id(), pane.active_tab().id(), cx)
            .unwrap()
    })
}

fn relocate_to_new_window(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    source: &TabRelocationSource,
    cx: &mut TestAppContext,
    create_destination: impl FnOnce(&mut gpui::App) -> anyhow::Result<gpui::WindowHandle<KnotWindow>>,
) -> CommandOutcome {
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            window.relocate_tab_to_new_window_with(source, cx, create_destination)
        })
    })
    .unwrap()
}

#[cfg(unix)]
#[gpui::test]
async fn terminal_mouse_reporting_toggle_routes_to_the_focused_session_and_plain_drag_copies(
    cx: &mut TestAppContext,
) {
    use crate::app::{
        terminal::session::TerminalSession,
        terminal::view::TOGGLE_TERMINAL_MOUSE_REPORTING_COMMAND, terminal::view::TerminalView,
    };
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, window_handle) = product_window(document, model, cx);
    assert_eq!(
        dispatch_product_command(
            &window,
            window_handle,
            TOGGLE_TERMINAL_MOUSE_REPORTING_COMMAND,
            cx
        )
        .completion
        .await
        .unwrap(),
        CommandOutcome::Unavailable
    );
    let session = cx.new(|cx| {
        TerminalSession::new_with_options(
            knot_terminal::PtyOptions {
                shell: Some(knot_terminal::Shell::new(
                    "/bin/sh",
                    vec!["-c".into(), "read line".into()],
                )),
                ..Default::default()
            },
            cx,
        )
    });
    let view = cx
        .update_window(window_handle, |_, _, cx| {
            let id = cx
                .global::<ApplicationTerminalSessions>()
                .0
                .borrow_mut()
                .allocate_id();
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow_mut()
                .sessions
                .insert(id, session.clone());
            let view = cx.new(|cx| TerminalView::new(session.clone(), cx));
            window.update(cx, |window, cx| {
                let pane = window.workbench.read(cx).focused_pane_id().unwrap();
                let tab = window.workbench.update(cx, |workbench, _| {
                    workbench.open_terminal_tab(pane, id, view.clone()).unwrap()
                });
                window.activate_tab(pane, tab, cx);
            });
            view
        })
        .unwrap();
    cx.refresh().unwrap();
    assert_eq!(
        dispatch_product_command(
            &window,
            window_handle,
            TOGGLE_TERMINAL_MOUSE_REPORTING_COMMAND,
            cx
        )
        .completion
        .await
        .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| assert!(!session.read(cx).mouse_reporting_enabled()));
    // With reporting off, a plain drag selects even though the program asks
    // for drag reports.
    let (start, end) = cx
        .update_window(window_handle, |_, native_window, cx| {
            session.read(cx).with_terminal(|terminal| {
                terminal.write(b"\x1b[?1002h\x1b[?1006h\x1b[2J\x1b[3J\x1b[Hcopy me")
            });
            let bounds = view.read(cx).interaction_bounds();
            let width =
                px(session.read(cx).size().cell_width as f32 / native_window.scale_factor());
            (
                bounds.origin + point(px(0.1), px(0.1)),
                bounds.origin + point(width * 3.8, px(0.1)),
            )
        })
        .unwrap();
    let mut window_cx = VisualTestContext::from_window(window_handle, cx);
    window_cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    window_cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    window_cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    cx.simulate_keystrokes(
        window_handle,
        if cfg!(target_os = "macos") {
            "cmd-c"
        } else {
            "ctrl-shift-c"
        },
    );
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("copy")
        );
    });
    assert_eq!(
        dispatch_product_command(
            &window,
            window_handle,
            TOGGLE_TERMINAL_MOUSE_REPORTING_COMMAND,
            cx
        )
        .completion
        .await
        .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| assert!(session.read(cx).mouse_reporting_enabled()));
    session.update(cx, |session, cx| session.close(cx));
}

#[gpui::test]
async fn terminal_tab_has_a_captured_surface_and_document_commands_are_unavailable(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let (pane, tab) = window.update(cx, |window, cx| {
        let (session, view) = window.create_terminal(cx);
        let pane = window.workbench.read(cx).focused_pane_id().unwrap();
        let tab = window.workbench.update(cx, |workbench, _| {
            workbench.open_terminal_tab(pane, session, view).unwrap()
        });
        (pane, tab)
    });
    let session = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .pane(pane)
            .unwrap()
            .active_tab()
            .surface_id()
    });
    let target = cx
        .update_window(native_window, |_, _, cx| {
            window.update(cx, |window, cx| {
                window.activate_tab(pane, tab, cx);
                window.capture_command_target(cx)
            })
        })
        .unwrap();
    assert_eq!(target.tab.unwrap().surface, session);
    cx.read(|cx| {
        assert_eq!(window.read(cx).workbench.read(cx).view_count(document), 1);
        assert_eq!(
            cx.global::<ApplicationWindows>().view_count(document, cx),
            1
        );
    });
    let (completion, receiver) = super::super::CommandCompletion::new();
    window.update(cx, |window, cx| {
        window.start_save_command(false, target.clone(), completion, cx)
    });
    assert_eq!(receiver.await.unwrap(), CommandOutcome::Unavailable);
    let dispatcher = cx.read(|cx| cx.global::<ApplicationProductCommands>().0.clone());
    let result = dispatcher.update(cx, |dispatcher, cx| {
        dispatcher.dispatch(
            Command {
                name: "editor.move-left".into(),
                arguments: CommandArgumentValue::Null,
            },
            target.clone(),
            cx,
        )
    });
    assert_eq!(
        result.completion.await.unwrap(),
        CommandOutcome::Unavailable
    );
    let mut wrong_surface = target;
    wrong_surface.tab.as_mut().unwrap().surface = TabSurfaceId::Document(document);
    let result = dispatcher.update(cx, |dispatcher, cx| {
        dispatcher.dispatch(
            Command {
                name: NEW_COMMAND.into(),
                arguments: CommandArgumentValue::Null,
            },
            wrong_surface,
            cx,
        )
    });
    assert_eq!(
        result.completion.await.unwrap(),
        CommandOutcome::InvalidTarget
    );
}

#[gpui::test]
async fn closing_terminal_tab_preserves_dirty_document_and_invalidates_its_target(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    model.update(cx, |model, _| model.replace(0..0, "unsaved").unwrap());
    let (window, native_window) = product_window(document, model, cx);
    let (pane, tab) = window.update(cx, |window, cx| {
        let (session, view) = window.create_terminal(cx);
        let pane = window.workbench.read(cx).focused_pane_id().unwrap();
        let tab = window.workbench.update(cx, |workbench, _| {
            workbench.open_terminal_tab(pane, session, view).unwrap()
        });
        (pane, tab)
    });
    let stale = cx
        .update_window(native_window, |_, _, cx| {
            window.update(cx, |window, cx| {
                window.activate_tab(pane, tab, cx);
                window.capture_command_target(cx)
            })
        })
        .unwrap();
    let execution = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    assert_eq!(
        execution.completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert!(!cx.has_pending_prompt());
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.clone();
        assert_eq!(workbench.read(cx).tab_snapshots().len(), 1);
        assert_eq!(workbench.read(cx).view_count(document), 1);
        assert!(documents.read(cx).get(document).unwrap().is_dirty(cx));
    });
    let dispatcher = cx.read(|cx| cx.global::<ApplicationProductCommands>().0.clone());
    let result = dispatcher.update(cx, |dispatcher, cx| {
        dispatcher.dispatch(
            Command {
                name: NEW_COMMAND.into(),
                arguments: CommandArgumentValue::Null,
            },
            stale,
            cx,
        )
    });
    assert_eq!(
        result.completion.await.unwrap(),
        CommandOutcome::InvalidTarget
    );
}

#[gpui::test]
async fn mixed_window_and_quit_cancellation_preserve_every_surface(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    install_failing_recovery(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    model.update(cx, |model, _| model.replace(0..0, "unsaved").unwrap());
    let (window, native_window) = product_window(document, model, cx);
    let (pane, tab) = window.update(cx, |window, cx| {
        let (session, view) = window.create_terminal(cx);
        let pane = window.workbench.read(cx).focused_pane_id().unwrap();
        let tab = window.workbench.update(cx, |workbench, _| {
            workbench.open_terminal_tab(pane, session, view).unwrap()
        });
        (pane, tab)
    });
    let session = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .pane(pane)
            .unwrap()
            .active_tab()
            .surface_id()
    });
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.activate_tab(pane, tab, cx));
    })
    .unwrap();
    for command in [CLOSE_WINDOW_COMMAND, QUIT_COMMAND] {
        let execution = dispatch_product_command(&window, native_window, command, cx);
        cx.run_until_parked();
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("Cancel");
        assert_eq!(
            execution.completion.await.unwrap(),
            CommandOutcome::Cancelled
        );
        cx.read(|cx| {
            assert!(cx.windows().contains(&native_window));
            let snapshots = window.read(cx).workbench.read(cx).tab_snapshots();
            assert_eq!(snapshots.len(), 2);
            assert!(
                snapshots
                    .iter()
                    .any(|tab| tab.surface == TabSurfaceId::Document(document))
            );
            assert!(snapshots.iter().any(|tab| tab.surface == session));
            let TabSurfaceId::Terminal(id) = session else {
                panic!()
            };
            let registry = cx.global::<ApplicationTerminalSessions>().0.borrow();
            assert_ne!(
                registry.sessions[&id].read(cx).status(),
                crate::app::terminal::session::TerminalStatus::Closed
            );
            assert!(documents.read(cx).get(document).unwrap().is_dirty(cx));
        });
    }
}

#[gpui::test]
async fn terminal_command_focuses_a_live_view_and_split_creates_an_independent_session(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);

    let command =
        dispatch_product_command(&window, native_window, NEW_TERMINAL_IN_CENTER_COMMAND, cx);
    assert_eq!(command.completion.await.unwrap(), CommandOutcome::Completed);
    let (pane, first, document_tab) = cx.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        (
            pane.id(),
            pane.active_tab().surface_id(),
            pane.tabs()[0].id(),
        )
    });
    let TabSurfaceId::Terminal(first_id) = first else {
        panic!("terminal command opened a document")
    };
    cx.update_window(native_window, |_, native_window, cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        let pane = workbench.pane(pane).unwrap();
        assert!(
            workbench
                .terminal_view(pane.active_tab())
                .unwrap()
                .focus_handle(cx)
                .contains_focused(native_window, cx)
        );
    })
    .unwrap();
    cx.read(|cx| {
        assert!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .contains_key(&first_id)
        );
    });

    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.activate_tab(pane, document_tab, cx));
    })
    .unwrap();
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            let terminal_tab = window.workbench.read(cx).pane(pane).unwrap().tabs()[1].id();
            window.activate_tab(pane, terminal_tab, cx);
        });
    })
    .unwrap();
    cx.read(|cx| {
        assert_eq!(
            window
                .read(cx)
                .workbench
                .read(cx)
                .pane(pane)
                .unwrap()
                .active_tab()
                .surface_id(),
            first
        );
        assert!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .contains_key(&first_id)
        );
    });

    let split = dispatch_product_command(&window, native_window, SPLIT_HORIZONTAL_COMMAND, cx);
    assert_eq!(split.completion.await.unwrap(), CommandOutcome::Completed);
    let second_id = cx.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        assert_eq!(workbench.panes().len(), 2);
        match workbench.focused_pane().unwrap().active_tab().surface_id() {
            TabSurfaceId::Terminal(id) => id,
            TabSurfaceId::Document(_) | TabSurfaceId::Composite(_) | TabSurfaceId::Tool(_) => {
                panic!("split did not create terminal")
            }
        }
    });
    assert_ne!(first_id, second_id);
    cx.read(|cx| {
        let registry = cx.global::<ApplicationTerminalSessions>().0.borrow();
        assert_eq!(registry.sessions.len(), 2);
        assert!(registry.sessions.contains_key(&first_id));
        assert!(registry.sessions.contains_key(&second_id));
    });
}

#[gpui::test]
async fn terminal_close_and_reopen_settle_the_exact_session(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    assert_eq!(
        dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let (first_id, first_session) = cx.read(|cx| {
        let TabSurfaceId::Terminal(id) = window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .surface_id()
        else {
            panic!()
        };
        (
            id,
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions[&id]
                .clone(),
        )
    });
    assert_eq!(
        dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert_eq!(
            first_session.read(cx).status(),
            crate::app::terminal::session::TerminalStatus::Closed
        );
        assert!(
            !cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .contains_key(&first_id)
        );
    });
    assert_eq!(
        dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        let TabSurfaceId::Terminal(second_id) = window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .surface_id()
        else {
            panic!()
        };
        assert_ne!(first_id, second_id);
        assert!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .contains_key(&second_id)
        );
    });
}

#[gpui::test]
async fn closing_the_final_terminal_tab_leaves_an_empty_center(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    assert_eq!(
        dispatch_product_command(&window, native_window, NEW_TERMINAL_IN_CENTER_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let (pane, document_tab, session) = cx.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        let TabSurfaceId::Terminal(id) = pane.active_tab().surface_id() else {
            panic!()
        };
        (
            pane.id(),
            pane.tabs()[0].id(),
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions[&id]
                .clone(),
        )
    });
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.activate_tab(pane, document_tab, cx));
    })
    .unwrap();
    assert_eq!(
        dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        let window = window.read(cx);
        let tabs = window.workbench.read(cx).tab_snapshots();
        assert_eq!(tabs.len(), 1);
        assert!(matches!(tabs[0].surface, TabSurfaceId::Terminal(_)));
        assert!(documents.read(cx).get(document).is_none());
    });
    assert_eq!(
        dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        let window = window.read(cx);
        assert!(window.workbench.read(cx).tab_snapshots().is_empty());
        assert_eq!(
            session.read(cx).status(),
            crate::app::terminal::session::TerminalStatus::Closed
        );
        assert!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .is_empty()
        );
    });
}

#[gpui::test]
async fn window_close_shuts_down_terminal_after_protected_decisions(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    assert_eq!(
        dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let session = cx.read(|cx| {
        let TabSurfaceId::Terminal(id) = window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .surface_id()
        else {
            panic!()
        };
        cx.global::<ApplicationTerminalSessions>()
            .0
            .borrow()
            .sessions[&id]
            .clone()
    });
    assert_eq!(
        dispatch_product_command(&window, native_window, CLOSE_WINDOW_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert!(!cx.windows().contains(&native_window));
        assert_eq!(
            session.read(cx).status(),
            crate::app::terminal::session::TerminalStatus::Closed
        );
        assert!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .is_empty()
        );
    });
}

#[gpui::test]
async fn mixed_window_save_failure_keeps_terminal_running(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    cx.set_global(ApplicationSaveDialog(Arc::new(FixedSaveDialog(
        SaveDialogOutcome::Selected(ResourceUri::parse("mem://product/src").unwrap()),
    ))));
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    model.update(cx, |model, _| model.replace(0..0, "unsaved").unwrap());
    let (window, native_window) = product_window(document, model, cx);
    assert_eq!(
        dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let (id, session) = cx.read(|cx| {
        let TabSurfaceId::Terminal(id) = window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .surface_id()
        else {
            panic!()
        };
        (
            id,
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions[&id]
                .clone(),
        )
    });
    let execution = dispatch_product_command(&window, native_window, CLOSE_WINDOW_COMMAND, cx);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Save");
    assert!(matches!(
        execution.completion.await.unwrap(),
        CommandOutcome::HandlerFailure { .. }
    ));
    cx.read(|cx| {
        assert!(cx.windows().contains(&native_window));
        assert_eq!(window.read(cx).workbench.read(cx).tab_snapshots().len(), 2);
        assert!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .contains_key(&id)
        );
        assert_ne!(
            session.read(cx).status(),
            crate::app::terminal::session::TerminalStatus::Closed
        );
        assert!(documents.read(cx).get(document).unwrap().is_dirty(cx));
    });
}

#[gpui::test]
async fn quit_shuts_down_terminal_sessions_in_every_window(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let first = cx.update(|cx| create_untitled_document(&documents, cx));
    let first_model = cx.read(|cx| documents.read(cx).get(first).unwrap().model().clone());
    let (first_window, first_native_window) = product_window(first, first_model, cx);
    let second = cx.update(|cx| create_untitled_document(&documents, cx));
    let second_model = cx.read(|cx| documents.read(cx).get(second).unwrap().model().clone());
    let (second_window, second_native_window) = product_window(second, second_model, cx);
    for (window, native_window) in [
        (&first_window, first_native_window),
        (&second_window, second_native_window),
    ] {
        assert_eq!(
            dispatch_product_command(window, native_window, NEW_TERMINAL_COMMAND, cx)
                .completion
                .await
                .unwrap(),
            CommandOutcome::Completed
        );
    }
    let sessions = cx.read(|cx| {
        cx.global::<ApplicationTerminalSessions>()
            .0
            .borrow()
            .sessions
            .values()
            .cloned()
            .collect::<Vec<_>>()
    });
    assert_eq!(sessions.len(), 2);
    assert_eq!(
        dispatch_product_command(&first_window, first_native_window, QUIT_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .is_empty()
        );
        for session in &sessions {
            assert_eq!(
                session.read(cx).status(),
                crate::app::terminal::session::TerminalStatus::Closed
            );
        }
    });
}

#[gpui::test]
async fn moving_terminal_keeps_its_session_and_source_window_close_does_not_stop_it(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (source, source_window) = product_window(document, model, cx);
    assert_eq!(
        dispatch_product_command(&source, source_window, NEW_TERMINAL_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let (id, session, grid, source_view) = cx.read(|cx| {
        let window = source.read(cx);
        let workbench = window.workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        let TabSurfaceId::Terminal(id) = tab.surface_id() else {
            panic!()
        };
        let session = cx
            .global::<ApplicationTerminalSessions>()
            .0
            .borrow()
            .sessions[&id]
            .clone();
        let grid = session.read(cx).terminal().unwrap().clone();
        (
            id,
            session,
            grid,
            workbench.terminal_view(tab).unwrap().clone(),
        )
    });
    let relocation = capture_active_tab(&source, cx);
    assert_eq!(
        relocate_to_new_window(
            &source,
            source_window,
            &relocation,
            cx,
            open_tab_transfer_window
        ),
        CommandOutcome::Completed
    );
    let destination_window = cx.read(|cx| {
        *cx.windows()
            .iter()
            .find(|handle| **handle != source_window)
            .unwrap()
    });
    let destination = cx
        .update_window(destination_window, |_, native_window, _| {
            native_window.root::<KnotWindow>().flatten().unwrap()
        })
        .unwrap();
    let destination_view = cx.read(|cx| {
        destination
            .read(cx)
            .workbench
            .read(cx)
            .focused_terminal_view()
            .unwrap()
            .clone()
    });
    cx.update_window(destination_window, |_, native_window, cx| {
        assert!(
            destination_view
                .focus_handle(cx)
                .contains_focused(native_window, cx)
        );
    })
    .unwrap();
    let destination_size = knot_terminal::TerminalSize {
        columns: 80,
        rows: 20,
        cell_width: 9,
        cell_height: 18,
    };
    destination_view.update(cx, |view, cx| view.resize(destination_size, cx));
    cx.read(|cx| assert_eq!(session.read(cx).size(), destination_size));
    let stale_size = knot_terminal::TerminalSize {
        columns: 40,
        rows: 10,
        cell_width: 9,
        cell_height: 18,
    };
    source_view.update(cx, |view, cx| view.resize(stale_size, cx));
    cx.read(|cx| assert_eq!(session.read(cx).size(), destination_size));
    cx.read(|cx| {
        assert!(
            !source
                .read(cx)
                .workbench
                .read(cx)
                .tab_snapshots()
                .iter()
                .any(|tab| tab.surface == TabSurfaceId::Terminal(id))
        );
        let destination_window = destination.read(cx);
        let workbench = destination_window.workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        assert_eq!(tab.surface_id(), TabSurfaceId::Terminal(id));
        assert_ne!(workbench.terminal_view(tab), Some(&source_view));
        assert!(std::sync::Arc::ptr_eq(
            session.read(cx).terminal().unwrap(),
            &grid
        ));
        assert_eq!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions[&id],
            session
        );
    });
    assert_eq!(
        dispatch_product_command(&source, source_window, CLOSE_WINDOW_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert_eq!(
            session.read(cx).status(),
            crate::app::terminal::session::TerminalStatus::Running
        );
        assert!(cx.windows().contains(&destination_window));
    });
    assert_eq!(
        dispatch_product_command(&destination, destination_window, CLOSE_TAB_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    cx.read(|cx| {
        assert_eq!(
            session.read(cx).status(),
            crate::app::terminal::session::TerminalStatus::Closed
        );
        assert!(
            !cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .contains_key(&id)
        );
    });
}

#[gpui::test]
async fn failed_terminal_destination_leaves_source_attached(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    assert_eq!(
        dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let relocation = capture_active_tab(&window, cx);
    let outcome = relocate_to_new_window(&window, native_window, &relocation, cx, |_| {
        anyhow::bail!("simulated window creation failure")
    });
    assert!(matches!(outcome, CommandOutcome::HandlerFailure { .. }));
    cx.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        let TabSurfaceId::Terminal(id) = tab.surface_id() else {
            panic!()
        };
        assert!(workbench.terminal_view(tab).is_some());
        assert!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .contains_key(&id)
        );
        assert_eq!(cx.windows().len(), 1);
    });
}

#[gpui::test]
async fn vanished_terminal_destination_leaves_source_attached(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    assert_eq!(
        dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let relocation = capture_active_tab(&window, cx);
    let outcome = relocate_to_new_window(&window, native_window, &relocation, cx, |cx| {
        let destination = open_tab_transfer_window(cx)?;
        cx.update_window(destination.into(), |_, native_window, _| {
            native_window.remove_window()
        })?;
        Ok(destination)
    });
    assert_eq!(outcome, CommandOutcome::InvalidTarget);
    cx.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        let tab = workbench.focused_pane().unwrap().active_tab();
        let TabSurfaceId::Terminal(id) = tab.surface_id() else {
            panic!()
        };
        assert!(workbench.terminal_view(tab).is_some());
        assert!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .contains_key(&id)
        );
        assert_eq!(cx.windows().len(), 1);
    });
}

#[gpui::test]
async fn stale_terminal_relocation_is_rejected(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    assert_eq!(
        dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let relocation = capture_active_tab(&window, cx);
    assert_eq!(
        dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(
        relocate_to_new_window(
            &window,
            native_window,
            &relocation,
            cx,
            open_tab_transfer_window
        ),
        CommandOutcome::InvalidTarget
    );
    cx.read(|cx| assert_eq!(cx.windows().len(), 1));
}

#[gpui::test]
async fn process_exit_during_terminal_handoff_keeps_final_grid(cx: &mut TestAppContext) {
    use knot_terminal::{PtyOptions, Shell};
    use std::{
        thread,
        time::{Duration, Instant},
    };

    #[cfg(windows)]
    let shell = Shell::new(
        "cmd.exe",
        vec!["/C".into(), "echo handoff-output & exit /b 7".into()],
    );
    #[cfg(not(windows))]
    let shell = Shell::new(
        "/bin/sh",
        vec![
            "-c".into(),
            "printf 'handoff-output\\n'; sleep 0.05; exit 7".into(),
        ],
    );

    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (source, source_window) = product_window(document, model, cx);
    let session = cx.update(|cx| {
        cx.new(|cx| {
            crate::app::terminal::session::TerminalSession::new_with_options(
                PtyOptions {
                    shell: Some(shell),
                    ..Default::default()
                },
                cx,
            )
        })
    });
    let id = source.update(cx, |window, cx| {
        let id = cx
            .global::<ApplicationTerminalSessions>()
            .0
            .borrow_mut()
            .allocate_id();
        cx.global::<ApplicationTerminalSessions>()
            .0
            .borrow_mut()
            .sessions
            .insert(id, session.clone());
        let view = cx.new(|cx| crate::app::terminal::view::TerminalView::new(session.clone(), cx));
        let pane = window.workbench.read(cx).focused_pane_id().unwrap();
        window.workbench.update(cx, |workbench, _| {
            workbench.open_terminal_tab(pane, id, view).unwrap()
        });
        id
    });
    cx.update_window(source_window, |_, _, cx| {
        source.update(cx, |window, cx| window.focus_active_surface(cx))
    })
    .unwrap();
    let relocation = capture_active_tab(&source, cx);
    let outcome = relocate_to_new_window(&source, source_window, &relocation, cx, |cx| {
        thread::sleep(Duration::from_millis(100));
        open_tab_transfer_window(cx)
    });
    assert_eq!(outcome, CommandOutcome::Completed);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if cx.read(|cx| {
            matches!(
                session.read(cx).status(),
                crate::app::terminal::session::TerminalStatus::Exited(_)
            )
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "terminal did not exit after handoff"
        );
        thread::sleep(Duration::from_millis(5));
    }
    cx.read(|cx| {
        assert!(matches!(
            session.read(cx).status(),
            crate::app::terminal::session::TerminalStatus::Exited(_)
        ));
        let output = session
            .read(cx)
            .with_terminal(|terminal| terminal.text())
            .unwrap();
        assert!(output.contains("handoff-output"));
        assert!(
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .contains_key(&id)
        );
    });
}

fn focused_terminal_directory(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> Option<std::path::PathBuf> {
    cx.read(|cx| {
        let surface = window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .surface_id();
        let TabSurfaceId::Terminal(id) = surface else {
            panic!("no terminal is focused")
        };
        cx.global::<ApplicationTerminalSessions>()
            .0
            .borrow()
            .sessions[&id]
            .read(cx)
            .working_directory()
            .map(Path::to_owned)
    })
}

fn directory_uri(path: &Path) -> ResourceUri {
    ResourceUri::parse(url::Url::from_directory_path(path).unwrap().as_str()).unwrap()
}

fn focus_document(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    document: crate::app::documents::DocumentId,
    cx: &mut TestAppContext,
) {
    cx.update_window(native_window, |_, native_window, cx| {
        window.update(cx, |window, cx| {
            let tab = window
                .workbench
                .read(cx)
                .tab_snapshots()
                .into_iter()
                .find(|tab| tab.surface == TabSurfaceId::Document(document))
                .unwrap();
            window.activate_tab(tab.pane_id, tab.tab_id, cx);
        });
        native_window.activate_window();
    })
    .unwrap();
    // Focus events are delivered when the window draws.
    cx.refresh().unwrap();
    cx.run_until_parked();
}

#[gpui::test]
async fn new_terminals_from_a_focused_terminal_start_in_its_directory(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp_directory(&temp);
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            window.replace_workspace_roots(vec![directory_uri(&root)], cx);
            let pane = window.workbench.read(cx).focused_pane_id().unwrap();
            window
                .new_terminal_in_pane(
                    Some(pane),
                    &crate::app::product::operations::TerminalLocation::Center,
                    Some(nested.clone()),
                    cx,
                )
                .unwrap();
        });
    })
    .unwrap();

    // New Terminal follows the focused terminal.
    let command = dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx);
    assert_eq!(command.completion.await.unwrap(), CommandOutcome::Completed);
    assert_eq!(
        focused_terminal_directory(&window, cx),
        Some(nested.clone())
    );

    // So does splitting a terminal pane.
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            let pane = window.workbench.read(cx).focused_pane_id().unwrap();
            assert!(window.split_pane(
                pane,
                SplitDirection::Horizontal,
                super::super::SplitPlacement::After,
                cx
            ));
        });
    })
    .unwrap();
    assert_eq!(focused_terminal_directory(&window, cx), Some(nested));
}

#[gpui::test]
async fn terminals_without_a_requested_directory_start_in_the_first_root(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp_directory(&temp);
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            window.replace_workspace_roots(vec![directory_uri(&root)], cx);
            let (session, _) = window.create_terminal(cx);
            assert_eq!(
                KnotWindow::terminal_session_directory(session, cx),
                Some(root.clone())
            );
        });
    })
    .unwrap();
}

/// The area and kind of the focused tab, and how many terminal sessions live.
fn focused_area(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> Option<crate::app::workbench::Area> {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        workbench.focused_pane().map(|pane| pane.area())
    })
}

fn session_count(cx: &TestAppContext) -> usize {
    cx.read(|cx| {
        cx.global::<ApplicationTerminalSessions>()
            .0
            .borrow()
            .sessions
            .len()
    })
}

fn terminal_panes(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> Vec<(crate::app::workbench::Area, usize)> {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        workbench
            .panes()
            .iter()
            .filter_map(|pane| {
                let terminals = pane
                    .tabs()
                    .iter()
                    .filter(|tab| matches!(tab.surface_id(), TabSurfaceId::Terminal(_)))
                    .count();
                (terminals > 0).then_some((pane.area(), terminals))
            })
            .collect()
    })
}

/// The Terminal tool window's area and tool.
fn terminal_area(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> (
    crate::app::workbench::Area,
    crate::app::workbench::ToolFrameId,
) {
    cx.read(|cx| {
        let tool = window
            .read(cx)
            .workbench
            .read(cx)
            .tool_by_stable_id(crate::app::terminal::TERMINAL_TOOL_ID)
            .unwrap();
        (crate::app::workbench::Area::Tool(tool), tool)
    })
}

#[gpui::test]
async fn toggle_terminal_creates_shows_focuses_and_hides_the_terminal_window(
    cx: &mut TestAppContext,
) {
    use crate::app::workbench::Area;
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let (right, terminal_tool) = terminal_area(&window, cx);
    let toggle = |cx: &mut TestAppContext| {
        dispatch_product_command(&window, native_window, TOGGLE_TERMINAL_COMMAND, cx)
    };

    // With no terminal in its window, one opens there and takes focus.
    assert_eq!(
        toggle(cx).completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(terminal_panes(&window, cx), [(right, 1)]);
    assert_eq!(focused_area(&window, cx), Some(right));
    assert_eq!(session_count(cx), 1);

    // Focused: its window hides and focus returns to the document.
    assert_eq!(
        toggle(cx).completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(focused_area(&window, cx), Some(Area::Center));
    cx.read(|cx| {
        assert!(
            !window
                .read(cx)
                .workbench
                .read(cx)
                .tool_visible(terminal_tool)
        );
    });

    // Hidden: it shows again, the same session.
    assert_eq!(
        toggle(cx).completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(focused_area(&window, cx), Some(right));
    assert_eq!(session_count(cx), 1);

    // Visible but unfocused: it takes focus without hiding.
    focus_document(&window, native_window, document, cx);
    assert_eq!(focused_area(&window, cx), Some(Area::Center));
    assert_eq!(
        toggle(cx).completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(focused_area(&window, cx), Some(right));
    cx.read(|cx| {
        assert!(
            window
                .read(cx)
                .workbench
                .read(cx)
                .tool_visible(terminal_tool)
        );
    });

    // Center terminals are ignored: the Terminal window still toggles.
    focus_document(&window, native_window, document, cx);
    let center =
        dispatch_product_command(&window, native_window, NEW_TERMINAL_IN_CENTER_COMMAND, cx);
    assert_eq!(center.completion.await.unwrap(), CommandOutcome::Completed);
    assert_eq!(focused_area(&window, cx), Some(Area::Center));
    assert_eq!(
        toggle(cx).completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(focused_area(&window, cx), Some(right));
    assert_eq!(session_count(cx), 2);
}

#[gpui::test]
async fn new_terminal_follows_a_terminal_then_opens_in_the_terminal_window(
    cx: &mut TestAppContext,
) {
    use crate::app::workbench::Area;
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let (right, terminal_tool) = terminal_area(&window, cx);
    let new = |name: &'static str, cx: &mut TestAppContext| {
        dispatch_product_command(&window, native_window, name, cx)
    };

    // From a document with no terminal in the window: the window, which it shows.
    assert_eq!(
        new(NEW_TERMINAL_COMMAND, cx).completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(terminal_panes(&window, cx), [(right, 1)]);

    // From a document: in the window's last focused pane.
    focus_document(&window, native_window, document, cx);
    assert_eq!(
        new(NEW_TERMINAL_COMMAND, cx).completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(terminal_panes(&window, cx), [(right, 2)]);
    assert_eq!(focused_area(&window, cx), Some(right));

    // New Terminal in Center: the last focused center pane.
    assert_eq!(
        new(NEW_TERMINAL_IN_CENTER_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(terminal_panes(&window, cx), [(Area::Center, 1), (right, 2)]);

    // From a center terminal: still the Terminal window, which New Terminal
    // in Editor is for.
    assert_eq!(
        new(NEW_TERMINAL_COMMAND, cx).completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(terminal_panes(&window, cx), [(Area::Center, 1), (right, 3)]);

    // Splitting a window terminal starts a new session beside it, in the window.
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| {
            let workbench = window.workbench.read(cx);
            let pane = workbench.area_panes(Area::Tool(terminal_tool))[0];
            assert!(window.split_pane(
                pane,
                SplitDirection::Horizontal,
                super::super::SplitPlacement::After,
                cx
            ));
        });
    })
    .unwrap();
    assert_eq!(
        terminal_panes(&window, cx),
        [(Area::Center, 1), (right, 3), (right, 1)]
    );
    assert_eq!(session_count(cx), 5);
    assert_eq!(focused_area(&window, cx), Some(right));
}

#[gpui::test]
async fn a_terminal_moves_between_the_center_and_its_window_and_the_slot_reaches_fresh_windows(
    cx: &mut TestAppContext,
) {
    use crate::app::tool_layout::ApplicationToolLayout;
    use crate::app::workbench::{Area, Slot};
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let (right, tool) = terminal_area(&window, cx);
    let slot = |window: &Entity<KnotWindow>, cx: &TestAppContext| {
        let tool = terminal_area(window, cx).1;
        cx.read(|cx| {
            window
                .read(cx)
                .workbench
                .read(cx)
                .tool(tool)
                .unwrap()
                .slot()
        })
    };
    assert_eq!(slot(&window, cx), Slot::BottomLeft);

    let center =
        dispatch_product_command(&window, native_window, NEW_TERMINAL_IN_CENTER_COMMAND, cx);
    assert_eq!(center.completion.await.unwrap(), CommandOutcome::Completed);
    assert_eq!(terminal_panes(&window, cx), [(Area::Center, 1)]);
    // The terminal moves into its window, which shows with it, and out
    // again, which hides the window with its last tab.
    window.update(cx, |window, cx| {
        let (pane, tab) = {
            let workbench = window.workbench.read(cx);
            let pane = workbench.focused_pane_id().unwrap();
            (pane, workbench.pane(pane).unwrap().active_tab_id())
        };
        window.move_tab_to_area(pane, tab, right, cx).unwrap();
    });
    assert_eq!(terminal_panes(&window, cx), [(right, 1)]);
    assert!(cx.read(|cx| window.read(cx).workbench.read(cx).tool_visible(tool)));
    let to_center =
        dispatch_product_command(&window, native_window, "workbench.open-as-editor-tab", cx);
    assert_eq!(
        to_center.completion.await.unwrap(),
        CommandOutcome::Completed
    );
    assert_eq!(terminal_panes(&window, cx), [(Area::Center, 1)]);
    assert!(!cx.read(|cx| window.read(cx).workbench.read(cx).tool_visible(tool)));

    // Moving the window to another slot reaches fresh windows, which open
    // their first terminal there.
    window.update(cx, |window, cx| {
        window.move_tool_to_slot(tool, Slot::RightTop, cx)
    });
    assert_eq!(
        cx.read(|cx| cx
            .global::<ApplicationToolLayout>()
            .0
            .current()
            .tool(crate::app::terminal::TERMINAL_TOOL_ID)
            .map(|tool| tool.slot)),
        Some(Slot::RightTop)
    );
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (fresh, fresh_window) = product_window(document, model, cx);
    assert_eq!(slot(&fresh, cx), Slot::RightTop);
    let toggle = dispatch_product_command(&fresh, fresh_window, TOGGLE_TERMINAL_COMMAND, cx);
    assert_eq!(toggle.completion.await.unwrap(), CommandOutcome::Completed);
    assert_eq!(
        terminal_panes(&fresh, cx),
        [(terminal_area(&fresh, cx).0, 1)]
    );
}

#[gpui::test]
async fn window_layouts_restore_container_terminals_in_their_window(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (source, source_window) = product_window(document, model, cx);
    let toggle = dispatch_product_command(&source, source_window, TOGGLE_TERMINAL_COMMAND, cx);
    assert_eq!(toggle.completion.await.unwrap(), CommandOutcome::Completed);
    let layout = cx
        .update_window(source_window, |_, native_window, cx| {
            source.read(cx).capture_layout(Some(native_window), cx)
        })
        .unwrap();
    let value = serde_json::to_value(&layout).unwrap();
    let recorded = value["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["tool"] == crate::app::terminal::TERMINAL_TOOL_ID)
        .unwrap();
    assert_eq!(recorded["root"]["tabs"][0]["kind"], "terminal");
    assert_eq!(recorded["slot"], "bottomLeft");

    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (restored, _) = product_window(document, model, cx);
    restored.update(cx, |window, cx| window.apply_layout(&layout, &[], &[], cx));
    assert_eq!(
        terminal_panes(&restored, cx),
        [(terminal_area(&restored, cx).0, 1)]
    );
    // Restored terminals are fresh sessions: one per terminal tab, per window.
    assert_eq!(session_count(cx), 2);
}

#[gpui::test]
async fn toggle_terminal_restores_a_maximized_window_terminal(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::TOGGLE_MAXIMIZE_PANE_COMMAND;
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let run = |command, cx: &mut TestAppContext| {
        dispatch_product_command(&window, native_window, command, cx)
    };
    let painted_panes = |cx: &mut TestAppContext| {
        cx.run_until_parked();
        cx.read(|cx| window.read(cx).tab_drag.borrow().painted_panes().len())
    };

    run(TOGGLE_TERMINAL_COMMAND, cx).completion.await.unwrap();
    let both = painted_panes(cx);
    run(TOGGLE_MAXIMIZE_PANE_COMMAND, cx)
        .completion
        .await
        .unwrap();
    assert_eq!(painted_panes(cx), 1);

    // Hiding the window restores the layout; showing it maximizes the terminal.
    run(TOGGLE_TERMINAL_COMMAND, cx).completion.await.unwrap();
    assert!(painted_panes(cx) >= 1);
    run(TOGGLE_TERMINAL_COMMAND, cx).completion.await.unwrap();
    assert_eq!(painted_panes(cx), 1);
    assert_eq!(
        focused_area(&window, cx),
        Some(terminal_area(&window, cx).0)
    );
    assert!(both > 1);
}

#[gpui::test]
async fn restart_command_and_overlay_restart_the_focused_session(cx: &mut TestAppContext) {
    use crate::app::terminal::view::RESTART_TERMINAL_COMMAND;
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let (pane, tab, session, view) = window.update(cx, |window, cx| {
        let (session_id, view) = window.create_terminal(cx);
        let pane = window.workbench.read(cx).focused_pane_id().unwrap();
        let tab = window.workbench.update(cx, |workbench, _| {
            workbench
                .open_terminal_tab(pane, session_id, view.clone())
                .unwrap()
        });
        let session = cx
            .global::<ApplicationTerminalSessions>()
            .0
            .borrow()
            .sessions[&session_id]
            .clone();
        window.activate_tab(pane, tab, cx);
        (pane, tab, session, view)
    });
    // The palette command restarts the focused session, even from a closed
    // one. The generation bump is the environment-independent trace:
    // sandboxes without PTY spawn observe Failed throughout.
    session.update(cx, |session, cx| session.close(cx));
    let before = cx.read(|cx| session.read(cx).generation());
    assert_eq!(
        dispatch_product_command(&window, native_window, RESTART_TERMINAL_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    assert!(cx.read(|cx| session.read(cx).generation()) > before);

    // The overlay's restart affordance calls the same view restart.
    session.update(cx, |session, cx| session.close(cx));
    let before = cx.read(|cx| session.read(cx).generation());
    window.update(cx, |_, cx| {
        view.update(cx, |view, cx| view.restart(cx));
    });
    assert!(cx.read(|cx| session.read(cx).generation()) > before);
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(workbench.pane(pane).unwrap().active_tab().id(), tab);
    });
    session.update(cx, |session, cx| session.close(cx));
}

fn open_terminals_in_window(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    count: usize,
    cx: &mut TestAppContext,
) {
    for _ in 0..count {
        let new = dispatch_product_command(window, native_window, NEW_TERMINAL_COMMAND, cx);
        assert_eq!(wait_for_command(new, cx), CommandOutcome::Completed);
    }
}

fn run_command(
    window: &Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    name: &'static str,
    cx: &mut TestAppContext,
) -> CommandOutcome {
    wait_for_command(
        dispatch_product_command(window, native_window, name, cx),
        cx,
    )
}

#[gpui::test]
fn a_container_splits_moves_unsplits_and_walks_its_panes(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::{
        NEXT_SPLITTER_COMMAND, OPEN_AS_EDITOR_TAB_COMMAND, PREVIOUS_SPLITTER_COMMAND,
        RETURN_TO_TOOL_WINDOW_COMMAND, SPLIT_AND_MOVE_DOWN_COMMAND, SPLIT_AND_MOVE_RIGHT_COMMAND,
        UNSPLIT_COMMAND,
    };
    use crate::app::workbench::Area;
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let (area, _) = terminal_area(&window, cx);

    open_terminals_in_window(&window, native_window, 2, cx);
    assert_eq!(terminal_panes(&window, cx), [(area, 2)]);
    // A lone tab has nothing to split away from.
    open_terminals_in_window(&window, native_window, 0, cx);

    assert_eq!(
        run_command(&window, native_window, SPLIT_AND_MOVE_RIGHT_COMMAND, cx),
        CommandOutcome::Completed
    );
    assert_eq!(terminal_panes(&window, cx), [(area, 1), (area, 1)]);
    assert_eq!(
        run_command(&window, native_window, SPLIT_AND_MOVE_DOWN_COMMAND, cx),
        CommandOutcome::Unavailable
    );

    // The splitter commands walk the window's panes and wrap.
    let focused_pane =
        |cx: &TestAppContext| cx.read(|cx| window.read(cx).workbench.read(cx).focused_pane_id());
    let first = focused_pane(cx);
    run_command(&window, native_window, NEXT_SPLITTER_COMMAND, cx);
    let second = focused_pane(cx);
    assert_ne!(first, second);
    run_command(&window, native_window, NEXT_SPLITTER_COMMAND, cx);
    assert_eq!(focused_pane(cx), first);
    run_command(&window, native_window, PREVIOUS_SPLITTER_COMMAND, cx);
    assert_eq!(focused_pane(cx), second);

    assert_eq!(
        run_command(&window, native_window, UNSPLIT_COMMAND, cx),
        CommandOutcome::Completed
    );
    assert_eq!(terminal_panes(&window, cx), [(area, 2)]);
    assert_eq!(
        run_command(&window, native_window, UNSPLIT_COMMAND, cx),
        CommandOutcome::Unavailable
    );

    // A terminal tab goes to the editor and back to its window.
    assert_eq!(
        run_command(&window, native_window, OPEN_AS_EDITOR_TAB_COMMAND, cx),
        CommandOutcome::Completed
    );
    assert_eq!(terminal_panes(&window, cx), [(Area::Center, 1), (area, 1)]);
    assert_eq!(
        run_command(&window, native_window, RETURN_TO_TOOL_WINDOW_COMMAND, cx),
        CommandOutcome::Completed
    );
    assert_eq!(terminal_panes(&window, cx), [(area, 2)]);
    assert_eq!(
        run_command(&window, native_window, RETURN_TO_TOOL_WINDOW_COMMAND, cx),
        CommandOutcome::Unavailable
    );
}

#[gpui::test]
fn escape_in_a_terminal_stays_with_the_shell(cx: &mut TestAppContext) {
    use crate::app::workbench::Area;
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    let (area, _) = terminal_area(&window, cx);
    open_terminals_in_window(&window, native_window, 1, cx);
    assert_eq!(focused_area(&window, cx), Some(area));

    press_keys(&window, "escape", cx);
    assert_eq!(focused_area(&window, cx), Some(area));

    // A terminal in the editor keeps Esc for the shell.
    let center =
        dispatch_product_command(&window, native_window, NEW_TERMINAL_IN_CENTER_COMMAND, cx);
    assert_eq!(wait_for_command(center, cx), CommandOutcome::Completed);
    press_keys(&window, "escape", cx);
    assert_eq!(focused_area(&window, cx), Some(Area::Center));
}

#[gpui::test]
fn rename_session_labels_the_tab_and_the_label_persists(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::RENAME_SESSION_COMMAND;
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    open_terminals_in_window(&window, native_window, 1, cx);

    let rename = dispatch_product_command(&window, native_window, RENAME_SESSION_COMMAND, cx);
    assert_eq!(wait_for_command(rename, cx), CommandOutcome::Completed);
    press_keys(&window, "ctrl-a ctrl-k", cx);
    type_text(&window, "build", cx);
    press_keys(&window, "enter", cx);

    let session_name = |cx: &TestAppContext| {
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            let pane = workbench.focused_pane().unwrap();
            let TabSurfaceId::Terminal(id) = pane.active_tab().surface_id() else {
                panic!("the terminal is focused");
            };
            KnotWindow::terminal_session_name(id, cx)
        })
    };
    assert_eq!(session_name(cx).as_deref(), Some("build"));

    // The window layout records the label.
    let layout = cx.read(|cx| window.read(cx).capture_layout(None, cx));
    let json = serde_json::to_string(&layout).unwrap();
    assert!(json.contains("\"name\":\"build\""), "{json}");
}

#[gpui::test]
fn a_program_title_labels_the_tab_and_rename_wins(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::RENAME_SESSION_COMMAND;
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    open_terminals_in_window(&window, native_window, 1, cx);

    let session_id = |cx: &TestAppContext| {
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            let pane = workbench.focused_pane().unwrap();
            let TabSurfaceId::Terminal(id) = pane.active_tab().surface_id() else {
                panic!("the terminal is focused");
            };
            id
        })
    };
    let id = session_id(cx);
    let label = |cx: &TestAppContext| cx.read(|cx| KnotWindow::terminal_session_label(id, cx));
    let default = label(cx).unwrap();
    let send_title = |title: &[u8], cx: &mut TestAppContext| {
        let session = cx.read(|cx| {
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions[&id]
                .clone()
        });
        cx.read(|cx| {
            session.read(cx).with_terminal(|terminal| {
                terminal.write(&[b"\x1b]2;".as_slice(), title, b"\x07"].concat())
            })
        });
    };
    let wait_for_label = |expected: &str, cx: &mut TestAppContext| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while label(cx).as_deref() != Some(expected) {
            assert!(
                std::time::Instant::now() < deadline,
                "label {:?}",
                label(cx)
            );
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };

    send_title(b"vim: notes", cx);
    wait_for_label("vim: notes", cx);
    // A title is not part of the layout; the default label is regenerated.
    let layout = cx.read(|cx| window.read(cx).capture_layout(None, cx));
    let json = serde_json::to_string(&layout).unwrap();
    assert!(
        !json.contains("vim: notes") && !json.contains("\"name\""),
        "{json}"
    );

    send_title(b"", cx);
    wait_for_label(&default, cx);

    let rename = dispatch_product_command(&window, native_window, RENAME_SESSION_COMMAND, cx);
    assert_eq!(wait_for_command(rename, cx), CommandOutcome::Completed);
    press_keys(&window, "ctrl-a ctrl-k", cx);
    type_text(&window, "build", cx);
    press_keys(&window, "enter", cx);
    send_title(b"vim: notes", cx);
    cx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    assert_eq!(label(cx).as_deref(), Some("build"));
    let layout = cx.read(|cx| window.read(cx).capture_layout(None, cx));
    let json = serde_json::to_string(&layout).unwrap();
    assert!(json.contains("\"name\":\"build\""), "{json}");
}

#[gpui::test]
fn a_bell_marks_background_tabs_and_hidden_tool_windows(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    open_terminals_in_window(&window, native_window, 2, cx);

    let (pane, tabs) = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        let tabs = pane
            .tabs()
            .iter()
            .map(|tab| {
                let TabSurfaceId::Terminal(session) = tab.surface_id() else {
                    panic!("only terminals")
                };
                (pane.id(), tab.id(), session)
            })
            .collect::<Vec<_>>();
        (pane.id(), tabs)
    });
    let (_, first_tab, first) = tabs[0];
    let (_, second_tab, second) = tabs[1];
    let ring = |id, cx: &mut TestAppContext| {
        let session = cx.read(|cx| {
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions[&id]
                .clone()
        });
        cx.read(|cx| {
            session
                .read(cx)
                .with_terminal(|terminal| terminal.write(b"\x07"))
        });
    };
    let marked =
        |id, cx: &TestAppContext| cx.read(|cx| window.read(cx).workbench.read(cx).bell_marked(id));
    let wait_for_mark = |id, cx: &mut TestAppContext| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marked(id, cx) {
            assert!(std::time::Instant::now() < deadline, "no mark");
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    let has_selector = |selector: String, cx: &mut TestAppContext| {
        let selector: &'static str = Box::leak(selector.into_boxed_str());
        VisualTestContext::from_window(native_window, cx)
            .debug_bounds(selector)
            .is_some()
    };

    // A bell in the shown tab marks nothing; the one in a background tab
    // paints the tab's icon until the tab is shown.
    ring(second, cx);
    ring(first, cx);
    wait_for_mark(first, cx);
    assert!(!marked(second, cx));
    assert!(has_selector(format!("bell-{}", first_tab.value()), cx));
    assert!(!has_selector(format!("bell-{}", second_tab.value()), cx));
    window.update(cx, |window, cx| window.activate_tab(pane, first_tab, cx));
    cx.run_until_parked();
    assert!(!marked(first, cx));
    assert!(!has_selector(format!("bell-{}", first_tab.value()), cx));

    // A hidden tool window shows the info dot on its stripe button.
    let badge = format!("stripe-badge-{}", crate::app::terminal::TERMINAL_TOOL_ID);
    window.update(cx, |window, cx| {
        window.toggle_tool(crate::app::terminal::TERMINAL_TOOL_ID, cx)
    });
    cx.run_until_parked();
    assert!(!has_selector(badge.clone(), cx));
    ring(first, cx);
    wait_for_mark(first, cx);
    assert!(has_selector(badge.clone(), cx));
    window.update(cx, |window, cx| {
        window.toggle_tool(crate::app::terminal::TERMINAL_TOOL_ID, cx)
    });
    cx.run_until_parked();
    assert!(!marked(first, cx));
    assert!(!has_selector(badge, cx));
}

#[gpui::test]
fn a_terminal_notification_becomes_a_notice_that_shows_the_terminal(cx: &mut TestAppContext) {
    use crate::app::commands::dispatch::SHOW_TERMINAL_COMMAND;
    use crate::app::notifications::{ApplicationNotifications, Severity};

    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    open_terminals_in_window(&window, native_window, 2, cx);

    let (pane, first_tab, first, second) = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        let session = |index: usize| {
            let TabSurfaceId::Terminal(session) = pane.tabs()[index].surface_id() else {
                panic!("only terminals")
            };
            session
        };
        (pane.id(), pane.tabs()[0].id(), session(0), session(1))
    });
    let write = |id, bytes: &[u8], cx: &mut TestAppContext| {
        let session = cx.read(|cx| {
            cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions[&id]
                .clone()
        });
        cx.read(|cx| {
            session
                .read(cx)
                .with_terminal(|terminal| terminal.write(bytes))
        });
    };
    let notices = |count: usize, cx: &mut TestAppContext| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            cx.run_until_parked();
            let notices = cx.read(|cx| {
                let store = &cx.global::<ApplicationNotifications>().0;
                let mut terminal = store.notices().to_vec();
                terminal.retain(|notice| notice.target.is_some());
                terminal
            });
            if notices.len() == count {
                return notices;
            }
            assert!(std::time::Instant::now() < deadline, "notices: {notices:?}");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };

    // A notification raises a notice even from the shown terminal; without a
    // title of its own it carries the tab's label.
    // Each terminal parses on its own thread, so the second waits for the
    // first notice to fix their order.
    write(second, b"\x1b]777;notify;Build;all green\x07", cx);
    notices(1, cx);
    write(first, b"\x1b]9;done\x07", cx);
    let notices = notices(2, cx);
    let window_id = super::super::window_id(window.entity_id());
    let label = cx.read(|cx| KnotWindow::terminal_session_label(first, cx).unwrap());
    assert_eq!(notices[0].title.as_deref(), Some("Build"));
    assert_eq!(notices[0].message, "all green");
    assert_eq!(notices[1].title.as_deref(), Some(label.as_str()));
    assert_eq!(notices[1].message, "done");
    for notice in &notices {
        assert_eq!(notice.severity, Severity::Info);
        assert_eq!(notice.target.as_deref(), Some(window_id.as_str()));
    }

    // Its action shows the terminal that sent it.
    let active = |cx: &TestAppContext| {
        cx.read(|cx| {
            window
                .read(cx)
                .workbench
                .read(cx)
                .pane(pane)
                .unwrap()
                .active_tab_id()
        })
    };
    assert_ne!(active(cx), first_tab);
    let action = notices[1].actions[0].command.clone();
    assert_eq!(action.name.as_ref() as &str, SHOW_TERMINAL_COMMAND);
    let show = dispatch_product_command_with(
        &window,
        native_window,
        SHOW_TERMINAL_COMMAND,
        action.arguments,
        cx,
    );
    assert_eq!(wait_for_command(show, cx), CommandOutcome::Completed);
    assert_eq!(active(cx), first_tab);
}

#[gpui::test]
async fn a_terminal_tab_presents_one_terminal_view(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    assert_eq!(
        dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let root = cx.read(|cx| {
        window
            .read(cx)
            .application_window(window.entity_id(), &std::collections::HashMap::new(), cx)
            .panes
            .into_iter()
            .flat_map(|pane| pane.tabs)
            .find(|tab| tab.kind == knot_protocol::SurfaceKind::Terminal)
            .unwrap()
            .root
    });
    assert_eq!(root.kind, "terminal");
    assert!(root.children.is_empty());
    assert!(root.document.is_none() && root.buffer.is_none());
}
