//! Native tools participate in the same presentation and closure boundaries as tabs.
use super::operations::empty_product_window;
use super::*;
use crate::app::documents::DocumentDirty;
use crate::app::workbench::DockLocation;

#[gpui::test]
fn a_docked_tool_opens_resources_beside_it_and_has_a_nontext_snapshot(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    window.update(cx, |window, cx| window.ensure_test_tool(true, cx));
    cx.run_until_parked();
    let tree = cx.read(|cx| window.read(cx).test_tool_tree(cx));
    let uri = ResourceUri::parse("mem://product/notes.txt").unwrap();
    cx.update_window(native_window, |_, native_window, cx| {
        focus_view(&tree.focus_handle(cx), native_window, cx);
        window.update(cx, |window, cx| {
            window.open_from_test_tool(uri.clone(), false, cx)
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let pane = workbench.center_pane().unwrap();
        assert!(workbench.pane(pane).unwrap().tabs().iter().any(|tab| {
            tab.document_id()
                .is_some_and(|id| documents.read(cx).get(id).unwrap().resource_uri() == Some(&uri))
        }));
        let id = workbench.tools()[0].id;
        assert_eq!(
            workbench.dock_location(workbench.tool_tab(id).unwrap().0),
            Some(DockLocation::Left)
        );
    });
    let target = cx
        .update_window(native_window, |_, native_window, cx| {
            focus_view(&tree.focus_handle(cx), native_window, cx);
            window.update(cx, |window, cx| window.capture_command_target(cx))
        })
        .unwrap();
    cx.read(|cx| {
        assert!(matches!(target.tab.unwrap().surface, TabSurfaceId::Tool(_)));
        assert!(target.document_id(cx).is_err());
    });
    cx.update_window(native_window, |_, _, cx| {
        let state = window.read(cx).application_window(
            window.entity_id(),
            &std::collections::HashMap::new(),
            cx,
        );
        let tool = state
            .panes
            .iter()
            .flat_map(|pane| &pane.tabs)
            .find(|tab| tab.kind == knot_protocol::SurfaceKind::Tool)
            .unwrap();
        assert!(tool.buffer.is_none());
        assert!(tool.document.is_none());
        assert_eq!(state.current_target, Some(tool.target.clone()));
    })
    .unwrap();
    place_explorer(&window, native_window, DockLocation::Bottom, cx);
    cx.read(|cx| assert_eq!(target.validate_tab(cx), Err(CommandOutcome::InvalidTarget)));
    let close = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    assert_eq!(wait_for_command(close, cx), CommandOutcome::Completed);
    assert!(!cx.has_pending_prompt());
    window.update(cx, |window, cx| {
        window.toggle_tool(crate::app::product::tools::TEST_TOOL_ID, cx)
    });
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        assert_eq!(
            workbench.tool_presenting(tree.entity_id()),
            Some(id_of(workbench))
        );
        // The closed tool returns to the dock it was moved to.
        let pane = workbench.tool_tab(id_of(workbench)).unwrap().0;
        assert_eq!(workbench.dock_location(pane), Some(DockLocation::Bottom));
    });
}

fn id_of(workbench: &Workbench) -> crate::app::workbench::ToolFrameId {
    workbench.tools()[0].id
}

