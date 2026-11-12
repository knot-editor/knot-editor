//! Watching persisted documents' files, and reloading clean documents whose
//! file changed outside Knot.

use std::collections::HashMap;

use futures::StreamExt as _;
use gpui::{App, Entity, Global, Task};
use knot_model::{
    filesystem::{ResourceKind, ResourceVersion},
    open::{OpenResource, load_resource_kind},
    reload::{FileCheck, check_file},
    resource::ResourceUri,
    watch::WatchSubscription,
};

use super::ApplicationFileSystems;
use crate::app::{
    application_observation,
    documents::{ApplicationDocuments, DocumentDirty as _, DocumentState},
    model::Buffer,
    settings::{self, ApplicationSettings},
};
use knot_model::workbench::DocumentId;

struct DocumentWatch {
    uri: ResourceUri,
    _subscription: WatchSubscription,
    _task: Task<()>,
}

/// The watches on persisted documents' parent directories, one per document.
#[derive(Default)]
struct DocumentWatches {
    watches: HashMap<DocumentId, DocumentWatch>,
    enabled: bool,
}

impl Global for DocumentWatches {}

/// Follow the application's documents and `files.watchDocuments`. Idempotent.
pub(super) fn install(cx: &mut App) {
    if cx.has_global::<DocumentWatches>() {
        return;
    }
    cx.set_global(DocumentWatches::default());
    let signal = application_observation::signal(cx);
    cx.observe(&signal, |_, cx| sync(cx)).detach();
    cx.observe_global::<ApplicationSettings>(|cx| {
        let enabled = settings::boolean(settings::FILES_WATCH_DOCUMENTS, cx);
        let was_enabled =
            std::mem::replace(&mut cx.global_mut::<DocumentWatches>().enabled, enabled);
        sync(cx);
        if enabled && !was_enabled {
            recheck_all(cx);
        }
    })
    .detach();
    cx.global_mut::<DocumentWatches>().enabled =
        settings::boolean(settings::FILES_WATCH_DOCUMENTS, cx);
    sync(cx);
}

/// The resource of each document associated with one, to be watched.
fn persisted_documents(cx: &App) -> Vec<(DocumentId, ResourceUri)> {
    let Some(documents) = cx.try_global::<ApplicationDocuments>() else {
        return Vec::new();
    };
    documents
        .0
        .read(cx)
        .documents()
        .filter_map(|document| match document.state() {
            DocumentState::Persisted { uri, .. } | DocumentState::Destination { uri } => {
                Some((document.id(), uri.clone()))
            }
            DocumentState::Untitled { .. } => None,
        })
        .collect()
}

/// Make the watches match the persisted documents: one on each document's
/// parent directory, none when the setting is off.
fn sync(cx: &mut App) {
    let wanted = if settings::boolean(settings::FILES_WATCH_DOCUMENTS, cx) {
        persisted_documents(cx)
    } else {
        Vec::new()
    };
    let Some(providers) = cx
        .try_global::<ApplicationFileSystems>()
        .map(|g| g.0.clone())
    else {
        return;
    };
    let registry = cx
        .default_global::<crate::app::file_watch::ApplicationFileWatches>()
        .0
        .clone();
    cx.global_mut::<DocumentWatches>()
        .watches
        .retain(|id, watch| {
            wanted
                .iter()
                .any(|(wanted_id, uri)| wanted_id == id && *uri == watch.uri)
        });
    for (id, uri) in wanted {
        if cx.global::<DocumentWatches>().watches.contains_key(&id) {
            continue;
        }
        let Some(parent) = uri.parent() else {
            continue;
        };
        let (sender, mut batches) = futures::channel::mpsc::unbounded();
        let Ok(subscription) = registry.watch(
            &providers,
            parent.clone(),
            false,
            std::sync::Arc::new(move |batch| {
                let _ = sender.unbounded_send(batch);
            }),
        ) else {
            continue;
        };
        let file = uri.clone();
        let task = cx.spawn(async move |cx| {
            while let Some(batch) = batches.next().await {
                if batch
                    .iter()
                    .any(|changed| *changed == file || *changed == parent)
                {
                    cx.update(|cx| recheck(id, cx));
                }
            }
        });
        cx.global_mut::<DocumentWatches>().watches.insert(
            id,
            DocumentWatch {
                uri,
                _subscription: subscription,
                _task: task,
            },
        );
    }
}

