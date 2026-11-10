//! The ignore policy: which entries listings hide, mark ignored, and walks skip.
//!
//! Evaluation reads ignore files through the entry's own provider, so every
//! scheme follows the same rules. Repository ignore files follow ripgrep's
//! semantics: `.ignore` files apply from any ancestor directory, while
//! `.gitignore` files, `.git/info/exclude`, and the global excludes file apply
//! only inside a Git repository, the nearest ancestor containing `.git`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use url::Url;

use crate::{
    filesystem::{FileSystemProvider, ResourceStat},
    resource::ResourceUri,
};

/// The names `files.hidden` hides by default.
pub const DEFAULT_HIDDEN: [&str; 7] = [
    ".git",
    ".hg",
    ".svn",
    ".jj",
    "CVS",
    ".DS_Store",
    "Thumbs.db",
];

/// The resolved `files.hidden`, `files.ignored`, and `files.respectGitignore`
/// settings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IgnorePolicy {
    /// Gitignore-style patterns of entries that are never shown.
    pub hidden: Vec<String>,
    /// Gitignore-style patterns of entries that are shown but ignored.
    pub ignored: Vec<String>,
    /// Whether repository ignore files apply.
    pub respect_gitignore: bool,
}

impl Default for IgnorePolicy {
    fn default() -> Self {
        Self {
            hidden: DEFAULT_HIDDEN
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            ignored: Vec::new(),
            respect_gitignore: true,
        }
    }
}

/// How the policy treats one entry, from least to most restrictive.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Visibility {
    Visible,
    Ignored,
    Hidden,
}

/// The decoded path segments of a normalized URI.
pub(super) fn segments(uri: &ResourceUri) -> Vec<String> {
    uri.as_url()
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            percent_encoding::percent_decode_str(segment)
                .decode_utf8_lossy()
                .into_owned()
        })
        .collect()
}

fn virtual_path(segments: &[String]) -> PathBuf {
    let mut path = PathBuf::from("/");
    path.extend(segments);
    path
}

fn build(root: &Path, patterns: impl IntoIterator<Item = impl AsRef<str>>) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    for pattern in patterns {
        let _ = builder.add_line(None, pattern.as_ref());
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

fn verdict(rules: &Gitignore, path: &Path, is_dir: bool) -> Option<bool> {
    let matched = rules.matched(path, is_dir);
    if matched.is_ignore() {
        Some(true)
    } else if matched.is_whitelist() {
        Some(false)
    } else {
        None
    }
}

fn global_excludes() -> &'static Gitignore {
    static GLOBAL: OnceLock<Gitignore> = OnceLock::new();
    GLOBAL.get_or_init(|| Gitignore::global().0)
}

#[derive(Default)]
struct DirectoryRules {
    ignore: Option<Gitignore>,
    gitignore: Option<Gitignore>,
}

/// Classifies entries below one directory against a policy.
pub(super) struct Classifier {
    provider: Arc<dyn FileSystemProvider>,
    template: Url,
    hidden: Gitignore,
    ignored: Gitignore,
    respect_gitignore: bool,
    uses_global_excludes: bool,
    /// The segment count of the repository root above the directory.
    repository: Option<usize>,
    exclude: Option<Gitignore>,
    directories: HashMap<usize, Arc<DirectoryRules>>,
    directory: Vec<String>,
}

impl Classifier {
    /// A classifier for entries of `directory`, a normalized directory URI.
    pub(super) async fn new(
        provider: Arc<dyn FileSystemProvider>,
        directory: &ResourceUri,
        policy: &IgnorePolicy,
    ) -> Self {
        let root = Path::new("/");
        let mut classifier = Self {
            provider,
            template: directory.as_url().clone(),
            hidden: build(root, &policy.hidden),
            ignored: build(root, &policy.ignored),
            respect_gitignore: policy.respect_gitignore,
            uses_global_excludes: directory.scheme() == "file",
            repository: None,
            exclude: None,
            directories: HashMap::new(),
            directory: segments(directory),
        };
        if classifier.respect_gitignore {
            classifier.find_repository().await;
        }
        classifier
    }

    fn uri(&self, segments: &[String]) -> Option<ResourceUri> {
        let mut url = self.template.clone();
        url.set_path("/");
        url.path_segments_mut().ok()?.clear().extend(segments);
        Some(ResourceUri::from_url(url))
    }

    async fn read(&self, segments: &[String]) -> Option<String> {
        let file = self.provider.read(self.uri(segments)?).await.ok()?;
        Some(String::from_utf8_lossy(&file.bytes).into_owned())
    }

    async fn find_repository(&mut self) {
        for depth in (0..=self.directory.len()).rev() {
            let mut git = self.directory[..depth].to_vec();
            git.push(".git".into());
            let Some(uri) = self.uri(&git) else { continue };
            if matches!(
                self.provider.stat(uri).await,
                Ok(ResourceStat::Missing) | Err(_)
            ) {
                continue;
            }
            self.repository = Some(depth);
            git.extend(["info".into(), "exclude".into()]);
            let root = virtual_path(&self.directory[..depth]);
            self.exclude = self.read(&git).await.map(|text| build(&root, text.lines()));
            return;
        }
    }

