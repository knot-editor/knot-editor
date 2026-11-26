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
await commands.register("probe.compare", async () => {
  const request = JSON.parse(await workbench.readClipboardText());
  const client = await backendContract("git").client();
  await workbench.writeClipboardText(JSON.stringify(await client.compare(request)));
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

/// The answer of `status` for `directory`.
fn answer(window: &TestWindow, directory: &Path, cx: &mut TestAppContext) -> Value {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(uri(directory)));
    assert_eq!(
        window.run_command("probe.status", cx),
        CommandOutcome::Completed
    );
    let text = cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap());
    serde_json::from_str(&text).unwrap()
}

/// The groups of `directory` as `(id, sorted relative paths)`, in the backend's order.
fn groups(
    window: &TestWindow,
    directory: &Path,
    cx: &mut TestAppContext,
) -> Vec<(String, Vec<String>)> {
    let root = uri(directory);
    let answer = answer(window, directory, cx);
    answer["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|group| {
            let mut files: Vec<String> = group["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|file| {
                    file.as_str()
                        .unwrap()
                        .strip_prefix(root.trim_end_matches('/'))
                        .unwrap()
                        .trim_start_matches('/')
                        .to_owned()
                })
                .collect();
            files.sort();
            (group["id"].as_str().unwrap().to_owned(), files)
        })
        .collect()
}

/// `compare` of `file` in `group`: `(original, modified)` as `label: text`, or `"document"` for the working file.
fn compare(
    window: &TestWindow,
    directory: &Path,
    group: &str,
    file: &str,
    cx: &mut TestAppContext,
) -> Option<(String, String)> {
    let root = uri(directory);
    let request = serde_json::json!({
        "repository": root,
        "group": group,
        "file": format!("{}/{file}", root.trim_end_matches('/')),
    });
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(request.to_string()));
    assert_eq!(
        window.run_command("probe.compare", cx),
        CommandOutcome::Completed
    );
    let text = cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap());
    let sides: Value = serde_json::from_str(&text).unwrap();
    if sides.is_null() {
        return None;
    }
    let side = |side: &Value| match side.as_str() {
        Some(document) => document.to_owned(),
        None => format!(
            "{}: {:?}",
            side["label"].as_str().unwrap(),
            side["text"].as_str().unwrap()
        ),
    };
    Some((side(&sides["original"]), side(&sides["modified"])))
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
    let entries: Vec<Value> = serde_json::from_str::<Value>(&text).unwrap()["files"]
        .as_array()
        .unwrap()
        .clone();
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

