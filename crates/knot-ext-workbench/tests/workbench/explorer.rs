//! The bundled explorer, browsing the roots of a window in the launched product.
use std::path::Path;

use gpui::TestAppContext;

use super::bundled::PACKAGES;
use super::projects::file_uri;
use knot::test_support::{CommandOutcome, TestProduct, TestTree, TestWindow, wait_until};

const VIEW: &str = "builtin.explorer";

fn write(directory: &Path, path: &str, text: &str) {
    let path = directory.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// Two roots, `app` and `lib`, under a temporary directory.
fn fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "app/src/main.rs", "fn main() {}");
    write(directory.path(), "app/src/a10.rs", "a10");
    write(directory.path(), "app/src/a9.rs", "a9");
    write(directory.path(), "app/docs/guide.md", "guide");
    write(directory.path(), "app/README.md", "readme");
    write(directory.path(), "lib/lib.rs", "lib");
    directory
}

struct Explorer {
    product: TestProduct,
    window: TestWindow,
    tree: TestTree,
}

impl Explorer {
    fn start(roots: &[&Path], cx: &mut TestAppContext) -> Self {
        let product = TestProduct::launch(PACKAGES, cx);
        let window = product.active_window(cx);
        let uris = roots.iter().map(|root| file_uri(root)).collect::<Vec<_>>();
        window.set_roots(&uris.iter().map(String::as_str).collect::<Vec<_>>(), cx);
        let tree = window.tree(VIEW, cx);
        Self {
            product,
            window,
            tree,
        }
    }

    fn wait_for_labels(&self, labels: &[&str], cx: &mut TestAppContext) {
        wait_until(cx, |cx| self.tree.labels(cx) == labels);
    }
}

#[gpui::test]
fn every_root_is_a_row_with_folders_before_files_in_natural_order(cx: &mut TestAppContext) {
    let directory = fixture();
    let explorer = Explorer::start(
        &[&directory.path().join("app"), &directory.path().join("lib")],
        cx,
    );
    explorer.wait_for_labels(&["app", "docs", "src", "README.md", "lib", "lib.rs"], cx);
    explorer.tree.gesture("src", "click", cx);
    explorer.wait_for_labels(
        &[
            "app",
            "docs",
            "src",
            "a9.rs",
            "a10.rs",
            "main.rs",
            "README.md",
            "lib",
            "lib.rs",
        ],
        cx,
    );
}

#[gpui::test]
fn a_window_without_roots_shows_an_open_folder_row(cx: &mut TestAppContext) {
    let product = TestProduct::launch(PACKAGES, cx);
    let window = product.active_window(cx);
    let tree = window.tree(VIEW, cx);
    wait_until(cx, |cx| tree.labels(cx) == ["Open Folder…"]);
}

#[gpui::test]
fn click_previews_and_double_click_or_enter_open_tabs_and_middle_click_keeps_focus(
    cx: &mut TestAppContext,
) {
    let directory = fixture();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(&["app", "docs", "src", "README.md"], cx);
    let (window, tree) = (&explorer.window, &explorer.tree);

    tree.gesture("README.md", "click", cx);
    wait_until(cx, |cx| {
        window.document_tabs(cx) == [("README.md".to_owned(), true)]
    });
    assert!(
        window.editor_focused(cx),
        "a click moves focus to the editor"
    );

    tree.gesture("README.md", "doubleClick", cx);
    wait_until(cx, |cx| {
        window.document_tabs(cx) == [("README.md".to_owned(), false)]
    });

    // A right click opens nothing.
    tree.gesture("docs", "click", cx);
    explorer.wait_for_labels(&["app", "docs", "guide.md", "src", "README.md"], cx);
    tree.focus(cx);
    tree.gesture("guide.md", "rightClick", cx);
    cx.run_until_parked();
    assert_eq!(window.document_tabs(cx).len(), 1);

    // A middle click opens a regular tab and leaves the explorer focused.
    tree.focus(cx);
    tree.gesture("guide.md", "middleClick", cx);
    wait_until(cx, |cx| window.document_tabs(cx).len() == 2);
    assert_eq!(window.document_tabs(cx)[1], ("guide.md".to_owned(), false));
    assert!(
        tree.is_focused(cx),
        "a middle click keeps the explorer focused"
    );

    // Enter opens a regular tab and moves focus to it.
    tree.gesture("src", "click", cx);
    explorer.wait_for_labels(
        &[
            "app",
            "docs",
            "guide.md",
            "src",
            "a9.rs",
            "a10.rs",
            "main.rs",
            "README.md",
        ],
        cx,
    );
    tree.gesture("main.rs", "enter", cx);
    wait_until(cx, |cx| window.document_tabs(cx).len() == 3);
    assert_eq!(window.document_tabs(cx)[2], ("main.rs".to_owned(), false));
    assert!(window.editor_focused(cx));
}