    /// The rules of an ancestor of the directory, or the directory itself.
    async fn rules(&mut self, depth: usize) -> Arc<DirectoryRules> {
        if let Some(rules) = self.directories.get(&depth) {
            return rules.clone();
        }
        let directory = self.directory[..depth].to_vec();
        let root = virtual_path(&directory);
        let file = |name: &str| {
            let mut path = directory.clone();
            path.push(name.into());
            path
        };
        let ignore = self.read(&file(".ignore")).await;
        let gitignore = if self
            .repository
            .is_some_and(|repository| depth >= repository)
        {
            self.read(&file(".gitignore")).await
        } else {
            None
        };
        let rules = Arc::new(DirectoryRules {
            ignore: ignore.map(|text| build(&root, text.lines())),
            gitignore: gitignore.map(|text| build(&root, text.lines())),
        });
        self.directories.insert(depth, rules.clone());
        rules
    }

    /// How the policy treats the entry `name` of the ancestor of the
    /// classifier's directory that has `depth` segments, judging the entry
    /// alone.
    pub(super) async fn entry(&mut self, depth: usize, name: &str, is_dir: bool) -> Visibility {
        let mut segments = self.directory[..depth].to_vec();
        segments.push(name.to_owned());
        let path = virtual_path(&segments);
        if self.hidden.matched(&path, is_dir).is_ignore() {
            return Visibility::Hidden;
        }
        if self.ignored.matched(&path, is_dir).is_ignore() {
            return Visibility::Ignored;
        }
        if !self.respect_gitignore {
            return Visibility::Visible;
        }
        for level in (0..=depth).rev() {
            let rules = self.rules(level).await;
            for source in [&rules.ignore, &rules.gitignore].into_iter().flatten() {
                if let Some(ignored) = verdict(source, &path, is_dir) {
                    return if ignored {
                        Visibility::Ignored
                    } else {
                        Visibility::Visible
                    };
                }
            }
        }
        if self.repository.is_some() {
            let global = self.uses_global_excludes.then(global_excludes);
            for source in [self.exclude.as_ref(), global].into_iter().flatten() {
                if let Some(ignored) = verdict(source, &path, is_dir) {
                    return if ignored {
                        Visibility::Ignored
                    } else {
                        Visibility::Visible
                    };
                }
            }
        }
        Visibility::Visible
    }

