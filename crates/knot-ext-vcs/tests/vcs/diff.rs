//! The diff of a file against its baseline in the launched product, over a
//! temporary repository made with the `git` program.
use std::path::Path;
use std::process::Command;

use gpui::TestAppContext;
use knot::test_support::{CommandOutcome, TestProduct, resolved_tempdir, wait_until};

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

fn uri(path: &Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

#[gpui::test]
fn the_diff_of_a_file_follows_a_commit(cx: &mut TestAppContext) {
    let directory = resolved_tempdir();
    let root = directory.path();
    git(root, &["init", "-q", "-b", "main"]);
    // The baseline keeps the LF the test writes, whatever the machine's Git config says.
    git(root, &["config", "core.autocrlf", "false"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    std::fs::write(root.join("a.txt"), "two\n").unwrap();

    let product = TestProduct::launch(knot_ext_vcs::PACKAGES, cx);
    // The commit needs the operating system's file events.
    product.watch_files(cx);
    let window = product.active_window(cx);
    window.set_roots(&[&uri(root)], cx);
    window.open_path(&root.join("a.txt"), cx);
    wait_until(cx, |cx| !window.gutter_bars(0, cx).is_empty());
    window.focus_document(cx);

    assert_eq!(
        window.run_command("vcs.showChanges", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| !texts(&window, cx).is_empty());
    assert_eq!(
        texts(&window, cx),
        [("one\n".to_owned(), "two\n".to_owned())]
    );

    git(root, &["commit", "-q", "-am", "two"]);
    wait_until(cx, |cx| {
        texts(&window, cx) == [("two\n".to_owned(), "two\n".to_owned())]
    });
}
