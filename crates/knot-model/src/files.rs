//! The extension file API: read, list, and stat through the registered
//! resource providers.
//!
//! Every operation reads the stored resource. Open documents and their unsaved
//! edits are never consulted.

use std::sync::Arc;

use crate::{
    filesystem::{
        FileSystemProviderRegistry, ResourceEntry, ResourceError, ResourceKind, ResourceStat,
    },
    open::{OpenResourceError, enumerate_directory},
    resource::ResourceUri,
};

fn parse(uri: &str) -> Result<ResourceUri, ResourceError> {
    ResourceUri::parse(uri).map_err(|error| ResourceError::InvalidUri {
        uri: uri.to_owned(),
        reason: error.to_string(),
    })
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

/// The entries of exactly one directory, in provider order.
pub async fn list(
    providers: Arc<FileSystemProviderRegistry>,
    uri: &str,
) -> Result<Vec<ResourceEntry>, ResourceError> {
    enumerate_directory(providers, parse(uri)?).await
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
        let mut names = run(list(providers.clone(), "mem://f/"))
            .unwrap()
            .into_iter()
            .map(|entry| (entry.name, entry.kind))
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
            run(list(providers.clone(), "mem://f/notes.txt")),
            Err(ResourceError::WrongKind { .. })
        ));
        assert!(matches!(
            run(list(providers.clone(), "mem://f/missing")),
            Err(ResourceError::NotFound { .. })
        ));
        assert!(matches!(
            run(list(providers, "nonsense")),
            Err(ResourceError::InvalidUri { .. })
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
