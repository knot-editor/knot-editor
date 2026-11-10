//! Watching persisted documents and reloading clean ones from changed files.
use super::editor_api::active_editor;
use super::*;
use crate::app::documents::DocumentDirty;
use crate::app::product::document_watch;
use crate::app::settings::{FILES_WATCH_DOCUMENTS, set_boolean};

const TEXT: &str = "one\ntwo\n    three\n    four\nfive\n";

struct Fixture {
    provider: Arc<MemoryFileSystemProvider>,
    uri: ResourceUri,
    model: Entity<Buffer>,
    document: super::super::DocumentId,
    documents: Entity<DocumentCollection>,
    shell: Entity<ProductShell>,
    window: gpui::AnyWindowHandle,
}

async fn fixture(cx: &mut TestAppContext) -> Fixture {
    let documents = install_globals(cx);
    cx.set_global(save_without_cleanup());
    let (filesystems, provider) = memory_filesystems_with_provider();
    cx.set_global(ApplicationFileSystems(filesystems));
    let uri = ResourceUri::parse("mem://product/notes.txt").unwrap();
    provider.seed_file(uri.clone(), TEXT).unwrap();
    let opened = provider.read(uri.clone()).await.unwrap();
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(TEXT)));
    let document = documents.update(cx, |documents, cx| {
        documents.create_persisted(
            "notes.txt",
            model.clone(),
            uri.clone(),
            0,
            opened.version,
            cx,
        )
    });
    let (shell, window) = product_window(document, model.clone(), cx);
    cx.run_until_parked();
    Fixture {
        provider,
        uri,
        model,
        document,
        documents,
        shell,
        window,
    }
}

impl Fixture {
    fn text(&self, cx: &mut TestAppContext) -> String {
        self.model.read_with(cx, |model, _| model.text())
    }

    fn dirty(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| {
            self.documents
                .read(cx)
                .get(self.document)
                .unwrap()
                .is_dirty(cx)
        })
    }

    fn version(&self, cx: &mut TestAppContext) -> ResourceVersion {
        cx.read(|cx| {
            self.documents
                .read(cx)
                .get(self.document)
                .unwrap()
                .state()
                .persisted_version()
                .unwrap()
                .clone()
        })
    }

    /// Give event batches, which arrive from another thread, time to land.
    fn settle(&self, cx: &mut TestAppContext) {
        std::thread::sleep(std::time::Duration::from_millis(400));
        cx.run_until_parked();
    }
}

#[gpui::test]
async fn external_write_reloads_a_clean_document_in_place_as_one_undo_step(
    cx: &mut TestAppContext,
) {
    let f = fixture(cx).await;
    assert_eq!(f.provider.watch_count(), 1);
    let editor = active_editor(&f.shell, cx);
    let caret = TEXT.find("five").unwrap();
    editor.update(cx, |editor, cx| {
        editor.set_selection(caret, caret, crate::app::editor::Reveal::Nearest, cx);
        assert!(editor.fold(2..4, cx));
    });
    let folded = cx.read(|cx| editor.read(cx).fold_regions(cx));
    assert_eq!(folded.len(), 1);
    let recorded = f.version(cx);

    let changed = "zero\none\ntwo\n    three\n    four\nfive\n";
    f.provider.seed_file(f.uri.clone(), changed).unwrap();
    wait_for_api_condition(cx, |cx| f.model.read(cx).text() == changed);

    assert!(!f.dirty(cx));
    assert_ne!(f.version(cx), recorded);
    cx.read(|cx| {
        assert_eq!(editor.read(cx).selection(cx), (caret + 5, caret + 5));
        let shifted = folded[0].start + 1..folded[0].end + 1;
        assert_eq!(&*editor.read(cx).fold_regions(cx), &[shifted]);
    });
    f.model.update(cx, |model, _| model.undo().unwrap());
    assert_eq!(f.text(cx), TEXT);
    assert!(f.dirty(cx));
}

