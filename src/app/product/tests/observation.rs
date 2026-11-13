//! Product commit-boundary regressions for application metadata observation.

use super::*;
use crate::app::documents::DocumentDirty;
use crate::app::{
    entry::{LaunchCommand, parse_launch_arguments},
    extensions::{self, host::ProductExtensionHost},
};
use knot_protocol::{ApplicationChange, ApplicationEvent, DocumentInfo, DocumentKind};

fn take_events(
    host: &Entity<ProductExtensionHost>,
    cx: &mut TestAppContext,
) -> Vec<ApplicationEvent> {
    cx.run_until_parked();
    host.update(cx, |host, _| host.take_test_application_events())
}

fn changed_documents(events: &[ApplicationEvent]) -> Vec<&DocumentInfo> {
    events
        .iter()
        .filter_map(|event| match &event.change {
            ApplicationChange::DocumentOpened { document }
            | ApplicationChange::DocumentChanged { document } => Some(document),
            _ => None,
        })
        .collect()
}

fn start_open(
    window: &Entity<KnotWindow>,
    request: OpenRequest,
    replace: bool,
    cx: &mut TestAppContext,
) -> tokio::sync::oneshot::Receiver<CommandOutcome> {
    let target = cx.read(|cx| current_open_target(window, cx));
    let (completion, receiver) = super::super::CommandCompletion::new();
    window.update(cx, |window, cx| {
        window.start_open_request(request, target, replace, Some(completion), cx)
    });
    receiver
}

fn wait_open(
    mut receiver: tokio::sync::oneshot::Receiver<CommandOutcome>,
    cx: &mut TestAppContext,
) -> CommandOutcome {
    let mut outcome = None;
    crate::test_support::wait_until(cx, |_| {
        match receiver.try_recv() {
            Ok(done) => outcome = Some(done),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            Err(error) => panic!("native Open completion closed: {error}"),
        }
        outcome.is_some()
    });
    outcome.unwrap()
}

#[gpui::test]
fn open_entry_points_share_document_identity_and_publish_only_committed_resources(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let host = cx.update(extensions::host::install);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.txt");
    std::fs::write(&path, "native file").unwrap();
    let cli = parse_launch_arguments(
        ["knot".to_owned(), "notes.txt".to_owned()],
        directory.path(),
    )
    .unwrap();
    let LaunchCommand::Open(batch) = cli else {
        panic!("CLI target missing");
    };
    let cli = batch.requests()[0].clone();
    let placeholder = cx.update(|cx| create_untitled_document(&documents, cx));
    let placeholder_model =
        cx.read(|cx| documents.read(cx).get(placeholder).unwrap().model().clone());
    let old_id = cx.read(|cx| {
        documents
            .read(cx)
            .get(placeholder)
            .unwrap()
            .identity()
            .to_string()
    });
    let window = headless_window(placeholder, placeholder_model.clone(), cx);
    let native_window = Headless;
    take_events(&host, cx);
    assert_eq!(
        wait_open(start_open(&window, cli.clone(), true, cx), cx),
        CommandOutcome::Completed
    );
    let opened = take_events(&host, cx);
    let file = changed_documents(&opened)
        .into_iter()
        .find(|document| document.kind == DocumentKind::Persisted)
        .unwrap();
    let id = file.id.clone();
    assert!(!file.dirty);
    assert_eq!(file.title, "notes.txt");
    assert!(opened.iter().any(|event| matches!(&event.change, ApplicationChange::DocumentClosed { document } if document == &old_id)));
    assert!(!cx.read(|cx| placeholder_model.read(cx).is_open()));

    let platform =
        OpenRequest::from_url(url::Url::from_file_path(&path).unwrap().as_str()).unwrap();
    assert_eq!(
        wait_open(start_open(&window, platform, false, cx), cx),
        CommandOutcome::Completed
    );
    assert!(changed_documents(&take_events(&host, cx)).is_empty());
    let second = cx.update(|cx| create_untitled_document(&documents, cx));
    let second_model = cx.read(|cx| documents.read(cx).get(second).unwrap().model().clone());
    let second_window = headless_window(second, second_model, cx);
    let second_native_window = Headless;
    take_events(&host, cx);
    cx.set_global(ApplicationOpenDialog(Arc::new(FixedOpenDialog(
        OpenDialogOutcome::Selected(path),
    ))));
    assert_eq!(
        wait_for_command(
            dispatch_product_command(
                &second_window,
                second_native_window,
                super::super::OPEN_COMMAND,
                cx
            ),
            cx
        ),
        CommandOutcome::Completed
    );
    let presented = take_events(&host, cx);
    assert!(changed_documents(&presented).is_empty());
    assert!(presented.iter().any(|event| matches!(&event.change, ApplicationChange::PresentationsChanged { window } if window.panes.iter().flat_map(|pane| &pane.tabs).any(|tab| tab.document.as_ref() == Some(&id)))));

    let missing =
        OpenRequest::from_path(&directory.path().join("missing.txt"), directory.path()).unwrap();
    assert_eq!(
        wait_open(start_open(&window, missing, false, cx), cx),
        CommandOutcome::Completed
    );
    let missing_events = take_events(&host, cx);
    assert!(
        changed_documents(&missing_events)
            .iter()
            .any(|document| document.kind == DocumentKind::Destination && document.dirty)
    );
    assert_eq!(
        wait_open(start_open(&window, cli, false, cx), cx),
        CommandOutcome::Completed
    );
    let activated = take_events(&host, cx);
    assert!(changed_documents(&activated).is_empty());
    assert!(activated.iter().any(|event| matches!(&event.change, ApplicationChange::PresentationsChanged { window } if window.panes.iter().any(|pane| pane.tabs.iter().any(|tab| tab.document.as_ref() == Some(&id) && pane.selected == tab.target && window.current_target.as_ref() == Some(&tab.target))))));
    let invalid_path = directory.path().join("invalid.txt");
    std::fs::write(&invalid_path, [0xff]).unwrap();
    for request in [
        OpenRequest::from_path(&invalid_path, directory.path()).unwrap(),
        OpenRequest::from_uri_for_product(ResourceUri::parse("unknown://provider/file").unwrap()),
    ] {
        assert!(matches!(
            wait_open(start_open(&window, request, false, cx), cx),
            CommandOutcome::HandlerFailure { .. }
        ));
        assert!(changed_documents(&take_events(&host, cx)).is_empty());
    }
    cx.set_global(ApplicationOpenDialog(Arc::new(FixedOpenDialog(
        OpenDialogOutcome::Cancelled,
    ))));
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&window, native_window, super::super::OPEN_COMMAND, cx),
            cx
        ),
        CommandOutcome::Cancelled
    );
    assert!(changed_documents(&take_events(&host, cx)).is_empty());
}