#[gpui::test]
fn files_created_outside_the_editor_appear_in_loaded_folders(cx: &mut TestAppContext) {
    let directory = fixture();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(&["app", "docs", "src", "README.md"], cx);
    explorer.tree.gesture("docs", "click", cx);
    explorer.wait_for_labels(&["app", "docs", "guide.md", "src", "README.md"], cx);

    write(directory.path(), "app/docs/new.md", "new");
    explorer.wait_for_labels(
        &["app", "docs", "guide.md", "new.md", "src", "README.md"],
        cx,
    );
    std::fs::remove_file(directory.path().join("app/docs/guide.md")).unwrap();
    explorer.wait_for_labels(&["app", "docs", "new.md", "src", "README.md"], cx);
    write(directory.path(), "app/top.txt", "top");
    explorer.wait_for_labels(
        &["app", "docs", "new.md", "src", "README.md", "top.txt"],
        cx,
    );
}

#[gpui::test]
fn ignored_entries_are_dimmed_until_the_toggle_hides_them(cx: &mut TestAppContext) {
    let directory = fixture();
    write(directory.path(), "app/.gitignore", "scratch.txt\n");
    write(directory.path(), "app/scratch.txt", "scratch");
    std::fs::create_dir(directory.path().join("app/.git")).unwrap();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(
        &[
            "app",
            "docs",
            "src",
            ".gitignore",
            "README.md",
            "scratch.txt",
        ],
        cx,
    );
    let scratch = explorer.tree.row_id("scratch.txt", cx).unwrap();
    let readme = explorer.tree.row_id("README.md", cx).unwrap();
    assert!(explorer.tree.is_muted(&scratch, cx));
    assert!(!explorer.tree.is_muted(&readme, cx));

    assert_eq!(
        explorer.window.run_command("explorer.toggle-ignored", cx),
        CommandOutcome::Completed
    );
    explorer.wait_for_labels(&["app", "docs", "src", ".gitignore", "README.md"], cx);
    assert_eq!(
        explorer.window.run_command("explorer.toggle-ignored", cx),
        CommandOutcome::Completed
    );
    explorer.wait_for_labels(
        &[
            "app",
            "docs",
            "src",
            ".gitignore",
            "README.md",
            "scratch.txt",
        ],
        cx,
    );
}

#[gpui::test]
fn the_active_file_is_revealed_automatically_and_on_command(cx: &mut TestAppContext) {
    let directory = fixture();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(&["app", "docs", "src", "README.md"], cx);
    let (window, tree) = (&explorer.window, &explorer.tree);

    window.open_path(&directory.path().join("app/src/main.rs"), cx);
    wait_until(cx, |cx| tree.selected(cx).as_deref() == Some("main.rs"));
    assert_eq!(
        tree.labels(cx),
        [
            "app",
            "docs",
            "src",
            "a9.rs",
            "a10.rs",
            "main.rs",
            "README.md"
        ]
    );

    // Moving the selection away and running the command brings it back.
    tree.gesture("README.md", "rightClick", cx);
    assert_eq!(tree.selected(cx).as_deref(), Some("README.md"));
    window.focus_document(cx);
    assert_eq!(
        window.run_command("explorer.reveal-active-file", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| tree.selected(cx).as_deref() == Some("main.rs"));
    assert!(tree.is_focused(cx));
}

#[gpui::test]
fn toggle_explorer_hides_a_focused_explorer_and_shows_it_again(cx: &mut TestAppContext) {
    let directory = fixture();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(&["app", "docs", "src", "README.md"], cx);
    let tree = &explorer.tree;
    assert!(tree.is_attached(cx));
    tree.focus(cx);
    // Hiding the focused explorer closes the presentation the command was
    // captured on, so the command reports an invalid target.
    explorer.window.run_command("explorer.toggle", cx);
    assert!(!tree.is_attached(cx));
    assert_eq!(
        explorer.window.run_command("explorer.toggle", cx),
        CommandOutcome::Completed
    );
    assert!(tree.is_attached(cx) && tree.is_focused(cx));
    let _ = &explorer.product;
}

fn tool_in(layout: &serde_json::Value, dock: &str) -> Option<String> {
    layout["docks"][dock]["root"]["tabs"]
        .as_array()?
        .iter()
        .find_map(|tab| tab["tool"].as_str().map(str::to_owned))
}

#[gpui::test]
fn the_explorer_is_an_ordinary_tool_for_layout_memory(cx: &mut TestAppContext) {
    let directory = fixture();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(&["app", "docs", "src", "README.md"], cx);
    let window = &explorer.window;
    assert_eq!(
        tool_in(&window.layout(cx), "left").as_deref(),
        Some(VIEW),
        "the explorer starts in the left dock"
    );

    explorer.tree.focus(cx);
    assert_eq!(
        window.run_command("workbench.move-tab-to-right-dock", cx),
        CommandOutcome::Completed
    );
    assert_eq!(tool_in(&window.layout(cx), "right").as_deref(), Some(VIEW));
    assert_eq!(tool_in(&window.layout(cx), "left"), None);

    assert_eq!(
        window.run_command("workbench.reset-layout", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| {
        tool_in(&window.layout(cx), "left").as_deref() == Some(VIEW)
    });
}