    /// How the policy treats the classifier's directory: the most restrictive
    /// verdict on it and its ancestors inside the repository.
    pub(super) async fn directory(&mut self) -> Visibility {
        let length = self.directory.len();
        let first = self.repository.unwrap_or(length.saturating_sub(1));
        let mut visibility = Visibility::Visible;
        for depth in first..length {
            let name = self.directory[depth].clone();
            visibility = visibility.max(self.entry(depth, &name, true).await);
        }
        visibility
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{
        files::{is_ignored, list},
        filesystem::{
            FileSystemProviderRegistry, LocalFileSystemProvider, MemoryFileSystemProvider,
        },
    };

    fn run<T>(future: impl std::future::Future<Output = T>) -> T {
        pollster::block_on(future)
    }

    fn memory(files: &[(&str, &str)]) -> Arc<FileSystemProviderRegistry> {
        let provider = Arc::new(
            MemoryFileSystemProvider::new(ResourceUri::parse("mem://t/").unwrap()).unwrap(),
        );
        for (path, text) in files {
            let uri = ResourceUri::parse(&format!("mem://t/{path}")).unwrap();
            if let Some((directory, _)) = path.rsplit_once('/') {
                provider
                    .seed_directory(ResourceUri::parse(&format!("mem://t/{directory}")).unwrap())
                    .unwrap();
            }
            if text.is_empty() && path.ends_with('/') {
                provider.seed_directory(uri).unwrap();
            } else {
                provider.seed_file(uri, text.as_bytes()).unwrap();
            }
        }
        let mut providers = FileSystemProviderRegistry::default();
        providers.register("mem", provider).unwrap();
        Arc::new(providers)
    }

    /// `name` or `name!` for ignored entries, sorted.
    fn listing(
        providers: &Arc<FileSystemProviderRegistry>,
        directory: &str,
        policy: &IgnorePolicy,
    ) -> Vec<String> {
        let mut names = run(list(
            providers.clone(),
            &format!("mem://t/{directory}"),
            policy,
        ))
        .unwrap()
        .into_iter()
        .map(|listed| {
            format!(
                "{}{}",
                listed.entry.name,
                if listed.ignored { "!" } else { "" }
            )
        })
        .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn ignored(providers: &Arc<FileSystemProviderRegistry>, path: &str) -> bool {
        run(is_ignored(
            providers.clone(),
            &format!("mem://t/{path}"),
            &IgnorePolicy::default(),
        ))
        .unwrap()
    }

    #[test]
    fn hidden_names_never_appear_at_any_depth() {
        let providers = memory(&[
            (".git/HEAD", "ref"),
            (".DS_Store", ""),
            ("a/.DS_Store", ""),
            ("a/b.txt", ""),
        ]);
        let policy = IgnorePolicy::default();
        assert_eq!(listing(&providers, "", &policy), ["a"]);
        assert_eq!(listing(&providers, "a", &policy), ["b.txt"]);
        assert!(ignored(&providers, "a/.DS_Store"));
        assert!(ignored(&providers, ".git/HEAD"));
        assert!(!ignored(&providers, "a/b.txt"));
    }

    #[test]
    fn listing_a_hidden_directory_shows_its_entries_as_ignored() {
        let providers = memory(&[(".git/HEAD", "ref")]);
        assert_eq!(
            listing(&providers, ".git", &IgnorePolicy::default()),
            ["HEAD!"]
        );
    }

    #[test]
    fn nested_gitignore_files_apply_with_negation_and_ignored_directories() {
        let providers = memory(&[
            (".git/config", ""),
            (".gitignore", "*.log\n!keep.log\ntarget/\n"),
            ("a.log", ""),
            ("keep.log", ""),
            ("target/out.o", ""),
            ("src/.gitignore", "!a.log\n/local.txt\n"),
            ("src/a.log", ""),
            ("src/local.txt", ""),
            ("src/deep/local.txt", ""),
            ("src/b.log", ""),
        ]);
        let policy = IgnorePolicy::default();
        assert_eq!(
            listing(&providers, "", &policy),
            [".gitignore", "a.log!", "keep.log", "src", "target!"]
        );
        assert_eq!(
            listing(&providers, "src", &policy),
            [".gitignore", "a.log", "b.log!", "deep", "local.txt!"]
        );
        assert_eq!(listing(&providers, "src/deep", &policy), ["local.txt"]);
        assert_eq!(listing(&providers, "target", &policy), ["out.o!"]);
        assert!(ignored(&providers, "target/out.o"));
        assert!(!ignored(&providers, "src/a.log"));
    }

    #[test]
    fn gitignore_needs_a_repository_but_ignore_files_do_not() {
        let providers = memory(&[
            (".gitignore", "*.log\n"),
            (".ignore", "*.tmp\n"),
            ("a.log", ""),
            ("a.tmp", ""),
        ]);
        assert_eq!(
            listing(&providers, "", &IgnorePolicy::default()),
            [".gitignore", ".ignore", "a.log", "a.tmp!"]
        );
    }

    #[test]
    fn exclude_file_and_configured_patterns_apply() {
        let providers = memory(&[
            (".git/info/exclude", "secret.txt\n"),
            ("secret.txt", ""),
            ("notes.md", ""),
            ("build/x", ""),
        ]);
        let mut policy = IgnorePolicy {
            ignored: vec!["build".into()],
            ..IgnorePolicy::default()
        };
        assert_eq!(
            listing(&providers, "", &policy),
            ["build!", "notes.md", "secret.txt!"]
        );
        policy.respect_gitignore = false;
        assert_eq!(
            listing(&providers, "", &policy),
            ["build!", "notes.md", "secret.txt"]
        );
        policy.hidden.push("notes.md".into());
        assert_eq!(listing(&providers, "", &policy), ["build!", "secret.txt"]);
    }

    #[test]
    fn local_provider_reads_repository_files_and_the_policy_follows_git_roots() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("repo/.git")).unwrap();
        fs::create_dir_all(root.join("repo/dist")).unwrap();
        fs::write(root.join("repo/.gitignore"), "dist/\n").unwrap();
        fs::write(root.join("repo/dist/out.js"), "").unwrap();
        fs::write(root.join("repo/main.rs"), "").unwrap();
        let mut providers = FileSystemProviderRegistry::default();
        providers
            .register("file", Arc::new(LocalFileSystemProvider::unrestricted()))
            .unwrap();
        let providers = Arc::new(providers);
        let uri = |path: &str| {
            let base = url::Url::from_file_path(root.join(path)).unwrap();
            base.to_string()
        };
        let policy = IgnorePolicy::default();
        let mut names = run(list(providers.clone(), &uri("repo"), &policy))
            .unwrap()
            .into_iter()
            .map(|listed| (listed.entry.name, listed.ignored))
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            [
                (".gitignore".to_owned(), false),
                ("dist".to_owned(), true),
                ("main.rs".to_owned(), false),
            ]
        );
        assert!(
            run(is_ignored(
                providers.clone(),
                &uri("repo/dist/out.js"),
                &policy
            ))
            .unwrap()
        );
        assert!(!run(is_ignored(providers, &uri("repo/main.rs"), &policy)).unwrap());
    }
}
