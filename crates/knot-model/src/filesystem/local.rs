use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use url::Url;

use super::{
    FileSystemProvider, ProviderFuture, ResourceEntry, ResourceError, ResourceFile, ResourceKind,
    ResourceStat, ResourceVersion, WatchEvent, WatchGuard, WatchSink, child_uri,
    validate_and_normalize_uri,
};
use crate::resource::ResourceUri;

/// A provider for one trusted local folder.
///
/// Containment is lexical. Filesystem operations follow symlinks, including
/// symlinks whose targets are outside the selected folder. Each operation runs
/// blocking `std::fs` calls on a small shared pool of worker threads.
pub struct LocalFileSystemProvider {
    root: Option<ResourceUri>,
    mutation_lock: Arc<Mutex<()>>,
}

impl LocalFileSystemProvider {
    #[cfg(any(test, feature = "test-support"))]
    pub fn new(root_path: impl AsRef<Path>) -> Result<Self, ResourceError> {
        let root_path = root_path.as_ref().to_path_buf();
        if !root_path.is_absolute() {
            return Err(ResourceError::InvalidUri {
                uri: root_path.display().to_string(),
                reason: "local provider root must be an absolute path".into(),
            });
        }
        let root_url = Url::from_file_path(&root_path).map_err(|_| ResourceError::InvalidUri {
            uri: root_path.display().to_string(),
            reason: "local provider root cannot be represented as a file URI".into(),
        })?;
        let root = normalize_file_uri(ResourceUri::from_url(root_url))?;
        Ok(Self {
            root: Some(root),
            mutation_lock: Arc::default(),
        })
    }

    /// Creates the application-wide provider for user-selected local resources.
    /// Workspace containment remains a workspace consumer concern.
    pub fn unrestricted() -> Self {
        Self {
            root: None,
            mutation_lock: Arc::default(),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn root(&self) -> &ResourceUri {
        self.root
            .as_ref()
            .expect("an unrestricted local provider has no root")
    }

    fn normalize_scoped(&self, uri: ResourceUri) -> Result<ResourceUri, ResourceError> {
        let uri = normalize_file_uri(uri)?;
        if self.root.as_ref().is_some_and(|root| !uri.is_within(root)) {
            return Err(ResourceError::OutsideWorkspace {
                uri,
                root: Box::new(self.root.clone().unwrap()),
            });
        }
        Ok(uri)
    }

    fn uri_to_path(&self, uri: &ResourceUri) -> Result<PathBuf, ResourceError> {
        uri.as_url()
            .to_file_path()
            .map_err(|_| ResourceError::InvalidUri {
                uri: uri.to_string(),
                reason: "file URI cannot be represented as a platform path".into(),
            })
    }

    #[cfg(test)]
    fn path_to_uri(&self, path: &Path) -> Result<ResourceUri, ResourceError> {
        self.normalize_scoped(path_to_unrestricted_uri(path)?)
    }

    fn spawn<T: Send + 'static>(
        &self,
        operation: &'static str,
        work: impl FnOnce() -> Result<T, ResourceError> + Send + 'static,
    ) -> ProviderFuture<'_, T> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let queued = io_pool().send(Box::new(move || {
            let _ = sender.send(work());
        }));
        Box::pin(async move {
            queued.map_err(|error| ResourceError::io(operation, error))?;
            receiver
                .await
                .map_err(|error| ResourceError::io(operation, error))?
        })
    }
}

type IoJob = Box<dyn FnOnce() + Send>;

struct IoPool {
    jobs: std::sync::mpsc::Sender<IoJob>,
}

impl IoPool {
    fn send(&self, job: IoJob) -> std::io::Result<()> {
        self.jobs
            .send(job)
            .map_err(|_| std::io::Error::other("file system workers stopped"))
    }
}

/// The workers that run blocking file system calls, shared by every local
/// provider so that bursts of operations, such as a recursive walk, queue
/// instead of spawning threads.
fn io_pool() -> &'static IoPool {
    static POOL: std::sync::OnceLock<IoPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let (jobs, queue) = std::sync::mpsc::channel::<IoJob>();
        let queue = Arc::new(std::sync::Mutex::new(queue));
        let workers = std::thread::available_parallelism()
            .map_or(4, |count| count.get())
            .clamp(4, 16);
        for _ in 0..workers {
            let queue = queue.clone();
            let _ = std::thread::Builder::new()
                .name("knot-filesystem".into())
                .spawn(move || {
                    while let Ok(job) = queue.lock().unwrap().recv() {
                        job();
                    }
                });
        }
        IoPool { jobs }
    })
}

