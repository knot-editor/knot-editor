//! The extension file API: read, list, and stat through the registered
//! resource providers.
//!
//! Every operation reads the stored resource. Open documents and their unsaved
//! edits are never consulted.

#[cfg(test)]
mod equivalence;
mod ignore;
mod index;
mod search;

use ignore::{Classifier, Scope, Visibility, segments};
pub use ignore::{DEFAULT_HIDDEN, IgnorePolicy};
pub use index::FileIndex;
pub use search::{Search, SearchHit, SearchResults};

use std::sync::Arc;

use futures::{StreamExt, stream::FuturesUnordered};

use crate::{
    filesystem::{
        FileSystemProvider, FileSystemProviderRegistry, ResourceEntry, ResourceError, ResourceKind,
        ResourceStat,
    },
    matching::Query,
    open::{OpenResourceError, enumerate_directory},
    resource::ResourceUri,
};

/// How many directories a walk reads at once.
const WALK_CONCURRENCY: usize = 16;

fn parse(uri: &str) -> Result<ResourceUri, ResourceError> {
    ResourceUri::parse(uri).map_err(|error| ResourceError::InvalidUri {
        uri: uri.to_owned(),
        reason: error.to_string(),
    })
}

/// The normalized form of `uri` according to its provider.
pub async fn normalize(
    providers: &FileSystemProviderRegistry,
    uri: &str,
) -> Result<ResourceUri, ResourceError> {
    providers.normalize(parse(uri)?).await
}

/// The file's text, decoded as UTF-8 without a leading byte-order mark.
pub async fn read_text(
    providers: Arc<FileSystemProviderRegistry>,
    uri: &str,
) -> Result<String, OpenResourceError> {
    let uri = providers.normalize(parse(uri)?).await?;
    let provider = providers.provider(&uri)?;
    match provider.stat(uri.clone()).await? {
        ResourceStat::Missing => Err(ResourceError::NotFound { uri }.into()),
        ResourceStat::Directory => Err(ResourceError::WrongKind {
            uri,
            expected: ResourceKind::File,
            actual: ResourceKind::Directory,
        }
        .into()),
        ResourceStat::File => {
            let file = provider.read(uri.clone()).await?;
            let mut text = String::from_utf8(file.bytes)
                .map_err(|_| OpenResourceError::InvalidUtf8 { uri })?;
            if text.starts_with('\u{feff}') {
                text.remove(0);
            }
            Ok(text)
        }
    }
}

/// One entry of a listing, with whether the ignore policy ignores it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListedEntry {
    pub entry: ResourceEntry,
    pub ignored: bool,
}

/// The entries of exactly one directory, in provider order, without those the
/// policy hides. Entries of an ignored directory are ignored.
pub async fn list(
    providers: Arc<FileSystemProviderRegistry>,
    uri: &str,
    policy: &IgnorePolicy,
) -> Result<Vec<ListedEntry>, ResourceError> {
    let uri = providers.normalize(parse(uri)?).await?;
    let entries = enumerate_directory(providers.clone(), uri.clone()).await?;
    let classifier = Classifier::new(providers.provider(&uri)?, &uri, policy);
    let (scope, inherited) = classifier.resolve(&uri).await;
    let mut listed = Vec::with_capacity(entries.len());
    for entry in entries {
        let is_dir = entry.kind == ResourceKind::Directory;
        match classifier.entry(&scope, &entry.name, is_dir) {
            Visibility::Hidden => {}
            own => listed.push(ListedEntry {
                ignored: own == Visibility::Ignored || inherited != Visibility::Visible,
                entry,
            }),
        }
    }
    Ok(listed)
}

/// What one directory of a walk contributes.
struct Visited {
    files: Vec<ResourceEntry>,
    /// The directories to enter: location, the scope of their parent, name.
    directories: Vec<(ResourceUri, Arc<Scope>, String)>,
}

/// Reads one directory of a walk and classifies its entries. `entered` is the
/// parent's scope and the directory's name, or `None` when the scope is
/// already the directory's own.
async fn visit(
    provider: Arc<dyn FileSystemProvider>,
    classifier: Arc<Classifier>,
    uri: ResourceUri,
    scope: Arc<Scope>,
    name: Option<String>,
) -> Result<Visited, ResourceError> {
    let entries = provider.enumerate(uri.clone()).await?;
    let scope = match name {
        Some(name) => Arc::new(classifier.descend(&scope, &name, &uri, &entries).await),
        None => scope,
    };
    let mut visited = Visited {
        files: Vec::new(),
        directories: Vec::new(),
    };
    for entry in entries {
        let is_dir = entry.kind == ResourceKind::Directory;
        if classifier.entry(&scope, &entry.name, is_dir) != Visibility::Visible {
            continue;
        }
        if !is_dir {
            visited.files.push(entry);
        } else if !entry.symlink {
            visited
                .directories
                .push((entry.uri, scope.clone(), entry.name));
        }
    }
    Ok(visited)
}