/// Recheck every persisted document, as when the application becomes active.
pub(super) fn recheck_all(cx: &mut App) {
    if !settings::boolean(settings::FILES_WATCH_DOCUMENTS, cx) {
        return;
    }
    for (id, _) in persisted_documents(cx) {
        recheck(id, cx);
    }
}

/// What a recheck captured before it read the file, to revalidate after.
struct Snapshot {
    model: Entity<Buffer>,
    uri: ResourceUri,
    /// The recorded version; `None` for a document with no file yet.
    recorded: Option<ResourceVersion>,
    revision: u64,
    dirty: bool,
}

/// Read a document's file and reload, mark, or record what changed.
fn recheck(id: DocumentId, cx: &mut App) {
    let Some(snapshot) = snapshot(id, cx) else {
        return;
    };
    let Some(providers) = cx
        .try_global::<ApplicationFileSystems>()
        .map(|g| g.0.clone())
    else {
        return;
    };
    let uri = snapshot.uri.clone();
    cx.spawn(async move |cx| {
        let loaded = load_resource_kind(providers, uri, Some(ResourceKind::File)).await;
        cx.update(|cx| match loaded {
            Ok(OpenResource::File { text, version, .. }) => {
                apply(id, &snapshot, &text, version, cx)
            }
            Ok(OpenResource::Missing(_)) => deleted(id, &snapshot, cx),
            _ => {}
        });
    })
    .detach();
}

fn snapshot(id: DocumentId, cx: &App) -> Option<Snapshot> {
    let documents = cx.try_global::<ApplicationDocuments>()?.0.read(cx);
    let document = documents.get(id)?;
    let (uri, recorded) = match document.state() {
        DocumentState::Persisted { uri, version, .. } => (uri, Some(version.clone())),
        DocumentState::Destination { uri } => (uri, None),
        DocumentState::Untitled { .. } => return None,
    };
    Some(Snapshot {
        model: document.model().clone(),
        uri: uri.clone(),
        recorded,
        revision: document.model().read(cx).revision(),
        dirty: document.is_dirty(cx),
    })
}

/// Whether the document is still the one the snapshot read about.
fn unchanged_since(id: DocumentId, snapshot: &Snapshot, cx: &App) -> bool {
    cx.global::<ApplicationDocuments>()
        .0
        .read(cx)
        .get(id)
        .is_some_and(|document| {
            document.model() == &snapshot.model
                && document.state().persisted_version() == snapshot.recorded.as_ref()
        })
}

/// The file is gone: the document keeps its text and becomes dirty.
fn deleted(id: DocumentId, snapshot: &Snapshot, cx: &mut App) {
    if snapshot.recorded.is_none() || !unchanged_since(id, snapshot, cx) {
        return;
    }
    let documents = cx.global::<ApplicationDocuments>().0.clone();
    if documents.update(cx, |documents, _| documents.mark_deleted(id)) {
        application_observation::changed(cx);
    }
}

/// Record, reload, or mark what the read found, unless the document moved on
/// while it was read: edits that race the read win.
fn apply(id: DocumentId, snapshot: &Snapshot, text: &str, version: ResourceVersion, cx: &mut App) {
    if !unchanged_since(id, snapshot, cx) {
        return;
    }
    let documents = cx.global::<ApplicationDocuments>().0.clone();
    let model = &snapshot.model;
    let racing = model.read(cx).revision() != snapshot.revision;
    // A document with unsaved changes, or no file yet, is never reloaded
    // unasked; the user decides once the document is focused.
    let check = match &snapshot.recorded {
        Some(recorded) if !snapshot.dirty => {
            if racing {
                return;
            }
            check_file(recorded, &version, text, &model.read(cx).text())
        }
        Some(recorded) if *recorded == version => return,
        _ => FileCheck::Changed,
    };
    if check == FileCheck::Unchanged {
        return;
    }
    let reload_now = !snapshot.dirty && settings::boolean(settings::FILES_AUTO_RELOAD, cx);
    if check == FileCheck::Changed && !reload_now {
        if documents.update(cx, |documents, _| documents.mark_changed(id, version)) {
            application_observation::changed(cx);
            prompt_windows(cx);
        }
        return;
    }
    let title = documents
        .read(cx)
        .get(id)
        .map(|document| document.title().to_owned())
        .unwrap_or_default();
    documents.update(cx, |documents, cx| {
        let Some(capture) = documents.begin_persistence(id, model) else {
            return;
        };
        let resource = (title, snapshot.uri.clone());
        if check == FileCheck::VersionOnly {
            documents.finish_persistence(
                id,
                model,
                &capture,
                resource,
                (version, snapshot.revision),
                cx,
            );
        } else {
            documents.finish_reload(id, model, &capture, text, resource, version, cx);
        }
    });
}