fn path_to_unrestricted_uri(path: &Path) -> Result<ResourceUri, ResourceError> {
    let url = Url::from_file_path(path).map_err(|_| ResourceError::InvalidUri {
        uri: path.display().to_string(),
        reason: "platform path cannot be represented as a file URI".into(),
    })?;
    normalize_file_uri(ResourceUri::from_url(url))
}

impl FileSystemProvider for LocalFileSystemProvider {
    fn normalize(&self, uri: ResourceUri) -> ProviderFuture<'_, ResourceUri> {
        Box::pin(async move { self.normalize_scoped(uri) })
    }

    fn enumerate(&self, uri: ResourceUri) -> ProviderFuture<'_, Vec<ResourceEntry>> {
        let uri = match self.normalize_scoped(uri) {
            Ok(uri) => uri,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        let path = match self.uri_to_path(&uri) {
            Ok(path) => path,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        self.spawn("enumerate task", move || {
            let directory = std::fs::read_dir(&path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotADirectory {
                    ResourceError::WrongKind {
                        uri: uri.clone(),
                        expected: ResourceKind::Directory,
                        actual: ResourceKind::File,
                    }
                } else {
                    map_io("enumerate directory", &uri, error)
                }
            })?;
            let mut entries = Vec::new();
            for entry in directory {
                let entry =
                    entry.map_err(|error| ResourceError::io("enumerate directory", error))?;
                // A listing skips what it cannot represent: names that are not
                // UTF-8 or that a URI path segment cannot hold, broken
                // symlinks, and special files.
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                if name.contains('\\') {
                    continue;
                }
                // A child of a normalized, in-scope directory is normalized
                // and in scope.
                let child_uri = child_uri(&uri, &name);
                let file_type = entry
                    .file_type()
                    .map_err(|error| map_io("stat child", &child_uri, error))?;
                let symlink = file_type.is_symlink();
                // Only a symlink needs another call, to learn its target's kind.
                let file_type = if symlink {
                    match std::fs::metadata(entry.path()) {
                        Ok(metadata) => metadata.file_type(),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(error) => return Err(map_io("stat child", &child_uri, error)),
                    }
                } else {
                    file_type
                };
                let kind = if file_type.is_dir() {
                    ResourceKind::Directory
                } else if file_type.is_file() {
                    ResourceKind::File
                } else {
                    continue;
                };
                entries.push(ResourceEntry {
                    uri: child_uri,
                    name,
                    kind,
                    symlink,
                });
            }
            Ok(entries)
        })
    }

    fn read(&self, uri: ResourceUri) -> ProviderFuture<'_, ResourceFile> {
        let uri = match self.normalize_scoped(uri) {
            Ok(uri) => uri,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        let path = match self.uri_to_path(&uri) {
            Ok(path) => path,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        self.spawn("read task", move || {
            let initial_metadata = metadata(&path, &uri, "stat file")?;
            if !initial_metadata.is_file() {
                return Err(ResourceError::WrongKind {
                    uri,
                    expected: ResourceKind::File,
                    actual: ResourceKind::Directory,
                });
            }
            let before = resource_version(&initial_metadata)?;
            let bytes = std::fs::read(&path).map_err(|error| map_io("read", &uri, error))?;
            let after = resource_version(&metadata(&path, &uri, "stat file")?)?;
            if before != after {
                return Err(ResourceError::Conflict { uri });
            }
            Ok(ResourceFile {
                bytes,
                version: after,
            })
        })
    }

    fn create(&self, uri: ResourceUri, bytes: Vec<u8>) -> ProviderFuture<'_, ResourceVersion> {
        let uri = match self.normalize_scoped(uri) {
            Ok(uri) => uri,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        let path = match self.uri_to_path(&uri) {
            Ok(path) => path,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        let mutation_lock = self.mutation_lock.clone();
        self.spawn("create task", move || {
            let _guard = mutation_lock
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            atomic_create(&path, &uri, bytes)?;
            resource_version(&metadata(&path, &uri, "stat created file")?)
        })
    }

    fn replace(
        &self,
        uri: ResourceUri,
        expected: ResourceVersion,
        bytes: Vec<u8>,
    ) -> ProviderFuture<'_, ResourceVersion> {
        let uri = match self.normalize_scoped(uri) {
            Ok(uri) => uri,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        let path = match self.uri_to_path(&uri) {
            Ok(path) => path,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        let mutation_lock = self.mutation_lock.clone();
        self.spawn("replace task", move || {
            let _guard = mutation_lock
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let current = metadata(&path, &uri, "stat file")?;
            if !current.is_file() {
                return Err(ResourceError::WrongKind {
                    uri,
                    expected: ResourceKind::File,
                    actual: ResourceKind::Directory,
                });
            }
            if resource_version(&current)? != expected {
                return Err(ResourceError::Conflict { uri });
            }
            atomic_replace(&path, &uri, &expected, &current, bytes)?;
            resource_version(&metadata(&path, &uri, "stat replaced file")?)
        })
    }

    fn stat(&self, uri: ResourceUri) -> ProviderFuture<'_, ResourceStat> {
        let uri = match self.normalize_scoped(uri) {
            Ok(uri) => uri,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        let path = match self.uri_to_path(&uri) {
            Ok(path) => path,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        self.spawn("stat task", move || match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => Ok(ResourceStat::File),
            Ok(metadata) if metadata.is_dir() => Ok(ResourceStat::Directory),
            Ok(_) => Err(ResourceError::Io {
                operation: "stat",
                message: format!("unsupported resource kind at {uri}"),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ResourceStat::Missing),
            Err(error) => Err(ResourceError::io("stat", error)),
        })
    }

    fn watch(
        &self,
        uri: ResourceUri,
        recursive: bool,
        sink: WatchSink,
    ) -> Result<WatchGuard, ResourceError> {
        use notify::{RecursiveMode, Watcher};

        let uri = self.normalize_scoped(uri)?;
        let path = self.uri_to_path(&uri)?;
        // The platform reports resolved paths, so events are mapped back onto
        // the path that was asked for.
        let resolved =
            std::fs::canonicalize(&path).map_err(|error| map_io("watch", &uri, error))?;
        let requested = path.clone();
        let mut watcher = notify::recommended_watcher(
            move |result: notify::Result<notify::Event>| match result {
                Ok(event) if event.need_rescan() => sink(WatchEvent::Rescan),
                Ok(event) if event.kind.is_access() => {}
                Ok(event) => {
                    let uris: Vec<ResourceUri> = event
                        .paths
                        .iter()
                        .filter_map(|path| {
                            let path = match path.strip_prefix(&resolved) {
                                Ok(relative) => requested.join(relative),
                                Err(_) => path.clone(),
                            };
                            path_to_unrestricted_uri(&path).ok()
                        })
                        .collect();
                    if !uris.is_empty() {
                        sink(WatchEvent::Changed(uris));
                    }
                }
                Err(_) => sink(WatchEvent::Rescan),
            },
        )
        .map_err(|error| ResourceError::io("watch", error))?;
        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        watcher
            .watch(&path, mode)
            .map_err(|error| ResourceError::io("watch", error))?;
        Ok(Box::new(watcher))
    }
}

fn resource_version(metadata: &std::fs::Metadata) -> Result<ResourceVersion, ResourceError> {
    let modified = metadata
        .modified()
        .map_err(|error| ResourceError::io("read file metadata", error))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| ResourceError::io("read file metadata", error))?;
    let mut bytes = Vec::with_capacity(20);
    bytes.extend_from_slice(&metadata.len().to_le_bytes());
    bytes.extend_from_slice(&modified.as_secs().to_le_bytes());
    bytes.extend_from_slice(&modified.subsec_nanos().to_le_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        bytes.extend_from_slice(&metadata.dev().to_le_bytes());
        bytes.extend_from_slice(&metadata.ino().to_le_bytes());
    }
    Ok(ResourceVersion::new(bytes))
}

fn temporary_file(
    path: &Path,
    uri: &ResourceUri,
) -> Result<tempfile::NamedTempFile, ResourceError> {
    let parent = path.parent().ok_or_else(|| ResourceError::InvalidUri {
        uri: uri.to_string(),
        reason: "file destination has no parent directory".into(),
    })?;
    tempfile::Builder::new()
        .prefix(".knot-save-")
        .tempfile_in(parent)
        .map_err(|error| map_io("create temporary sibling", uri, error))
}

fn write_temporary(
    temporary: &mut tempfile::NamedTempFile,
    uri: &ResourceUri,
    bytes: Vec<u8>,
) -> Result<(), ResourceError> {
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| map_io("write temporary sibling", uri, error))
}

fn atomic_create(path: &Path, uri: &ResourceUri, bytes: Vec<u8>) -> Result<(), ResourceError> {
    let mut temporary = temporary_file(path, uri)?;
    write_temporary(&mut temporary, uri, bytes)?;
    temporary.persist_noclobber(path).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            ResourceError::AlreadyExists { uri: uri.clone() }
        } else {
            map_io("install new file", uri, error.error)
        }
    })?;
    sync_parent(path, uri)
}

