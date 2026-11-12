//! Native window hot exit: recording, startup restoration, and precedence.

use crate::app::documents::DocumentDirty;
use std::rc::Rc;

use super::*;
use crate::app::documents::DocumentId;
use crate::app::editor::EditorViewState;
use crate::app::window_record::{self, WindowRecord};
use crate::app::workbench::{DockLocation, SplitPlacement};

fn uri(path: &str) -> ResourceUri {
    ResourceUri::parse(&format!("mem://product/{path}")).unwrap()
}

/// A launch environment whose stores survive simulated restarts.
struct Launch {
    sessions: tempfile::TempDir,
    provider: Arc<MemoryFileSystemProvider>,
}

fn wait<T: 'static>(task: gpui::Task<T>, cx: &mut TestAppContext) -> T {
    let result = Rc::new(RefCell::new(None));
    let sink = result.clone();
    cx.spawn(async move |_| *sink.borrow_mut() = Some(task.await))
        .detach();
    cx.run_until_parked();
    result.borrow_mut().take().expect("task settled")
}

impl Launch {
    fn new(cx: &mut TestAppContext) -> Self {
        install_globals(cx);
        // Restarts reopen recovery from its file.
        install_file_recovery(cx);
        let (filesystems, provider) = memory_filesystems_with_provider();
        for directory in ["a", "b"] {
            provider.seed_directory(uri(directory)).unwrap();
        }
        provider.seed_file(uri("a/one.txt"), b"one").unwrap();
        provider.seed_file(uri("b/two.txt"), b"two").unwrap();
        cx.set_global(ApplicationFileSystems(filesystems));
        let launch = Self {
            sessions: tempfile::tempdir().unwrap(),
            provider,
        };
        launch.install_windows(cx);
        launch
    }

    fn path(&self) -> std::path::PathBuf {
        self.sessions.path().join("state.sqlite3")
    }

    fn install_windows(&self, cx: &mut TestAppContext) {
        let executor = cx.background_executor.clone();
        let database = crate::app::record_store::StateDatabase::open(self.path(), executor);
        cx.update(|cx| window_record::install(&database, cx));
    }

    /// Run product startup and wait until its windows are open.
    fn start(&self, requests: Vec<OpenRequest>, cx: &mut TestAppContext) {
        install_launch_gate(cx);
        cx.update(|cx| {
            super::super::finish_product_startup(Ok(()), cx);
            // Requests arriving while restoration stages wait for it.
            for request in requests {
                super::super::dispatch_launch_request(Some(request), cx);
            }
        });
        assert!(wait_for_product_startup(cx));
        cx.run_until_parked();
    }

    /// Quit as a hot exit, then start a fresh application on the same stores.
    fn restart(&self, requests: Vec<OpenRequest>, cx: &mut TestAppContext) {
        let drain = cx.update(|cx| crate::app::recovery::drain(cx));
        wait(drain, cx).unwrap();
        let recorded = cx.update(window_record::record_for_quit).unwrap();
        wait(recorded, cx).unwrap();
        self.relaunch(requests, cx);
    }

    /// Stop as a crash would, without recording, then start again.
    fn relaunch(&self, requests: Vec<OpenRequest>, cx: &mut TestAppContext) {
        self.reset(cx);
        self.start(requests, cx);
    }

    /// Stop as a crash would and install fresh application state on the same
    /// stores.
    fn reset(&self, cx: &mut TestAppContext) {
        cx.update(|cx| window_record::stop_for_test(cx));
        let recovery = cx.read(|cx| {
            cx.global::<crate::app::recovery::ApplicationRecovery>()
                .0
                .store()
                .flush()
        });
        wait(recovery, cx).unwrap();
        for window in cx.read(|cx| cx.windows()) {
            let _ = cx.update_window(window, |_, native_window, _| native_window.remove_window());
        }
        cx.run_until_parked();
        let documents = cx.new(|_| DocumentCollection::new());
        cx.set_global(ApplicationDocuments(documents));
        let path = cx.read(|cx| {
            cx.global::<TestRecoveryDir>()
                .0
                .path()
                .join("recovery.sqlite3")
        });
        let executor = cx.background_executor.clone();
        cx.set_global(crate::app::recovery::ApplicationRecovery(
            crate::app::recovery::RecoveryService::new(crate::app::recovery::RecoveryStore::open(
                path, executor,
            )),
        ));
        cx.set_global(ApplicationWindows::default());
        self.install_windows(cx);
    }