#[gpui::test]
async fn stale_open_does_not_publish_or_leak_a_document(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let host = cx.update(extensions::host::install);
    let (_, provider) = memory_filesystems_with_provider();
    let (release, gate) = tokio::sync::oneshot::channel();
    let gated = Arc::new(GatedFileSystemProvider {
        inner: provider,
        read_release: std::sync::Mutex::new(Some(gate)),
        replace_release: std::sync::Mutex::new(None),
    });
    let mut filesystems = FileSystemProviderRegistry::default();
    filesystems.register("mem", gated.clone()).unwrap();
    cx.set_global(ApplicationFileSystems(Arc::new(filesystems)));
    let placeholder = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(placeholder).unwrap().model().clone());
    let window = headless_window(placeholder, model, cx);
    take_events(&host, cx);
    let pending = start_open(
        &window,
        OpenRequest::from_uri_for_product(ResourceUri::parse("mem://product/notes.txt").unwrap()),
        true,
        cx,
    );
    wait_for_api_condition(cx, |_| gated.read_release.lock().unwrap().is_none());
    assert_eq!(
        start_open(
            &window,
            OpenRequest::from_uri_for_product(ResourceUri::parse("mem://product/new.txt").unwrap()),
            true,
            cx
        )
        .await
        .unwrap(),
        CommandOutcome::Completed
    );
    take_events(&host, cx);
    release.send(()).unwrap();
    assert_eq!(pending.await.unwrap(), CommandOutcome::InvalidTarget);
    assert!(changed_documents(&take_events(&host, cx)).is_empty());
    cx.read(|cx| {
        assert!(
            documents
                .read(cx)
                .document_for_resource(&ResourceUri::parse("mem://product/notes.txt").unwrap())
                .is_none()
        )
    });
}

