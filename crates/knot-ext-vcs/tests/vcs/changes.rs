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
    show_in(false, cx)
}

/// `show` in a native window, which prompts need.
fn show_windowed(
    cx: &mut TestAppContext,
) -> (TestProduct, TestWindow, TestTree, tempfile::TempDir) {
    show_in(true, cx)
}

fn show_in(
    windowed: bool,
    cx: &mut TestAppContext,
) -> (TestProduct, TestWindow, TestTree, tempfile::TempDir) {
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

    let product = if windowed {
        TestProduct::launch_windowed(knot_ext_vcs::PACKAGES, cx)
    } else {
        TestProduct::launch(knot_ext_vcs::PACKAGES, cx)
    };
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
fn click_opens_a_preview_diff_that_the_next_click_replaces(cx: &mut TestAppContext) {
    let (_product, window, tree, _directory) = show(cx);
    tree.gesture("s.txt", "click", cx);
    wait_until(cx, |cx| !texts(&window, cx).is_empty());
    assert_eq!(texts(&window, cx), [("s1\n".to_owned(), "s2\n".to_owned())]);
    assert_eq!(window.composite_previews(cx), [true]);

    // The working file has no tab of its own.
    tree.gesture("b.txt", "click", cx);
    wait_until(cx, |cx| {
        texts(&window, cx) == [("b1\n".to_owned(), "b2\n".to_owned())]
    });
    assert_eq!(window.composite_titles(cx).len(), 1);
    assert_eq!(window.composite_previews(cx), [true]);
    assert!(window.document_texts(cx).is_empty());
}

#[gpui::test]
fn double_click_opens_a_regular_diff_and_a_second_request_reuses_it(cx: &mut TestAppContext) {
    let (_product, window, tree, _directory) = show(cx);
    tree.gesture("s.txt", "doubleClick", cx);
    wait_until(cx, |cx| !texts(&window, cx).is_empty());
    assert_eq!(texts(&window, cx), [("s1\n".to_owned(), "s2\n".to_owned())]);
    assert_eq!(window.composite_previews(cx), [false]);

    // Another file keeps its own diff, and the same file has one at most.
    tree.gesture("b.txt", "doubleClick", cx);
    wait_until(cx, |cx| window.composite_titles(cx).len() == 2);
    tree.gesture("s.txt", "enter", cx);
    wait_until(cx, |cx| texts(&window, cx).len() == 1);
    assert_eq!(window.composite_titles(cx).len(), 2);
}

#[gpui::test]
fn a_gesture_on_a_row_whose_preview_is_open_activates_or_keeps_it(cx: &mut TestAppContext) {
    let (_product, window, tree, _directory) = show(cx);
    tree.gesture("s.txt", "click", cx);
    wait_until(cx, |cx| window.composite_previews(cx) == [true]);

    // A click on the same row leaves the one preview as it is.
    tree.gesture("s.txt", "click", cx);
    assert_eq!(window.composite_previews(cx), [true]);

    // Double-click keeps that diff instead of opening another.
    tree.gesture("s.txt", "doubleClick", cx);
    wait_until(cx, |cx| window.composite_previews(cx) == [false]);
    assert_eq!(window.composite_titles(cx).len(), 1);

    // A new preview does not replace the kept diff.
    tree.gesture("b.txt", "click", cx);
    wait_until(cx, |cx| window.composite_titles(cx).len() == 2);
    assert_eq!(window.composite_previews(cx), [false, true]);
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

/// `git status --porcelain` of the repository.
fn porcelain(directory: &Path) -> String {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .current_dir(directory)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap()
}

/// Waits until `git status --porcelain` is `expected`, and says what it was otherwise.
#[track_caller]
fn wait_porcelain(root: &Path, expected: &str, cx: &mut TestAppContext) {
    let last = std::cell::RefCell::new(String::new());
    let waited = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_until(cx, |_| {
            *last.borrow_mut() = porcelain(root);
            *last.borrow() == expected
        })
    }));
    if waited.is_err() {
        assert_eq!(*last.borrow(), expected);
    }
}

/// Runs the action of `row` and, when it asks, answers the prompt with `answer`.
fn act(tree: &TestTree, row: &str, command: &str, answer: Option<&str>, cx: &mut TestAppContext) {
    let id = tree.row_id(row, cx).unwrap();
    tree.invoke(&id, command, cx);
    if let Some(answer) = answer {
        wait_until(cx, |cx| cx.has_pending_prompt());
        cx.simulate_prompt_answer(answer);
    }
}