/// Replace the document's text with its file's, as a line diff that Undo
/// takes back. Edits made while the file is read win.
pub(super) fn reload(id: DocumentId, cx: &mut App) {
    let Some(snapshot) = snapshot(id, cx) else {
        return;
    };
    let Some(providers) = cx
        .try_global::<ApplicationFileSystems>()
        .map(|g| g.0.clone())
    else {
        return;
    };
    let uri = snapshot.uri.clone();
    cx.spawn(async move |cx| {
        let loaded = load_resource_kind(providers, uri, Some(ResourceKind::File)).await;
        let Ok(OpenResource::File { text, version, .. }) = loaded else {
            return;
        };
        cx.update(|cx| {
            if !unchanged_since(id, &snapshot, cx)
                || snapshot.model.read(cx).revision() != snapshot.revision
            {
                return;
            }
            let documents = cx.global::<ApplicationDocuments>().0.clone();
            let title = documents
                .read(cx)
                .get(id)
                .map(|document| document.title().to_owned())
                .unwrap_or_default();
            documents.update(cx, |documents, cx| {
                if let Some(capture) = documents.begin_persistence(id, &snapshot.model) {
                    documents.finish_reload(
                        id,
                        &snapshot.model,
                        &capture,
                        &text,
                        (title, snapshot.uri.clone()),
                        version,
                        cx,
                    );
                }
            });
        });
    })
    .detach();
}

/// Ask each window whose focused document was marked changed on disk.
pub(super) fn prompt_windows(cx: &mut App) {
    let windows: Vec<_> = cx
        .global::<super::ApplicationWindows>()
        .0
        .borrow()
        .iter()
        .map(|open| open.window.clone())
        .collect();
    for window in windows {
        window
            .update(cx, |window, cx| window.prompt_external_change(cx))
            .ok();
    }
}

impl super::KnotWindow {
    /// Offer Reload or Keep for the focused document when its file changed
    /// outside Knot and it has not been asked about that version.
    pub(super) fn prompt_external_change(&mut self, cx: &mut gpui::Context<Self>) {
        use gpui::{AppContext as _, PromptLevel};

        let Some(native_window) = self.native_window else {
            return;
        };
        if !cx
            .update_window(native_window, |_, native_window, _| {
                native_window.is_window_active()
            })
            .unwrap_or(false)
        {
            return;
        }
        let Some(pane) = self.workbench.read(cx).focused_pane() else {
            return;
        };
        let crate::app::workbench::TabSurfaceId::Document(id) = pane.active_tab().surface_id()
        else {
            return;
        };
        if !Self::documents(cx).update(cx, |documents, _| documents.take_prompt(id)) {
            return;
        }
        let answer = self.prompt(
            PromptLevel::Warning,
            "The file changed outside Knot",
            Some("Reload it, replacing the text in Knot, or keep the text in Knot.".into()),
            vec![
                gpui::PromptButton::new("Reload"),
                gpui::PromptButton::cancel("Keep"),
            ],
            cx,
        );
        let task = cx.spawn(async move |_, cx| {
            if answer.await.unwrap_or(1) == 0 {
                cx.update(|cx| reload(id, cx));
            }
        });
        self.tasks.push(task);
    }
}