#[gpui::test]
async fn save_as_and_racing_edits_publish_current_metadata_without_changing_identity(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    cx.set_global(save_without_cleanup());
    let host = cx.update(extensions::host::install);
    let (_, provider) = memory_filesystems_with_provider();
    let (release, gate) = tokio::sync::oneshot::channel();
    let gated = Arc::new(GatedFileSystemProvider {
        inner: provider.clone(),
        read_release: std::sync::Mutex::new(None),
        replace_release: std::sync::Mutex::new(Some(gate)),
    });
    let mut filesystems = FileSystemProviderRegistry::default();
    filesystems.register("mem", gated.clone()).unwrap();
    cx.set_global(ApplicationFileSystems(Arc::new(filesystems)));
    let uri = ResourceUri::parse("mem://product/notes.txt").unwrap();
    cx.set_global(ApplicationSaveDialog(Arc::new(FixedSaveDialog(
        SaveDialogOutcome::Selected(uri.clone()),
    ))));
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("initial")));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Untitled", model.clone(), cx)
    });
    let window = headless_window(document, model.clone(), cx);
    let native_window = Headless;
    let opened = take_events(&host, cx);
    let initial = changed_documents(&opened)
        .into_iter()
        .find(|document| document.title == "Untitled")
        .unwrap()
        .clone();
    model.update(cx, |model, cx| {
        model.replace(0..7, "captured").unwrap();
        cx.notify();
    });
    take_events(&host, cx);
    let save_as =
        dispatch_product_command(&window, native_window, super::super::SAVE_AS_COMMAND, cx);
    wait_for_api_condition(cx, |_| gated.replace_release.lock().unwrap().is_none());
    assert!(changed_documents(&take_events(&host, cx)).is_empty());
    model.update(cx, |model, cx| {
        model.replace(0..8, "newer").unwrap();
        cx.notify();
    });
    let raced = take_events(&host, cx);
    assert!(
        changed_documents(&raced)
            .iter()
            .any(|document| document.id == initial.id
                && document.kind == DocumentKind::Untitled
                && document.revision == 2
                && document.dirty)
    );
    release.send(()).unwrap();
    assert_eq!(save_as.completion.await.unwrap(), CommandOutcome::Completed);
    assert_eq!(provider.read(uri.clone()).await.unwrap().bytes, b"captured");
    let committed = take_events(&host, cx);
    let saved = changed_documents(&committed)
        .into_iter()
        .find(|document| document.id == initial.id)
        .unwrap();
    assert_eq!(saved.buffer, initial.buffer);
    assert_eq!(saved.title, "notes.txt");
    assert_eq!(
        saved.resource_uri.as_deref(),
        Some("mem://product/notes.txt")
    );
    assert_eq!(saved.kind, DocumentKind::Persisted);
    assert_eq!(saved.revision, 2);
    assert!(saved.dirty);
    assert!(!committed.iter().any(|event| matches!(
        event.change,
        ApplicationChange::DocumentOpened { .. } | ApplicationChange::DocumentClosed { .. }
    )));
    assert_eq!(
        dispatch_product_command(&window, native_window, SAVE_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let clean = take_events(&host, cx);
    assert!(
        changed_documents(&clean)
            .iter()
            .any(|document| document.id == initial.id && document.revision == 2 && !document.dirty)
    );
    assert_eq!(provider.read(uri.clone()).await.unwrap().bytes, b"newer");
    for (dialog, expected_cancelled) in [
        (SaveDialogOutcome::Cancelled, true),
        (
            SaveDialogOutcome::Selected(
                ResourceUri::parse("mem://product/missing-parent/file.txt").unwrap(),
            ),
            false,
        ),
    ] {
        cx.set_global(ApplicationSaveDialog(Arc::new(FixedSaveDialog(dialog))));
        let outcome =
            dispatch_product_command(&window, native_window, super::super::SAVE_AS_COMMAND, cx)
                .completion
                .await
                .unwrap();
        if expected_cancelled {
            assert_eq!(outcome, CommandOutcome::Cancelled);
        } else {
            assert!(matches!(outcome, CommandOutcome::HandlerFailure { .. }));
        }
        assert!(changed_documents(&take_events(&host, cx)).is_empty());
    }
}

#[gpui::test]
async fn completed_save_remains_observable_when_a_later_close_prompt_is_cancelled(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let host = cx.update(extensions::host::install);
    let (filesystems, provider) = memory_filesystems_with_provider();
    cx.set_global(ApplicationFileSystems(filesystems));
    let uri = ResourceUri::parse("mem://product/notes.txt").unwrap();
    let file = provider.read(uri.clone()).await.unwrap();
    let first_model = cx.new(|_| Buffer::new(BufferModel::from_text("loaded text")));
    let first = documents.update(cx, |documents, cx| {
        documents.create_persisted(
            "notes.txt",
            first_model.clone(),
            uri.clone(),
            0,
            file.version,
            cx,
        )
    });
    let (window, native_window) = product_window(first, first_model.clone(), cx);
    let second_model = cx.new(|_| Buffer::new(BufferModel::from_text("")));
    let second = documents.update(cx, |documents, cx| {
        documents.create_untitled("Second", second_model.clone(), cx)
    });
    window.update(cx, |window, cx| {
        let pane = window.workbench.read(cx).focused_pane_id().unwrap();
        window.workbench.update(cx, |workbench, cx| {
            workbench
                .open_tab_for_document(pane, second, second_model.clone(), cx)
                .unwrap()
        });
        cx.notify();
    });
    for model in [&first_model, &second_model] {
        model.update(cx, |model, cx| {
            model.replace(0..0, "changed ").unwrap();
            cx.notify();
        });
    }
    let first_identity = cx.read(|cx| {
        documents
            .read(cx)
            .get(first)
            .unwrap()
            .identity()
            .to_string()
    });
    take_events(&host, cx);
    let close = dispatch_product_command(
        &window,
        native_window,
        super::super::CLOSE_WINDOW_COMMAND,
        cx,
    );
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    assert_eq!(close.completion.await.unwrap(), CommandOutcome::Cancelled);
    let events = take_events(&host, cx);
    assert!(
        changed_documents(&events)
            .iter()
            .any(|document| document.id == first_identity && !document.dirty)
    );
    assert!(!events.iter().any(|event| matches!(
        event.change,
        ApplicationChange::DocumentClosed { .. } | ApplicationChange::WindowClosed { .. }
    )));
    cx.read(|cx| {
        assert!(!documents.read(cx).get(first).unwrap().is_dirty(cx));
        assert!(documents.read(cx).get(second).unwrap().is_dirty(cx));
        assert_eq!(window.read(cx).workbench.read(cx).view_count(first), 1);
        assert_eq!(window.read(cx).workbench.read(cx).view_count(second), 1);
    });
}

#[gpui::test]
async fn replaced_or_closed_open_presentations_cannot_accept_in_flight_results(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let host = cx.update(extensions::host::install);
    for close_window in [false, true] {
        let (_, provider) = memory_filesystems_with_provider();
        let (release, gate) = tokio::sync::oneshot::channel();
        let gated = Arc::new(GatedFileSystemProvider {
            inner: provider,
            read_release: std::sync::Mutex::new(Some(gate)),
            replace_release: std::sync::Mutex::new(None),
        });
        let mut filesystems = FileSystemProviderRegistry::default();
        filesystems.register("mem", gated.clone()).unwrap();
        cx.set_global(ApplicationFileSystems(Arc::new(filesystems)));
        let document = cx.update(|cx| create_untitled_document(&documents, cx));
        let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
        let window = headless_window(document, model.clone(), cx);
        let native_window = Headless;
        let pending = start_open(
            &window,
            OpenRequest::from_uri_for_product(
                ResourceUri::parse("mem://product/notes.txt").unwrap(),
            ),
            true,
            cx,
        );
        wait_for_api_condition(cx, |_| gated.read_release.lock().unwrap().is_none());
        if close_window {
            assert_eq!(
                dispatch_product_command(
                    &window,
                    native_window,
                    super::super::CLOSE_WINDOW_COMMAND,
                    cx
                )
                .completion
                .await
                .unwrap(),
                CommandOutcome::Completed
            );
        } else {
            window.update(cx, |window, cx| {
                let workbench = window.workbench.read(cx);
                let pane = workbench.focused_pane().unwrap();
                let (pane, tab) = (pane.id(), pane.active_tab_id());
                window.workbench.update(cx, |workbench, cx| {
                    assert!(workbench.replace_tab_document(pane, tab, document, model.clone(), cx))
                });
                cx.notify();
            });
        }
        take_events(&host, cx);
        release.send(()).unwrap();
        assert_eq!(pending.await.unwrap(), CommandOutcome::InvalidTarget);
        assert!(changed_documents(&take_events(&host, cx)).is_empty());
        cx.read(|cx| {
            assert!(
                documents
                    .read(cx)
                    .document_for_resource(&ResourceUri::parse("mem://product/notes.txt").unwrap())
                    .is_none()
            )
        });
    }
}

#[gpui::test]
async fn reordering_retains_exact_targets_while_replacement_and_transfer_change_them(
    cx: &mut TestAppContext,
) {
    use crate::app::workbench::{TabMoveDestination, TabMovePlacement};
    use knot_protocol::WindowInfo;
    fn window_state(events: &[ApplicationEvent], handle: gpui::AnyWindowHandle) -> &WindowInfo {
        events
            .iter()
            .rev()
            .find_map(|event| match &event.change {
                ApplicationChange::WindowOpened { window }
                | ApplicationChange::PresentationsChanged { window }
                    if window.target.window == crate::app::product::test_window_id(handle) =>
                {
                    Some(window)
                }
                _ => None,
            })
            .unwrap()
    }
    let documents = install_globals(cx);
    let host = cx.update(extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model.clone(), cx);
    assert_eq!(
        dispatch_product_command(&window, native_window, super::super::NEW_COMMAND, cx)
            .completion
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    let initial_events = take_events(&host, cx);
    let initial = window_state(&initial_events, native_window);
    let original = initial.panes[0].tabs[0].clone();
    let window_ref = window.downgrade();
    let (source, destination) = cx.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        (
            window
                .capture_tab_relocation(pane.id(), pane.tabs()[0].id(), cx)
                .unwrap(),
            super::super::relocate::TabRelocationDestination {
                window: window_ref.clone(),
                workbench: window.workbench.downgrade(),
                tab: TabMoveDestination::At {
                    anchor: workbench
                        .capture_tab_move(pane.id(), pane.tabs()[1].id())
                        .unwrap(),
                    placement: TabMovePlacement::End,
                },
            },
        )
    });
    assert_eq!(
        cx.update_window(native_window, |_, _, cx| window.update(cx, |window, cx| {
            window.relocate_tab_to(&source, &destination, cx)
        }))
        .unwrap(),
        CommandOutcome::Completed
    );
    let reordered_events = take_events(&host, cx);
    let reordered = window_state(&reordered_events, native_window);
    assert_eq!(reordered.panes[0].tabs[1].target, original.target);
    assert_eq!(reordered.panes[0].tabs[1].buffer, original.buffer);
    assert!(changed_documents(&reordered_events).is_empty());
    let before_replacement = cx.read(|cx| current_open_target(&window, cx));
    window.update(cx, |window, cx| {
        assert!(before_replacement.matches_presentation(&window.workbench, cx));
        window.workbench.update(cx, |workbench, cx| {
            assert!(workbench.replace_tab_document(
                before_replacement.tab.unwrap().pane,
                before_replacement.tab.unwrap().tab,
                document,
                model.clone(),
                cx
            ))
        });
        cx.notify();
    });
    let replaced_events = take_events(&host, cx);
    let replacement = window_state(&replaced_events, native_window).panes[0].tabs[1].clone();
    assert_ne!(replacement.target, original.target);
    assert_eq!(replacement.document, original.document);
    assert_eq!(replacement.buffer, original.buffer);
    assert!(!cx.read(|cx| before_replacement.matches_presentation(&window.read(cx).workbench, cx)));
    assert!(changed_documents(&replaced_events).is_empty());
    assert_eq!(
        dispatch_product_command(
            &window,
            native_window,
            super::super::SPLIT_HORIZONTAL_COMMAND,
            cx
        )
        .completion
        .await
        .unwrap(),
        CommandOutcome::Completed
    );
    let old_target = cx.read(|cx| {
        let workbench = &window.read(cx).workbench;
        let first_pane = &workbench.read(cx).panes()[0];
        let tab = first_pane
            .tabs()
            .iter()
            .find(|tab| tab.document_id() == Some(document))
            .unwrap();
        OpenTarget {
            window: window.downgrade(),
            workbench: workbench.downgrade(),
            tab: Some(crate::app::commands::dispatch::TargetTab {
                pane: first_pane.id(),
                tab: tab.id(),
                surface: tab.surface_id(),
                presentation: tab.presentation(),
            }),
        }
    });
    let window_ref = window.downgrade();
    let (source, destination) = cx.read(|cx| {
        let window = window.read(cx);
        let workbench = window.workbench.read(cx);
        let target_pane = &workbench.panes()[1];
        (
            window
                .capture_tab_relocation(
                    old_target.tab.unwrap().pane,
                    old_target.tab.unwrap().tab,
                    cx,
                )
                .unwrap(),
            super::super::relocate::TabRelocationDestination {
                window: window_ref.clone(),
                workbench: window.workbench.downgrade(),
                tab: TabMoveDestination::At {
                    anchor: workbench
                        .capture_tab_move(target_pane.id(), target_pane.active_tab_id())
                        .unwrap(),
                    placement: TabMovePlacement::End,
                },
            },
        )
    });
    take_events(&host, cx);
    assert_eq!(
        cx.update_window(native_window, |_, _, cx| window.update(cx, |window, cx| {
            window.relocate_tab_to(&source, &destination, cx)
        }))
        .unwrap(),
        CommandOutcome::Completed
    );
    let transferred_events = take_events(&host, cx);
    let moved = window_state(&transferred_events, native_window)
        .panes
        .iter()
        .flat_map(|pane| &pane.tabs)
        .find(|tab| tab.target.presentation == replacement.target.presentation)
        .unwrap();
    assert_eq!(moved.buffer, original.buffer);
    assert_ne!(moved.target.pane, replacement.target.pane);
    assert!(!cx.read(|cx| old_target.matches_presentation(&window.read(cx).workbench, cx)));
    assert!(changed_documents(&transferred_events).is_empty());
}