#[gpui::test]
fn stage_acts_on_a_file_a_folder_and_a_group(cx: &mut TestAppContext) {
    let (_product, _window, tree, directory) = show(cx);
    let root = directory.path();

    act(&tree, "a.txt", "git.stage", None, cx);
    wait_porcelain(root, "M  a.txt\nM  s.txt\n M src/b.txt\n?? new.txt\n", cx);

    act(&tree, "src", "git.stage", None, cx);
    wait_porcelain(root, "M  a.txt\nM  s.txt\nM  src/b.txt\n?? new.txt\n", cx);

    act(&tree, "Untracked", "git.stage", None, cx);
    wait_porcelain(root, "M  a.txt\nA  new.txt\nM  s.txt\nM  src/b.txt\n", cx);
    wait_until(cx, |cx| {
        tree.labels(cx) == ["Staged", "src", "b.txt", "a.txt", "new.txt", "s.txt"]
    });
}

#[gpui::test]
fn unstage_acts_on_a_file_and_a_group(cx: &mut TestAppContext) {
    let (_product, _window, tree, directory) = show(cx);
    let root = directory.path();
    git(root, &["add", "a.txt", "src"]);
    wait_until(cx, |cx| {
        tree.labels(cx)
            .starts_with(&["Staged", "src", "b.txt", "a.txt", "s.txt"].map(str::to_owned))
    });

    act(&tree, "s.txt", "git.unstage", None, cx);
    wait_porcelain(root, "M  a.txt\n M s.txt\nM  src/b.txt\n?? new.txt\n", cx);

    wait_until(cx, |cx| {
        tree.labels(cx)
            == [
                "Staged",
                "src",
                "b.txt",
                "a.txt",
                "Unstaged",
                "s.txt",
                "Untracked",
                "new.txt",
            ]
    });
    act(&tree, "Staged", "git.unstage", None, cx);
    wait_porcelain(root, " M a.txt\n M s.txt\n M src/b.txt\n?? new.txt\n", cx);
}

#[gpui::test]
fn discard_restores_tracked_files_and_deletes_untracked_ones_after_asking(cx: &mut TestAppContext) {
    let (_product, _window, tree, directory) = show_windowed(cx);
    let root = directory.path();

    act(&tree, "a.txt", "git.discard", Some("Cancel"), cx);
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "a2\n");

    act(&tree, "a.txt", "git.discard", Some("Discard"), cx);
    wait_until(cx, |_| {
        std::fs::read_to_string(root.join("a.txt")).unwrap() == "a1\n"
    });
    assert_eq!(porcelain(root), "M  s.txt\n M src/b.txt\n?? new.txt\n");

    act(&tree, "Untracked", "git.discard", Some("Delete"), cx);
    wait_until(cx, |_| !root.join("new.txt").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("src/b.txt")).unwrap(),
        "b2\n"
    );
}

#[gpui::test]
fn rollback_restores_head_and_unstages_an_added_file(cx: &mut TestAppContext) {
    let (_product, _window, tree, directory) = show_windowed(cx);
    let root = directory.path();
    git(root, &["add", "new.txt"]);
    wait_until(cx, |cx| !tree.labels(cx).contains(&"Untracked".to_owned()));

    act(&tree, "Staged", "git.rollback", Some("Roll Back"), cx);
    wait_porcelain(root, " M a.txt\n M src/b.txt\n?? new.txt\n", cx);
    assert_eq!(std::fs::read_to_string(root.join("s.txt")).unwrap(), "s1\n");
    assert_eq!(
        std::fs::read_to_string(root.join("new.txt")).unwrap(),
        "new\n"
    );
}

#[gpui::test]
fn rollback_file_restores_the_focused_file_and_reloads_its_document(cx: &mut TestAppContext) {
    let (_product, window, _tree, directory) = show_windowed(cx);
    let root = directory.path();
    window.open_path(&root.join("a.txt"), cx);
    wait_until(cx, |cx| window.document_texts(cx) == ["a2\n"]);
    window.append("unsaved\n", cx);
    window.focus_document(cx);

    let command = window.start_command("vcs.rollbackFile", cx);
    wait_until(cx, |cx| cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    assert_eq!(command.finish(cx), CommandOutcome::Completed);
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "a2\n");

    let command = window.start_command("vcs.rollbackFile", cx);
    wait_until(cx, |cx| cx.has_pending_prompt());
    cx.simulate_prompt_answer("Roll Back");
    assert_eq!(command.finish(cx), CommandOutcome::Completed);
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "a1\n");
    wait_until(cx, |cx| window.document_texts(cx) == ["a1\n"]);
    assert_eq!(window.document_tabs(cx), [("a.txt".to_owned(), false)]);
}