/// Make a rename durable by syncing the directory entry that now names it.
fn sync_parent(path: &Path, uri: &ResourceUri) -> Result<(), ResourceError> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| map_io("sync parent directory", uri, error))?;
    }
    #[cfg(not(unix))]
    let _ = (path, uri);
    Ok(())
}

fn atomic_replace(
    path: &Path,
    uri: &ResourceUri,
    expected: &ResourceVersion,
    current: &std::fs::Metadata,
    bytes: Vec<u8>,
) -> Result<(), ResourceError> {
    // Replace the file a symlink points to, so the link itself survives.
    let path = &std::fs::canonicalize(path).map_err(|error| map_io("resolve file", uri, error))?;
    let mut temporary = temporary_file(path, uri)?;
    temporary
        .as_file()
        .set_permissions(current.permissions())
        .map_err(|error| map_io("preserve file permissions", uri, error))?;
    write_temporary(&mut temporary, uri, bytes)?;
    let latest = std::fs::metadata(path).map_err(|error| map_io("stat file", uri, error))?;
    if resource_version(&latest)? != *expected {
        return Err(ResourceError::Conflict { uri: uri.clone() });
    }
    temporary
        .persist(path)
        .map_err(|error| map_io("replace file", uri, error.error))?;
    sync_parent(path, uri)
}