#[gpui::test]
fn empty_workbench_and_clean_untitled_documents_keep_their_actual_metadata(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let host = cx.update(extensions::host::install);
    let handle = cx.update(open_tab_transfer_window).unwrap();
    let native_window = handle.into();
    let opened = take_events(&host, cx);
    assert!(opened.iter().any(|event| matches!(&event.change, ApplicationChange::WindowOpened { window } if window.panes.is_empty() && window.current_target.is_none())));
    assert!(changed_documents(&opened).is_empty());
    let window = cx
        .update_window(native_window, |_, native_window, _| {
            native_window.root::<KnotWindow>().flatten().unwrap()
        })
        .unwrap();
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("clean")));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Untitled", model.clone(), cx)
    });
    window.update(cx, |window, cx| {
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model.clone(), cx));
        cx.global::<ApplicationWindows>()
            .register(cx.weak_entity(), &workbench);
        window.workbench = workbench;
        cx.notify();
    });
    let filled = take_events(&host, cx);
    let info = changed_documents(&filled)
        .into_iter()
        .find(|document| document.kind == DocumentKind::Untitled)
        .unwrap();
    assert!(!info.dirty);
    assert!(info.resource_uri.is_none());
    let old_target = filled
        .iter()
        .rev()
        .find_map(|event| match &event.change {
            ApplicationChange::PresentationsChanged { window } => {
                Some(window.panes[0].tabs[0].target.clone())
            }
            _ => None,
        })
        .unwrap();
    let retained_workbench = cx.read(|cx| window.read(cx).workbench.clone());
    let old_command_target = cx
        .update_window(native_window, |_, _, cx| {
            window.update(cx, |window, cx| {
                window.focus_active_surface(cx);
                window.capture_command_target(cx)
            })
        })
        .unwrap();
    let close = dispatch_product_command(&window, native_window, CLOSE_TAB_COMMAND, cx);
    assert_eq!(wait_for_command(close, cx), CommandOutcome::Completed);
    let closed = take_events(&host, cx);
    assert!(closed.iter().any(|event| matches!(&event.change, ApplicationChange::DocumentClosed { document } if document == &info.id)));
    // The window is empty again, with no replacement document.
    assert!(changed_documents(&closed).is_empty());
    assert!(closed.iter().any(|event| matches!(&event.change,
        ApplicationChange::PresentationsChanged { window }
            if window.panes.is_empty() && window.target == old_target.pane.window)));
    cx.read(|cx| {
        assert_eq!(retained_workbench.read(cx).view_count(document), 0);
        assert_eq!(&window.read(cx).workbench, &retained_workbench);
        assert_eq!(
            old_command_target.validate_tab(cx),
            Err(CommandOutcome::InvalidTarget)
        );
    });
    assert!(!cx.read(|cx| model.read(cx).is_open()));
}

