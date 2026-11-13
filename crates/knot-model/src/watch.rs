//! Shared file watching: one registry reference-counts provider watches and
//! delivers coalesced batches of changed URIs to its subscribers.
//!
//! A subscription asks for a URI, recursive or not. A provider watch that
//! already covers it is reused: a recursive watch of an ancestor, or the same
//! URI watched alike. Every provider event is routed to each subscription whose
//! scope contains it, so sharing a watch never widens what a subscriber sees.

use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    },
    time::{Duration, Instant},
};

use crate::{
    filesystem::{FileSystemProviderRegistry, ResourceError, WatchEvent, WatchGuard},
    resource::ResourceUri,
};

/// How long events gather before a batch is delivered.
pub const COALESCE_WINDOW: Duration = Duration::from_millis(100);

/// Receives batches on the registry's delivery thread.
pub type WatchListener = Arc<dyn Fn(Vec<ResourceUri>) + Send + Sync>;

enum Message {
    Event(u64, WatchEvent),
    Shutdown,
}

struct ProviderWatch {
    uri: ResourceUri,
    recursive: bool,
    _guard: WatchGuard,
}

struct Subscriber {
    uri: ResourceUri,
    recursive: bool,
    listener: WatchListener,
    /// The provider watch this subscription holds.
    watch: u64,
    pending: BTreeSet<String>,
    pending_uris: Vec<ResourceUri>,
}

#[derive(Default)]
struct State {
    watches: HashMap<u64, ProviderWatch>,
    subscribers: HashMap<u64, Subscriber>,
    next_watch: u64,
    next_subscriber: u64,
}

struct Inner {
    state: Arc<Mutex<State>>,
    messages: Sender<Message>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.messages.send(Message::Shutdown);
    }
}

/// Reference-counted provider watches with coalesced delivery.
#[derive(Clone)]
pub struct WatchRegistry {
    inner: Arc<Inner>,
}

impl Default for WatchRegistry {
    fn default() -> Self {
        Self::new(COALESCE_WINDOW)
    }
}

/// Keeps a watch alive; dropping it stops delivery and releases the provider
/// watch once no other subscription uses it.
pub struct WatchSubscription {
    inner: Arc<Inner>,
    id: u64,
}

impl Drop for WatchSubscription {
    fn drop(&mut self) {
        let mut state = lock(&self.inner.state);
        let Some(subscriber) = state.subscribers.remove(&self.id) else {
            return;
        };
        let shared = state
            .subscribers
            .values()
            .any(|other| other.watch == subscriber.watch);
        if !shared {
            state.watches.remove(&subscriber.watch);
        }
    }
}

impl WatchRegistry {
    pub fn new(window: Duration) -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let (messages, receiver) = channel();
        let delivery = state.clone();
        let _ = std::thread::Builder::new()
            .name("knot-watch".into())
            .spawn(move || deliver(&delivery, &receiver, window));
        Self {
            inner: Arc::new(Inner { state, messages }),
        }
    }

    /// Watches `uri` through its provider and reports batches to `listener`.
    /// `uri` must be normalized.
    pub fn watch(
        &self,
        providers: &FileSystemProviderRegistry,
        uri: ResourceUri,
        recursive: bool,
        listener: WatchListener,
    ) -> Result<WatchSubscription, ResourceError> {
        let mut state = lock(&self.inner.state);
        let covering = state
            .watches
            .iter()
            .find(|(_, watch)| {
                watch.uri.scheme() == uri.scheme()
                    && ((watch.recursive && uri.is_within(&watch.uri))
                        || (watch.uri == uri && (watch.recursive || !recursive)))
            })
            .map(|(id, _)| *id);
        let watch = match covering {
            Some(id) => id,
            None => {
                let provider = providers.provider(&uri)?;
                let id = state.next_watch;
                let messages = self.inner.messages.clone();
                let guard = provider.watch(
                    uri.clone(),
                    recursive,
                    Arc::new(move |event| {
                        let _ = messages.send(Message::Event(id, event));
                    }),
                )?;
                state.next_watch += 1;
                state.watches.insert(
                    id,
                    ProviderWatch {
                        uri: uri.clone(),
                        recursive,
                        _guard: guard,
                    },
                );
                id
            }
        };
        let id = state.next_subscriber;
        state.next_subscriber += 1;
        state.subscribers.insert(
            id,
            Subscriber {
                uri,
                recursive,
                listener,
                watch,
                pending: BTreeSet::new(),
                pending_uris: Vec::new(),
            },
        );
        Ok(WatchSubscription {
            inner: self.inner.clone(),
            id,
        })
    }

    /// How many provider watches are live.
    pub fn provider_watch_count(&self) -> usize {
        lock(&self.inner.state).watches.len()
    }
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Whether a change at `changed` falls in the scope of a subscription.
fn in_scope(subscriber: &Subscriber, changed: &ResourceUri) -> bool {
    if *changed == subscriber.uri {
        return true;
    }
    if subscriber.recursive {
        return changed.is_within(&subscriber.uri);
    }
    let mut parent = changed.as_url().clone();
    if let Ok(mut segments) = parent.path_segments_mut() {
        segments.pop();
    }
    ResourceUri::from_url(parent) == subscriber.uri
}