pub fn normalize_file_uri(uri: ResourceUri) -> Result<ResourceUri, ResourceError> {
    if uri.as_url().host_str().is_some() {
        return Err(ResourceError::InvalidUri {
            uri: uri.to_string(),
            reason: "file URI authorities are unsupported".into(),
        });
    }
    validate_and_normalize_uri(uri, "file")
}

fn metadata(
    path: &Path,
    uri: &ResourceUri,
    operation: &'static str,
) -> Result<std::fs::Metadata, ResourceError> {
    std::fs::metadata(path).map_err(|error| map_io(operation, uri, error))
}

fn map_io(operation: &'static str, uri: &ResourceUri, error: std::io::Error) -> ResourceError {
    if error.kind() == std::io::ErrorKind::NotFound {
        ResourceError::NotFound { uri: uri.clone() }
    } else {
        ResourceError::io(operation, error)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn run<T>(future: impl std::future::Future<Output = T>) -> T {
        pollster::block_on(future)
    }

    #[test]
    fn local_folder_supports_normalize_enumerate_read_replace_and_stat() {
        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir(temporary.path().join("src")).unwrap();
        fs::write(temporary.path().join("src/lib.rs"), b"old").unwrap();
        let provider = LocalFileSystemProvider::new(temporary.path()).unwrap();
        let directory = provider.path_to_uri(&temporary.path().join("src")).unwrap();
        let file = provider
            .path_to_uri(&temporary.path().join("src/lib.rs"))
            .unwrap();

        assert_eq!(
            run(provider.stat(file.clone())).unwrap(),
            ResourceStat::File
        );
        let old = run(provider.read(file.clone())).unwrap();
        assert_eq!(old.bytes, b"old");
        run(provider.replace(file.clone(), old.version, b"new".to_vec())).unwrap();
        assert_eq!(
            fs::read(temporary.path().join("src/lib.rs")).unwrap(),
            b"new"
        );
        assert_eq!(
            run(provider.enumerate(directory)).unwrap(),
            vec![ResourceEntry {
                uri: file,
                name: "lib.rs".into(),
                kind: ResourceKind::File,
                symlink: false
            }]
        );
    }

    #[test]
    fn atomic_create_does_not_clobber_or_leave_a_temporary_sibling() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("existing.txt");
        fs::write(&path, b"existing").unwrap();
        let provider = LocalFileSystemProvider::new(temporary.path()).unwrap();
        let file = provider.path_to_uri(&path).unwrap();

        assert!(matches!(
            run(provider.create(file.clone(), b"replacement".to_vec())),
            Err(ResourceError::AlreadyExists { uri }) if uri == file
        ));
        assert_eq!(fs::read(&path).unwrap(), b"existing");
        assert_eq!(
            fs::read_dir(temporary.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>(),
            vec![std::ffi::OsString::from("existing.txt")]
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replace_preserves_permissions() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("executable.sh");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();
        let provider = LocalFileSystemProvider::new(temporary.path()).unwrap();
        let file = provider.path_to_uri(&path).unwrap();
        let before = run(provider.read(file.clone())).unwrap();

        run(provider.replace(file, before.version, b"new".to_vec())).unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o751);
    }

    #[test]
    fn rejected_atomic_replace_preserves_content_and_cleans_up_its_temporary_sibling() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("document.txt");
        fs::write(&path, b"external").unwrap();
        let provider = LocalFileSystemProvider::new(temporary.path()).unwrap();
        let file = provider.path_to_uri(&path).unwrap();
        let metadata = fs::metadata(&path).unwrap();

        assert!(matches!(
            atomic_replace(
                &path,
                &file,
                &ResourceVersion::new(b"stale"),
                &metadata,
                b"replacement".to_vec(),
            ),
            Err(ResourceError::Conflict { uri }) if uri == file
        ));
        assert_eq!(fs::read(&path).unwrap(), b"external");
        assert_eq!(
            fs::read_dir(temporary.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>(),
            vec![std::ffi::OsString::from("document.txt")]
        );
    }

    #[test]
    fn rejects_outside_and_wrong_kind_operations() {
        let temporary = tempfile::tempdir().unwrap();
        let provider = LocalFileSystemProvider::new(temporary.path()).unwrap();
        assert!(matches!(
            run(provider.read(provider.root().clone())),
            Err(ResourceError::WrongKind { .. })
        ));
        assert!(matches!(
            run(provider.stat(ResourceUri::parse("file:///definitely/outside").unwrap())),
            Err(ResourceError::OutsideWorkspace { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn enumerate_skips_entries_a_listing_cannot_represent() {
        use std::os::unix::{ffi::OsStrExt, fs::symlink, net::UnixListener};

        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::write(root.join("kept.txt"), b"kept").unwrap();
        fs::create_dir(root.join("folder")).unwrap();
        symlink(root.join("missing"), root.join("dangling")).unwrap();
        let _socket = UnixListener::bind(root.join("socket")).unwrap();
        // Some file systems (APFS) reject non-UTF-8 names.
        let _ = fs::write(
            root.join(std::ffi::OsStr::from_bytes(b"bad-\xff-name")),
            b"x",
        );
        let provider = LocalFileSystemProvider::new(root).unwrap();

        let mut names = run(provider.enumerate(provider.root().clone()))
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();
        names.sort();

        assert_eq!(names, ["folder", "kept.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinks_outside_the_trusted_root() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("target"), b"outside").unwrap();
        symlink(outside.path().join("target"), root.path().join("link")).unwrap();
        let provider = LocalFileSystemProvider::new(root.path()).unwrap();
        let link = provider.path_to_uri(&root.path().join("link")).unwrap();
        assert_eq!(run(provider.read(link)).unwrap().bytes, b"outside");
    }

    #[cfg(unix)]
    #[test]
    fn replace_through_a_symlink_keeps_the_link_and_updates_its_target() {
        use std::os::unix::fs::symlink;

        let links = tempfile::tempdir().unwrap();
        let targets = tempfile::tempdir().unwrap();
        let target = targets.path().join("dotfile");
        let link = links.path().join(".dotfile");
        fs::write(&target, b"old").unwrap();
        symlink(&target, &link).unwrap();
        let provider = LocalFileSystemProvider::unrestricted();
        let uri = path_to_unrestricted_uri(&link).unwrap();
        let before = run(provider.read(uri.clone())).unwrap();

        let version = run(provider.replace(uri.clone(), before.version, b"new".to_vec())).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&link).unwrap(), target);
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(run(provider.read(uri)).unwrap().version, version);
        for directory in [links.path(), targets.path()] {
            assert!(fs::read_dir(directory).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".knot-save-")
            }));
        }
    }
}