#[gpui::test]
fn deactivation_publishes_null_targets_without_repaint_events(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let host = cx.update(extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, visual) = cx.add_window_view(|_, cx| {
        let workbench = cx.new(|cx| Workbench::new_for_document(document, model, cx));
        KnotWindow::new(workbench, cx)
    });
    visual.update(|native_window, cx| {
        window.update(cx, |window, cx| {
            window.observe_extension_host(native_window, cx)
        })
    });
    visual.refresh().unwrap();
    visual.update(|native_window, cx| {
        native_window.activate_window();
        window.update(cx, |window, cx| window.focus_active_editor(cx));
    });
    let focused = take_events(&host, visual);
    assert!(focused.iter().any(|event| matches!(
        &event.change,
        ApplicationChange::ActiveTargetChanged {
            active_window: Some(_),
            active_target: Some(_)
        }
    )));
    visual.refresh().unwrap();
    assert!(take_events(&host, visual).is_empty());
    // Focus is the window's, so it keeps it when the native window loses focus.
    visual.update(|window, cx| window.blur(cx));
    assert!(take_events(&host, visual).is_empty());
    visual.deactivate_window();
    let deactivated = take_events(&host, visual);
    assert!(changed_documents(&deactivated).is_empty());
    assert!(deactivated.iter().any(|event| matches!(
        &event.change,
        ApplicationChange::ActiveTargetChanged {
            active_window: None,
            active_target: None
        }
    )));
}

#[gpui::test]
fn application_metadata_and_buffer_listeners_share_each_edit_once(cx: &mut TestAppContext) {
    use knot_protocol::ExtensionKey;
    let documents = install_globals(cx);
    let host = cx.update(extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(709), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, r#"
import { commands, workbench } from "knot";
const edits = [], metadata = [];
await workbench.observe(async event => {
  if (event.type === "documentOpened") await event.document.buffer.onDidChange(change => { edits.push([change.beforeRevision, change.revision]); });
  if (event.type === "documentChanged") metadata.push([event.document.revision, event.document.dirty]);
});
await commands.register("observation.barrier", () => {});
await commands.register("observation.stream", () => {
  if (JSON.stringify(edits) !== "[[0,1],[1,2]]") throw new Error(`buffer stream ${JSON.stringify(edits)}`);
  if (JSON.stringify(metadata) !== "[[1,true],[2,false]]") throw new Error(`metadata stream ${JSON.stringify(metadata)}`);
});
"#).unwrap();
        cx.spawn(async move |_, _| { execution.await.unwrap(); }).detach();
    });
    wait_for_api_condition(cx, |cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog()
            .borrow()
            .resolve("observation.stream")
            .is_ok()
    });
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let window = headless_window(document, model.clone(), cx);
    let native_window = Headless;
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&window, native_window, "observation.barrier", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    model.update(cx, |model, cx| {
        model.replace(0..0, "edit").unwrap();
        cx.notify();
    });
    model.update(cx, |model, cx| {
        model.undo().unwrap();
        cx.notify();
    });
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&window, native_window, "observation.stream", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
}

