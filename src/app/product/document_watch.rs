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

/// A persisted document's resource, for the documents that should be watched.
fn persisted_documents(cx: &App) -> Vec<(DocumentId, ResourceUri)> {
    let Some(documents) = cx.try_global::<ApplicationDocuments>() else {
        return Vec::new();
    };
    documents
        .0
        .read(cx)
        .documents()
        .filter_map(|document| match document.state() {
            DocumentState::Persisted { uri, .. } => Some((document.id(), uri.clone())),
            _ => None,
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
    recorded: ResourceVersion,
    revision: u64,
}

/// Read a clean document's file and reload the document if it changed.
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
        if let Ok(OpenResource::File { text, version, .. }) = loaded {
            cx.update(|cx| apply(id, &snapshot, &text, version, cx));
        }
    })
    .detach();
}

fn snapshot(id: DocumentId, cx: &App) -> Option<Snapshot> {
    let documents = cx.try_global::<ApplicationDocuments>()?.0.read(cx);
    let document = documents.get(id)?;
    let DocumentState::Persisted { uri, version, .. } = document.state() else {
        return None;
    };
    // A dirty document is not reloaded.
    if document.is_dirty(cx) {
        return None;
    }
    Some(Snapshot {
        model: document.model().clone(),
        uri: uri.clone(),
        recorded: version.clone(),
        revision: document.model().read(cx).revision(),
    })
}

/// Record or load what the read found, unless the document moved on while it
/// was read: edits that race the read win.
fn apply(id: DocumentId, snapshot: &Snapshot, text: &str, version: ResourceVersion, cx: &mut App) {
    let documents = cx.global::<ApplicationDocuments>().0.clone();
    let model = &snapshot.model;
    let current = documents.read(cx).get(id).map(|document| {
        (
            document.model() == model
                && document.state().persisted_version() == Some(&snapshot.recorded),
            document.title().to_owned(),
        )
    });
    let Some((true, title)) = current else {
        return;
    };
    if model.read(cx).revision() != snapshot.revision {
        return;
    }
    let saved = model.read(cx).text();
    let check = check_file(&snapshot.recorded, &version, text, &saved);
    if check == FileCheck::Unchanged {
        return;
    }
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