/// Every file below the directory, at any depth, that the policy neither hides
/// nor ignores. Ignored and hidden directories are not entered, symlinked
/// directories are not followed, and directories that cannot be read are
/// skipped. The order is unspecified.
pub async fn walk(
    providers: Arc<FileSystemProviderRegistry>,
    uri: &str,
    policy: &IgnorePolicy,
) -> Result<Vec<ResourceEntry>, ResourceError> {
    let uri = providers.normalize(parse(uri)?).await?;
    let provider = providers.provider(&uri)?;
    let classifier = Arc::new(Classifier::new(provider.clone(), &uri, policy));
    let (scope, visibility) = classifier.resolve(&uri).await;
    if visibility != Visibility::Visible {
        return Ok(Vec::new());
    }
    // Directories are read and classified concurrently, because each read is a
    // provider round trip.
    let visit = |directory: ResourceUri, scope: Arc<Scope>, name: Option<String>| {
        visit(provider.clone(), classifier.clone(), directory, scope, name)
    };
    let mut files = Vec::new();
    let mut pending = FuturesUnordered::new();
    pending.push(visit(uri, Arc::new(scope), None));
    let mut first = true;
    let mut waiting = Vec::new();
    while let Some(visited) = pending.next().await {
        match visited {
            Ok(visited) => {
                files.extend(visited.files);
                waiting.extend(visited.directories);
            }
            Err(error) if first => return Err(error),
            Err(_) => {}
        }
        first = false;
        while pending.len() < WALK_CONCURRENCY {
            let Some((directory, scope, name)) = waiting.pop() else {
                break;
            };
            pending.push(visit(directory, scope, Some(name)));
        }
    }
    Ok(files)
}

/// The best `limit` visible files below the directory that match `query`, by
/// walking the disk. The index answers the same without reading it.
pub async fn search(
    providers: Arc<FileSystemProviderRegistry>,
    uri: &str,
    query: &Query,
    limit: usize,
    policy: &IgnorePolicy,
) -> Result<SearchResults, ResourceError> {
    let mut search = Search::new(query, limit);
    offer(providers, uri, policy, &mut search).await?;
    Ok(search.finish())
}

/// Begins the directory in `search` and offers the visible files below it, by
/// walking the disk.
pub async fn offer(
    providers: Arc<FileSystemProviderRegistry>,
    uri: &str,
    policy: &IgnorePolicy,
    search: &mut Search<'_>,
) -> Result<(), ResourceError> {
    let directory = providers.normalize(parse(uri)?).await?;
    let depth = segments(&directory).len();
    let files = walk(providers, uri, policy).await?;
    search.begin(directory);
    for file in files {
        let path = segments(&file.uri)[depth..].join("/");
        search.offer(&path, path.len() - file.name.len());
    }
    Ok(())
}

/// Whether the policy hides or ignores the resource, or any directory above it
/// inside its repository. A missing resource is judged as a file.
pub async fn is_ignored(
    providers: Arc<FileSystemProviderRegistry>,
    uri: &str,
    policy: &IgnorePolicy,
) -> Result<bool, ResourceError> {
    let uri = providers.normalize(parse(uri)?).await?;
    let provider = providers.provider(&uri)?;
    let Some(name) = segments(&uri).pop() else {
        return Ok(false);
    };
    let is_dir = provider.stat(uri.clone()).await? == ResourceStat::Directory;
    let mut parent = uri.as_url().clone();
    parent
        .path_segments_mut()
        .map_err(|_| ResourceError::InvalidUri {
            uri: uri.to_string(),
            reason: "URI cannot contain hierarchical path segments".into(),
        })?
        .pop();
    let parent = ResourceUri::from_url(parent);
    let classifier = Classifier::new(provider, &parent, policy);
    let (scope, inherited) = classifier.resolve(&parent).await;
    let own = classifier.entry(&scope, &name, is_dir);
    Ok(own.max(inherited) != Visibility::Visible)
}

