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
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex, MutexGuard},
    task::{Poll, Waker},
};

use futures::{StreamExt, stream::FuturesUnordered};

use super::{
    ListedEntry, SearchResults, WALK_CONCURRENCY,
    ignore::{Classifier, IgnorePolicy, Scope, Visibility, segments},
    search::Search,
};
use crate::{
    filesystem::{FileSystemProvider, ResourceEntry, ResourceError, ResourceKind, child_uri},
    matching::Query,
    resource::ResourceUri,
};

/// One indexed entry. `dir` is set for directories whose contents are indexed.
struct Entry {
    name: String,
    kind: ResourceKind,
    /// Whether the entry is a symbolic link; `kind` is then the target's.
    symlink: bool,
    ignored: bool,
    dir: Option<Dir>,
}

impl Entry {
    /// The entry as the provider reports it, inside the directory `parent`.
    fn resource(&self, parent: &ResourceUri) -> ResourceEntry {
        ResourceEntry {
            uri: child_uri(parent, &self.name),
            name: self.name.clone(),
            kind: self.kind,
            symlink: self.symlink,
        }
    }
}

/// An indexed directory: the rules of its entries and the entries.
struct Dir {
    scope: Arc<Scope>,
    children: Children,
}

/// The entries of a directory, sorted by name. The name is stored only in the
/// entry.
#[derive(Default)]
struct Children(Vec<Entry>);

impl Children {
    fn find(&self, name: &str) -> Result<usize, usize> {
        self.0
            .binary_search_by(|entry| entry.name.as_str().cmp(name))
    }

    fn get(&self, name: &str) -> Option<&Entry> {
        self.find(name).ok().map(|index| &self.0[index])
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Entry> {
        self.find(name).ok().map(|index| &mut self.0[index])
    }

    /// Adds `entry`, replacing the entry of the same name.
    fn insert(&mut self, entry: Entry) {
        match self.find(&entry.name) {
            Ok(index) => self.0[index] = entry,
            Err(index) => self.0.insert(index, entry),
        }
    }

    fn remove(&mut self, name: &str) {
        if let Ok(index) = self.find(name) {
            self.0.remove(index);
        }
    }

    fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.0.iter()
    }
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

    /// Offers the visible files below this directory to `ranker`. `path`
    /// holds the directory's path below the searched one, ending with `/`.
    fn offer_files(&self, path: &mut String, search: &mut Search) {
        for entry in self.children.iter().filter(|entry| !entry.ignored) {
            let length = path.len();
            match (&entry.dir, entry.kind) {
                (Some(dir), _) => {
                    path.push_str(&entry.name);
                    path.push('/');
                    dir.offer_files(path, search);
                }
                (None, ResourceKind::File) => {
                    path.push_str(&entry.name);
                    search.offer(path, length);
                }
                (None, ResourceKind::Directory) => {}
            }
            path.truncate(length);
        }
    }

