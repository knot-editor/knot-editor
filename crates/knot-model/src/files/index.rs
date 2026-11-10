//! The in-memory tree of one workspace root.
//!
//! The index holds the visible and ignored entries of a root, scanned with the
//! ignore policy's classifier and kept current by re-listing the directories
//! that change. Its queries answer exactly what the disk implementations in
//! the parent module answer and return `None` for anything the tree does not
//! cover, so callers fall through to the disk: URIs outside the root, and
//! paths through hidden, ignored, symlinked, or unreadable directories.
//!
//! Hidden entries are not stored. Ignored directories are recorded but their
//! contents are never scanned.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, Mutex, MutexGuard},
    task::{Poll, Waker},
};

use futures::{StreamExt, stream::FuturesUnordered};

use super::{
    ListedEntry, WALK_CONCURRENCY,
    ignore::{Classifier, IgnorePolicy, Scope, Visibility, segments},
};
use crate::{
    filesystem::{FileSystemProvider, ResourceEntry, ResourceError, ResourceKind, child_uri},
    resource::ResourceUri,
};

/// One indexed entry. `dir` is set for directories whose contents are indexed.
struct Entry {
    entry: ResourceEntry,
    ignored: bool,
    dir: Option<Dir>,
}

/// An indexed directory: the rules of its entries and the entries.
struct Dir {
    scope: Arc<Scope>,
    children: BTreeMap<String, Entry>,
}

impl Dir {
    /// The directory at `path`, when every directory on the way is indexed.
    fn at(&self, path: &[String]) -> Option<&Dir> {
        let mut dir = self;
        for name in path {
            dir = dir.children.get(name)?.dir.as_ref()?;
        }
        Some(dir)
    }

    fn at_mut(&mut self, path: &[String]) -> Option<&mut Dir> {
        let mut dir = self;
        for name in path {
            dir = dir.children.get_mut(name)?.dir.as_mut()?;
        }
        Some(dir)
    }

    fn collect_files(&self, files: &mut Vec<ResourceEntry>) {
        for entry in self.children.values().filter(|entry| !entry.ignored) {
            match (&entry.dir, entry.entry.kind) {
                (Some(dir), _) => dir.collect_files(files),
                (None, ResourceKind::File) => files.push(entry.entry.clone()),
                (None, ResourceKind::Directory) => {}
            }
        }
    }
}

/// What a scan or a re-listing decides about the entries of one directory.
struct Listing {
    dir: Dir,
    /// The names of the subdirectories to scan.
    enter: Vec<String>,
}

/// Reads one directory and classifies its entries. `name` is the directory's
/// name when `scope` is its parent's, or `None` when `scope` is its own.
async fn read_directory(
    provider: &Arc<dyn FileSystemProvider>,
    classifier: &Classifier,
    uri: &ResourceUri,
    scope: Arc<Scope>,
    name: Option<&str>,
) -> Result<Listing, ResourceError> {
    let entries = provider.enumerate(uri.clone()).await?;
    let scope = match name {
        Some(name) => Arc::new(classifier.descend(&scope, name, uri, &entries).await),
        None => scope,
    };
    let (children, enter) = classify(classifier, &scope, entries);
    Ok(Listing {
        dir: Dir { scope, children },
        enter,
    })
}

/// The stored entries among `entries`, and which of them to scan.
fn classify(
    classifier: &Classifier,
    scope: &Scope,
    entries: Vec<ResourceEntry>,
) -> (BTreeMap<String, Entry>, Vec<String>) {
    let mut children = BTreeMap::new();
    let mut enter = Vec::new();
    for entry in entries {
        let is_dir = entry.kind == ResourceKind::Directory;
        let visibility = classifier.entry(scope, &entry.name, is_dir);
        if visibility == Visibility::Hidden {
            continue;
        }
        if visibility == Visibility::Visible && is_dir && !entry.symlink {
            enter.push(entry.name.clone());
        }
        children.insert(
            entry.name.clone(),
            Entry {
                ignored: visibility == Visibility::Ignored,
                entry,
                dir: None,
            },
        );
    }
    (children, enter)
}