#[gpui::test]
fn installed_application_observer_tracks_zero_window_activation_and_native_commits(
    cx: &mut TestAppContext,
) {
    fn report(
        window: &Entity<KnotWindow>,
        native_window: gpui::AnyWindowHandle,
        cx: &mut TestAppContext,
    ) -> serde_json::Value {
        let execution =
            dispatch_product_command(window, native_window, "example.application-state", cx);
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        cx.read(|cx| {
            serde_json::from_str(&cx.read_from_clipboard().unwrap().text().unwrap()).unwrap()
        })
    }
    let root = tempfile::tempdir().unwrap();
    install_extension(
        root.path(),
        "@example/application-state",
        &[],
        &[("main.js", include_str!("fixtures/application_state.js"))],
    );
    install_extension(
        root.path(),
        "@example/broken-observer",
        &[],
        &[(
            "main.js",
            "import {workbench} from 'knot'; await workbench.observe(() => { throw new Error('failed activation listener ran'); }); throw new Error('activation failed');",
        )],
    );
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_installed_extensions(root.path().to_path_buf(), cx)
    });
    let startup = wait_for_extension_report(&host, cx);
    assert_eq!(
        startup.report.entries[0].result,
        super::super::super::extensions::load::LoadResult::Loaded
    );
    assert!(matches!(
        startup.report.entries[1].result,
        super::super::super::extensions::load::LoadResult::Failed(_)
    ));
    cx.read(|cx| assert_eq!(host.read(cx).application_observer_count(), 1));
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let id = cx.read(|cx| {
        documents
            .read(cx)
            .get(document)
            .unwrap()
            .identity()
            .to_string()
    });
    let (first, first_window) = product_window(document, model.clone(), cx);
    cx.update_window(first_window, |_, native_window, cx| {
        native_window.activate_window();
        first.update(cx, |window, cx| window.focus_active_editor(cx));
    })
    .unwrap();
    let initial = report(&first, first_window, cx);
    assert_eq!(initial["initial"]["documents"], 0);
    assert_eq!(initial["initial"]["windows"], 0);
    assert_eq!(initial["documents"].as_array().unwrap().len(), 1);
    assert!(
        initial["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["type"] == "documentOpened" && event["document"] == id)
    );
    model.update(cx, |model, cx| {
        model.replace(0..0, "edit").unwrap();
        cx.notify();
    });
    let edited = report(&first, first_window, cx);
    assert_eq!(edited["documents"][0]["dirty"], true);
    assert_eq!(edited["documents"][0]["revision"], 1);
    model.update(cx, |model, cx| {
        model.undo().unwrap();
        cx.notify();
    });
    let split = dispatch_product_command(
        &first,
        first_window,
        super::super::SPLIT_HORIZONTAL_COMMAND,
        cx,
    );
    assert_eq!(wait_for_command(split, cx), CommandOutcome::Completed);
    let (second, second_window) = product_window(document, model.clone(), cx);
    first.update(cx, |window, cx| {
        window.install_workspace(ResourceUri::parse("mem://observer/").unwrap(), cx);
        window.ensure_test_tool(false, cx);
    });
    let source = cx.read(|cx| {
        let window = first.read(cx);
        let workbench = window.workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        window
            .capture_tab_relocation(pane.id(), pane.active_tab_id(), cx)
            .unwrap()
    });
    let destination = cx.read(|cx| {
        let workbench = &second.read(cx).workbench;
        let pane = workbench.read(cx).focused_pane().unwrap();
        super::super::relocate::TabRelocationDestination {
            window: second.downgrade(),
            workbench: workbench.downgrade(),
            tab: crate::app::workbench::TabMoveDestination::At {
                anchor: workbench
                    .read(cx)
                    .capture_tab_move(pane.id(), pane.active_tab_id())
                    .unwrap(),
                placement: crate::app::workbench::TabMovePlacement::End,
            },
        }
    });
    assert_eq!(
        cx.update_window(first_window, |_, _, cx| first.update(cx, |window, cx| {
            window.relocate_tab_to(&source, &destination, cx)
        }))
        .unwrap(),
        CommandOutcome::Completed
    );
    let updated = report(&first, first_window, cx);
    let mut tabs = updated["windows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|window| {
            window["panes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pane| pane["tabs"].as_array().unwrap().len())
                .sum::<usize>()
        })
        .collect::<Vec<_>>();
    tabs.sort();
    assert_eq!(tabs, [2, 2]);
    assert_eq!(updated["documents"][0]["dirty"], false);
    assert_eq!(updated["documents"][0]["revision"], 2);
    assert_eq!(updated["windows"].as_array().unwrap().len(), 2);
    let events = updated["events"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event["type"] == "documentOpened" && event["document"] == id)
            .count(),
        1
    );
    for kind in [
        "documentChanged",
        "windowOpened",
        "presentationsChanged",
        "workspaceChanged",
        "activeTargetChanged",
    ] {
        assert!(
            events.iter().any(|event| event["type"] == kind),
            "missing {kind}: {events:?}"
        );
    }
    let mut previous = updated["initial"]["watermark"].as_u64().unwrap();
    for event in events {
        let sequence = event["sequence"].as_u64().unwrap();
        assert!(sequence > previous);
        previous = sequence;
    }
    let installed_root = updated["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|window| window["roots"].as_array().unwrap().first())
        .unwrap()["id"]
        .clone();
    first.update(cx, |window, cx| {
        window.install_workspace(ResourceUri::parse("mem://observer/").unwrap(), cx)
    });
    let retained = report(&first, first_window, cx);
    assert!(
        retained["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|window| window["generation"] == 2 && window["roots"][0]["id"] == installed_root)
    );
    first.update(cx, |window, cx| {
        window.replace_workspace_roots(Vec::new(), cx)
    });
    let cleared = report(&first, first_window, cx);
    assert!(
        cleared["windows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|window| window["roots"].as_array().unwrap().is_empty())
    );
    first.update(cx, |window, cx| {
        window.replace_workspace_roots(
            vec![
                ResourceUri::parse("mem://one/").unwrap(),
                ResourceUri::parse("mem://two/").unwrap(),
            ],
            cx,
        )
    });
    let multiple = report(&first, first_window, cx);
    assert!(
        multiple["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|window| {
                window["generation"] == 4
                    && window["roots"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|root| root["uri"].as_str().unwrap())
                        .eq(["mem://one/", "mem://two/"])
            })
    );
    first.update(cx, |window, cx| {
        window.install_workspace(ResourceUri::parse("mem://observer/").unwrap(), cx)
    });
    let reinstalled = report(&first, first_window, cx);
    assert!(
        reinstalled["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|window| window["generation"] == 5
                && window["roots"][0]["id"] != installed_root
                && !window["roots"].as_array().unwrap().is_empty())
    );
    for (window, native_window) in [(&second, second_window), (&first, first_window)] {
        let close = dispatch_product_command(
            window,
            native_window,
            super::super::CLOSE_WINDOW_COMMAND,
            cx,
        );
        assert_eq!(wait_for_command(close, cx), CommandOutcome::Completed);
    }
    let replacement = cx.update(|cx| create_untitled_document(&documents, cx));
    let replacement_model =
        cx.read(|cx| documents.read(cx).get(replacement).unwrap().model().clone());
    let (replacement_window, replacement_native_window) =
        product_window(replacement, replacement_model.clone(), cx);
    let closed = report(&replacement_window, replacement_native_window, cx);
    assert!(!cx.read(|cx| model.read(cx).is_open()));
    let events = closed["events"].as_array().unwrap();
    assert!(
        events
            .iter()
            .any(|event| event["type"] == "documentClosed" && event["document"] == id)
    );
    assert!(events.iter().any(|event| event["type"] == "windowClosed"));
    let stopped = dispatch_product_command(
        &replacement_window,
        replacement_native_window,
        "example.stop-application-observer",
        cx,
    );
    assert_eq!(wait_for_command(stopped, cx), CommandOutcome::Completed);
    cx.read(|cx| assert_eq!(host.read(cx).application_observer_count(), 0));
    let stopped_count = report(&replacement_window, replacement_native_window, cx)["events"]
        .as_array()
        .unwrap()
        .len();
    replacement_model.update(cx, |model, cx| {
        model.replace(0..0, "after disposal").unwrap();
        cx.notify();
    });
    assert_eq!(
        report(&replacement_window, replacement_native_window, cx)["events"]
            .as_array()
            .unwrap()
            .len(),
        stopped_count
    );
}

#[gpui::test]
fn application_listener_can_query_edit_fail_and_dispose_without_blocking_queued_roots(
    cx: &mut TestAppContext,
) {
    use knot_protocol::ExtensionKey;
    const SOURCE: &str = r#"
import { commands, documents, workbench } from "knot";
let count = 0, last = 0;
const failing = await workbench.observe(() => { throw new Error("isolated observer failure"); });
const observation = await workbench.observe(async (event) => {
  if (event.sequence <= observation.watermark || event.sequence <= last) throw new Error("observation order");
  last = event.sequence;
  if (event.type !== "documentChanged") return;
  count++;
  const current = await documents.get(event.document.id);
  const snapshot = await current.buffer.snapshot();
  await current.buffer.applyEdits([{ range: { startByteOffset: 0, endByteOffset: 0 }, text: "listener " }], { ifRevision: snapshot.revision });
  const disposal = observation.dispose();
  if (disposal !== observation.dispose()) throw new Error("disposal promise changed");
  await disposal;
});
await commands.register("observation.verify", async () => {
  if (count !== 1) throw new Error(`listener count ${count}`);
  if (!(await (await documents.list())[0].buffer.snapshot()).text.startsWith("listener ")) throw new Error("listener mutation missing");
  await failing.dispose();
});
await commands.register("observation.live", () => {});
await commands.register("observation.install-for-unload", async () => { await workbench.observe(() => {}); });
"#;
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(708), ExtensionLifecycleId::new(1));
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
            .resolve("observation.install-for-unload")
            .is_ok()
    });
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let window = headless_window(document, model.clone(), cx);
    let native_window = Headless;
    model.update(cx, |model, cx| {
        model.replace(0..0, "edit").unwrap();
        cx.notify();
    });
    let verify = dispatch_product_command(&window, native_window, "observation.verify", cx);
    assert_eq!(wait_for_command(verify, cx), CommandOutcome::Completed);
    cx.read(|cx| assert_eq!(host.read(cx).application_observer_count(), 0));
    let install =
        dispatch_product_command(&window, native_window, "observation.install-for-unload", cx);
    assert_eq!(wait_for_command(install, cx), CommandOutcome::Completed);
    cx.read(|cx| assert_eq!(host.read(cx).application_observer_count(), 1));
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
    cx.read(|cx| assert_eq!(host.read(cx).application_observer_count(), 0));
}