#[gpui::test]
fn last_document_closure_leaves_tool_only_window_and_hidden_definitions_survive_replacement(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let (window, native_window) = empty_product_window(&documents, cx);
    let workbench = cx.read(|cx| window.read(cx).workbench.clone());
    window.update(cx, |window, cx| window.ensure_test_tool(false, cx));
    let document = cx.read(|cx| {
        workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .document_id()
            .unwrap()
    });
    let close = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    assert_eq!(wait_for_command(close, cx), CommandOutcome::Completed);
    cx.read(|cx| {
        assert!(documents.read(cx).get(document).is_none());
        assert_eq!(documents.read(cx).documents().count(), 0);
        assert_eq!(&window.read(cx).workbench, &workbench);
        assert_eq!(workbench.read(cx).tab_snapshots().len(), 1);
        assert!(matches!(
            workbench.read(cx).tab_snapshots()[0].surface,
            TabSurfaceId::Tool(_)
        ));
    });
    cx.update_window(native_window, |_, _, cx| {
        let target = window.update(cx, |window, cx| window.capture_command_target(cx));
        assert!(
            target.tab.is_none(),
            "the tool beside the empty center keeps no focus"
        );
    })
    .unwrap();
    let new = dispatch_product_command(&window, native_window, NEW_COMMAND, cx);
    assert_eq!(wait_for_command(new, cx), CommandOutcome::Completed);
    window.update(cx, |window, cx| {
        let id = id_of(window.workbench.read(cx));
        window.workbench.update(cx, |workbench, _| {
            workbench.hide_tool(id);
        });
    });
    let close = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    assert_eq!(wait_for_command(close, cx), CommandOutcome::Completed);
    cx.read(|cx| {
        assert_eq!(&window.read(cx).workbench, &workbench);
        assert_eq!(documents.read(cx).documents().count(), 0);
        assert_eq!(workbench.read(cx).center(), None);
        assert_eq!(workbench.read(cx).tools().len(), 1);
        assert!(
            workbench
                .read(cx)
                .tool_tab(id_of(workbench.read(cx)))
                .is_none()
        );
    });
}

#[gpui::test]
fn workspace_refresh_does_not_reopen_or_reposition_tool(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, _) = empty_product_window(&documents, cx);
    window.update(cx, |window, cx| {
        window.ensure_test_tool(false, cx);
        let id = id_of(window.workbench.read(cx));
        window.workbench.update(cx, |workbench, _| {
            workbench.place_tool(id, DockLocation::Bottom);
            workbench.hide_tool(id);
        });
        window.install_workspace(ResourceUri::parse("mem://product/src").unwrap(), cx);
        assert!(window.workbench.read(cx).tool_tab(id).is_none());
        window.workbench.update(cx, |workbench, _| {
            let (pane, _) = workbench.show_tool(id).unwrap();
            assert_eq!(workbench.dock_location(pane), Some(DockLocation::Bottom));
        });
    });
}

#[gpui::test]
async fn tools_do_not_bypass_dirty_document_or_terminal_protection(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    install_failing_recovery(cx);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    model.update(cx, |model, _| model.replace(0..0, "unsaved").unwrap());
    let (window, native_window) = product_window(document, model, cx);
    window.update(cx, |window, cx| window.ensure_test_tool(false, cx));
    let close = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    assert_eq!(close.completion.await.unwrap(), CommandOutcome::Cancelled);
    place_explorer(&window, native_window, DockLocation::Bottom, cx);
    let close_tool = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    assert_eq!(wait_for_command(close_tool, cx), CommandOutcome::Completed);
    assert!(!cx.has_pending_prompt());
    cx.read(|cx| assert!(documents.read(cx).get(document).unwrap().is_dirty(cx)));
    toggle_test_tool(&window, cx);
    let terminal = dispatch_product_command(&window, native_window, NEW_TERMINAL_COMMAND, cx);
    assert_eq!(wait_for_command(terminal, cx), CommandOutcome::Completed);
    let session = cx.read(|cx| {
        match window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane()
            .unwrap()
            .active_tab()
            .surface_id()
        {
            TabSurfaceId::Terminal(id) => id,
            _ => panic!("terminal missing"),
        }
    });
    for command in [CLOSE_WINDOW_COMMAND, QUIT_COMMAND] {
        let close = dispatch_product_command(&window, native_window, command, cx);
        cx.run_until_parked();
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("Cancel");
        assert_eq!(close.completion.await.unwrap(), CommandOutcome::Cancelled);
        cx.read(|cx| {
            assert!(
                cx.global::<ApplicationTerminalSessions>()
                    .0
                    .borrow()
                    .sessions
                    .contains_key(&session)
            );
            assert!(documents.read(cx).get(document).unwrap().is_dirty(cx));
            assert!(
                window
                    .read(cx)
                    .workbench
                    .read(cx)
                    .tool_tab(id_of(window.read(cx).workbench.read(cx)))
                    .is_some()
            );
        });
    }
    let close_terminal = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    assert_eq!(
        wait_for_command(close_terminal, cx),
        CommandOutcome::Completed
    );
    assert!(!cx.has_pending_prompt());
    cx.read(|cx| {
        assert!(
            !cx.global::<ApplicationTerminalSessions>()
                .0
                .borrow()
                .sessions
                .contains_key(&session)
        )
    });
}

