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

/// A repository in `directory` whose HEAD holds `files`. Line endings are
/// converted only by `.gitattributes`, whatever the machine's Git config says,
/// since the tests write working-tree files directly.
fn repository(directory: &Path, files: &[(&str, &[u8])]) {
    std::fs::create_dir_all(directory).unwrap();
    git(directory, &["init", "-q", "-b", "main"]);
    git(directory, &["config", "core.autocrlf", "false"]);
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

/// As `product`, in a native window, which clicks need.
fn windowed_product(cx: &mut TestAppContext) -> (TestProduct, TestWindow) {
    let product = TestProduct::launch_windowed(knot_ext_vcs::PACKAGES, cx);
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

/// A bar of an added line the index already holds.
fn staged_added(line: usize) -> (usize, &'static str, String) {
    (line, "bar", "diff.added.staged".to_owned())
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
    assert_eq!(bars(&window, 0, cx), [staged_added(0), staged_added(1)]);
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
    git(&outer.join("module"), &["config", "core.autocrlf", "false"]);

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
    // The soft reset leaves the index holding the line.
    git(directory.path(), &["reset", "-q", "--soft", "HEAD~1"]);
    wait_until(cx, |cx| bars(&window, 0, cx) == [staged_added(4)]);
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
    // The index still holds the line.
    wait_until(cx, |cx| bars(&window, 0, cx) == [staged_added(4)]);
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
    wait_until(cx, |cx| {
        bars(&window, 1, cx) == [staged_added(0), staged_added(1)]
    });
}

#[gpui::test]
fn staging_a_file_outside_knot_dims_its_hunks_and_unstaging_restores_them(cx: &mut TestAppContext) {
    let directory = resolved_tempdir();
    repository(directory.path(), &[("file.txt", BASE.as_bytes())]);
    std::fs::write(directory.path().join("file.txt"), "a\nB\nc\nd\n").unwrap();
    let (product, window) = product(cx);
    product.watch_files(cx);
    window.set_roots(&[&uri(directory.path())], cx);
    window.open_path(&directory.path().join("file.txt"), cx);
    let modified = |color: &str| vec![(1, "bar", color.to_owned())];
    wait_until(cx, |cx| bars(&window, 0, cx) == modified("diff.modified"));
    git(directory.path(), &["add", "file.txt"]);
    wait_until(cx, |cx| {
        bars(&window, 0, cx) == modified("diff.modified.staged")
    });
    // Unstaging brings the full color back.
    git(directory.path(), &["reset", "-q", "file.txt"]);
    wait_until(cx, |cx| bars(&window, 0, cx) == modified("diff.modified"));
}

#[gpui::test]
fn deleting_the_repository_ends_the_gutter(cx: &mut TestAppContext) {
    let directory = resolved_tempdir();
    let (_product, window) = open_with_added_line(directory.path(), cx);
    std::fs::remove_dir_all(directory.path().join(".git")).unwrap();
    wait_until(cx, |cx| bars(&window, 0, cx).is_empty());
}

/// What `git diff --cached` shows for `file.txt`: the added and removed lines.
fn cached_changes(directory: &Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--unified=0", "--", "file.txt"])
        .current_dir(directory)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            (line.starts_with('+') || line.starts_with('-'))
                && !line.starts_with("+++")
                && !line.starts_with("---")
        })
        .map(str::to_owned)
        .collect()
}

#[gpui::test]
fn stage_hunk_and_unstage_hunk_from_the_popup_change_only_that_hunk(cx: &mut TestAppContext) {
    let directory = resolved_tempdir();
    repository(directory.path(), &[("file.txt", BASE.as_bytes())]);
    std::fs::write(directory.path().join("file.txt"), "a\nB\nc\nD\n").unwrap();
    let (product, window) = windowed_product(cx);
    product.watch_files(cx);
    window.set_roots(&[&uri(directory.path())], cx);
    window.open_path(&directory.path().join("file.txt"), cx);
    let colors = |first: &str, second: &str| {
        vec![(1, "bar", first.to_owned()), (3, "bar", second.to_owned())]
    };
    wait_until(cx, |cx| {
        bars(&window, 0, cx) == colors("diff.modified", "diff.modified")
    });
    window.focus_document(cx);

    window.click_gutter_bar(1, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    assert!(
        window.zones(cx)[0]
            .buttons
            .contains(&"Stage Hunk".to_owned())
    );
    window.click_zone_button("Stage Hunk", cx);
    wait_until(cx, |cx| {
        bars(&window, 0, cx) == colors("diff.modified.staged", "diff.modified")
    });
    assert_eq!(cached_changes(directory.path()), ["-b", "+B"]);

    // The popup stays open on the hunk, now with a Staged row and the other button.
    wait_until(cx, |cx| {
        window.zones(cx).first().is_some_and(|zone| {
            zone.buttons.contains(&"Unstage Hunk".to_owned())
                && zone.texts.first().map(String::as_str) == Some("Staged")
        })
    });
    assert_eq!(
        window.zones(cx)[0].buttons[..3],
        ["Previous Hunk", "Next Hunk", "Unstage Hunk"]
    );
    window.click_zone_button("Unstage Hunk", cx);
    wait_until(cx, |cx| {
        bars(&window, 0, cx) == colors("diff.modified", "diff.modified")
    });
    assert!(cached_changes(directory.path()).is_empty());
}

#[gpui::test]
fn stage_hunk_stages_an_unsaved_edit_as_the_editor_shows_it(cx: &mut TestAppContext) {
    let directory = resolved_tempdir();
    repository(directory.path(), &[("file.txt", BASE.as_bytes())]);
    let (product, window) = windowed_product(cx);
    product.watch_files(cx);
    window.set_roots(&[&uri(directory.path())], cx);
    window.open_path(&directory.path().join("file.txt"), cx);
    window.focus_document(cx);
    window.press("X", cx);
    wait_until(cx, |cx| {
        bars(&window, 0, cx) == [(0, "bar", "diff.modified".to_owned())]
    });
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    window.click_zone_button("Stage Hunk", cx);
    wait_until(cx, |cx| {
        bars(&window, 0, cx) == [(0, "bar", "diff.modified.staged".to_owned())]
    });
    assert_eq!(cached_changes(directory.path()), ["-a", "+Xa"]);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("file.txt")).unwrap(),
        BASE
    );
}

#[gpui::test]
fn stage_hunk_stores_the_index_entry_as_the_clean_filters_make_it(cx: &mut TestAppContext) {
    let directory = resolved_tempdir();
    repository(
        directory.path(),
        &[
            (".gitattributes", b"*.txt text eol=crlf\n"),
            ("file.txt", BASE.as_bytes()),
        ],
    );
    std::fs::write(directory.path().join("file.txt"), "a\r\nB\r\nc\r\nd\r\n").unwrap();
    let (product, window) = windowed_product(cx);
    product.watch_files(cx);
    window.set_roots(&[&uri(directory.path())], cx);
    window.open_path(&directory.path().join("file.txt"), cx);
    wait_until(cx, |cx| {
        bars(&window, 0, cx) == [(1, "bar", "diff.modified".to_owned())]
    });
    window.focus_document(cx);
    window.click_gutter_bar(1, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    window.click_zone_button("Stage Hunk", cx);
    wait_until(cx, |cx| {
        bars(&window, 0, cx) == [(1, "bar", "diff.modified.staged".to_owned())]
    });
    // The index holds LF text, so the staged change is the one line.
    assert_eq!(cached_changes(directory.path()), ["-b", "+B"]);
}