#[gpui::test]
fn application_queries_use_live_product_state_and_distinct_target_authority(
    cx: &mut TestAppContext,
) {
    use knot_protocol::ExtensionKey;
    const SOURCE: &str = r#"
import { commands, documents, windows, workspaces, editor } from "knot";
if ((await documents.list()).length || (await windows.list()).length) throw new Error("startup catalog not empty");
let savedId, savedWindows;
await commands.register("queries.catalog", async ({ target }) => {
  const docs = await documents.list();
  const wins = await windows.list();
  if (docs.length !== 1 || wins.length !== 2) throw new Error("catalog scope/dedup");
  const doc = docs[0];
  if (doc.kind !== "untitled" || doc.title !== "Clean" || doc.dirty || doc.resourceUri !== null || doc.revision !== 0) throw new Error("document metadata");
  if ((await doc.buffer.snapshot()).text !== "owned") throw new Error("document buffer");
  if ((await documents.get(doc.id)).buffer !== doc.buffer) throw new Error("document lookup");
  if (await documents.get("missing") !== null) throw new Error("missing identity");
  for (const win of wins) {
const pane = win.panes[0], tab = pane.tabs[0];
if (tab.document !== doc.id || tab.buffer !== doc.buffer || tab.kind !== "document") throw new Error("presentation association");
if (pane.selected !== tab.target || tab.target.pane !== pane.target || tab.target.window !== win.target) throw new Error("target identity");
const again = await windows.get(win.target);
if (again.panes[0].tabs[0].target !== tab.target) throw new Error("unstable target");
const workspace = await workspaces.get(win.target);
if (workspace.window !== win.target || workspace.roots.length !== 0 || workspace.generation !== 0) throw new Error("empty workspace");
for (const value of [docs, doc, wins, win, pane, pane.tabs, tab, tab.target, workspace, workspace.roots]) {
  if (!Object.isFrozen(value)) throw new Error("mutable snapshot");
}
  }
  for (const invalid of [target, {}, wins[0].panes[0].target, wins[0].panes[0].tabs[0].target]) {
try { await windows.get(invalid); } catch (error) {
  if (error.code === "InvalidTarget") continue;
  throw error;
}
throw new Error("invalid live window accepted");
  }
  try { await editor.activeBuffer(wins[0].panes[0].tabs[0].target); throw new Error("live target gained invocation authority"); }
  catch (error) { if (error.code !== "InvalidTarget") throw error; }
  savedId = doc.id; savedWindows = wins.map(win => win.target);
});
await commands.register("queries.closed", async () => {
  if (await documents.get(savedId) !== null) throw new Error("closed document catalogued");
  const values = await Promise.all(savedWindows.map(target => windows.get(target)));
  if (values.filter(value => value === null).length !== 1) throw new Error("closed window catalogued");
  const closed = savedWindows[values.findIndex(value => value === null)];
  if (await workspaces.get(closed) !== null) throw new Error("closed workspace catalogued");
});
await commands.register("queries.terminal", async () => {
  const wins = await windows.list();
  const terminal = wins.flatMap(win => win.panes.flatMap(pane => pane.tabs)).find(tab => tab.kind === "terminal");
  if (!terminal || terminal.buffer !== null || terminal.document !== null) throw new Error("terminal pretends to be a document");
  if (!wins.some(win => win.currentTarget === terminal.target)) throw new Error("terminal focus missing");
});
"#;
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(707), ExtensionLifecycleId::new(1));
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
            .resolve("queries.terminal")
            .is_ok()
    });
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("owned")));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Clean", model.clone(), cx)
    });
    let (first, first_window) = product_window(document, model.clone(), cx);
    let (second, second_window) = product_window(document, model, cx);
    let (_diagnostic, _) =
        cx.add_window_view(|_, cx| crate::app::tree_view::TreeView::new("diagnostic", "1", cx));
    for (window, native_window, command) in [
        (&first, first_window, "queries.catalog"),
        (&first, first_window, CLOSE_TAB_COMMAND),
        (&second, second_window, super::super::CLOSE_WINDOW_COMMAND),
        (&first, first_window, "queries.closed"),
        (&first, first_window, NEW_TERMINAL_COMMAND),
        (&first, first_window, "queries.terminal"),
    ] {
        let execution = dispatch_product_command(window, native_window, command, cx);
        assert_eq!(
            wait_for_command(execution, cx),
            CommandOutcome::Completed,
            "{command}"
        );
    }
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
}