#[gpui::test]
async fn knots_own_save_triggers_nothing(cx: &mut TestAppContext) {
    let f = fixture(cx).await;
    f.model
        .update(cx, |model, _| model.replace(0..0, "local ").unwrap());
    let execution = dispatch_product_command(&f.shell, f.window, SAVE_COMMAND, cx);
    assert_eq!(
        execution.completion.await.unwrap(),
        CommandOutcome::Completed
    );
    let revision = f.model.read_with(cx, |model, _| model.revision());

    f.settle(cx);

    assert_eq!(f.model.read_with(cx, |model, _| model.revision()), revision);
    assert_eq!(f.text(cx), format!("local {TEXT}"));
    assert!(!f.dirty(cx));
}

#[gpui::test]
async fn a_same_content_write_only_records_the_version(cx: &mut TestAppContext) {
    let f = fixture(cx).await;
    let revision = f.model.read_with(cx, |model, _| model.revision());
    let recorded = f.version(cx);

    f.provider.seed_file(f.uri.clone(), TEXT).unwrap();
    wait_for_api_condition(cx, |cx| {
        f.documents
            .read(cx)
            .get(f.document)
            .unwrap()
            .state()
            .persisted_version()
            != Some(&recorded)
    });

    assert_eq!(f.model.read_with(cx, |model, _| model.revision()), revision);
    assert!(!f.dirty(cx));
}

#[gpui::test]
async fn activation_recheck_reloads_a_document_whose_event_was_missed(cx: &mut TestAppContext) {
    let f = fixture(cx).await;
    f.provider.seed_file(f.uri.clone(), "changed\n").unwrap();

    cx.update(document_watch::recheck_all);
    cx.run_until_parked();

    assert_eq!(f.text(cx), "changed\n");
    assert!(!f.dirty(cx));
}

#[gpui::test]
async fn an_edit_racing_the_read_wins(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let root = ResourceUri::parse("mem://product/").unwrap();
    let provider = Arc::new(MemoryFileSystemProvider::new(root).unwrap());
    let uri = ResourceUri::parse("mem://product/notes.txt").unwrap();
    provider.seed_file(uri.clone(), TEXT).unwrap();
    let opened = provider.read(uri.clone()).await.unwrap();
    provider.seed_file(uri.clone(), "external\n").unwrap();
    let (read_release, gated_read) = tokio::sync::oneshot::channel();
    let mut filesystems = FileSystemProviderRegistry::default();
    filesystems
        .register(
            "mem",
            Arc::new(GatedFileSystemProvider {
                inner: provider,
                read_release: Mutex::new(Some(gated_read)),
                replace_release: Mutex::new(None),
            }),
        )
        .unwrap();
    cx.set_global(ApplicationFileSystems(Arc::new(filesystems)));
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(TEXT)));
    let document = documents.update(cx, |documents, cx| {
        documents.create_persisted("notes.txt", model.clone(), uri, 0, opened.version, cx)
    });
    let _window = product_window(document, model.clone(), cx);

    cx.update(document_watch::recheck_all);
    cx.run_until_parked();
    model.update(cx, |model, _| model.replace(0..0, "newer ").unwrap());
    read_release.send(()).unwrap();
    cx.run_until_parked();

    assert_eq!(
        model.read_with(cx, |model, _| model.text()),
        format!("newer {TEXT}")
    );
}

#[gpui::test]
async fn turning_watch_documents_off_releases_watches_and_on_rechecks(cx: &mut TestAppContext) {
    let f = fixture(cx).await;
    assert_eq!(f.provider.watch_count(), 1);

    cx.update(|cx| set_boolean(FILES_WATCH_DOCUMENTS, false, cx));
    cx.run_until_parked();
    assert_eq!(f.provider.watch_count(), 0);
    f.provider.seed_file(f.uri.clone(), "changed\n").unwrap();
    cx.update(document_watch::recheck_all);
    f.settle(cx);
    assert_eq!(f.text(cx), TEXT);

    cx.update(|cx| set_boolean(FILES_WATCH_DOCUMENTS, true, cx));
    cx.run_until_parked();
    assert_eq!(f.provider.watch_count(), 1);
    assert_eq!(f.text(cx), "changed\n");
}