fn record(subscriber: &mut Subscriber, uri: &ResourceUri) {
    if subscriber.pending.insert(uri.to_string()) {
        subscriber.pending_uris.push(uri.clone());
    }
}

fn absorb(state: &Mutex<State>, watch: u64, event: WatchEvent) {
    let mut state = lock(state);
    for subscriber in state.subscribers.values_mut() {
        match &event {
            WatchEvent::Changed(uris) => {
                for uri in uris {
                    if in_scope(subscriber, uri) {
                        record(subscriber, uri);
                    }
                }
            }
            WatchEvent::Rescan if subscriber.watch == watch => {
                let uri = subscriber.uri.clone();
                record(subscriber, &uri);
            }
            WatchEvent::Rescan => {}
        }
    }
}

fn flush(state: &Mutex<State>) {
    let ready: Vec<(WatchListener, Vec<ResourceUri>)> = {
        let mut state = lock(state);
        state
            .subscribers
            .values_mut()
            .filter(|subscriber| !subscriber.pending_uris.is_empty())
            .map(|subscriber| {
                subscriber.pending.clear();
                (
                    subscriber.listener.clone(),
                    std::mem::take(&mut subscriber.pending_uris),
                )
            })
            .collect()
    };
    for (listener, batch) in ready {
        listener(batch);
    }
}

