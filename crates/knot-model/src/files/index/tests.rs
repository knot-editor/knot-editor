use std::{
    future::Future,
    pin::pin,
    task::{Context, Poll},
};

use super::*;
use crate::{
    files::{SearchHit, equivalence::Random, is_ignored, list, search, walk},
    filesystem::{FileSystemProviderRegistry, MemoryFileSystemProvider},
    matching::{Matcher, Query},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    pollster::block_on(future)
}

fn uri(path: &str) -> ResourceUri {
    ResourceUri::parse(&format!("mem://t/{path}")).unwrap()
}

struct Fixture {
    memory: Arc<MemoryFileSystemProvider>,
    providers: Arc<FileSystemProviderRegistry>,
}

impl Fixture {
    fn new() -> Self {
        let memory = Arc::new(MemoryFileSystemProvider::new(uri("")).unwrap());
        let mut providers = FileSystemProviderRegistry::default();
        providers.register("mem", memory.clone()).unwrap();
        Self {
            memory,
            providers: Arc::new(providers),
        }
    }

    fn file(&self, path: &str) {
        let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        self.memory.seed_directory(uri(parent)).unwrap();
        self.memory.seed_file(uri(path), b"").unwrap();
    }

    fn index(&self, root: &str) -> FileIndex {
        FileIndex::new(self.memory.clone(), uri(root))
    }

    fn scanned(&self, root: &str) -> FileIndex {
        let index = self.index(root);
        run(index.scan(IgnorePolicy::default()));
        index
    }

