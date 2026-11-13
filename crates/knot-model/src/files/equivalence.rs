//! The walk against `ignore::WalkBuilder`, which implements the ripgrep
//! semantics the ignore policy claims, on local fixture trees.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use ignore::{WalkBuilder, gitignore::GitignoreBuilder};

use crate::{
    files::{IgnorePolicy, walk},
    filesystem::{FileSystemProviderRegistry, LocalFileSystemProvider},
};

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn knot_walk(directory: &Path, policy: &IgnorePolicy) -> BTreeSet<PathBuf> {
    let mut providers = FileSystemProviderRegistry::default();
    providers
        .register("file", Arc::new(LocalFileSystemProvider::unrestricted()))
        .unwrap();
    let uri = url::Url::from_directory_path(directory)
        .unwrap()
        .to_string();
    pollster::block_on(walk(Arc::new(providers), &uri, policy))
        .unwrap()
        .into_iter()
        .map(|entry| entry.uri.as_url().to_file_path().unwrap())
        .collect()
}

fn reference_walk(directory: &Path, policy: &IgnorePolicy) -> BTreeSet<PathBuf> {
    let patterns = |lines: &[String]| {
        let mut builder = GitignoreBuilder::new("/");
        for line in lines {
            let _ = builder.add_line(None, line);
        }
        builder.build().unwrap()
    };
    let hidden = patterns(&policy.hidden);
    let ignored = patterns(&policy.ignored);
    let respect = policy.respect_gitignore;
    WalkBuilder::new(directory)
        .standard_filters(false)
        .parents(respect)
        .ignore(respect)
        .git_ignore(respect)
        .git_exclude(respect)
        .git_global(respect)
        .require_git(true)
        .follow_links(false)
        .filter_entry(move |entry| {
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            !hidden.matched(entry.path(), is_dir).is_ignore()
                && !ignored.matched(entry.path(), is_dir).is_ignore()
        })
        .build()
        .map(|entry| entry.unwrap())
        // The walk reports symlinked directories as neither files nor entered.
        .filter(|entry| entry.path().is_file() || entry.path_is_symlink() && !entry.path().is_dir())
        .map(|entry| entry.into_path())
        .collect()
}

fn assert_equivalent(directory: &Path, policy: &IgnorePolicy) {
    let knot = knot_walk(directory, policy);
    let reference = reference_walk(directory, policy);
    let only_knot = knot.difference(&reference).collect::<Vec<_>>();
    let only_reference = reference.difference(&knot).collect::<Vec<_>>();
    assert!(
        only_knot.is_empty() && only_reference.is_empty(),
        "walking {}\nonly in the walk: {only_knot:#?}\nonly in the reference: {only_reference:#?}",
        directory.display()
    );
}

fn root() -> (tempfile::TempDir, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temporary.path()).unwrap();
    (temporary, root)
}

#[test]
fn nested_rules_negations_excludes_and_ancestor_ignore_files_match_ripgrep() {
    let (_guard, root) = root();
    // An `.ignore` above the repository applies inside it; a `.gitignore` there
    // does not.
    write(&root, ".ignore", "*.above\n");
    write(&root, ".gitignore", "outside.txt\n");
    write(&root, "repo/.git/HEAD", "");
    write(
        &root,
        "repo/.git/info/exclude",
        "excluded.txt\n/only-root.md\n",
    );
    write(
        &root,
        "repo/.gitignore",
        "*.log\n!keep.log\ntarget/\n/rooted.txt\n",
    );
    write(&root, "repo/.ignore", "scratch/\n!important.log\n");
    for path in [
        "a.log",
        "keep.log",
        "important.log",
        "x.above",
        "outside.txt",
        "excluded.txt",
        "only-root.md",
        "rooted.txt",
        "target/out.o",
        "scratch/notes.txt",
        "src/lib.rs",
        "src/excluded.txt",
        "src/only-root.md",
        "src/rooted.txt",
        "src/target",
        "src/a.log",
        "src/deep/b.log",
        "src/deep/y.above",
        ".hidden-file",
        ".DS_Store",
        "src/.DS_Store",
    ] {
        write(&root, &format!("repo/{path}"), "");
    }
    write(&root, "repo/src/.gitignore", "!a.log\n/local.txt\nsub/\n");
    write(&root, "repo/src/.ignore", "*.tmp\n");
    for path in [
        "src/local.txt",
        "src/deep/local.txt",
        "src/sub/file.txt",
        "src/deep/sub",
        "src/z.tmp",
    ] {
        write(&root, &format!("repo/{path}"), "");
    }
    write(
        &root,
        "repo/src/deep/.gitignore",
        "*\n!b.log\n!.gitignore\n",
    );
    write(&root, "other/outside.txt", "");
    write(&root, "other/z.above", "");
    let policy = IgnorePolicy::default();
    for start in ["", "repo", "repo/src", "repo/src/deep", "other"] {
        assert_equivalent(&root.join(start), &policy);
    }
}

