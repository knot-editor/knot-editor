//! Save cleanup: trailing whitespace and the final newline.
use super::*;
use crate::app::documents::DocumentDirty;
use crate::app::settings::{
    ApplicationSettings, FILES_INSERT_FINAL_NEWLINE, FILES_TRIM_TRAILING_WHITESPACE, SettingsScope,
};

// The caret starts on the first line, which has nothing to trim.
const DIRTY: &str = "x\na  \nb\t\nc";
const CLEAN: &str = "x\na\nb\nc\n";

struct Saved {
    document: crate::app::documents::DocumentId,
    model: Entity<Buffer>,
    shell: Entity<KnotWindow>,
    window: gpui::AnyWindowHandle,
    provider: Arc<MemoryFileSystemProvider>,
    uri: ResourceUri,
}

fn saved_document(cx: &mut TestAppContext, model: BufferModel) -> Saved {
    let documents = install_globals(cx);
    cx.set_global(ApplicationSettings::default());
    let (filesystems, provider) = memory_filesystems_with_provider();
    cx.set_global(ApplicationFileSystems(filesystems));
    let uri = ResourceUri::parse("mem://product/created.txt").unwrap();
    let model = cx.new(|_| Buffer::new(model));
    let document = documents.update(cx, |documents, cx| {
        documents.create_destination("created.txt", model.clone(), uri.clone(), cx)
    });
    let (shell, window) = product_window(document, model.clone(), cx);
    Saved {
        document,
        model,
        shell,
        window,
        provider,
        uri,
    }
}

fn save(saved: &Saved, cx: &mut TestAppContext) {
    let execution = dispatch_product_command(&saved.shell, saved.window, SAVE_COMMAND, cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
}

fn written(saved: &Saved) -> Vec<u8> {
    pollster::block_on(saved.provider.read(saved.uri.clone()))
        .unwrap()
        .bytes
}

fn set(key: &str, value: serde_json::Value, scope: SettingsScope, cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.global_mut::<ApplicationSettings>()
            .0
            .set(key, value, scope)
            .unwrap()
    });
}

#[gpui::test]
fn save_writes_the_cleaned_text_and_undo_restores_it_as_unsaved(cx: &mut TestAppContext) {
    let saved = saved_document(cx, BufferModel::from_text(DIRTY));
    save(&saved, cx);
    assert_eq!(written(&saved), CLEAN.as_bytes());
    assert_eq!(cx.read(|cx| saved.model.read(cx).text()), CLEAN);
    let is_dirty = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            let documents = cx.global::<ApplicationDocuments>().0.read(cx);
            documents.get(saved.document).unwrap().is_dirty(cx)
        })
    };
    assert!(!is_dirty(cx));

    cx.update_window(saved.window, |_, _, cx| {
        let workbench = saved.shell.read(cx).workbench.read(cx);
        let pane = workbench.focused_pane().unwrap();
        let editor = workbench
            .editor(pane.active_tab())
            .unwrap()
            .read(cx)
            .editor()
            .clone();
        editor.update(cx, |editor, cx| {
            editor.execute_editing_command("editor.undo", cx)
        });
    })
    .unwrap();
    assert_eq!(cx.read(|cx| saved.model.read(cx).text()), DIRTY);
    assert!(is_dirty(cx));
}

#[gpui::test]
fn a_language_or_document_setting_can_opt_out(cx: &mut TestAppContext) {
    let saved = saved_document(cx, BufferModel::from_text(DIRTY));
    let language =
        cx.read(|cx| crate::app::languages::effective_language(saved.model.read(cx), cx));
    let scope = SettingsScope {
        language: Some(language),
        ..Default::default()
    };
    set(
        FILES_TRIM_TRAILING_WHITESPACE,
        "off".into(),
        scope.clone(),
        cx,
    );
    set(FILES_INSERT_FINAL_NEWLINE, false.into(), scope, cx);
    save(&saved, cx);
    assert_eq!(written(&saved), DIRTY.as_bytes());

    // A document's own value beats the language's.
    let document = cx.read(|cx| saved.model.read(cx).document()).unwrap();
    set(
        FILES_INSERT_FINAL_NEWLINE,
        true.into(),
        SettingsScope {
            document: Some(document.to_string()),
            ..Default::default()
        },
        cx,
    );
    save(&saved, cx);
    assert_eq!(written(&saved), b"x\na  \nb\t\nc\n");
}

#[gpui::test]
fn save_as_cleans_the_text_too(cx: &mut TestAppContext) {
    let saved = saved_document(cx, BufferModel::from_text(DIRTY));
    let uri = ResourceUri::parse("mem://product/elsewhere.txt").unwrap();
    cx.set_global(ApplicationSaveDialog(Arc::new(FixedSaveDialog(
        SaveDialogOutcome::Selected(uri.clone()),
    ))));
    let execution = dispatch_product_command(&saved.shell, saved.window, SAVE_AS_COMMAND, cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    let bytes = pollster::block_on(saved.provider.read(uri)).unwrap().bytes;
    assert_eq!(bytes, CLEAN.as_bytes());
}

#[gpui::test]
fn a_read_only_document_saves_unchanged(cx: &mut TestAppContext) {
    let saved = saved_document(cx, BufferModel::from_read_only_text(DIRTY));
    save(&saved, cx);
    assert_eq!(written(&saved), DIRTY.as_bytes());
}