#[gpui::test]
fn docked_tools_have_visible_geometry(cx: &mut TestAppContext) {
    for dock in DockLocation::ALL {
        let documents = install_globals(cx);
        cx.set_global(ApplicationFileSystems(memory_filesystems()));
        let (window, native_window) = empty_product_window(&documents, cx);
        window.update(cx, |window, cx| {
            window.ensure_test_tool(false, cx);
            let id = window.test_tool_frame(cx).unwrap();
            window.workbench.update(cx, |workbench, _| {
                assert!(workbench.place_tool(id, dock));
                assert_eq!(workbench.tool_tab(id).unwrap().0.value(), 3);
            });
            cx.notify();
        });
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(native_window, cx);
        let explorer = visual.debug_bounds("pane-3").unwrap();
        let editor = visual.debug_bounds("pane-1").unwrap();
        match dock {
            DockLocation::Left | DockLocation::Right => {
                assert_eq!(explorer.size.width, px(240.), "{dock:?}: {explorer:?}");
                assert!(explorer.size.height > px(100.));
            }
            DockLocation::Bottom => {
                assert_eq!(explorer.size.height, px(220.), "{explorer:?}");
                assert!(explorer.size.width > px(100.));
            }
        }
        assert!(editor.size.width > px(100.));
        assert!(editor.size.height > px(100.));
        window.update(&mut visual, |window, cx| {
            let id = window.test_tool_frame(cx).unwrap();
            window.workbench.update(cx, |workbench, _| {
                assert!(workbench.hide_tool(id));
            });
            cx.notify();
        });
        visual.run_until_parked();
        window.update(&mut visual, |window, cx| {
            let id = window.test_tool_frame(cx).unwrap();
            window.workbench.update(cx, |workbench, _| {
                workbench.show_tool(id).unwrap();
            });
            cx.notify();
        });
        visual.run_until_parked();
        let reopened = visual.debug_bounds("pane-4").unwrap();
        assert_eq!(reopened.size, explorer.size);
    }
}