/// Scans the directory at `uri` and everything indexed below it. Only a
/// failure to read the directory itself is an error; unreadable
/// subdirectories stay unindexed.
async fn scan(
    provider: &Arc<dyn FileSystemProvider>,
    classifier: &Classifier,
    uri: &ResourceUri,
    scope: Arc<Scope>,
    name: Option<&str>,
) -> Result<Dir, ResourceError> {
    let first = read_directory(provider, classifier, uri, scope, name).await?;
    let mut root = first.dir;
    // Directories are read concurrently, because each read is a provider round
    // trip. A directory's listing is attached before its children are queued.
    let mut waiting: Vec<Vec<String>> = first.enter.into_iter().map(|name| vec![name]).collect();
    let mut pending = FuturesUnordered::new();
    let read = |path: Vec<String>, root: &Dir| {
        let parent = root.at(&path[..path.len() - 1])?;
        let entry = parent.children.get(path.last()?)?;
        let (uri, scope) = (entry.entry.uri.clone(), parent.scope.clone());
        Some(async move {
            let name = path.last().cloned().unwrap_or_default();
            let listing = read_directory(provider, classifier, &uri, scope, Some(&name)).await;
            (path, listing)
        })
    };
    loop {
        while pending.len() < WALK_CONCURRENCY {
            let Some(path) = waiting.pop() else { break };
            if let Some(future) = read(path, &root) {
                pending.push(future);
            }
        }
        let Some((path, listing)) = pending.next().await else {
            break;
        };
        let Ok(listing) = listing else { continue };
        let Some((last, parents)) = path.split_last() else {
            continue;
        };
        if let Some(entry) = root
            .at_mut(parents)
            .and_then(|parent| parent.children.get_mut(last))
        {
            entry.dir = Some(listing.dir);
            waiting.extend(listing.enter.into_iter().map(|name| {
                let mut child = path.clone();
                child.push(name);
                child
            }));
        }
    }
    Ok(root)
}

/// The indexed state built with one policy.
struct Tree {
    policy: IgnorePolicy,
    classifier: Arc<Classifier>,
    /// `None` when the root cannot be read or is itself ignored, so every
    /// query falls through.
    root: Option<Dir>,
}

struct State {
    tree: Option<Tree>,
    /// Incremented whenever a scan installs a tree, so a re-listing that
    /// started before can tell its result is stale.
    generation: u64,
    waiting: Vec<Waker>,
}

/// The index of one root directory.
pub struct FileIndex {
    provider: Arc<dyn FileSystemProvider>,
    root: ResourceUri,
    root_segments: Vec<String>,
    state: Mutex<State>,
}

