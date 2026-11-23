//! The Git backend's `status` through its contract, called by a test package
//! over temporary repositories made with the `git` program.
use std::path::Path;
use std::process::Command;

use gpui::TestAppContext;
use knot::test_support::{CommandOutcome, TestProduct, TestWindow};
use serde_json::Value;

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success() || args[0] == "merge",
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repository(directory: &Path, files: &[(&str, &str)]) {
    std::fs::create_dir_all(directory).unwrap();
    git(directory, &["init", "-q", "-b", "main"]);
    for (name, content) in files {
        write(directory, name, content);
    }
    git(directory, &["add", "."]);
    git(directory, &["commit", "-q", "-m", "base", "--allow-empty"]);
}

fn write(directory: &Path, name: &str, content: &str) {
    let path = directory.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn uri(path: &Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

const PROBE_MANIFEST: &str = r#"{ "name": "@knot/test-probe", "apiVersion": 1, "version": "0.1.0", "main": "dist/main.js", "requires": ["@knot/vcs"] }"#;
/// Calls the Git backend's `status` for the repository URI on the clipboard.
const PROBE: &str = r#"
import { backendContract } from "@knot/vcs";
import { commands, workbench } from "knot";
await commands.register("probe.status", async () => {
  const repository = await workbench.readClipboardText();
  const client = await backendContract("git").client();
  await workbench.writeClipboardText(JSON.stringify(await client.status(repository)));
});
"#;

fn launch(cx: &mut TestAppContext) -> (TestProduct, TestWindow) {
    let mut packages = knot_ext_vcs::PACKAGES.to_vec();
    packages.push(knot_bundled::BundledSource {
        manifest: PROBE_MANIFEST,
        sources: &[("dist/main.js", PROBE)],
    });
    let product = TestProduct::launch(Box::leak(packages.into_boxed_slice()), cx);
    let window = product.active_window(cx);
    (product, window)
}

/// The changed files of `directory` as sorted `(path, status)` pairs.
fn status(window: &TestWindow, directory: &Path, cx: &mut TestAppContext) -> Vec<(String, String)> {
    let root = uri(directory);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(root.clone()));
    assert_eq!(
        window.run_command("probe.status", cx),
        CommandOutcome::Completed
    );
    let text = cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap());
    let entries: Vec<Value> = serde_json::from_str(&text).unwrap();
    let mut found: Vec<(String, String)> = entries
        .iter()
        .map(|entry| {
            let file = entry["file"].as_str().unwrap();
            (
                file.strip_prefix(root.trim_end_matches('/'))
                    .unwrap()
                    .trim_start_matches('/')
                    .to_owned(),
                entry["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    found.sort();
    found
}

fn expect(found: Vec<(String, String)>, expected: &[(&str, &str)]) {
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(path, status)| ((*path).to_owned(), (*status).to_owned()))
        .collect();
    assert_eq!(found, expected);
}

#[gpui::test]
fn each_status_is_measured_against_head(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    repository(
        root,
        &[
            ("clean.txt", "clean\n"),
            ("edited.txt", "a\n"),
            ("staged.txt", "a\n"),
            ("removed.txt", "a\n"),
            ("staged-removed.txt", "a\n"),
            ("recreated.txt", "a\n"),
            ("moved.txt", "one\ntwo\nthree\nfour\n"),
            ("later.txt", "one\ntwo\nthree\nfour\n"),
            (".gitignore", "ignored.txt\nbuild/\n"),
        ],
    );
    write(root, "edited.txt", "b\n");
    // Staged and then edited again stays modified.
    write(root, "staged.txt", "b\n");
    git(root, &["add", "staged.txt"]);
    write(root, "staged.txt", "c\n");
    std::fs::remove_file(root.join("removed.txt")).unwrap();
    git(root, &["rm", "-q", "staged-removed.txt"]);
    // Removed from the index and put back on disk: HEAD has it and it differs.
    git(root, &["rm", "-q", "--cached", "recreated.txt"]);
    // A staged rename decorates the new path.
    git(root, &["mv", "moved.txt", "renamed.txt"]);
    // An unstaged rename is a deletion plus an untracked file.
    std::fs::rename(root.join("later.txt"), root.join("later-new.txt")).unwrap();
    // Added to the index.
    write(root, "added.txt", "new\n");
    git(root, &["add", "added.txt"]);
    // Added to the index and then removed from disk equals HEAD.
    write(root, "vanished.txt", "new\n");
    git(root, &["add", "vanished.txt"]);
    std::fs::remove_file(root.join("vanished.txt")).unwrap();
    write(root, "fresh.txt", "new\n");
    write(root, "folder/deep/inside.txt", "new\n");
    write(root, "ignored.txt", "x\n");
    write(root, "build/out.txt", "x\n");

    let (_product, window) = launch(cx);
    expect(
        status(&window, root, cx),
        &[
            ("added.txt", "added"),
            ("edited.txt", "modified"),
            ("folder/deep/inside.txt", "untracked"),
            ("fresh.txt", "untracked"),
            ("later-new.txt", "untracked"),
            ("later.txt", "deleted"),
            ("recreated.txt", "modified"),
            ("removed.txt", "deleted"),
            ("renamed.txt", "renamed"),
            ("staged-removed.txt", "deleted"),
            ("staged.txt", "modified"),
        ],
    );
}

#[gpui::test]
fn a_failed_merge_conflicts_its_files(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    repository(root, &[("both.txt", "base\n"), ("other.txt", "base\n")]);
    git(root, &["checkout", "-q", "-b", "side"]);
    write(root, "both.txt", "side\n");
    git(root, &["commit", "-q", "-am", "side"]);
    git(root, &["checkout", "-q", "main"]);
    write(root, "both.txt", "main\n");
    git(root, &["commit", "-q", "-am", "main"]);
    git(root, &["merge", "side"]);

    let (_product, window) = launch(cx);
    expect(status(&window, root, cx), &[("both.txt", "conflicted")]);
}

#[gpui::test]
fn a_file_with_a_space_and_a_nested_repository_are_listed(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    repository(root, &[("kept.txt", "a\n")]);
    write(root, "with space/é.txt", "new\n");
    repository(&root.join("nested"), &[("inner.txt", "a\n")]);

    let (_product, window) = launch(cx);
    // The nested repository is one untracked folder; its files are its own.
    expect(
        status(&window, root, cx),
        &[
            ("nested", "untracked"),
            ("with%20space/%C3%A9.txt", "untracked"),
        ],
    );
    expect(status(&window, &root.join("nested"), cx), &[]);
}

#[gpui::test]
fn a_repository_without_changes_or_commits_is_listed_as_it_is(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    git(root, &["init", "-q", "-b", "main"]);
    let (_product, window) = launch(cx);
    expect(status(&window, root, cx), &[]);
    write(root, "new.txt", "a\n");
    git(root, &["add", "new.txt"]);
    write(root, "loose.txt", "a\n");
    expect(
        status(&window, root, cx),
        &[("loose.txt", "untracked"), ("new.txt", "added")],
    );
}