    /// Every entry below the root, directories first reported before their
    /// contents: its path relative to the root and its kind.
    fn entries(&self) -> Vec<(String, ResourceKind)> {
        let mut found = Vec::new();
        let mut pending = vec![String::new()];
        while let Some(directory) = pending.pop() {
            for entry in run(self.memory.enumerate(uri(&directory))).unwrap() {
                let path = if directory.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{directory}/{}", entry.name)
                };
                if entry.kind == ResourceKind::Directory {
                    pending.push(path.clone());
                }
                found.push((path, entry.kind));
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
    }
}

fn sorted_listing(mut listed: Vec<ListedEntry>) -> Vec<ListedEntry> {
    listed.sort_by(|a, b| a.entry.name.cmp(&b.entry.name));
    listed
}

fn sorted_files(mut files: Vec<ResourceEntry>) -> Vec<ResourceEntry> {
    files.sort_by_key(|file| file.uri.to_string());
    files
}

/// Asserts that the index answers like the disk for every directory and
/// entry, and that it answers for every directory the policy does not ignore.
fn assert_matches_disk(fixture: &Fixture, index: &FileIndex, policy: &IgnorePolicy, context: &str) {
    let providers = &fixture.providers;
    let disk_is_ignored = |path: &str| {
        run(is_ignored(
            providers.clone(),
            &uri(path).to_string(),
            policy,
        ))
        .unwrap()
    };
    let entries = fixture.entries();
    let mut directories = vec![String::new()];
    directories.extend(
        entries
            .iter()
            .filter(|(_, kind)| *kind == ResourceKind::Directory)
            .map(|(path, _)| path.clone()),
    );
    for directory in &directories {
        let target = uri(directory);
        // The index covers a directory when no directory on its way is
        // ignored.
        let covered = directory
            .match_indices('/')
            .map(|(end, _)| &directory[..end])
            .chain((!directory.is_empty()).then_some(directory.as_str()))
            .all(|prefix| !disk_is_ignored(prefix));
        let listed = run(index.list(&target, policy));
        let walked = run(index.walk(&target, policy));
        assert_eq!(
            listed.is_some(),
            covered,
            "{context}: list {directory:?} coverage"
        );
        assert_eq!(
            walked.is_some(),
            covered,
            "{context}: walk {directory:?} coverage"
        );
        if let Some(listed) = listed {
            let disk = run(list(providers.clone(), &target.to_string(), policy)).unwrap();
            assert_eq!(
                sorted_listing(listed),
                sorted_listing(disk),
                "{context}: list {directory:?}"
            );
        }
        if let Some(walked) = walked {
            let disk = run(walk(providers.clone(), &target.to_string(), policy)).unwrap();
            assert_eq!(
                sorted_files(walked),
                sorted_files(disk),
                "{context}: walk {directory:?}"
            );
        }
        if covered {
            assert_search(fixture, index, &target, policy, context);
        }
        for name in ["zz", ".DS_Store", "keep.log", "build"] {
            let path = if directory.is_empty() {
                name.to_owned()
            } else {
                format!("{directory}/{name}")
            };
            if entries.iter().any(|(existing, _)| *existing == path) {
                continue;
            }
            if let Some(indexed) = run(index.is_ignored(&uri(&path), policy)) {
                assert_eq!(
                    indexed,
                    disk_is_ignored(&path),
                    "{context}: missing {path:?}"
                );
            }
        }
    }
    for (path, _) in &entries {
        if let Some(indexed) = run(index.is_ignored(&uri(path), policy)) {
            assert_eq!(
                indexed,
                disk_is_ignored(path),
                "{context}: is_ignored {path:?}"
            );
        }
    }
}

const QUERIES: [&str; 6] = ["", "a", "s", "rs", "src", "m r"];

/// The best matches by ranking every file of the walk, independently of the
/// bounded search.
fn brute_force_search(
    directory: &ResourceUri,
    files: &[ResourceEntry],
    query: &Query,
    limit: usize,
) -> SearchResults {
    let depth = segments(directory).len();
    let mut matcher = Matcher::new();
    let mut ranked = Vec::new();
    for file in files {
        let path = segments(&file.uri)[depth..].join("/");
        let (tier, found, offset) = match matcher.find(query, &file.name) {
            Some(found) => (0, found, path.len() - file.name.len()),
            None => match matcher.find(query, &path) {
                Some(found) => (1, found, 0),
                None => continue,
            },
        };
        let ranges = found
            .ranges
            .iter()
            .map(|range| range.start + offset..range.end + offset)
            .collect::<Vec<_>>();
        ranked.push((
            (tier, found.style, std::cmp::Reverse(found.score), path),
            ranges,
            file,
        ));
    }
    ranked.sort_by(|a, b| a.0.cmp(&b.0));
    let more = ranked.len() > limit;
    SearchResults {
        hits: ranked
            .into_iter()
            .take(limit)
            .map(|((_, _, _, path), ranges, file)| SearchHit {
                uri: file.uri.clone(),
                directory: directory.clone(),
                path,
                name: file.name.clone(),
                ranges,
            })
            .collect(),
        more,
    }
}

/// Asserts that `search` equals the ranking of every file, in the index and
/// on the disk.
fn assert_search(
    fixture: &Fixture,
    index: &FileIndex,
    directory: &ResourceUri,
    policy: &IgnorePolicy,
    context: &str,
) {
    let providers = &fixture.providers;
    let files = run(walk(providers.clone(), &directory.to_string(), policy)).unwrap();
    for text in QUERIES {
        let query = Query::new(text, &crate::matching::DEFAULT_STYLES);
        for limit in [2, 100] {
            let expected = brute_force_search(directory, &files, &query, limit);
            let indexed = run(index.search(directory, &query, limit, policy)).unwrap();
            assert_eq!(
                indexed, expected,
                "{context}: search {directory} {text:?} {limit}"
            );
            if limit != 2 {
                continue;
            }
            let disk = run(search(
                providers.clone(),
                &directory.to_string(),
                &query,
                limit,
                policy,
            ))
            .unwrap();
            assert_eq!(
                disk, expected,
                "{context}: disk search {directory} {text:?} {limit}"
            );
        }
    }
}

const NAMES: [&str; 10] = [
    "a",
    "b",
    "src",
    "build",
    "c.log",
    "d.tmp",
    "e.txt",
    "x.rs",
    ".DS_Store",
    "keep.log",
];
const PATTERNS: [&str; 10] = [
    "*.log",
    "!keep.log",
    "build/",
    "/a",
    "src",
    "*.tmp",
    "b/",
    "/e.txt",
    "**/x.rs",
    "d*",
];

fn generate(random: &mut Random, fixture: &Fixture, directory: &str, depth: usize) {
    for file in [".gitignore", ".ignore"] {
        if random.below(3) == 0 {
            let lines = (0..1 + random.below(3))
                .map(|_| random.pick(&PATTERNS))
                .collect::<Vec<_>>();
            let path = if directory.is_empty() {
                file.to_owned()
            } else {
                format!("{directory}/{file}")
            };
            fixture.memory.seed_directory(uri(directory)).unwrap();
            fixture
                .memory
                .seed_file(uri(&path), lines.join("\n"))
                .unwrap();
        }
    }
    for _ in 0..2 + random.below(4) {
        let name = random.pick(&NAMES);
        let path = if directory.is_empty() {
            name.to_owned()
        } else {
            format!("{directory}/{name}")
        };
        if fixture
            .entries()
            .iter()
            .any(|(existing, _)| *existing == path)
        {
            continue;
        }
        if depth < 3 && random.below(2) == 0 {
            fixture.memory.seed_directory(uri(&path)).unwrap();
            generate(random, fixture, &path, depth + 1);
        } else {
            fixture.file(&path);
        }
    }
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

/// Applies one random change and returns the URIs a watch would report.
fn mutate(random: &mut Random, fixture: &Fixture) -> Vec<ResourceUri> {
    let entries = fixture.entries();
    let mut directories = vec![String::new()];
    directories.extend(
        entries
            .iter()
            .filter(|(_, kind)| *kind == ResourceKind::Directory)
            .map(|(path, _)| path.clone()),
    );
    let exists = |path: &str| entries.iter().any(|(existing, _)| existing == path);
    let existing = entries.iter().collect::<Vec<_>>();
    let parent = directories[random.below(directories.len())].clone();
    let created = join(&parent, random.pick(&NAMES));
    let chosen = (!existing.is_empty()).then(|| existing[random.below(existing.len())]);
    match (random.below(9), chosen) {
        (0, _) if !exists(&created) => {
            fixture.memory.seed_file(uri(&created), b"").unwrap();
            vec![uri(&created)]
        }
        (1, _) if !exists(&created) => {
            fixture.memory.seed_directory(uri(&created)).unwrap();
            fixture.file(&join(&created, "inner.txt"));
            vec![uri(&created)]
        }
        (2, Some((path, _))) => {
            fixture.memory.remove(uri(path)).unwrap();
            vec![uri(path)]
        }
        (3, Some((path, ResourceKind::File))) if !exists(&created) => {
            fixture.memory.remove(uri(path)).unwrap();
            fixture.memory.seed_file(uri(&created), b"").unwrap();
            vec![uri(path), uri(&created)]
        }
        (4, Some((path, kind))) if !path.ends_with(".git") && !path.contains("ignore") => {
            fixture.memory.remove(uri(path)).unwrap();
            if *kind == ResourceKind::File {
                fixture.memory.seed_directory(uri(path)).unwrap();
                fixture.file(&join(path, "inner.txt"));
            } else {
                fixture.memory.seed_file(uri(path), b"").unwrap();
            }
            vec![uri(path)]
        }
        // Ignore files, repositories, and exclude files change what the
        // other entries are.
        (5, _) => {
            let path = join(&parent, random.pick(&[".gitignore", ".ignore"]));
            let lines = (0..1 + random.below(3))
                .map(|_| random.pick(&PATTERNS))
                .collect::<Vec<_>>();
            fixture
                .memory
                .seed_file(uri(&path), lines.join("\n"))
                .unwrap();
            vec![uri(&path)]
        }
        (6, _) => {
            let path = join(&parent, ".git");
            if exists(&path) {
                fixture.memory.remove(uri(&path)).unwrap();
            } else {
                fixture.memory.seed_directory(uri(&path)).unwrap();
            }
            vec![uri(&path)]
        }
        (7, _) => {
            let path = join(&parent, ".git/info/exclude");
            fixture.file(&path);
            let lines = (0..1 + random.below(3))
                .map(|_| random.pick(&PATTERNS))
                .collect::<Vec<_>>();
            fixture
                .memory
                .seed_file(uri(&path), lines.join("\n"))
                .unwrap();
            vec![uri(&path)]
        }
        _ => Vec::new(),
    }
}

#[test]
fn the_index_matches_the_disk_through_random_changes_and_reclassification() {
    for seed in 0..40 {
        let fixture = Fixture::new();
        let mut random = Random(seed);
        fixture.memory.seed_directory(uri(".git")).unwrap();
        generate(&mut random, &fixture, "", 0);
        let policy = IgnorePolicy {
            ignored: if seed % 5 == 0 {
                vec!["src".into()]
            } else {
                Vec::new()
            },
            ..IgnorePolicy::default()
        };
        let index = fixture.index("");
        run(index.scan(policy.clone()));
        assert_matches_disk(&fixture, &index, &policy, &format!("seed {seed} scan"));
        let mut policy = policy;
        for step in 0..40 {
            // A query with another policy scans the tree again.
            if random.below(8) == 0 {
                policy.respect_gitignore = !policy.respect_gitignore;
            }
            if random.below(8) == 0 {
                policy.ignored = if policy.ignored.is_empty() {
                    vec!["src".into()]
                } else {
                    Vec::new()
                };
            }
            let changed = mutate(&mut random, &fixture);
            run(index.apply(&changed));
            assert_matches_disk(
                &fixture,
                &index,
                &policy,
                &format!("seed {seed} step {step} {changed:?}"),
            );
        }
    }
}

#[test]
fn a_name_match_outranks_a_directory_match_and_the_limit_reports_more() {
    let fixture = Fixture::new();
    for path in [
        "docs/maintenance/notes.md",
        "src/main.rs",
        "src/lib.rs",
        "README.md",
    ] {
        fixture.file(path);
    }
    let index = fixture.scanned("");
    let policy = IgnorePolicy::default();
    let query = Query::new("main", &crate::matching::DEFAULT_STYLES);

    let found = run(index.search(&uri(""), &query, 10, &policy)).unwrap();
    let paths = found
        .hits
        .iter()
        .map(|hit| hit.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["src/main.rs", "docs/maintenance/notes.md"]);
    assert!(!found.more);
    assert_eq!(found.hits[0].name, "main.rs");
    assert_eq!(found.hits[0].uri, uri("src/main.rs"));
    assert_eq!(found.hits[0].ranges, vec![4..8]);

    let limited = run(index.search(&uri(""), &query, 1, &policy)).unwrap();
    assert_eq!(limited.hits.len(), 1);
    assert!(limited.more);

    let everything = Query::new("", &crate::matching::DEFAULT_STYLES);
    let first = run(index.search(&uri(""), &everything, 2, &policy)).unwrap();
    assert_eq!(
        first
            .hits
            .iter()
            .map(|hit| hit.path.as_str())
            .collect::<Vec<_>>(),
        ["README.md", "docs/maintenance/notes.md"]
    );
    assert!(first.more);

    // A directory outside every indexed one falls through.
    let nested = fixture.index("src/");
    run(nested.scan(policy.clone()));
    assert!(run(nested.search(&uri(""), &query, 10, &policy)).is_none());
}

#[test]
fn queries_wait_for_the_first_scan() {
    let fixture = Fixture::new();
    fixture.file("a.txt");
    let index = fixture.index("");
    let policy = IgnorePolicy::default();
    let root = uri("");
    let mut query = pin!(index.walk(&root, &policy));
    let mut context = Context::from_waker(futures::task::noop_waker_ref());
    assert!(query.as_mut().poll(&mut context).is_pending());
    run(index.scan(policy.clone()));
    let Poll::Ready(Some(files)) = query.as_mut().poll(&mut context) else {
        panic!("the query did not finish after the scan");
    };
    assert_eq!(files.len(), 1);
}

#[test]
fn a_batch_naming_the_root_scans_the_tree_again() {
    let fixture = Fixture::new();
    fixture.file("a.txt");
    let index = fixture.scanned("");
    fixture.file("b.txt");
    let policy = IgnorePolicy::default();
    assert_eq!(run(index.list(&uri(""), &policy)).unwrap().len(), 1);
    run(index.apply(&[uri("")]));
    assert_eq!(run(index.list(&uri(""), &policy)).unwrap().len(), 2);
}

#[test]
fn a_batch_for_a_new_directory_indexes_its_contents() {
    let fixture = Fixture::new();
    fixture.file("a.txt");
    let index = fixture.scanned("");
    fixture.file("new/deep/lib.rs");
    run(index.apply(&[uri("new")]));
    let policy = IgnorePolicy::default();
    let files = run(index.walk(&uri(""), &policy)).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(run(index.list(&uri("new/deep"), &policy)).unwrap().len(), 1);
}

#[test]
fn changes_made_by_the_application_index_the_directories_created_on_the_way() {
    let fixture = Fixture::new();
    fixture.file("docs/a.md");
    let index = fixture.scanned("");
    fixture.file("docs/deep/er/new.txt");
    // Naming only the file leaves the new directories unknown.
    run(index.apply(&[uri("docs/deep/er/new.txt")]));
    let policy = IgnorePolicy::default();
    assert_eq!(run(index.list(&uri("docs"), &policy)).unwrap().len(), 1);
    run(index.apply_changes(&[uri("docs/deep/er/new.txt")]));
    assert_eq!(run(index.list(&uri("docs"), &policy)).unwrap().len(), 2);
    assert_eq!(
        run(index.list(&uri("docs/deep/er"), &policy))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn events_below_ignored_and_hidden_directories_change_nothing() {
    let fixture = Fixture::new();
    fixture.file(".git/HEAD");
    fixture.file(".gitignore");
    fixture
        .memory
        .seed_file(uri(".gitignore"), b"target/\n")
        .unwrap();
    fixture.file("target/out.o");
    fixture.file("a.txt");
    let index = fixture.scanned("");
    let policy = IgnorePolicy::default();
    fixture.file("target/more.o");
    fixture.file(".git/index");
    run(index.apply(&[uri("target/more.o"), uri(".git/index")]));
    assert!(run(index.list(&uri("target"), &policy)).is_none());
    assert!(run(index.list(&uri(".git"), &policy)).is_none());
    assert_eq!(run(index.is_ignored(&uri("target/out.o"), &policy)), None);
    assert_eq!(run(index.is_ignored(&uri("target"), &policy)), Some(true));
    assert_eq!(run(index.is_ignored(&uri(".git"), &policy)), Some(true));
    let names = run(index.list(&uri(""), &policy))
        .unwrap()
        .into_iter()
        .map(|listed| (listed.entry.name, listed.ignored))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            (".gitignore".to_owned(), false),
            ("a.txt".to_owned(), false),
            ("target".to_owned(), true),
        ]
    );
}

#[test]
fn uris_outside_the_root_fall_through_without_waiting() {
    let fixture = Fixture::new();
    fixture.file("sub/a.txt");
    fixture.file("other/b.txt");
    let index = fixture.index("sub");
    let policy = IgnorePolicy::default();
    assert!(!index.covers(&uri("other")));
    assert!(run(index.list(&uri("other"), &policy)).is_none());
    assert!(run(index.walk(&uri("other"), &policy)).is_none());
    assert!(run(index.is_ignored(&uri("other/b.txt"), &policy)).is_none());
    assert!(run(index.is_ignored(&uri("sub"), &policy)).is_none());
}

#[test]
fn a_different_policy_scans_again_before_answering() {
    let fixture = Fixture::new();
    fixture.file("a.txt");
    fixture.file("b.txt");
    let index = fixture.scanned("");
    let policy = IgnorePolicy {
        hidden: vec!["a.txt".into()],
        ..IgnorePolicy::default()
    };
    let names = run(index.list(&uri(""), &policy))
        .unwrap()
        .into_iter()
        .map(|listed| listed.entry.name)
        .collect::<Vec<_>>();
    assert_eq!(names, ["b.txt"]);
}

#[test]
fn an_ignore_file_change_scans_a_newly_visible_directory_and_empties_a_newly_ignored_one() {
    let fixture = Fixture::new();
    fixture.file(".git/HEAD");
    fixture
        .memory
        .seed_file(uri(".gitignore"), b"build/\n")
        .unwrap();
    fixture.file("build/out.o");
    fixture.file("src/main.rs");
    let index = fixture.scanned("");
    let policy = IgnorePolicy::default();
    assert!(run(index.list(&uri("build"), &policy)).is_none());

    fixture
        .memory
        .seed_file(uri(".gitignore"), b"src/\n")
        .unwrap();
    run(index.apply(&[uri(".gitignore")]));
    assert_eq!(run(index.list(&uri("build"), &policy)).unwrap().len(), 1);
    assert!(run(index.list(&uri("src"), &policy)).is_none());
    assert_matches_disk(&fixture, &index, &policy, "after editing .gitignore");

    fixture.file("sub/.gitignore");
    fixture.file("sub/a.txt");
    fixture.file("sub/b.txt");
    run(index.apply(&[uri("sub")]));
    fixture
        .memory
        .seed_file(uri("sub/.gitignore"), b"a.txt\n")
        .unwrap();
    run(index.apply(&[uri("sub/.gitignore")]));
    assert_matches_disk(&fixture, &index, &policy, "after a nested .gitignore");
}

#[test]
fn a_repository_appearing_or_its_exclude_file_changing_reclassifies_its_directory() {
    let fixture = Fixture::new();
    fixture.file("repo/.gitignore");
    fixture
        .memory
        .seed_file(uri("repo/.gitignore"), b"*.log\n")
        .unwrap();
    fixture.file("repo/a.log");
    fixture.file("repo/b.txt");
    let index = fixture.scanned("");
    let policy = IgnorePolicy::default();
    // Without `.git`, `.gitignore` does not apply.
    assert_matches_disk(&fixture, &index, &policy, "no repository");

    fixture.memory.seed_directory(uri("repo/.git")).unwrap();
    run(index.apply(&[uri("repo/.git")]));
    assert_eq!(
        run(index.is_ignored(&uri("repo/a.log"), &policy)),
        Some(true)
    );
    assert_matches_disk(&fixture, &index, &policy, "repository appeared");

    fixture.file("repo/.git/info/exclude");
    fixture
        .memory
        .seed_file(uri("repo/.git/info/exclude"), b"b.txt\n")
        .unwrap();
    run(index.apply(&[uri("repo/.git/info/exclude")]));
    assert_eq!(
        run(index.is_ignored(&uri("repo/b.txt"), &policy)),
        Some(true)
    );
    assert_matches_disk(&fixture, &index, &policy, "exclude edited");

    fixture.memory.remove(uri("repo/.git")).unwrap();
    run(index.apply(&[uri("repo/.git")]));
    assert_eq!(
        run(index.is_ignored(&uri("repo/a.log"), &policy)),
        Some(false)
    );
    assert_matches_disk(&fixture, &index, &policy, "repository removed");
}

#[test]
fn searching_several_directories_ranks_them_as_one() {
    let fixture = Fixture::new();
    for path in [
        "one/main/readme.md",
        "one/x/main.rs",
        "two/main.rs",
        "two/x/main.rs",
    ] {
        fixture.file(path);
    }
    let policy = IgnorePolicy::default();
    let query = Query::new("main", &crate::matching::DEFAULT_STYLES);
    let directories = [uri("one/"), uri("two/")];
    let paths = |found: &SearchResults| {
        found
            .hits
            .iter()
            .map(|hit| {
                let root = hit.directory.to_string();
                format!(
                    "{}:{}",
                    root.trim_end_matches('/').rsplit('/').next().unwrap(),
                    hit.path
                )
            })
            .collect::<Vec<_>>()
    };
    // Name matches in the second directory beat a directory-only match in the
    // first, and equal matches fall to directory order.
    let expected = [
        "one:x/main.rs",
        "two:main.rs",
        "two:x/main.rs",
        "one:main/readme.md",
    ];

    // The first directory is indexed and the second is read from the disk.
    let index = fixture.scanned("one/");
    let mut search = Search::new(&query, 10);
    assert!(run(index.offer(&directories[0], &policy, &mut search)));
    run(crate::files::offer(
        fixture.providers.clone(),
        &directories[1].to_string(),
        &policy,
        &mut search,
    ))
    .unwrap();
    let mixed = search.finish();
    assert_eq!(paths(&mixed), expected);
    assert!(!mixed.more);
    assert_eq!(mixed.hits[1].uri, uri("two/main.rs"));

    // Walking both gives the same answer, and the limit keeps the best.
    let mut search = Search::new(&query, 2);
    for directory in &directories {
        run(crate::files::offer(
            fixture.providers.clone(),
            &directory.to_string(),
            &policy,
            &mut search,
        ))
        .unwrap();
    }
    let limited = search.finish();
    assert_eq!(paths(&limited), expected[..2]);
    assert!(limited.more);
}