impl FileIndex {
    /// An index of the directory `root`, a normalized URI. It answers nothing
    /// until [`scan`](Self::scan) completes; queries wait for the first scan.
    pub fn new(provider: Arc<dyn FileSystemProvider>, root: ResourceUri) -> Self {
        Self {
            provider,
            root_segments: segments(&root),
            root,
            state: Mutex::new(State {
                tree: None,
                generation: 0,
                waiting: Vec::new(),
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Whether the index is responsible for `uri`: the root or anything
    /// below it.
    pub fn covers(&self, uri: &ResourceUri) -> bool {
        uri.is_within(&self.root)
    }

    /// The segments of `uri` below the root.
    fn relative(&self, uri: &ResourceUri) -> Option<Vec<String>> {
        self.covers(uri)
            .then(|| segments(uri)[self.root_segments.len()..].to_vec())
    }

    fn uri_at(&self, path: &[String]) -> ResourceUri {
        path.iter()
            .fold(self.root.clone(), |uri, name| child_uri(&uri, name))
    }

    /// Scans the whole root with `policy` and installs the result, replacing
    /// the tree that queries have been using.
    pub async fn scan(&self, policy: IgnorePolicy) {
        let classifier = Arc::new(Classifier::new(self.provider.clone(), &self.root, &policy));
        let (scope, visibility) = classifier.resolve(&self.root).await;
        let root = if visibility == Visibility::Visible {
            scan(
                &self.provider,
                &classifier,
                &self.root,
                Arc::new(scope),
                None,
            )
            .await
            .ok()
        } else {
            None
        };
        let mut state = self.state();
        state.tree = Some(Tree {
            policy,
            classifier,
            root,
        });
        state.generation += 1;
        for waker in state.waiting.drain(..) {
            waker.wake();
        }
    }

    /// Waits for the first scan, scans again when `policy` differs from the
    /// one the tree was built with, and then runs `query` on the tree.
    async fn query<T>(
        &self,
        policy: &IgnorePolicy,
        query: impl Fn(&Tree, &Dir) -> Option<T>,
    ) -> Option<T> {
        loop {
            std::future::poll_fn(|context| {
                let mut state = self.state();
                if state.tree.is_some() {
                    Poll::Ready(())
                } else {
                    state.waiting.push(context.waker().clone());
                    Poll::Pending
                }
            })
            .await;
            {
                let state = self.state();
                let tree = state.tree.as_ref()?;
                if tree.policy == *policy {
                    return query(tree, tree.root.as_ref()?);
                }
            }
            self.scan(policy.clone()).await;
        }
    }

    /// The indexed entries of the directory, as [`list`](super::list) returns
    /// them, or `None` when the directory is not indexed.
    pub async fn list(&self, uri: &ResourceUri, policy: &IgnorePolicy) -> Option<Vec<ListedEntry>> {
        let path = self.relative(uri)?;
        self.query(policy, |_, root| {
            let dir = root.at(&path)?;
            Some(
                dir.children
                    .values()
                    .map(|entry| ListedEntry {
                        entry: entry.entry.clone(),
                        ignored: entry.ignored,
                    })
                    .collect(),
            )
        })
        .await
    }

    /// The visible files below the directory, as [`walk`](super::walk)
    /// returns them, or `None` when the directory is not indexed.
    pub async fn walk(
        &self,
        uri: &ResourceUri,
        policy: &IgnorePolicy,
    ) -> Option<Vec<ResourceEntry>> {
        let path = self.relative(uri)?;
        self.query(policy, |_, root| {
            let mut files = Vec::new();
            root.at(&path)?.collect_files(&mut files);
            Some(files)
        })
        .await
    }

    /// Whether the policy hides or ignores the resource, as
    /// [`is_ignored`](super::is_ignored) judges it, or `None` when the index
    /// cannot tell without the disk.
    pub async fn is_ignored(&self, uri: &ResourceUri, policy: &IgnorePolicy) -> Option<bool> {
        let path = self.relative(uri)?;
        let (name, parents) = path.split_last()?;
        self.query(policy, |tree, root| {
            let dir = root.at(parents)?;
            if let Some(entry) = dir.children.get(name) {
                return Some(entry.ignored);
            }
            // The entry is hidden or missing. Its kind is unknown, so the
            // answer holds only if both kinds are judged alike.
            let judge =
                |is_dir| tree.classifier.entry(&dir.scope, name, is_dir) != Visibility::Visible;
            let (file, directory) = (judge(false), judge(true));
            (file == directory).then_some(file)
        })
        .await
    }

    /// Brings the index up to date with a batch of changed URIs: the root
    /// itself means anything may differ, so the tree is scanned again;
    /// otherwise each parent directory of a changed URI that is indexed is
    /// listed again and compared with its entries.
    pub async fn apply(&self, batch: &[ResourceUri]) {
        let mut directories = BTreeSet::new();
        for path in batch.iter().filter_map(|uri| self.relative(uri)) {
            match path.split_last() {
                Some((_, parent)) => {
                    directories.insert(parent.to_vec());
                }
                None => {
                    let policy = self.state().tree.as_ref().map(|tree| tree.policy.clone());
                    if let Some(policy) = policy {
                        self.scan(policy).await;
                    }
                    return;
                }
            }
        }
        let mut done = BTreeSet::new();
        while let Some(path) = directories.pop_first() {
            if !done.insert(path.clone()) {
                continue;
            }
            // A directory that cannot be listed is gone or unreadable; what
            // contained it changed.
            if !self.refresh(&path).await
                && let Some((_, parent)) = path.split_last()
            {
                directories.insert(parent.to_vec());
            }
        }
    }

    /// Lists the indexed directory at `path` again and merges the difference.
    /// Returns `false` when the directory could not be listed.
    async fn refresh(&self, path: &[String]) -> bool {
        let (classifier, scope, existing, generation) = {
            let state = self.state();
            let Some(tree) = &state.tree else { return true };
            let Some(dir) = tree.root.as_ref().and_then(|root| root.at(path)) else {
                return true;
            };
            let existing = dir
                .children
                .iter()
                .map(|(name, entry)| (name.clone(), Summary::of(entry)))
                .collect::<HashMap<_, _>>();
            (
                tree.classifier.clone(),
                dir.scope.clone(),
                existing,
                state.generation,
            )
        };
        let uri = self.uri_at(path);
        let Ok(entries) = self.provider.enumerate(uri).await else {
            return false;
        };
        let (mut children, enter) = classify(&classifier, &scope, entries);
        let enter = enter.into_iter().collect::<BTreeSet<_>>();
        let mut changes = Vec::new();
        for name in existing.keys().filter(|name| !children.contains_key(*name)) {
            changes.push((name.clone(), None));
        }
        for (name, mut entry) in std::mem::take(&mut children) {
            if existing.get(&name) == Some(&Summary::of(&entry)) {
                continue;
            }
            if enter.contains(&name) {
                entry.dir = scan(
                    &self.provider,
                    &classifier,
                    &entry.entry.uri,
                    scope.clone(),
                    Some(&name),
                )
                .await
                .ok();
            }
            changes.push((name, Some(entry)));
        }
        let mut state = self.state();
        if state.generation == generation
            && let Some(dir) = state
                .tree
                .as_mut()
                .and_then(|tree| tree.root.as_mut())
                .and_then(|root| root.at_mut(path))
        {
            for (name, entry) in changes {
                match entry {
                    Some(entry) => dir.children.insert(name, entry),
                    None => dir.children.remove(&name),
                };
            }
        }
        true
    }
}

/// What decides whether an entry's indexed state must be replaced.
#[derive(Eq, PartialEq)]
struct Summary {
    kind: ResourceKind,
    symlink: bool,
    ignored: bool,
}

impl Summary {
    fn of(entry: &Entry) -> Self {
        Self {
            kind: entry.entry.kind,
            symlink: entry.entry.symlink,
            ignored: entry.ignored,
        }
    }
}

#[cfg(test)]
mod tests;