/// The kind of the resource, or `None` when it does not exist.
pub async fn stat(
    providers: Arc<FileSystemProviderRegistry>,
    uri: &str,
) -> Result<Option<ResourceKind>, ResourceError> {
    let uri = providers.normalize(parse(uri)?).await?;
    Ok(match providers.provider(&uri)?.stat(uri).await? {
        ResourceStat::Missing => None,
        ResourceStat::File => Some(ResourceKind::File),
        ResourceStat::Directory => Some(ResourceKind::Directory),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::MemoryFileSystemProvider;

    fn fixture() -> Arc<FileSystemProviderRegistry> {
        let provider = Arc::new(
            MemoryFileSystemProvider::new(ResourceUri::parse("mem://f/").unwrap()).unwrap(),
        );
        let seed = |path: &str, bytes: &[u8]| {
            provider
                .seed_file(ResourceUri::parse(path).unwrap(), bytes)
                .unwrap()
        };
        seed("mem://f/notes.txt", b"hello");
        seed("mem://f/bom.txt", b"\xef\xbb\xbfhello");
        seed("mem://f/invalid.txt", &[0xff]);
        provider
            .seed_directory(ResourceUri::parse("mem://f/src").unwrap())
            .unwrap();
        seed("mem://f/src/lib.rs", b"fn main() {}");
        let mut providers = FileSystemProviderRegistry::default();
        providers.register("mem", provider).unwrap();
        Arc::new(providers)
    }

    fn run<T>(future: impl std::future::Future<Output = T>) -> T {
        pollster::block_on(future)
    }

    #[test]
    fn read_text_decodes_and_drops_one_leading_bom() {
        let providers = fixture();
        assert_eq!(
            run(read_text(providers.clone(), "mem://f/notes.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            run(read_text(providers, "mem://f/bom.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn read_text_reports_each_failure() {
        let providers = fixture();
        assert!(matches!(
            run(read_text(providers.clone(), "not a uri")),
            Err(OpenResourceError::Provider(
                ResourceError::InvalidUri { .. }
            ))
        ));
        assert!(matches!(
            run(read_text(providers.clone(), "other://f/x")),
            Err(OpenResourceError::Provider(
                ResourceError::ProviderNotFound { .. }
            ))
        ));
        assert!(matches!(
            run(read_text(providers.clone(), "mem://f/missing")),
            Err(OpenResourceError::Provider(ResourceError::NotFound { .. }))
        ));
        assert!(matches!(
            run(read_text(providers.clone(), "mem://f/src")),
            Err(OpenResourceError::Provider(ResourceError::WrongKind { .. }))
        ));
        assert!(matches!(
            run(read_text(providers, "mem://f/invalid.txt")),
            Err(OpenResourceError::InvalidUtf8 { .. })
        ));
    }

    #[test]
    fn list_returns_one_directory_and_rejects_files_and_missing() {
        let providers = fixture();
        let mut names = run(list(
            providers.clone(),
            "mem://f/",
            &IgnorePolicy::default(),
        ))
        .unwrap()
        .into_iter()
        .map(|listed| (listed.entry.name, listed.entry.kind))
        .collect::<Vec<_>>();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            names,
            [
                ("bom.txt".to_owned(), ResourceKind::File),
                ("invalid.txt".to_owned(), ResourceKind::File),
                ("notes.txt".to_owned(), ResourceKind::File),
                ("src".to_owned(), ResourceKind::Directory),
            ]
        );
        assert!(matches!(
            run(list(
                providers.clone(),
                "mem://f/notes.txt",
                &IgnorePolicy::default()
            )),
            Err(ResourceError::WrongKind { .. })
        ));
        assert!(matches!(
            run(list(
                providers.clone(),
                "mem://f/missing",
                &IgnorePolicy::default()
            )),
            Err(ResourceError::NotFound { .. })
        ));
        assert!(matches!(
            run(list(providers, "nonsense", &IgnorePolicy::default())),
            Err(ResourceError::InvalidUri { .. })
        ));
    }

    #[test]
    fn walk_reports_a_root_that_is_not_a_directory() {
        let providers = fixture();
        let policy = IgnorePolicy::default();
        assert!(matches!(
            run(walk(providers.clone(), "mem://f/notes.txt", &policy)),
            Err(ResourceError::WrongKind { .. })
        ));
        assert!(matches!(
            run(walk(providers, "mem://f/missing", &policy)),
            Err(ResourceError::NotFound { .. })
        ));
    }

    #[test]
    fn stat_reports_kinds_and_none_for_missing() {
        let providers = fixture();
        assert_eq!(
            run(stat(providers.clone(), "mem://f/notes.txt")).unwrap(),
            Some(ResourceKind::File)
        );
        assert_eq!(
            run(stat(providers.clone(), "mem://f/src")).unwrap(),
            Some(ResourceKind::Directory)
        );
        assert_eq!(
            run(stat(providers.clone(), "mem://f/missing")).unwrap(),
            None
        );
        assert!(matches!(
            run(stat(providers, "nonsense")),
            Err(ResourceError::InvalidUri { .. })
        ));
    }
}