#[gpui::test]
fn view_state_changes_coalesce_into_one_settled_event_per_window(cx: &mut TestAppContext) {
    use crate::app::editor::EditorViewState;
    let documents = install_globals(cx);
    let host = cx.update(extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    model.update(cx, |model, _| {
        model.replace(0..0, &"line\n".repeat(50)).unwrap()
    });
    let (window, native_window) = product_window(document, model, cx);
    let (_, other) = product_window(
        document,
        cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone()),
        cx,
    );
    cx.executor()
        .advance_clock(super::super::view_state::VIEW_STATE_DEBOUNCE * 2);
    take_events(&host, cx);
    let editor = cx.read(|cx| {
        window
            .read(cx)
            .workbench
            .read(cx)
            .focused_editor()
            .unwrap()
            .clone()
    });
    let settled = |cx: &mut TestAppContext| {
        take_events(&host, cx)
            .into_iter()
            .filter_map(|event| match event.change {
                ApplicationChange::ViewStateChanged { window } => Some(window.window),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    // Cursor moves inside the debounce restart it and report once settled.
    for caret in [5, 10, 15] {
        editor.update(cx, |editor, cx| {
            editor.editor().clone().update(cx, |editor, cx| {
                editor.restore_view_state(
                    EditorViewState {
                        anchor: caret,
                        caret,
                        top: 0,
                        scroll_x: 0.,
                        folds: Vec::new(),
                    },
                    cx,
                )
            })
        });
        cx.executor()
            .advance_clock(super::super::view_state::VIEW_STATE_DEBOUNCE / 2);
        assert!(settled(cx).is_empty());
    }
    cx.executor()
        .advance_clock(super::super::view_state::VIEW_STATE_DEBOUNCE);
    assert_eq!(
        settled(cx),
        [crate::app::product::test_window_id(native_window)]
    );
    // An unchanged layout reports nothing, and other windows stay quiet.
    window.update(cx, |_, cx| cx.notify());
    cx.executor()
        .advance_clock(super::super::view_state::VIEW_STATE_DEBOUNCE * 2);
    assert!(settled(cx).is_empty());
    // A focused-pane or dock-size change is view state too.
    window.update(cx, |window, cx| {
        window.workbench.update(cx, |workbench, _| {
            workbench.set_dock_size(crate::app::workbench::DockLocation::Bottom, 300.)
        });
        cx.notify();
    });
    cx.executor()
        .advance_clock(super::super::view_state::VIEW_STATE_DEBOUNCE * 2);
    assert_eq!(
        settled(cx),
        [crate::app::product::test_window_id(native_window)]
    );
    let _ = other;
}

#[gpui::test]
fn document_scoped_settings_publish_document_changed_and_end_with_the_document(
    cx: &mut TestAppContext,
) {
    use crate::app::settings::{ApplicationSettings, EDITOR_TAB_SIZE, SettingsScope};
    use knot_protocol::ExtensionKey;
    use knot_protocol::{IndentationInfo, IndentationStyle};
    let documents = install_globals(cx);
    let host = cx.update(extensions::host::install);
    let key = ExtensionKey::new(ExtensionId::new(710), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host
            .load_test_source(
                key,
                r#"
import { commands, documents, settings } from "knot";
await commands.register("indentation.write", async () => {
  const [document] = await documents.list();
  await settings.set("editor.insertSpaces", false, { document: document.id });
  await settings.set("editor.tabSize", 8, { document: document.id });
});
"#,
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
            .resolve("indentation.write")
            .is_ok()
    });
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let window = headless_window(document, model.clone(), cx);
    let native_window = Headless;
    take_events(&host, cx);
    assert_eq!(
        wait_for_command(
            dispatch_product_command(&window, native_window, "indentation.write", cx),
            cx
        ),
        CommandOutcome::Completed
    );
    let events = take_events(&host, cx);
    let last = changed_documents(&events).pop().expect("a document change");
    assert_eq!(
        last.indentation,
        IndentationInfo {
            style: IndentationStyle::Tabs,
            size: 8
        }
    );

    // Releasing a document drops its values; the open one keeps them.
    let other = cx.update(|cx| create_untitled_document(&documents, cx));
    let other_model = cx.read(|cx| documents.read(cx).get(other).unwrap().model().clone());
    let tab_size = |model: &Entity<Buffer>, cx: &mut TestAppContext| {
        cx.read(|cx| {
            let scope = SettingsScope {
                document: model.read(cx).document().map(|id| id.to_string()),
                ..Default::default()
            };
            cx.global::<ApplicationSettings>()
                .0
                .get(EDITOR_TAB_SIZE, &scope)
                .cloned()
        })
    };
    cx.update(|cx| {
        let scope = SettingsScope {
            document: other_model.read(cx).document().map(|id| id.to_string()),
            ..Default::default()
        };
        cx.global_mut::<ApplicationSettings>()
            .0
            .set(EDITOR_TAB_SIZE, 2.into(), scope)
            .unwrap();
    });
    assert_eq!(tab_size(&model, cx), Some(8.into()));
    assert_eq!(tab_size(&other_model, cx), Some(2.into()));
    cx.update(|cx| documents.update(cx, |documents, cx| documents.remove(other, cx)));
    assert_eq!(tab_size(&other_model, cx), Some(4.into()));
    assert_eq!(tab_size(&model, cx), Some(8.into()));
    host.update(cx, |host, cx| host.unload(key, cx).unwrap());
}
