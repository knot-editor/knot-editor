//! File status painted on the explorer's rows and on document tabs, over
//! temporary repositories made with the `git` program.
use std::path::Path;
use std::process::Command;

use gpui::TestAppContext;
use knot::test_support::{TestProduct, TestTree, TestWindow, resolved_tempdir, wait_until};

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

/// The version control packages and the explorer.
fn packages() -> &'static [knot_bundled::BundledSource] {
    let mut packages = knot_ext_vcs::PACKAGES.to_vec();
    packages.extend(
        knot_ext_workbench::PACKAGES
            .iter()
            .filter(|package| package.manifest.contains("\"name\": \"@knot/explorer\""))
            .copied(),
    );
    Box::leak(packages.into_boxed_slice())
}

type Decoration = Option<(Option<String>, Option<String>, Option<String>)>;

fn shown(color: &str, badge: &str, tooltip: &str) -> Decoration {
    Some((
        Some(color.to_owned()),
        Some(format!("text:{badge}")),
        Some(tooltip.to_owned()),
    ))
}

fn row(tree: &TestTree, label: &str, cx: &mut TestAppContext) -> Decoration {
    let id = tree.row_id(label, cx)?;
    tree.decoration(&id, cx)
}

/// A repository with `a.txt` and `src/lib.txt`, where `a.txt` is edited and
/// `src/new.txt` is untracked, shown in the explorer.
fn show(cx: &mut TestAppContext) -> (TestProduct, TestWindow, TestTree, tempfile::TempDir) {
    show_with(false, cx)
}

/// Like [`show`], for a test that changes the repository afterwards and needs
/// the operating system's file events.
fn show_watching(
    cx: &mut TestAppContext,
) -> (TestProduct, TestWindow, TestTree, tempfile::TempDir) {
    show_with(true, cx)
}

fn show_with(
    watch: bool,
    cx: &mut TestAppContext,
) -> (TestProduct, TestWindow, TestTree, tempfile::TempDir) {
    let directory = resolved_tempdir();
    let root = directory.path();
    std::fs::create_dir_all(root).unwrap();
    git(root, &["init", "-q", "-b", "main"]);
    write(root, "a.txt", "a\n");
    write(root, "src/lib.txt", "lib\n");
    write(root, ".gitignore", "build/\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    write(root, "a.txt", "b\n");
    write(root, "src/new.txt", "new\n");

    let product = TestProduct::launch(packages(), cx);
    if watch {
        product.watch_files(cx);
    }
    let window = product.active_window(cx);
    window.set_roots(&[&uri(root)], cx);
    let tree = window.tree("builtin.explorer", cx);
    wait_until(cx, |cx| tree.labels(cx).iter().any(|label| label == "src"));
    tree.gesture("src", "click", cx);
    wait_until(cx, |cx| {
        tree.labels(cx).iter().any(|label| label == "new.txt")
    });
    (product, window, tree, directory)
}

#[gpui::test]
fn the_explorer_shows_each_changed_file_and_the_folders_that_hold_changes(cx: &mut TestAppContext) {
    let (_product, _window, tree, _directory) = show(cx);
    wait_until(cx, |cx| {
        row(&tree, "a.txt", cx) == shown("vcs.modified", "M", "Modified")
    });
    assert_eq!(
        row(&tree, "new.txt", cx),
        shown("vcs.untracked", "U", "Untracked")
    );
    assert_eq!(row(&tree, "lib.txt", cx), None);
    // The folder takes its color without a badge or a tooltip.
    assert_eq!(
        row(&tree, "src", cx),
        Some((Some("vcs.untracked".to_owned()), None, None))
    );
}

#[gpui::test]
fn a_document_tab_takes_the_color_of_its_file(cx: &mut TestAppContext) {
    let (_product, window, _tree, directory) = show(cx);
    let file = directory.path().join("a.txt");
    window.open_path(&file, cx);
    wait_until(cx, |cx| {
        window.resource_decoration(&uri(&file), cx)
            == Some((Some("vcs.modified".to_owned()), Some("Modified".to_owned())))
    });
}

#[gpui::test]
fn a_commit_made_with_git_clears_the_decorations(cx: &mut TestAppContext) {
    let (_product, window, tree, directory) = show_watching(cx);
    let root = directory.path();
    let file = root.join("a.txt");
    window.open_path(&file, cx);
    wait_until(cx, |cx| row(&tree, "a.txt", cx).is_some());

    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "all"]);
    wait_until(cx, |cx| {
        ["a.txt", "new.txt", "src"]
            .iter()
            .all(|label| row(&tree, label, cx).is_none())
    });
    assert_eq!(window.resource_decoration(&uri(&file), cx), None);
}

#[gpui::test]
fn an_edit_and_a_new_file_on_disk_update_the_explorer(cx: &mut TestAppContext) {
    let (_product, _window, tree, directory) = show_watching(cx);
    let root = directory.path();
    wait_until(cx, |cx| row(&tree, "new.txt", cx).is_some());

    // Neither change goes through git, so no metadata watch reports it.
    write(root, "src/lib.txt", "edited\n");
    write(root, "src/second.txt", "second\n");
    wait_until(cx, |cx| {
        row(&tree, "lib.txt", cx) == shown("vcs.modified", "M", "Modified")
            && row(&tree, "second.txt", cx) == shown("vcs.untracked", "U", "Untracked")
    });

    write(root, "src/lib.txt", "lib\n");
    std::fs::remove_file(root.join("src/second.txt")).unwrap();
    wait_until(cx, |cx| {
        row(&tree, "lib.txt", cx).is_none() && row(&tree, "second.txt", cx).is_none()
    });
}

#[gpui::test]
fn output_in_an_ignored_folder_shows_nothing(cx: &mut TestAppContext) {
    let (_product, _window, tree, directory) = show_watching(cx);
    let root = directory.path();
    write(root, "build/out.txt", "out\n");
    // A later change is seen, which means the ignored one was settled before it.
    write(root, "src/lib.txt", "edited\n");
    wait_until(cx, |cx| row(&tree, "lib.txt", cx).is_some());
    assert!(
        tree.row_id("build", cx)
            .is_none_or(|id| tree.decoration(&id, cx).is_none())
    );
}
