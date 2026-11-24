//! The Git backend in the launched product: `@knot/vcs` and `@knot/git` over
//! temporary repositories made with the `git` program.
use std::path::Path;
use std::process::Command;

use gpui::TestAppContext;
use knot::test_support::{TestProduct, TestWindow, resolved_tempdir, wait_until};

const BASE: &str = "a\nb\nc\nd\n";

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "protocol.file.allow=always",
        ])
        .args(args)
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A repository in `directory` whose HEAD holds `files`.
fn repository(directory: &Path, files: &[(&str, &[u8])]) {
    std::fs::create_dir_all(directory).unwrap();
    git(directory, &["init", "-q", "-b", "main"]);
    for (name, content) in files {
        std::fs::write(directory.join(name), content).unwrap();
    }
    git(directory, &["add", "."]);
    git(directory, &["commit", "-q", "-m", "base", "--allow-empty"]);
}

fn uri(path: &Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

fn product(cx: &mut TestAppContext) -> (TestProduct, TestWindow) {
    let product = TestProduct::launch(knot_ext_vcs::PACKAGES, cx);
    let window = product.active_window(cx);
    (product, window)
}

fn bars(
    window: &TestWindow,
    tab: usize,
    cx: &mut TestAppContext,
) -> Vec<(usize, &'static str, String)> {
    window.gutter_bars(tab, cx)
}

fn added(line: usize) -> (usize, &'static str, String) {
    (line, "bar", "diff.added".to_owned())
}

/// Opens the edited sentinel `marker.txt` of a repository after the files
/// under test and waits for its mark, so what stayed empty had its turn.
fn settle(window: &TestWindow, directory: &Path, tab: usize, cx: &mut TestAppContext) {
    window.open_path(&directory.join("marker.txt"), cx);
    wait_until(cx, |cx| !bars(window, tab, cx).is_empty());
}

#[gpui::test]
fn a_committed_file_that_differs_from_head_shows_its_hunks(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    repository(directory.path(), &[("file.txt", BASE.as_bytes())]);
    std::fs::write(directory.path().join("file.txt"), "a\nB\nc\nd\ne\n").unwrap();
    let (_product, window) = product(cx);
    window.set_roots(&[&uri(directory.path())], cx);
    window.open_path(&directory.path().join("file.txt"), cx);
    wait_until(cx, |cx| !bars(&window, 0, cx).is_empty());
    assert_eq!(
        bars(&window, 0, cx),
        [(1, "bar", "diff.modified".to_owned()), added(4)]
    );
}

#[gpui::test]
fn line_ending_conversion_compares_cleanly(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    repository(
        directory.path(),
        &[
            (".gitattributes", b"*.txt text eol=crlf\n"),
            ("file.txt", BASE.as_bytes()),
            ("marker.txt", b"m\n"),
        ],
    );
    // The working tree has the converted form; HEAD stores LF.
    std::fs::write(
        directory.path().join("file.txt"),
        BASE.replace('\n', "\r\n"),
    )
    .unwrap();
    std::fs::write(directory.path().join("marker.txt"), "m\nchanged\n").unwrap();
    let (_product, window) = product(cx);
    window.set_roots(&[&uri(directory.path())], cx);
    window.open_path(&directory.path().join("file.txt"), cx);
    settle(&window, directory.path(), 1, cx);
    assert!(bars(&window, 0, cx).is_empty());
}

#[gpui::test]
fn untracked_ignored_and_binary_files_have_no_gutter(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    repository(
        directory.path(),
        &[
            (".gitignore", b"ignored.txt\n"),
            ("binary.bin", b"\0\x01\x02"),
            ("marker.txt", b"m\n"),
        ],
    );
    std::fs::write(directory.path().join("untracked.txt"), BASE).unwrap();
    std::fs::write(directory.path().join("ignored.txt"), BASE).unwrap();
    std::fs::write(directory.path().join("binary.bin"), b"\0\x01\x02\x03").unwrap();
    std::fs::write(directory.path().join("marker.txt"), "m\nchanged\n").unwrap();
    let (_product, window) = product(cx);
    window.set_roots(&[&uri(directory.path())], cx);
    window.open_path(&directory.path().join("untracked.txt"), cx);
    window.open_path(&directory.path().join("ignored.txt"), cx);
    window.open_path(&directory.path().join("binary.bin"), cx);
    settle(&window, directory.path(), 3, cx);
    for tab in 0..3 {
        assert!(bars(&window, tab, cx).is_empty(), "tab {tab}");
    }
}

#[gpui::test]
fn a_file_added_to_the_index_but_not_to_head_is_entirely_added(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    repository(directory.path(), &[("marker.txt", b"m\n")]);
    std::fs::write(directory.path().join("new.txt"), "x\ny\n").unwrap();
    git(directory.path(), &["add", "new.txt"]);
    let (_product, window) = product(cx);
    window.set_roots(&[&uri(directory.path())], cx);
    window.open_path(&directory.path().join("new.txt"), cx);
    wait_until(cx, |cx| !bars(&window, 0, cx).is_empty());
    assert_eq!(bars(&window, 0, cx), [added(0), added(1)]);
}

#[gpui::test]
fn a_repository_without_commits_has_no_gutter(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "-q", "-b", "main"]);
    std::fs::write(directory.path().join("file.txt"), BASE).unwrap();
    let other = tempfile::tempdir().unwrap();
    repository(other.path(), &[("marker.txt", b"m\n")]);
    std::fs::write(other.path().join("marker.txt"), "m\nchanged\n").unwrap();
    let (_product, window) = product(cx);
    window.set_roots(&[&uri(directory.path()), &uri(other.path())], cx);
    window.open_path(&directory.path().join("file.txt"), cx);
    settle(&window, other.path(), 1, cx);
    assert!(bars(&window, 0, cx).is_empty());
}