    /// The visible files below this directory, whose URI is `uri`.
    fn collect_files(&self, uri: &ResourceUri, files: &mut Vec<ResourceEntry>) {
        for entry in self.children.iter().filter(|entry| !entry.ignored) {
            match (&entry.dir, entry.kind) {
                (Some(dir), _) => dir.collect_files(&child_uri(uri, &entry.name), files),
                (None, ResourceKind::File) => files.push(entry.resource(uri)),
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
) -> (Children, Vec<String>) {
    let mut children = Vec::new();
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
        children.push(Entry {
            ignored: visibility == Visibility::Ignored,
            name: entry.name,
            kind: entry.kind,
            symlink: entry.symlink,
            dir: None,
        });
    }
    children.sort_by(|a, b| a.name.cmp(&b.name));
    (Children(children), enter)
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
        parent.children.get(path.last()?)?;
        let scope = parent.scope.clone();
        let uri = path
            .iter()
            .fold(uri.clone(), |uri, name| child_uri(&uri, name));
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
        mut query: impl FnMut(&Tree, &Dir) -> Option<T>,
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
        let uri = self.uri_at(&path);
        self.query(policy, |_, root| {
            let dir = root.at(&path)?;
            Some(
                dir.children
                    .iter()
                    .map(|entry| ListedEntry {
                        entry: entry.resource(&uri),
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
        let uri = self.uri_at(&path);
        self.query(policy, |_, root| {
            let mut files = Vec::new();
            root.at(&path)?.collect_files(&uri, &mut files);
            Some(files)
        })
        .await
    }

    /// The best `limit` visible files below the directory that match `query`,
    /// as [`search`](super::search) returns them, or `None` when the directory
    /// is not indexed.
    pub async fn search(
        &self,
        uri: &ResourceUri,
        query: &Query,
        limit: usize,
        policy: &IgnorePolicy,
    ) -> Option<SearchResults> {
        let mut search = Search::new(query, limit);
        self.offer(uri, policy, &mut search)
            .await
            .then(|| search.finish())
    }

    /// Begins `uri` in `search` and offers the visible files below it, as
    /// [`offer`](super::offer) does from the disk. Returns `false`, offering
    /// nothing, when the directory is not indexed.
    pub async fn offer(
        &self,
        uri: &ResourceUri,
        policy: &IgnorePolicy,
        search: &mut Search<'_>,
    ) -> bool {
        let Some(path) = self.relative(uri) else {
            return false;
        };
        let uri = self.uri_at(&path);
        self.query(policy, |_, root| {
            let dir = root.at(&path)?;
            search.begin(uri.clone());
            dir.offer_files(&mut String::new(), search);
            Some(())
        })
        .await
        .is_some()
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

    /// Brings the index up to date with a batch of changed URIs. The root
    /// itself means anything may differ, so the tree is scanned again.
    /// Otherwise each parent directory of a changed URI that is indexed is
    /// listed again and compared with its entries, and a change that
    /// reclassifies entries scans a subtree again: an ignore file, a
    /// repository's `.git` or `.git/info/exclude`.
    pub async fn apply(&self, batch: &[ResourceUri]) {
        let mut directories = BTreeSet::new();
        let mut rescans = BTreeSet::new();
        for path in batch.iter().filter_map(|uri| self.relative(uri)) {
            let Some((name, parent)) = path.split_last() else {
                self.scan_again(&[]).await;
                return;
            };
            directories.insert(parent.to_vec());
            if matches!(name.as_str(), ".gitignore" | ".ignore" | ".git") {
                rescans.insert(parent.to_vec());
            }
            // Hidden entries are not stored, so the exclude file is found by
            // its path.
            if let Some(git) = path.iter().rposition(|name| name == ".git")
                && path[git + 1..] == ["info", "exclude"]
            {
                rescans.insert(path[..git].to_vec());
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
        for path in rescans {
            self.scan_again(&path).await;
        }
    }

    /// Brings the index up to date with changes the application made itself,
    /// as the watch would report them. A change below a directory the index
    /// does not know yet also names that directory and each one above it, so
    /// the directories created on the way are indexed; the root is left out,
    /// since naming it scans everything.
    pub async fn apply_changes(&self, changed: &[ResourceUri]) {
        let mut batch = Vec::new();
        for uri in changed {
            let mut next = Some(uri.clone());
            while let Some(current) = next {
                if !self.covers(&current) || current == self.root {
                    break;
                }
                next = current.parent();
                if !batch.contains(&current) {
                    batch.push(current);
                }
            }
        }
        self.apply(&batch).await;
    }

    /// Scans the contents of the indexed directory at `path` again, with the
    /// tree's policy. The whole tree is scanned for the root.
    async fn scan_again(&self, path: &[String]) {
        let Some((name, parents)) = path.split_last() else {
            let policy = self.state().tree.as_ref().map(|tree| tree.policy.clone());
            if let Some(policy) = policy {
                self.scan(policy).await;
            }
            return;
        };
        let (classifier, parent, uri, generation) = {
            let state = self.state();
            let Some(tree) = &state.tree else { return };
            let Some(parent) = tree.root.as_ref().and_then(|root| root.at(parents)) else {
                return;
            };
            if !parent
                .children
                .get(name)
                .is_some_and(|entry| entry.dir.is_some())
            {
                return;
            }
            (
                tree.classifier.clone(),
                parent.scope.clone(),
                self.uri_at(path),
                state.generation,
            )
        };
        let dir = scan(&self.provider, &classifier, &uri, parent, Some(name))
            .await
            .ok();
        let mut state = self.state();
        if state.generation == generation
            && let Some(entry) = state
                .tree
                .as_mut()
                .and_then(|tree| tree.root.as_mut())
                .and_then(|root| root.at_mut(parents))
                .and_then(|parent| parent.children.get_mut(name))
        {
            entry.dir = dir;
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
                .map(|entry| (entry.name.clone(), Summary::of(entry)))
                .collect::<HashMap<_, _>>();
            (
                tree.classifier.clone(),
                dir.scope.clone(),
                existing,
                state.generation,
            )
        };
        let uri = self.uri_at(path);
        let Ok(entries) = self.provider.enumerate(uri.clone()).await else {
            return false;
        };
        let (mut children, enter) = classify(&classifier, &scope, entries);
        let enter = enter.into_iter().collect::<BTreeSet<_>>();
        let mut changes = Vec::new();
        for name in existing.keys().filter(|name| children.get(name).is_none()) {
            changes.push((name.clone(), None));
        }
        for mut entry in std::mem::take(&mut children.0) {
            let name = entry.name.clone();
            if existing.get(&name) == Some(&Summary::of(&entry)) {
                continue;
            }
            if enter.contains(&name) {
                entry.dir = scan(
                    &self.provider,
                    &classifier,
                    &child_uri(&uri, &name),
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
                    Some(entry) => dir.children.insert(entry),
                    None => dir.children.remove(&name),
                }
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
            kind: entry.kind,
            symlink: entry.symlink,
            ignored: entry.ignored,
        }
    }
}

#[cfg(test)]
mod tests;