#[gpui::test]
fn status_target_follows_the_focused_or_last_visible_editor(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    window.update(cx, |window, cx| {
        window.install_workspace(ResourceUri::parse("mem://product/").unwrap(), cx);
        window.ensure_test_tool(false, cx);
    });
    cx.run_until_parked();
    let tree = cx.read(|cx| window.read(cx).test_tool_tree(cx));
    let state = |cx: &mut TestAppContext| {
        cx.update_window(native_window, |_, _, cx| {
            window.read(cx).application_window(
                window.entity_id(),
                &std::collections::HashMap::new(),
                cx,
            )
        })
        .unwrap()
    };
    let settle = |cx: &mut TestAppContext| {
        cx.update_window(native_window, |_, native_window, cx| {
            native_window.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
    };
    let focus_editor = |cx: &mut TestAppContext| {
        cx.update_window(native_window, |_, native_window, cx| {
            window.update(cx, |window, cx| {
                window.observe_application_focus(native_window, cx)
            });
            window.update(cx, |window, cx| window.focus_active_editor(cx));
        })
        .unwrap();
        cx.update_window(native_window, |_, native_window, cx| {
            native_window.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
    };

    cx.update_window(native_window, |_, native_window, _| {
        native_window.activate_window()
    })
    .unwrap();

    focus_editor(cx);
    let editor = state(cx).current_target.expect("focused editor");
    assert_eq!(state(cx).status_target, Some(editor.clone()));

    // A tool keeps the editor it was focused from.
    cx.update_window(native_window, |_, native_window, cx| {
        window.update(cx, |window, cx| {
            window.observe_application_focus(native_window, cx)
        });
        focus_view(&tree.focus_handle(cx), native_window, cx);
    })
    .unwrap();
    settle(cx);
    let with_tool = state(cx);
    assert_ne!(with_tool.current_target, Some(editor.clone()));
    assert_eq!(with_tool.status_target, Some(editor.clone()));

    // A focused terminal clears the editor items, and its tab hides the
    // editor, so the editor does not return when focus moves to the tool.
    let command =
        dispatch_product_command(&window, native_window, NEW_TERMINAL_IN_CENTER_COMMAND, cx);
    assert_eq!(wait_for_command(command, cx), CommandOutcome::Completed);
    cx.update_window(native_window, |_, native_window, cx| {
        window.update(cx, |window, cx| {
            window.observe_application_focus(native_window, cx)
        });
    })
    .unwrap();
    assert!(state(cx).current_target.is_some());
    assert_eq!(state(cx).status_target, None);
    cx.update_window(native_window, |_, native_window, cx| {
        focus_view(&tree.focus_handle(cx), native_window, cx);
    })
    .unwrap();
    assert_eq!(state(cx).status_target, None);

    // Showing the editor again restores it, even with focus on the tool.
    cx.update(|cx| {
        let workbench = window.read(cx).workbench.clone();
        workbench.update(cx, |workbench, _| {
            let pane = workbench.center_pane().unwrap();
            let tab = workbench.pane(pane).unwrap().tabs()[0].id();
            assert!(workbench.activate_tab(pane, tab));
        });
    });
    assert_eq!(state(cx).status_target, Some(editor));
}

#[gpui::test]
fn toggle_explorer_and_toggle_tool_show_focus_then_hide_to_the_center(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.set_global(ApplicationFileSystems(memory_filesystems()));
    let (window, native_window) = empty_product_window(&documents, cx);
    let center = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_pane_id()
            .unwrap()
    });
    let state = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            let explorer = workbench
                .tool_by_stable_id(crate::app::product::tools::TEST_TOOL_ID)
                .unwrap();
            (
                workbench.tool_visible(explorer),
                workbench.focused_pane_id() == Some(center),
            )
        })
    };
    // Not registered yet: it shows and takes focus.
    toggle_test_tool(&window, cx);
    assert_eq!(state(cx), (true, false));
    // Focused: it hides and the center takes focus back.
    toggle_test_tool(&window, cx);
    assert_eq!(state(cx), (false, true));

    // The picker toggles the chosen tool the same way.
    for expected in [(true, false), (false, true)] {
        let picker = dispatch_product_command(&window, native_window, "workbench.toggle-tool", cx);
        assert_eq!(wait_for_command(picker, cx), CommandOutcome::Completed);
        wait_for_api_condition(cx, |cx| window.read(cx).transient.is_some());
        cx.simulate_input(native_window, "Test Tool");
        cx.simulate_keystrokes(native_window, "enter");
        cx.run_until_parked();
        assert_eq!(state(cx), expected);
    }

    // Visible but unfocused: it takes focus instead of hiding.
    toggle_test_tool(&window, cx);
    window.update(cx, |window, cx| {
        window
            .workbench
            .update(cx, |workbench, _| workbench.focus_pane(center));
    });
    toggle_test_tool(&window, cx);
    assert_eq!(state(cx), (true, false));
}
