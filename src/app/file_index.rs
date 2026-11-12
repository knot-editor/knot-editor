//! The application-wide registry of workspace file indexes.
//!
//! One [`FileIndex`] exists per workspace root URI, shared by every window that
//! shows the root. Each window holds a [`FileIndexLease`] per root; the
//! last lease to drop releases the index and its recursive watch.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use futures::StreamExt as _;
use gpui::{App, AppContext as _, Global, Task};
use knot_model::{
    files::FileIndex, filesystem::FileSystemProviderRegistry, resource::ResourceUri,
    watch::WatchSubscription,
};

struct Slot {
    index: Arc<FileIndex>,
    leases: usize,
    _subscription: WatchSubscription,
    /// Scans the root, then applies the watch's batches.
    _task: Task<()>,
}

type Slots = Arc<Mutex<HashMap<ResourceUri, Slot>>>;

fn lock(slots: &Slots) -> MutexGuard<'_, HashMap<ResourceUri, Slot>> {
    slots.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[derive(Default)]
pub(crate) struct ApplicationFileIndex {
    slots: Slots,
}

impl Global for ApplicationFileIndex {}

/// Keeps the index of one root alive.
pub(crate) struct FileIndexLease {
    slots: Slots,
    root: ResourceUri,
}

impl Drop for FileIndexLease {
    fn drop(&mut self) {
        let mut slots = lock(&self.slots);
        if let Some(slot) = slots.get_mut(&self.root) {
            slot.leases -= 1;
            if slot.leases == 0 {
                slots.remove(&self.root);
            }
        }
    }
}

impl ApplicationFileIndex {
    /// Leases the index of `root`, a normalized directory URI, starting its
    /// scan and watch when no other lease holds it. `None` when the root
    /// cannot be watched, so queries keep reading the disk.
    pub(crate) fn lease(
        cx: &mut App,
        providers: &FileSystemProviderRegistry,
        root: &ResourceUri,
    ) -> Option<FileIndexLease> {
        let slots = cx.default_global::<Self>().slots.clone();
        {
            let mut held = lock(&slots);
            if let Some(slot) = held.get_mut(root) {
                slot.leases += 1;
                return Some(FileIndexLease {
                    slots: slots.clone(),
                    root: root.clone(),
                });
            }
        }
        let provider = providers.provider(root).ok()?;
        let index = Arc::new(FileIndex::new(provider, root.clone()));
        let (sender, mut batches) = futures::channel::mpsc::unbounded();
        let registry = cx
            .default_global::<crate::app::file_watch::ApplicationFileWatches>()
            .0
            .clone();
        let subscription = registry
            .watch(
                providers,
                root.clone(),
                true,
                Arc::new(move |batch| {
                    let _ = sender.unbounded_send(batch);
                }),
            )
            .ok()?;
        let policy = crate::app::settings::files_ignore_policy(cx);
        let scanned = index.clone();
        let task = cx.background_spawn(async move {
            scanned.scan(policy).await;
            while let Some(batch) = batches.next().await {
                scanned.apply(&batch).await;
            }
        });
        lock(&slots).insert(
            root.clone(),
            Slot {
                index,
                leases: 1,
                _subscription: subscription,
                _task: task,
            },
        );
        Some(FileIndexLease {
            slots,
            root: root.clone(),
        })
    }

    /// The index of the innermost leased root that contains `uri`.
    pub(crate) fn covering(cx: &App, uri: &ResourceUri) -> Option<Arc<FileIndex>> {
        let slots = lock(&cx.try_global::<Self>()?.slots);
        slots
            .iter()
            .filter(|(root, _)| uri.is_within(root))
            .max_by_key(|(root, _)| root.as_url().path().len())
            .map(|(_, slot)| slot.index.clone())
    }
}
