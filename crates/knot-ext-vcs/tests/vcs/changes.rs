//! The Changes view over temporary repositories made with the `git` program.
use std::path::Path;
use std::process::Command;

use gpui::TestAppContext;
use knot::test_support::{
    CommandOutcome, TestProduct, TestTree, TestWindow, resolved_tempdir, wait_until,
};

use super::texts;

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false"])
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

fn write(directory: &Path, name: &str, content: &str) {
    let path = directory.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn uri(path: &Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

const TREE: [&str; 8] = [
    "Staged",
    "s.txt",
    "Unstaged",
    "src",
    "b.txt",
    "a.txt",
    "Untracked",
    "new.txt",
];

/// A repository whose `s.txt` is staged, `a.txt` and `src/b.txt` are edited,
/// and `new.txt` is untracked, shown in the Changes view.
fn show(cx: &mut TestAppContext) -> (TestProduct, TestWindow, TestTree, tempfile::TempDir) {
    let directory = resolved_tempdir();
    let root = directory.path();
    git(root, &["init", "-q", "-b", "main"]);
    write(root, "a.txt", "a1\n");
    write(root, "s.txt", "s1\n");
    write(root, "src/b.txt", "b1\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    write(root, "s.txt", "s2\n");
    git(root, &["add", "s.txt"]);
    write(root, "a.txt", "a2\n");
    write(root, "src/b.txt", "b2\n");
    write(root, "new.txt", "new\n");

    let product = TestProduct::launch(knot_ext_vcs::PACKAGES, cx);
    // Changes made with `git` need the operating system's file events.
    product.watch_files(cx);
    let window = product.active_window(cx);
    window.set_roots(&[&uri(root)], cx);
    let tree = window.tree("vcs.changes", cx);
    wait_until(cx, |cx| tree.labels(cx) == TREE);
    (product, window, tree, directory)
}

#[gpui::test]
fn the_groups_list_their_files_in_folders_and_a_window_without_repository_says_so(
    cx: &mut TestAppContext,
) {
    let (_product, window, tree, directory) = show(cx);
    let outside = resolved_tempdir();
    window.set_roots(&[&uri(outside.path())], cx);
    wait_until(cx, |cx| tree.labels(cx) == ["No repository"]);
    window.set_roots(&[&uri(directory.path())], cx);
    wait_until(cx, |cx| tree.labels(cx) == TREE);
}

#[gpui::test]
fn the_toggle_switches_between_folders_and_a_flat_list(cx: &mut TestAppContext) {
    let (_product, window, tree, _directory) = show(cx);
    assert_eq!(tree.frame_commands(cx), ["vcs.changes.toggleLayout"]);
    assert_eq!(
        window.run_command("vcs.changes.toggleLayout", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| {
        tree.labels(cx)
            == [
                "Staged",
                "s.txt",
                "Unstaged",
                "a.txt",
                "b.txt",
                "Untracked",
                "new.txt",
            ]
    });
    window.run_command("vcs.changes.toggleLayout", cx);
    wait_until(cx, |cx| tree.labels(cx) == TREE);
}

#[gpui::test]
fn a_change_made_with_git_moves_the_file_between_groups(cx: &mut TestAppContext) {
    let (_product, _window, tree, directory) = show(cx);
    git(directory.path(), &["add", "a.txt"]);
    wait_until(cx, |cx| {
        tree.labels(cx)
            == [
                "Staged",
                "a.txt",
                "s.txt",
                "Unstaged",
                "src",
                "b.txt",
                "Untracked",
                "new.txt",
            ]
    });
}

#[gpui::test]
fn double_click_opens_the_staged_diff_and_a_second_request_replaces_it(cx: &mut TestAppContext) {
    let (_product, window, tree, _directory) = show(cx);
    // A click only selects.
    tree.gesture("s.txt", "click", cx);
    assert!(window.composite_titles(cx).is_empty());

    tree.gesture("s.txt", "doubleClick", cx);
    wait_until(cx, |cx| !texts(&window, cx).is_empty());
    assert_eq!(texts(&window, cx), [("s1\n".to_owned(), "s2\n".to_owned())]);

    // Another file keeps its own diff, and the same file has one at most.
    tree.gesture("b.txt", "doubleClick", cx);
    wait_until(cx, |cx| window.composite_titles(cx).len() == 2);
    tree.gesture("s.txt", "enter", cx);
    wait_until(cx, |cx| texts(&window, cx).len() == 1);
    assert_eq!(window.composite_titles(cx).len(), 2);
}

#[gpui::test]
fn an_unstaged_diff_compares_the_index_with_the_working_file(cx: &mut TestAppContext) {
    let (_product, window, tree, _directory) = show(cx);
    tree.gesture("a.txt", "doubleClick", cx);
    wait_until(cx, |cx| !texts(&window, cx).is_empty());
    assert_eq!(texts(&window, cx), [("a1\n".to_owned(), "a2\n".to_owned())]);
}

#[gpui::test]
fn an_untracked_diff_compares_nothing_with_the_working_file(cx: &mut TestAppContext) {
    let (_product, window, tree, _directory) = show(cx);
    tree.gesture("new.txt", "enter", cx);
    wait_until(cx, |cx| !texts(&window, cx).is_empty());
    assert_eq!(texts(&window, cx), [(String::new(), "new\n".to_owned())]);
}

#[gpui::test]
fn an_open_diff_closes_when_the_file_has_no_change_left_in_its_group(cx: &mut TestAppContext) {
    let (_product, window, tree, directory) = show(cx);
    tree.gesture("s.txt", "doubleClick", cx);
    wait_until(cx, |cx| !texts(&window, cx).is_empty());
    git(directory.path(), &["commit", "-q", "-m", "s", "s.txt"]);
    wait_until(cx, |cx| window.composite_titles(cx).is_empty());
}