    fn open_file(
        &self,
        window: &Entity<KnotWindow>,
        path: &str,
        cx: &mut TestAppContext,
    ) -> DocumentId {
        let file = pollster::block_on(self.provider.read(uri(path))).unwrap();
        let text = String::from_utf8(file.bytes).unwrap();
        let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
        let document = cx.read(documents).update(cx, |documents, cx| {
            documents.create_persisted(
                path.to_owned(),
                model.clone(),
                uri(path),
                0,
                file.version,
                cx,
            )
        });
        window.update(cx, |window, cx| {
            window.workbench.update(cx, |workbench, cx| {
                workbench.open_document_in_center(document, model, cx)
            });
            cx.notify();
        });
        document
    }
}

fn documents(cx: &gpui::App) -> Entity<DocumentCollection> {
    cx.global::<ApplicationDocuments>().0.clone()
}

/// Product windows in creation order.
fn windows_by_creation(cx: &TestAppContext) -> Vec<(gpui::AnyWindowHandle, Entity<KnotWindow>)> {
    let mut windows = cx
        .read(|cx| cx.windows())
        .into_iter()
        .filter_map(|window| {
            let knot_window = window.downcast::<KnotWindow>()?;
            Some((window, cx.read(|cx| knot_window.entity(cx).unwrap())))
        })
        .collect::<Vec<_>>();
    windows.sort_by_key(|(_, window)| cx.read(|cx| window.read(cx).creation_order()));
    windows
}

fn texts(window: &Entity<KnotWindow>, cx: &TestAppContext) -> Vec<String> {
    cx.read(|cx| {
        let documents = cx.global::<ApplicationDocuments>().0.read(cx);
        window
            .read(cx)
            .workbench
            .read(cx)
            .tab_snapshots()
            .into_iter()
            .filter_map(|tab| match tab.surface {
                TabSurfaceId::Document(document) => documents
                    .get(document)
                    .map(|document| document.model().read(cx).text()),
                _ => None,
            })
            .collect()
    })
}

fn first_document(window: &Entity<KnotWindow>, cx: &TestAppContext) -> DocumentId {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let pane = workbench.area_panes(crate::app::workbench::Area::Center)[0];
        workbench.pane(pane).unwrap().tabs()[0]
            .document_id()
            .unwrap()
    })
}

fn edit(document: DocumentId, text: &str, cx: &mut TestAppContext) {
    let model = cx.read(|cx| {
        documents(cx)
            .read(cx)
            .get(document)
            .unwrap()
            .model()
            .clone()
    });
    model.update(cx, |model, cx| {
        let len = model.text().len();
        model.replace(len..len, text).unwrap();
        cx.notify();
    });
}

fn editors(
    window: &Entity<KnotWindow>,
    cx: &TestAppContext,
) -> Vec<Entity<crate::app::editor::Editor>> {
    cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        workbench
            .tabs()
            .filter_map(|tab| workbench.editor(tab))
            .map(|view| view.read(cx).editor().clone())
            .collect()
    })
}

fn record(cx: &mut TestAppContext) -> WindowRecord {
    cx.update(window_record::record);
    cx.read(window_record::current).unwrap()
}

/// What restoration reproduces: roots, arrangement, focus, and dock sizes.
fn arrangement(record: &WindowRecord) -> Vec<serde_json::Value> {
    record
        .windows
        .iter()
        .map(|window| {
            let mut layout = serde_json::to_value(&window.layout).unwrap();
            // Reopened clean files get new identities; dirty and untitled
            // documents keep theirs.
            forget_file_identities(&mut layout["center"]);
            serde_json::json!({
                "roots": window.roots,
                "center": layout["center"],
                "focused": layout["focused"],
                "docks": layout["docks"],
            })
        })
        .collect()
}

fn forget_file_identities(node: &mut serde_json::Value) {
    for tab in node["tabs"].as_array_mut().into_iter().flatten() {
        if tab["uri"].is_string() {
            tab["document"] = serde_json::Value::Null;
        }
    }
    for child in ["first", "second"] {
        if node.get(child).is_some() {
            forget_file_identities(&mut node[child]);
        }
    }
}

fn active(cx: &TestAppContext) -> Option<gpui::AnyWindowHandle> {
    cx.read(|cx| cx.active_window())
}

