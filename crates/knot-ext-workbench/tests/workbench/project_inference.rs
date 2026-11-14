//! `@knot/projects` inference: a file from a Git checkout opened in a window
//! without roots makes the checkout that window's project.

use std::path::{Path, PathBuf};

use gpui::TestAppContext;

use super::bundled::only;
use super::projects::{Projects, file_uri, list_projects};
use knot::test_support::{TestProduct, TestWindow, wait_until};

/// A checkout with a `.git` directory and `src/main.rs`; the root and file
/// paths are canonical, as the product names them.
fn checkout(parent: &Path, name: &str) -> (PathBuf, PathBuf) {
    let root = parent.canonicalize().unwrap().join(name);
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    let file = root.join("src/main.rs");
    std::fs::write(&file, "fn main() {}\n").unwrap();
    (root, file)
}

fn window_with_roots(
    product: &TestProduct,
    roots: &[String],
    cx: &mut TestAppContext,
) -> Option<TestWindow> {
    product
        .windows(cx)
        .into_iter()
        .find(|window| window.roots(cx) == roots)
}

#[gpui::test]
fn a_file_passed_at_launch_roots_its_window_at_the_checkout(cx: &mut TestAppContext) {
    let parent = tempfile::tempdir().unwrap();
    let (root, file) = checkout(parent.path(), "repo");
    let product = TestProduct::launch_opening(only(&["@knot/projects"]), &[&file], cx);
    let root_uri = file_uri(&root);

    wait_until(cx, |cx| {
        window_with_roots(&product, std::slice::from_ref(&root_uri), cx).is_some()
    });
    let windows = product.windows(cx);
    assert_eq!(windows.len(), 1);
    let window = &windows[0];
    assert_eq!(window.document_tabs(cx), [("main.rs".to_owned(), false)]);
    wait_until(cx, |cx| {
        list_projects(window, cx)["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|project| project["roots"][0] == root_uri.as_str())
    });
}

#[gpui::test]
fn a_file_of_a_suspended_project_restores_it_without_a_stray_window(cx: &mut TestAppContext) {
    let parent = tempfile::tempdir().unwrap();
    let (root, file) = checkout(parent.path(), "repo");
    let other = tempfile::tempdir().unwrap();
    let mut projects = Projects::start(cx);
    projects.create(&root, "Repo", cx);
    // Creating a second project from the first one's window suspends Repo.
    projects.create(other.path(), "Other", cx);
    let repo = projects.id("Repo", cx);
    wait_until(cx, |cx| {
        projects.list(cx)["sessions"][&repo]["state"] == "suspended"
    });

    projects.product.open_from_command_line(&[&file], cx);
    let root_uri = file_uri(&root);
    wait_until(cx, |cx| {
        window_with_roots(&projects.product, std::slice::from_ref(&root_uri), cx).is_some_and(
            |window| {
                window
                    .document_tabs(cx)
                    .iter()
                    .any(|(title, _)| title == "main.rs")
            },
        )
    });
    wait_until(cx, |cx| {
        projects.list(cx)["sessions"][&repo]["state"] == "active"
    });
    let rootless = projects
        .product
        .windows(cx)
        .into_iter()
        .filter(|window| window.roots(cx).is_empty())
        .count();
    assert_eq!(rootless, 0);
    assert_eq!(projects.product.windows(cx).len(), 2);
}