#[gpui::test]
fn a_nested_repository_and_a_submodule_resolve_to_the_deepest_root(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let outer = directory.path();
    repository(
        outer,
        &[("file.txt", BASE.as_bytes()), ("marker.txt", b"m\n")],
    );
    // An independent clone below the outer repository, and a submodule.
    repository(&outer.join("clone"), &[("file.txt", b"x\ny\n")]);
    let source = tempfile::tempdir().unwrap();
    repository(source.path(), &[("file.txt", b"p\nq\n")]);
    git(
        outer,
        &[
            "submodule",
            "add",
            "-q",
            source.path().to_str().unwrap(),
            "module",
        ],
    );
    git(outer, &["commit", "-q", "-m", "module"]);

    // The same edit, `z`, appended to each file: only the deepest repository's
    // baseline makes the outcome the one line the file really added.
    std::fs::write(outer.join("file.txt"), "a\nb\nc\nd\nz\n").unwrap();
    std::fs::write(outer.join("clone/file.txt"), "x\ny\nz\n").unwrap();
    std::fs::write(outer.join("module/file.txt"), "p\nq\nz\n").unwrap();
    std::fs::write(outer.join("marker.txt"), "m\nchanged\n").unwrap();
    let (_product, window) = product(cx);
    window.set_roots(&[&uri(outer)], cx);
    window.open_path(&outer.join("clone/file.txt"), cx);
    window.open_path(&outer.join("module/file.txt"), cx);
    settle(&window, outer, 2, cx);
    wait_until(cx, |cx| {
        !bars(&window, 0, cx).is_empty() && !bars(&window, 1, cx).is_empty()
    });
    assert_eq!(bars(&window, 0, cx), [added(2)]);
    assert_eq!(bars(&window, 1, cx), [added(2)]);
}

#[gpui::test]
fn a_file_outside_the_roots_is_found_from_its_own_directory(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    repository(directory.path(), &[("file.txt", BASE.as_bytes())]);
    std::fs::write(directory.path().join("file.txt"), "a\nb\nc\nd\ne\n").unwrap();
    let (_product, window) = product(cx);
    window.open_path(&directory.path().join("file.txt"), cx);
    wait_until(cx, |cx| !bars(&window, 0, cx).is_empty());
    assert_eq!(bars(&window, 0, cx), [added(4)]);
}

