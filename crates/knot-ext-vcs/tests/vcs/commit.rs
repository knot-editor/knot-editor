//! The commit panel of the Changes tool window over temporary repositories
//! made with the `git` program.
use std::path::Path;
use std::process::Command;

use gpui::TestAppContext;
use knot::test_support::{TestProduct, TestTree, TestWindow, resolved_tempdir, wait_until};

const TOOL: &str = "vcs.changes";

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn write(directory: &Path, name: &str, content: &str) {
    std::fs::write(directory.join(name), content).unwrap();
}

fn uri(path: &Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

/// A repository with one commit, "base", whose `s.txt` is changed and staged
/// when `staged`, and whose `a.txt` is changed and not staged. `new.txt` is
/// untracked.
fn show(
    staged: bool,
    cx: &mut TestAppContext,
) -> (TestProduct, TestWindow, TestTree, tempfile::TempDir) {
    let directory = resolved_tempdir();
    let root = directory.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.name", "T"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    write(root, "a.txt", "a1\n");
    write(root, "s.txt", "s1\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    write(root, "s.txt", "s2\n");
    if staged {
        git(root, &["add", "s.txt"]);
    }
    write(root, "a.txt", "a2\n");
    write(root, "new.txt", "new\n");

    let product = TestProduct::launch_windowed(knot_ext_vcs::PACKAGES, cx);
    product.watch_files(cx);
    let window = product.active_window(cx);
    window.set_roots(&[&uri(root)], cx);
    let tree = window.tree("vcs.changes.tree", cx);
    wait_until(cx, |cx| {
        !tree.labels(cx).is_empty() && tree.labels(cx)[0] != "No repository"
    });
    wait_until(cx, |cx| window.tool_editors(TOOL, cx).len() == 1);
    (product, window, tree, directory)
}

/// Types `message` in the panel's editor.
fn write_message(window: &TestWindow, message: &str, cx: &mut TestAppContext) {
    window.focus_tool_editor(TOOL, cx);
    window.type_input(message, cx);
    wait_until(cx, |cx| window.tool_editors(TOOL, cx)[0] == message);
}

/// Toggles the panel's option `option`, as its checkbox does.
fn toggle(window: &TestWindow, directory: &Path, option: &str, cx: &mut TestAppContext) {
    window.run_command_with(
        "vcs.commit.toggleOption",
        &[
            ("backend", "git"),
            ("repository", &uri(directory)),
            ("option", option),
        ],
        cx,
    );
}

fn subjects(root: &Path) -> Vec<String> {
    git(root, &["log", "--format=%s"])
        .lines()
        .map(str::to_owned)
        .collect()
}

#[gpui::test]
fn the_panel_commits_the_index_and_clears_the_draft(cx: &mut TestAppContext) {
    let (product, window, tree, directory) = show(true, cx);
    let root = directory.path();
    assert_eq!(
        window.tool_placeholder(TOOL, cx).as_deref(),
        Some(if cfg!(target_os = "macos") {
            "Commit message (⌘⏎ to commit)"
        } else {
            "Commit message (Ctrl+⏎ to commit)"
        })
    );
    write_message(&window, "Change s", cx);
    window.press(
        if cfg!(target_os = "macos") {
            "cmd-enter"
        } else {
            "ctrl-enter"
        },
        cx,
    );
    wait_until(cx, |_| subjects(root) == ["Change s", "base"]);
    assert_eq!(
        git(root, &["status", "--porcelain"]),
        " M a.txt\n?? new.txt\n"
    );
    wait_until(cx, |cx| window.tool_editors(TOOL, cx)[0].is_empty());
    wait_until(cx, |cx| !tree.labels(cx).contains(&"Staged".to_owned()));
    wait_until(cx, |cx| {
        product.notice_messages(cx).last().is_some_and(|message| {
            message.starts_with("Committed ") && message.ends_with(": Change s")
        })
    });
    let output = window.frame_text("vcs.output", cx).unwrap();
    assert!(output.contains("$ git commit -F -"), "{output}");
}

#[gpui::test]
fn amend_suggests_the_last_message_and_replaces_the_commit(cx: &mut TestAppContext) {
    let (_product, window, _tree, directory) = show(true, cx);
    let root = directory.path();
    write_message(&window, "my draft", cx);
    toggle(&window, root, "amend", cx);
    wait_until(cx, |cx| window.tool_editors(TOOL, cx)[0] == "base");
    toggle(&window, root, "amend", cx);
    wait_until(cx, |cx| window.tool_editors(TOOL, cx)[0] == "my draft");
    toggle(&window, root, "amend", cx);
    wait_until(cx, |cx| window.tool_editors(TOOL, cx)[0] == "base");

    window.run_command("vcs.commit", cx);
    wait_until(cx, |_| {
        git(root, &["status", "--porcelain"]) == " M a.txt\n?? new.txt\n"
    });
    assert_eq!(subjects(root), ["base"]);
    assert_eq!(git(root, &["show", "HEAD:s.txt"]), "s2\n");
}

#[gpui::test]
fn sign_off_adds_the_trailer(cx: &mut TestAppContext) {
    let (_product, window, _tree, directory) = show(true, cx);
    let root = directory.path();
    toggle(&window, root, "signoff", cx);
    write_message(&window, "Signed", cx);
    window.run_command("vcs.commit", cx);
    wait_until(cx, |_| {
        subjects(root).first().is_some_and(|s| s == "Signed")
    });
    assert!(git(root, &["log", "-1", "--format=%B"]).contains("Signed-off-by: T <t@example.com>"));
}

#[gpui::test]
fn with_nothing_staged_it_asks_to_stage_every_tracked_change(cx: &mut TestAppContext) {
    let (_product, window, _tree, directory) = show(false, cx);
    let root = directory.path();
    write_message(&window, "Everything", cx);

    let command = window.start_command("vcs.commit", cx);
    wait_until(cx, |cx| cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    command.finish(cx);
    assert_eq!(subjects(root), ["base"]);
    assert_eq!(window.tool_editors(TOOL, cx)[0], "Everything");

    let command = window.start_command("vcs.commit", cx);
    wait_until(cx, |cx| cx.has_pending_prompt());
    cx.simulate_prompt_answer("Commit");
    command.finish(cx);
    wait_until(cx, |_| subjects(root) == ["Everything", "base"]);
    // Untracked files are never included.
    assert_eq!(git(root, &["status", "--porcelain"]), "?? new.txt\n");
}

#[cfg(unix)]
#[gpui::test]
fn a_failing_hook_keeps_the_draft_and_shows_in_the_output(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;

    let (product, window, _tree, directory) = show(true, cx);
    let root = directory.path();
    let hook = root.join(".git/hooks/commit-msg");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'message rejected by hook' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();

    write_message(&window, "Rejected", cx);
    window.run_command("vcs.commit", cx);
    wait_until(cx, |cx| {
        product
            .notice_messages(cx)
            .last()
            .is_some_and(|message| message.contains("message rejected by hook"))
    });
    assert_eq!(subjects(root), ["base"]);
    assert_eq!(window.tool_editors(TOOL, cx)[0], "Rejected");
    let output = window.frame_text("vcs.output", cx).unwrap();
    assert!(output.contains("message rejected by hook"), "{output}");
    assert!(output.contains("Failed"), "{output}");
}

#[gpui::test]
fn the_focus_command_reaches_the_message_editor(cx: &mut TestAppContext) {
    let (_product, window, tree, _directory) = show(true, cx);
    tree.focus(cx);
    assert!(!window.tool_editor_focused(TOOL, cx));
    window.press(
        if cfg!(target_os = "macos") {
            "cmd-alt-c"
        } else {
            "ctrl-alt-c"
        },
        cx,
    );
    wait_until(cx, |cx| window.tool_editor_focused(TOOL, cx));
}