#[gpui::test]
fn a_huge_untracked_folder_is_one_entry_and_the_rest_is_still_listed(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    repository(root, &[("kept.txt", "a\n")]);
    write(root, "kept.txt", "edited\n");
    for index in 0..2100 {
        write(root, &format!("vendor/f{index}.txt"), "x\n");
    }
    write(root, "few/one.txt", "x\n");

    let (_product, window) = launch(cx);
    // Past the limit each untracked folder is one entry, including the small one.
    expect(
        status(&window, root, cx),
        &[
            ("few", "untracked"),
            ("kept.txt", "modified"),
            ("vendor", "untracked"),
        ],
    );
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[gpui::test]
fn files_are_grouped_by_where_their_change_is(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    repository(
        root,
        &[
            ("clean.txt", "clean\n"),
            ("twice.txt", "a\n"),
            ("staged.txt", "a\n"),
            ("edited.txt", "a\n"),
            ("moved.txt", "one\ntwo\nthree\nfour\n"),
        ],
    );
    write(root, "twice.txt", "b\n");
    git(root, &["add", "twice.txt"]);
    write(root, "twice.txt", "c\n");
    write(root, "staged.txt", "b\n");
    git(root, &["add", "staged.txt"]);
    write(root, "edited.txt", "b\n");
    git(root, &["mv", "moved.txt", "renamed.txt"]);
    write(root, "fresh.txt", "new\n");

    let (_product, window) = launch(cx);
    assert_eq!(
        groups(&window, root, cx),
        vec![
            (
                "staged".to_owned(),
                strings(&["renamed.txt", "staged.txt", "twice.txt"])
            ),
            ("unstaged".to_owned(), strings(&["edited.txt", "twice.txt"])),
            ("untracked".to_owned(), strings(&["fresh.txt"])),
        ]
    );
}

#[gpui::test]
fn a_conflict_has_its_own_group_and_a_clean_repository_has_empty_ones(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    repository(root, &[("both.txt", "base\n")]);
    let (_product, window) = launch(cx);
    assert_eq!(
        groups(&window, root, cx),
        vec![
            ("staged".to_owned(), vec![]),
            ("unstaged".to_owned(), vec![]),
            ("untracked".to_owned(), vec![]),
        ]
    );
    git(root, &["checkout", "-q", "-b", "side"]);
    write(root, "both.txt", "side\n");
    git(root, &["commit", "-q", "-am", "side"]);
    git(root, &["checkout", "-q", "main"]);
    write(root, "both.txt", "main\n");
    git(root, &["commit", "-q", "-am", "main"]);
    git(root, &["merge", "side"]);
    let found = groups(&window, root, cx);
    assert_eq!(found[2], ("conflicts".to_owned(), strings(&["both.txt"])));
    assert_eq!(found[0].1, Vec::<String>::new());
    assert_eq!(found[1].1, Vec::<String>::new());
}

#[gpui::test]
fn each_group_declares_its_actions(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    repository(root, &[("a.txt", "a\n")]);
    let (_product, window) = launch(cx);
    let answer = answer(&window, root, cx);
    let actions = |id: &str| -> Vec<(String, String, Vec<String>)> {
        answer["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|group| group["id"] == id)
            .unwrap()["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|action| {
                (
                    action["label"].as_str().unwrap().to_owned(),
                    action["command"].as_str().unwrap().to_owned(),
                    action["on"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|on| on.as_str().unwrap().to_owned())
                        .collect(),
                )
            })
            .collect()
    };
    let labels = |id: &str| {
        actions(id)
            .into_iter()
            .map(|(label, _, _)| label)
            .collect::<Vec<_>>()
    };
    assert_eq!(labels("staged"), ["Unstage", "Unstage All", "Rollback"]);
    assert_eq!(
        labels("unstaged"),
        ["Stage", "Stage All", "Discard", "Rollback"]
    );
    assert_eq!(labels("untracked"), ["Stage", "Stage All", "Discard"]);
    assert_eq!(
        actions("staged")[1],
        (
            "Unstage All".to_owned(),
            "git.unstage".to_owned(),
            strings(&["group"])
        )
    );
}

#[gpui::test]
fn compare_gives_each_groups_sides(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    repository(
        root,
        &[
            ("twice.txt", "a\n"),
            ("gone.txt", "a\n"),
            ("moved.txt", "one\ntwo\nthree\nfour\n"),
            ("both.txt", "base\n"),
        ],
    );
    write(root, "twice.txt", "b\n");
    git(root, &["add", "twice.txt"]);
    write(root, "twice.txt", "c\n");
    std::fs::remove_file(root.join("gone.txt")).unwrap();
    git(root, &["mv", "moved.txt", "renamed.txt"]);
    write(root, "fresh.txt", "new\n");
    write(root, "added.txt", "new\n");
    git(root, &["add", "added.txt"]);

    let (_product, window) = launch(cx);
    let some = |original: &str, modified: &str| Some((original.to_owned(), modified.to_owned()));
    assert_eq!(
        compare(&window, root, "staged", "twice.txt", cx),
        some("HEAD: \"a\\n\"", "Index: \"b\\n\"")
    );
    assert_eq!(
        compare(&window, root, "unstaged", "twice.txt", cx),
        some("Index: \"b\\n\"", "document")
    );
    assert_eq!(
        compare(&window, root, "unstaged", "gone.txt", cx),
        some("Index: \"a\\n\"", "Working Copy: \"\"")
    );
    assert_eq!(
        compare(&window, root, "staged", "renamed.txt", cx),
        some(
            "HEAD: \"one\\ntwo\\nthree\\nfour\\n\"",
            "Index: \"one\\ntwo\\nthree\\nfour\\n\""
        )
    );
    assert_eq!(
        compare(&window, root, "staged", "added.txt", cx),
        some("HEAD: \"\"", "Index: \"new\\n\"")
    );
    assert_eq!(
        compare(&window, root, "untracked", "fresh.txt", cx),
        some("Empty: \"\"", "document")
    );
    // A file with no change in the group has no diff there.
    assert_eq!(compare(&window, root, "unstaged", "fresh.txt", cx), None);
    assert_eq!(compare(&window, root, "staged", "both.txt", cx), None);
    assert_eq!(compare(&window, root, "conflicts", "both.txt", cx), None);

    git(root, &["checkout", "-q", "-b", "side"]);
    git(root, &["stash", "-q", "-u"]);
    write(root, "both.txt", "side\n");
    git(root, &["commit", "-q", "-am", "side"]);
    git(root, &["checkout", "-q", "main"]);
    write(root, "both.txt", "main\n");
    git(root, &["commit", "-q", "-am", "main"]);
    git(root, &["merge", "side"]);
    assert_eq!(
        compare(&window, root, "conflicts", "both.txt", cx),
        some("HEAD: \"main\\n\"", "document")
    );
}

#[gpui::test]
fn compare_gives_no_diff_for_binary_files_or_the_collapsed_folder(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    repository(root, &[("image.bin", "a\0b\n")]);
    std::fs::write(root.join("image.bin"), "a\0c\n").unwrap();
    write(root, "folder/inside.txt", "x\n");
    let (_product, window) = launch(cx);
    assert_eq!(compare(&window, root, "unstaged", "image.bin", cx), None);
    assert_eq!(compare(&window, root, "untracked", "folder", cx), None);
}