#[gpui::test]
fn repositories_below_a_root_that_is_not_one_are_found(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    repository(
        &directory.path().join("one"),
        &[("file.txt", BASE.as_bytes())],
    );
    std::fs::write(directory.path().join("one/file.txt"), "a\nb\nc\nd\ne\n").unwrap();
    let (_product, window) = product(cx);
    window.set_roots(&[&uri(directory.path())], cx);
    window.open_path(&directory.path().join("one/file.txt"), cx);
    wait_until(cx, |cx| !bars(&window, 0, cx).is_empty());
    assert_eq!(bars(&window, 0, cx), [added(4)]);
}

/// A repository whose `file.txt` has a line added after HEAD, open in a window
/// whose gutter has shown that line.
fn open_with_added_line(directory: &Path, cx: &mut TestAppContext) -> (TestProduct, TestWindow) {
    repository(
        directory,
        &[("file.txt", BASE.as_bytes()), ("marker.txt", b"m\n")],
    );
    std::fs::write(directory.join("file.txt"), "a\nb\nc\nd\ne\n").unwrap();
    let (product, window) = product(cx);
    product.watch_files(cx);
    window.set_roots(&[&uri(directory)], cx);
    window.open_path(&directory.join("file.txt"), cx);
    wait_until(cx, |cx| bars(&window, 0, cx) == [added(4)]);
    (product, window)
}

#[gpui::test]
fn a_commit_made_outside_knot_clears_the_marks_and_a_soft_reset_brings_them_back(
    cx: &mut TestAppContext,
) {
    let directory = resolved_tempdir();
    let (_product, window) = open_with_added_line(directory.path(), cx);
    git(directory.path(), &["commit", "-q", "-a", "-m", "more"]);
    wait_until(cx, |cx| bars(&window, 0, cx).is_empty());
    git(directory.path(), &["reset", "-q", "--soft", "HEAD~1"]);
    wait_until(cx, |cx| bars(&window, 0, cx) == [added(4)]);
}

#[gpui::test]
fn a_branch_switch_outside_knot_changes_the_baseline(cx: &mut TestAppContext) {
    let directory = resolved_tempdir();
    let (_product, window) = open_with_added_line(directory.path(), cx);
    // A branch whose file already has the added line, reached without
    // touching the working tree.
    git(directory.path(), &["branch", "other"]);
    git(directory.path(), &["checkout", "-q", "other"]);
    git(directory.path(), &["commit", "-q", "-a", "-m", "more"]);
    wait_until(cx, |cx| bars(&window, 0, cx).is_empty());
    git(
        directory.path(),
        &["symbolic-ref", "HEAD", "refs/heads/main"],
    );
    wait_until(cx, |cx| bars(&window, 0, cx) == [added(4)]);
}

#[gpui::test]
fn staging_a_file_outside_knot_gives_a_new_file_its_baseline(cx: &mut TestAppContext) {
    let directory = resolved_tempdir();
    let (_product, window) = open_with_added_line(directory.path(), cx);
    std::fs::write(directory.path().join("new.txt"), "x\ny\n").unwrap();
    window.open_path(&directory.path().join("new.txt"), cx);
    // Untracked until added to the index; the sentinel settles it.
    std::fs::write(directory.path().join("marker.txt"), "m\nchanged\n").unwrap();
    settle(&window, directory.path(), 2, cx);
    assert!(bars(&window, 1, cx).is_empty());
    git(directory.path(), &["add", "new.txt"]);
    wait_until(cx, |cx| bars(&window, 1, cx) == [added(0), added(1)]);
}

#[gpui::test]
fn deleting_the_repository_ends_the_gutter(cx: &mut TestAppContext) {
    let directory = resolved_tempdir();
    let (_product, window) = open_with_added_line(directory.path(), cx);
    std::fs::remove_dir_all(directory.path().join(".git")).unwrap();
    wait_until(cx, |cx| bars(&window, 0, cx).is_empty());
}