#[test]
fn sibling_and_nested_repositories_use_their_own_rules_like_ripgrep() {
    let (_guard, root) = root();
    write(&root, ".gitignore", "outer-only.txt\n*.skip\n");
    write(&root, ".ignore", "*.above\n");
    write(&root, "first/.git/HEAD", "");
    write(&root, "first/.git/info/exclude", "exclude-first.txt\n");
    write(&root, "first/.gitignore", "*.log\n");
    write(&root, "second/.git/HEAD", "");
    write(&root, "second/.gitignore", "*.tmp\n");
    write(&root, "plain/.gitignore", "*.log\n");
    // A repository nested in another, with and without its own exclude file.
    write(&root, "first/vendor/lib/.git", "gitdir: elsewhere\n");
    write(&root, "first/vendor/lib/.gitignore", "*.tmp\n");
    write(&root, "first/vendor/other/.git/HEAD", "");
    write(&root, "first/vendor/other/.git/info/exclude", "own.txt\n");
    write(&root, "first/vendor/.ignore", "from-ignore.txt\n");
    for path in [
        "first/a.log",
        "first/a.tmp",
        "first/exclude-first.txt",
        "first/outer-only.txt",
        "first/x.skip",
        "first/x.above",
        "first/vendor/a.log",
        "first/vendor/from-ignore.txt",
        "first/vendor/lib/a.log",
        "first/vendor/lib/a.tmp",
        "first/vendor/lib/exclude-first.txt",
        "first/vendor/lib/from-ignore.txt",
        "first/vendor/lib/x.above",
        "first/vendor/other/a.log",
        "first/vendor/other/own.txt",
        "first/vendor/other/exclude-first.txt",
        "second/a.log",
        "second/a.tmp",
        "second/outer-only.txt",
        "plain/a.log",
        "plain/a.tmp",
        "plain/outer-only.txt",
        "plain/x.skip",
        "plain/x.above",
    ] {
        write(&root, path, "");
    }
    let policy = IgnorePolicy::default();
    for start in ["", "first", "first/vendor", "first/vendor/lib", "plain"] {
        assert_equivalent(&root.join(start), &policy);
    }
}

#[test]
fn configured_patterns_and_disabled_gitignore_match_ripgrep() {
    let (_guard, root) = root();
    write(&root, "repo/.git/HEAD", "");
    write(&root, "repo/.gitignore", "*.log\n");
    write(&root, "repo/.ignore", "*.tmp\n");
    for path in [
        "a.log",
        "a.tmp",
        "a.rs",
        "generated/b.rs",
        "src/generated",
        "src/c.rs",
        "node_modules/m/index.js",
        "docs/readme.md",
        "docs/draft.md",
    ] {
        write(&root, &format!("repo/{path}"), "");
    }
    let mut policy = IgnorePolicy {
        hidden: vec!["docs/draft.md".into(), "node_modules".into(), ".git".into()],
        ignored: vec!["generated/".into(), "*.rs".into(), "!c.rs".into()],
        respect_gitignore: true,
    };
    assert_equivalent(&root.join("repo"), &policy);
    policy.respect_gitignore = false;
    assert_equivalent(&root.join("repo"), &policy);
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed_and_symlinked_files_are_listed_like_ripgrep() {
    use std::os::unix::fs::symlink;

    let (_guard, root) = root();
    write(&root, "repo/.git/HEAD", "");
    write(&root, "repo/src/main.rs", "");
    write(&root, "elsewhere/secret.txt", "");
    symlink(root.join("elsewhere"), root.join("repo/linked")).unwrap();
    symlink(root.join("repo/src"), root.join("repo/alias")).unwrap();
    symlink(
        root.join("repo/src/main.rs"),
        root.join("repo/main-alias.rs"),
    )
    .unwrap();
    assert_equivalent(&root.join("repo"), &IgnorePolicy::default());
}

/// A deterministic generator, so a failing tree reproduces from its seed.
pub(super) struct Random(pub(super) u64);

impl Random {
    pub(super) fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound
    }

    pub(super) fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

fn generate(random: &mut Random, directory: &Path, depth: usize) {
    const NAMES: [&str; 11] = [
        "a", "b", "src", "build", "c.log", "d.tmp", "e.txt", "x.rs", ".dot", "keep.log", ".git",
    ];
    const PATTERNS: [&str; 16] = [
        "*.log",
        "!keep.log",
        "build/",
        "/a",
        "src",
        "*.tmp",
        "!*.tmp",
        "b/",
        "/e.txt",
        "**/x.rs",
        "a/*.txt",
        "!src/",
        "d*",
        "*",
        "!*/",
        "/build",
    ];
    fs::create_dir_all(directory).unwrap();
    for file in [".gitignore", ".ignore"] {
        if random.below(3) == 0 {
            let lines = (0..1 + random.below(4))
                .map(|_| random.pick(&PATTERNS))
                .collect::<Vec<_>>();
            fs::write(directory.join(file), lines.join("\n")).unwrap();
        }
    }
    for _ in 0..3 + random.below(4) {
        let name = random.pick(&NAMES);
        // The test makes the top directory a repository itself.
        if depth == 0 && name == ".git" {
            continue;
        }
        let path = directory.join(name);
        if path.exists() {
            continue;
        }
        if depth < 3 && random.below(2) == 0 {
            generate(random, &path, depth + 1);
        } else {
            fs::write(path, "").unwrap();
        }
    }
}

#[test]
fn generated_trees_match_ripgrep() {
    for seed in 0..8 {
        let (_guard, root) = root();
        let mut random = Random(seed);
        let repository = root.join("repo");
        generate(&mut random, &repository, 0);
        if seed % 4 != 0 {
            fs::create_dir_all(repository.join(".git/info")).unwrap();
            if seed % 2 == 0 {
                fs::write(repository.join(".git/info/exclude"), "*.log\n!keep.log\n").unwrap();
            }
        }
        let policy = IgnorePolicy {
            ignored: if seed % 5 == 0 {
                vec!["src".into()]
            } else {
                Vec::new()
            },
            ..IgnorePolicy::default()
        };
        assert_equivalent(&repository, &policy);
        assert_equivalent(&root, &policy);
    }
}
