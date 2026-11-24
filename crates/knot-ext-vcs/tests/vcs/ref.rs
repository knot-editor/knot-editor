//! The current ref: the Git backend's `ref` through its contract, and the
//! status bar item in the launched product, over temporary repositories made
//! with the `git` program.
use std::path::Path;
use std::process::Command;

use gpui::TestAppContext;
use knot::test_support::{CommandOutcome, TestProduct, TestWindow, resolved_tempdir, wait_until};

fn git(directory: &Path, args: &[&str]) -> String {
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
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// A repository on `main` with one commit.
fn repository(directory: &Path) {
    std::fs::create_dir_all(directory).unwrap();
    git(directory, &["init", "-q", "-b", "main"]);
    std::fs::write(directory.join("a.txt"), "a\n").unwrap();
    git(directory, &["add", "."]);
    git(directory, &["commit", "-q", "-m", "base"]);
}

fn uri(path: &Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

const PROBE_MANIFEST: &str = r#"{ "name": "@knot/test-probe", "apiVersion": 1, "version": "0.1.0", "main": "dist/main.js", "requires": ["@knot/vcs"] }"#;
/// Calls the Git backend's `ref` for the repository URI on the clipboard.
const PROBE: &str = r#"
import { backendContract } from "@knot/vcs";
import { commands, workbench } from "knot";
await commands.register("probe.ref", async () => {
  const repository = await workbench.readClipboardText();
  const client = await backendContract("git").client();
  await workbench.writeClipboardText(JSON.stringify(await client.ref(repository)));
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

/// The backend's answer for `directory`, as JSON.
fn answer(window: &TestWindow, directory: &Path, cx: &mut TestAppContext) -> serde_json::Value {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(uri(directory)));
    assert_eq!(
        window.run_command("probe.ref", cx),
        CommandOutcome::Completed
    );
    let text = cx.read(|cx| cx.read_from_clipboard().unwrap().text().unwrap());
    serde_json::from_str(&text).unwrap()
}

#[gpui::test]
fn a_branch_is_named(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    repository(directory.path());
    git(directory.path(), &["checkout", "-q", "-b", "feature/x"]);
    let (_product, window) = launch(cx);
    let found = answer(&window, directory.path(), cx);
    assert_eq!(found["label"], "feature/x");
}

#[gpui::test]
fn an_unborn_branch_is_named(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "-q", "-b", "fresh"]);
    let (_product, window) = launch(cx);
    assert_eq!(answer(&window, directory.path(), cx)["label"], "fresh");
}

#[gpui::test]
fn a_detached_head_is_the_short_commit(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    repository(directory.path());
    git(directory.path(), &["checkout", "-q", "--detach"]);
    let short = git(directory.path(), &["rev-parse", "--short", "HEAD"]);
    let (_product, window) = launch(cx);
    assert_eq!(
        answer(&window, directory.path(), cx)["label"],
        short.as_str()
    );
}

#[gpui::test]
fn the_status_bar_follows_a_checkout_made_with_git(cx: &mut TestAppContext) {
    let directory = resolved_tempdir();
    let root = directory.path();
    repository(root);
    let (product, window) = launch(cx);
    // The checkout needs the operating system's file events.
    product.watch_files(cx);
    window.set_roots(&[&uri(root)], cx);
    let shows = |text: &str, cx: &mut TestAppContext| {
        let (left, right) = window.status_bar(cx);
        left.iter().chain(&right).any(|item| item == text)
    };
    wait_until(cx, |cx| shows("main", cx));

    git(root, &["checkout", "-q", "--detach"]);
    let short = git(root, &["rev-parse", "--short", "HEAD"]);
    wait_until(cx, |cx| shows(&short, cx));
    assert!(!shows("main", cx));
}