#[gpui::test]
fn several_windows_restore_with_splits_shared_views_docks_dirty_text_and_fresh_terminals(
    cx: &mut TestAppContext,
) {
    cx.set_global(crate::app::product::tools::TestToolInEveryWindow);
    let launch = Launch::new(cx);
    let text = format!("first\n{}", "line\n".repeat(300));
    launch
        .provider
        .seed_file(uri("a/long.txt"), text.as_bytes())
        .unwrap();
    launch.start(Vec::new(), cx);
    let (first_window, first) = windows_by_creation(cx).remove(0);
    first.update(cx, |window, cx| {
        window.replace_workspace_roots(vec![uri("a"), uri("b")], cx);
    });
    // An untitled document holds unsaved text, so recovery restores it too.
    first.update(cx, |window, cx| window.new_document_in_pane(None, cx));
    edit(first_document(&first, cx), "scratch", cx);
    let long = launch.open_file(&first, "a/long.txt", cx);
    edit(long, "dirty", cx);
    let terminal = first.update(cx, |window, cx| {
        let pane = window.workbench.read(cx).center_pane().unwrap();
        let (terminal, view) = window.create_terminal(cx);
        window.workbench.update(cx, |workbench, cx| {
            let split = workbench
                .split_pane(pane, SplitDirection::Vertical, SplitPlacement::After, cx)
                .unwrap();
            workbench.open_terminal_tab(split, terminal, view);
            workbench.set_split_ratio(split, 0.3);
            workbench.set_dock_size(DockLocation::Left, 280.);
        });
        terminal
    });
    let views = editors(&first, cx);
    assert_eq!(views.len(), 3);
    #[allow(clippy::single_range_in_vec_init)]
    let state = EditorViewState {
        anchor: 6,
        caret: 16,
        top: 6,
        scroll_x: 0.,
        // Lines 5 through 7 of the document, collapsed.
        folds: vec![26..40],
    };
    views[1].update(cx, |editor, cx| {
        editor.restore_view_state(state.clone(), cx)
    });

    let second = cx.update(|cx| {
        let handle = super::super::launch::open_product_window_with(None, None, cx);
        handle.downcast::<KnotWindow>().unwrap().entity(cx).unwrap()
    });
    second.update(cx, |window, cx| {
        window.replace_workspace_roots(vec![uri("b")], cx);
    });
    second.update(cx, |window, cx| {
        window.ensure_test_tool(true, cx);
        let tool = window.test_tool_frame(cx).unwrap();
        window.workbench.update(cx, |workbench, _| {
            workbench.show_tool(tool);
        });
        window.new_document_in_pane(None, cx)
    });
    edit(first_document(&second, cx), "notes", cx);
    launch.open_file(&second, "b/two.txt", cx);
    cx.update_window(first_window, |_, native_window, _| {
        native_window.activate_window()
    })
    .unwrap();
    cx.run_until_parked();
    let before = record(cx);
    assert_eq!(before.windows.len(), 2);
    assert_eq!(before.active, Some(0));
    // The split's ratio is recorded with the layout and restored with it.
    let recorded = serde_json::to_string(&before.windows[0].layout).unwrap();
    assert!(recorded.contains(r#""ratio":0.3"#), "{recorded}");
    let identity = cx.read(|cx| documents(cx).read(cx).get(long).unwrap().identity());

    // A contributed tool whose package fails at the next launch is pruned.
    let mut failing = before.clone();
    let mut layout = serde_json::to_value(&failing.windows[1].layout).unwrap();
    layout["docks"]["left"]["root"]["tabs"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "kind": "tool", "tool": "@example/failed.view" }));
    failing.windows[1].layout = serde_json::from_value(layout).unwrap();

    launch.restart(Vec::new(), cx);
    let after = record(cx);
    assert_eq!(arrangement(&after), arrangement(&before));
    assert_eq!(after.active, Some(0));
    let restored = windows_by_creation(cx);
    assert_eq!(restored.len(), 2);
    let (restored_window, first) = &restored[0];
    assert!(active(cx) == Some(*restored_window));
    // The dirty document keeps its identity, its text, and one shared model.
    cx.read(|cx| {
        let documents = documents(cx);
        let documents = documents.read(cx);
        let document = documents.document_by_identity(identity).unwrap();
        assert_eq!(document.model().read(cx).text(), format!("{text}dirty"));
        assert!(document.is_dirty(cx));
        assert_eq!(documents.documents().count(), 4);
    });
    let views = editors(first, cx);
    cx.read(|cx| {
        assert_eq!(views[1].read(cx).view_state(cx), state);
        assert_eq!(views[1].read(cx).model(), views[2].read(cx).model());
    });
    // The terminal pane restored in place with a fresh window.
    cx.read(|cx| {
        let terminals = first
            .read(cx)
            .workbench
            .read(cx)
            .tab_snapshots()
            .into_iter()
            .filter_map(|tab| match tab.surface {
                TabSurfaceId::Terminal(id) => Some(id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(terminals.len(), 1);
        assert_ne!(terminals[0], terminal);
    });
    assert_eq!(texts(&restored[1].1, cx), ["notes", "two"]);

    // Quitting and restarting again produces the same windows, and the
    // record's unknown tool never reaches a window.
    cx.update(|cx| {
        cx.global::<window_record::ApplicationWindowRecord>()
            .publish_for_test(failing)
    });
    let flush = cx.read(window_record::flush);
    wait(flush, cx).unwrap();
    launch.relaunch(Vec::new(), cx);
    assert_eq!(arrangement(&record(cx)), arrangement(&before));
}

#[gpui::test]
fn queued_requests_open_after_restoration_without_duplicating_documents(cx: &mut TestAppContext) {
    let launch = Launch::new(cx);
    launch.start(Vec::new(), cx);
    let (_, window) = windows_by_creation(cx).remove(0);
    window.update(cx, |window, cx| {
        window.replace_workspace_roots(vec![uri("a")], cx);
    });
    launch.open_file(&window, "a/one.txt", cx);
    record(cx);

    let file = OpenRequest::from_uri_for_product(uri("a/one.txt"));
    let folder = OpenRequest::from_uri_for_product(uri("b"));
    launch.restart(vec![file, folder], cx);
    let windows = windows_by_creation(cx);
    assert_eq!(windows.len(), 3);
    // The restored window comes first and the last request window is focused.
    assert_eq!(texts(&windows[0].1, cx), ["one"]);
    assert!(active(cx) == Some(windows[2].0));
    assert_eq!(
        cx.read(|cx| windows[2]
            .1
            .read(cx)
            .workspace
            .snapshot()
            .first_root()
            .map(ToString::to_string)),
        Some("mem://product/b".to_owned())
    );
    cx.read(|cx| {
        let documents = documents(cx);
        let documents = documents.read(cx);
        assert_eq!(
            documents
                .documents()
                .filter(|document| document.resource_uri() == Some(&uri("a/one.txt")))
                .count(),
            1
        );
    });

    // A reopen after launch opens one empty window and restores nothing.
    cx.update(|cx| super::super::dispatch_launch_request(None, cx));
    cx.run_until_parked();
    let windows = windows_by_creation(cx);
    assert_eq!(windows.len(), 4);
    assert!(texts(&windows[3].1, cx).is_empty());
}

#[gpui::test]
fn changed_files_clamp_view_state_and_missing_clean_files_are_skipped(cx: &mut TestAppContext) {
    let launch = Launch::new(cx);
    launch
        .provider
        .seed_file(uri("a/long.txt"), "x\n".repeat(400).as_bytes())
        .unwrap();
    launch
        .provider
        .seed_file(uri("a/gone.txt"), b"gone")
        .unwrap();
    launch.start(Vec::new(), cx);
    let (_, window) = windows_by_creation(cx).remove(0);
    launch.open_file(&window, "a/gone.txt", cx);
    launch.open_file(&window, "a/long.txt", cx);
    let view = editors(&window, cx).pop().unwrap();
    view.update(cx, |editor, cx| {
        editor.restore_view_state(
            EditorViewState {
                anchor: 700,
                caret: 700,
                top: 600,
                scroll_x: 0.,
                folds: Vec::new(),
            },
            cx,
        )
    });
    record(cx);

    launch
        .provider
        .seed_file(uri("a/long.txt"), "é\n".repeat(10).as_bytes())
        .unwrap();
    launch.provider.remove(uri("a/gone.txt")).unwrap();
    launch.restart(Vec::new(), cx);
    let (_, window) = windows_by_creation(cx).remove(0);
    assert_eq!(texts(&window, cx), ["é\n".repeat(10)]);
    let view = editors(&window, cx).pop().unwrap();
    let state = cx.read(|cx| view.read(cx).view_state(cx));
    assert_eq!(state.caret, "é\n".repeat(10).len());
    assert!(state.top <= state.caret);
}

#[gpui::test]
fn lost_unsupported_and_consumed_records_open_the_default_window(cx: &mut TestAppContext) {
    let launch = Launch::new(cx);
    launch.start(Vec::new(), cx);
    let (_, window) = windows_by_creation(cx).remove(0);
    launch.open_file(&window, "a/one.txt", cx);
    record(cx);
    let flush = cx.read(window_record::flush);
    wait(flush, cx).unwrap();

    // A restore interrupted after the record was marked consumed restores
    // nothing at the next launch.
    let taken = cx.read(window_record::take_for_restore);
    assert!(wait(taken, cx).is_some());
    launch.relaunch(Vec::new(), cx);
    let windows = windows_by_creation(cx);
    assert_eq!(windows.len(), 1);
    assert!(texts(&windows[0].1, cx).is_empty());

    // An unreadable record behaves like a missing one.
    std::fs::write(launch.path(), "not a database").unwrap();
    launch.relaunch(Vec::new(), cx);
    assert_eq!(windows_by_creation(cx).len(), 1);
    // A record from an unsupported version is ignored.
    let connection = rusqlite::Connection::open(launch.path().with_extension("v99")).unwrap();
    connection
        .execute_batch(&crate::app::record_store::test_schema("window_session"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO window_session(state, version, payload) VALUES (0, 99, '{}')",
            [],
        )
        .unwrap();
    drop(connection);
    std::fs::rename(launch.path().with_extension("v99"), launch.path()).unwrap();
    launch.relaunch(Vec::new(), cx);
    assert_eq!(windows_by_creation(cx).len(), 1);
}

#[gpui::test]
fn closing_a_window_removes_it_and_quitting_without_windows_restores_none(cx: &mut TestAppContext) {
    let launch = Launch::new(cx);
    launch.start(Vec::new(), cx);
    let (window, _) = windows_by_creation(cx).remove(0);
    cx.update_window(window, |_, native_window, _| native_window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(cx.read(window_record::current).unwrap().windows.is_empty());
    launch.restart(Vec::new(), cx);
    // With nothing recorded, the default window opens.
    assert_eq!(windows_by_creation(cx).len(), 1);
}

#[gpui::test]
fn a_failed_config_launch_never_records_windows(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("pre-init.js"),
        "throw new Error('pre failed');",
    )
    .unwrap();
    let launch = Launch::new(cx);
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
    assert_eq!(cx.read(|cx| cx.windows().len()), 1);
    cx.update(window_record::record);
    if let Some(flush) = cx.update(window_record::record_for_quit) {
        wait(flush, cx).unwrap();
    }
    assert!(cx.read(window_record::current).is_none());
    assert!(!launch.path().exists());
}

#[gpui::test]
fn native_windows_and_recovery_restore_when_an_extension_fails_at_startup(cx: &mut TestAppContext) {
    let launch = Launch::new(cx);
    launch.start(Vec::new(), cx);
    let (_, window) = windows_by_creation(cx).remove(0);
    let one = launch.open_file(&window, "a/one.txt", cx);
    edit(one, " unsaved", cx);
    let drain = cx.update(|cx| crate::app::recovery::drain(cx));
    wait(drain, cx).unwrap();
    let recorded = cx.update(window_record::record_for_quit).unwrap();
    wait(recorded, cx).unwrap();

    launch.reset(cx);
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    install_extension(
        extensions.path(),
        "@example/broken",
        &[],
        &[("main.js", "throw new Error('package failed');")],
    );
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
    cx.run_until_parked();
    let windows = windows_by_creation(cx);
    assert_eq!(windows.len(), 1);
    assert_eq!(texts(&windows[0].1, cx), ["one unsaved"]);
}

#[gpui::test]
fn restored_documents_have_their_languages_before_their_window_exists(cx: &mut TestAppContext) {
    let launch = Launch::new(cx);
    launch
        .provider
        .seed_file(uri("a/main.rs"), b"fn main() {}\n")
        .unwrap();
    launch.start(Vec::new(), cx);
    let (_, window) = windows_by_creation(cx).remove(0);
    launch.open_file(&window, "a/main.rs", cx);
    let recorded = cx.update(window_record::record_for_quit).unwrap();
    wait(recorded, cx).unwrap();

    launch.reset(cx);
    let extensions = tempfile::tempdir().unwrap();
    install_extension(
        extensions.path(),
        "@example/rust",
        &[],
        &[(
            "main.js",
            r#"import { languages } from "knot";
await languages.register({
  id: "rust", name: "Rust", grammar: "rust", fileExtensions: ["rs"],
  queries: { highlights: "(identifier) @variable" },
});"#,
        )],
    );
    // Each restored window records its documents' languages as it is
    // created, before it can paint.
    let created = Rc::new(RefCell::new(Vec::new()));
    let sink = created.clone();
    let _observer = cx.update(|cx| {
        cx.observe_new(move |window: &mut KnotWindow, _, cx| {
            let documents = cx.global::<ApplicationDocuments>().0.read(cx);
            sink.borrow_mut().extend(
                window
                    .workbench
                    .read(cx)
                    .tab_snapshots()
                    .into_iter()
                    .filter_map(|tab| match tab.surface {
                        TabSurfaceId::Document(document) => documents.get(document),
                        _ => None,
                    })
                    .map(|document| {
                        let model = document.model().read(cx);
                        (
                            document.title().to_string(),
                            model.language().map(str::to_owned),
                            model.syntax_generation().is_some(),
                        )
                    }),
            );
        })
    });
    install_launch_gate(cx);
    let config = tempfile::tempdir().unwrap();
    let host = cx.update(super::super::super::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Ok(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    assert!(
        created
            .borrow()
            .contains(&("main.rs".into(), Some("rust".into()), true)),
        "{:?}",
        created.borrow()
    );
}

#[gpui::test]
fn an_explicit_language_survives_hot_exit(cx: &mut TestAppContext) {
    let launch = Launch::new(cx);
    launch.start(Vec::new(), cx);
    let (_, window) = windows_by_creation(cx).remove(0);
    let document = launch.open_file(&window, "a/one.txt", cx);
    let model = cx.read(|cx| {
        documents(cx)
            .read(cx)
            .get(document)
            .unwrap()
            .model()
            .clone()
    });
    // An unregistered ID is kept, so restoration needs no registration.
    cx.update(|cx| {
        crate::app::languages::set_document_language(
            &model,
            Some(&uri("a/one.txt")),
            Some("cobol".into()),
            cx,
        )
    });
    launch.restart(Vec::new(), cx);

    let (_, window) = windows_by_creation(cx).remove(0);
    let document = first_document(&window, cx);
    cx.read(|cx| {
        let model = documents(cx)
            .read(cx)
            .get(document)
            .unwrap()
            .model()
            .read(cx);
        assert_eq!(model.language(), Some("cobol"));
        assert!(model.language_is_explicit());
    });
}

#[gpui::test]
fn closing_the_last_window_quits_and_restores_it_where_that_quits(cx: &mut TestAppContext) {
    let launch = Launch::new(cx);
    launch.start(Vec::new(), cx);
    cx.update(crate::app::extensions::host::install);
    cx.update(super::super::launch::quit_with_last_window);
    let (_, window) = windows_by_creation(cx).remove(0);
    let run = |name: &'static str, cx: &mut TestAppContext| {
        window.update(cx, |window, cx| {
            window.dispatch_source(ProductCommandSource::new(name).command(), cx)
        });
        cx.run_until_parked();
    };
    run(NEW_COMMAND, cx);
    let model = cx.read(|cx| {
        let workbench = window.read(cx).workbench.clone();
        let editor = workbench.read(cx).focused_editor().unwrap();
        editor.read(cx).model().clone()
    });
    model.update(cx, |model, _| model.replace(0..0, "unsaved").unwrap());

    run(CLOSE_WINDOW_COMMAND, cx);
    assert!(!cx.has_pending_prompt());
    let recorded = cx.read(window_record::current).unwrap();
    assert_eq!(recorded.windows.len(), 1);

    launch.relaunch(Vec::new(), cx);
    let (_, window) = windows_by_creation(cx).remove(0);
    let text = cx.read(|cx| {
        let workbench = window.read(cx).workbench.read(cx);
        let editor = workbench.focused_editor().unwrap();
        editor.read(cx).model().read(cx).text()
    });
    assert_eq!(text, "unsaved");
}