/// Gathers events for one window after the first, then delivers.
fn deliver(state: &Mutex<State>, receiver: &Receiver<Message>, window: Duration) {
    loop {
        match receiver.recv() {
            Ok(Message::Event(watch, event)) => absorb(state, watch, event),
            Ok(Message::Shutdown) | Err(_) => return,
        }
        let deadline = Instant::now() + window;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match receiver.recv_timeout(remaining) {
                Ok(Message::Event(watch, event)) => absorb(state, watch, event),
                Ok(Message::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
        flush(state);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::mpsc::{Receiver, channel},
    };

    use super::*;
    use crate::filesystem::{LocalFileSystemProvider, MemoryFileSystemProvider};

    const WINDOW: Duration = Duration::from_millis(40);
    const WAIT: Duration = Duration::from_secs(5);

    struct Fixture {
        provider: Arc<MemoryFileSystemProvider>,
        providers: FileSystemProviderRegistry,
        registry: WatchRegistry,
    }

    fn uri(path: &str) -> ResourceUri {
        ResourceUri::parse(&format!("mem://workspace{path}")).unwrap()
    }

    fn fixture() -> Fixture {
        let provider = Arc::new(
            MemoryFileSystemProvider::new(ResourceUri::parse("mem://workspace/").unwrap()).unwrap(),
        );
        provider.seed_directory(uri("/src")).unwrap();
        provider.seed_directory(uri("/src/deep")).unwrap();
        let mut providers = FileSystemProviderRegistry::default();
        providers.register("mem", provider.clone()).unwrap();
        Fixture {
            provider,
            providers,
            registry: WatchRegistry::new(WINDOW),
        }
    }

    fn listener() -> (WatchListener, Receiver<Vec<ResourceUri>>) {
        let (sender, receiver) = channel();
        let sender = Mutex::new(sender);
        (
            Arc::new(move |batch| {
                let _ = sender.lock().unwrap().send(batch);
            }),
            receiver,
        )
    }

    fn sorted(mut batch: Vec<ResourceUri>) -> Vec<String> {
        batch.sort_by_key(ToString::to_string);
        batch.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_burst_of_writes_arrives_as_one_deduplicated_batch() {
        let fixture = fixture();
        let (on_batch, batches) = listener();
        let _watch = fixture
            .registry
            .watch(&fixture.providers, uri("/src"), false, on_batch)
            .unwrap();
        for _ in 0..3 {
            fixture.provider.seed_file(uri("/src/a.txt"), "x").unwrap();
        }
        fixture.provider.seed_file(uri("/src/b.txt"), "y").unwrap();

        let batch = batches.recv_timeout(WAIT).unwrap();
        assert_eq!(
            sorted(batch),
            ["mem://workspace/src/a.txt", "mem://workspace/src/b.txt"]
        );
        // Events are delivered in order, so a later change arriving alone
        // proves the burst left no second batch behind.
        fixture.provider.seed_file(uri("/src/c.txt"), "z").unwrap();
        assert_eq!(
            sorted(batches.recv_timeout(WAIT).unwrap()),
            ["mem://workspace/src/c.txt"]
        );
    }

    #[test]
    fn a_non_recursive_watch_ignores_deeper_entries() {
        let fixture = fixture();
        let (on_batch, batches) = listener();
        let _watch = fixture
            .registry
            .watch(&fixture.providers, uri("/src"), false, on_batch)
            .unwrap();
        fixture
            .provider
            .seed_file(uri("/src/deep/inner.txt"), "x")
            .unwrap();
        fixture
            .provider
            .seed_file(uri("/src/top.txt"), "x")
            .unwrap();
        // The deeper entry was reported first, so it would precede or join
        // the batch carrying the direct child.
        assert_eq!(
            sorted(batches.recv_timeout(WAIT).unwrap()),
            ["mem://workspace/src/top.txt"]
        );
    }

    #[test]
    fn watches_under_a_recursive_ancestor_share_its_provider_watch() {
        let fixture = fixture();
        let (root_listener, root_batches) = listener();
        let (file_listener, file_batches) = listener();
        let (same_listener, same_batches) = listener();
        let root = fixture
            .registry
            .watch(&fixture.providers, uri("/"), true, root_listener)
            .unwrap();
        let inner = fixture
            .registry
            .watch(&fixture.providers, uri("/src"), false, file_listener)
            .unwrap();
        let twin = fixture
            .registry
            .watch(&fixture.providers, uri("/"), true, same_listener)
            .unwrap();
        assert_eq!(fixture.provider.watch_count(), 1);
        assert_eq!(fixture.registry.provider_watch_count(), 1);

        fixture
            .provider
            .seed_file(uri("/src/deep/inner.txt"), "x")
            .unwrap();
        for batches in [&root_batches, &same_batches] {
            assert_eq!(
                sorted(batches.recv_timeout(WAIT).unwrap()),
                ["mem://workspace/src/deep/inner.txt"]
            );
        }
        // The narrower subscriber never sees what lies outside its scope: its
        // first batch is the later change inside it.
        fixture
            .provider
            .seed_file(uri("/src/top.txt"), "x")
            .unwrap();
        assert_eq!(
            sorted(file_batches.recv_timeout(WAIT).unwrap()),
            ["mem://workspace/src/top.txt"]
        );

        drop((root, twin));
        assert_eq!(fixture.provider.watch_count(), 1);
        drop(inner);
        assert_eq!(fixture.provider.watch_count(), 0);
    }

    #[test]
    fn disposal_releases_the_provider_watch_and_stops_delivery() {
        let fixture = fixture();
        let (on_batch, batches) = listener();
        let watch = fixture
            .registry
            .watch(&fixture.providers, uri("/src"), false, on_batch)
            .unwrap();
        assert_eq!(fixture.provider.watch_count(), 1);
        drop(watch);
        assert_eq!(fixture.provider.watch_count(), 0);
        fixture.provider.seed_file(uri("/src/a.txt"), "x").unwrap();
        // Disposal dropped the listener, so its channel is closed and empty.
        assert!(batches.recv().is_err());
    }

    #[test]
    fn dropped_platform_events_name_the_watched_uri() {
        let fixture = fixture();
        let (on_batch, batches) = listener();
        let _watch = fixture
            .registry
            .watch(&fixture.providers, uri("/src"), true, on_batch)
            .unwrap();
        fixture.provider.report_rescan();
        assert_eq!(
            sorted(batches.recv_timeout(WAIT).unwrap()),
            ["mem://workspace/src"]
        );
    }

    #[test]
    fn the_local_provider_reports_creates_and_rename_replaces() {
        let temporary = tempfile::tempdir().unwrap();
        let provider = LocalFileSystemProvider::new(temporary.path()).unwrap();
        let root = provider.root().clone();
        let mut providers = FileSystemProviderRegistry::default();
        providers.register("file", Arc::new(provider)).unwrap();
        let file = temporary.path().join("note.txt");
        fs::write(&file, "one").unwrap();

        let registry = WatchRegistry::new(WINDOW);
        let (on_batch, batches) = listener();
        let _watch = registry
            .watch(&providers, root.clone(), false, on_batch)
            .unwrap();
        let note = ResourceUri::from_url(url::Url::from_file_path(&file).unwrap());
        let wait_for = |expected: &ResourceUri| {
            let deadline = Instant::now() + WAIT;
            while Instant::now() < deadline {
                if let Ok(batch) = batches.recv_timeout(Duration::from_millis(200))
                    && batch.contains(expected)
                {
                    return true;
                }
            }
            false
        };

        let created = temporary.path().join("new.txt");
        fs::write(&created, "x").unwrap();
        assert!(wait_for(&ResourceUri::from_url(
            url::Url::from_file_path(&created).unwrap()
        )));

        let replacement = temporary.path().join("replacement.tmp");
        fs::write(&replacement, "two").unwrap();
        fs::rename(&replacement, &file).unwrap();
        assert!(wait_for(&note));
    }
}
