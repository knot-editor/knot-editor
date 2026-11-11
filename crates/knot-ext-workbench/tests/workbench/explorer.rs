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

    #[track_caller]
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

/// Start `command` with the explorer focused and wait for its inline edit.
fn start_edit(
    explorer: &Explorer,
    command: &str,
    cx: &mut TestAppContext,
) -> knot::test_support::TestCommand {
    explorer.tree.focus(cx);
    let started = explorer.window.start_command(command, cx);
    wait_until(cx, |cx| explorer.tree.is_editing(cx));
    started
}

#[gpui::test]
fn new_file_creates_missing_folders_opens_a_tab_and_validates_the_name(cx: &mut TestAppContext) {
    let directory = fixture();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(&["app", "docs", "src", "README.md"], cx);
    let (window, tree) = (&explorer.window, &explorer.tree);
    tree.gesture("docs", "click", cx);
    explorer.wait_for_labels(&["app", "docs", "guide.md", "src", "README.md"], cx);

    // The new row is typed in place, first among the folder's children, and a
    // name that exists is refused.
    let command = start_edit(&explorer, "explorer.new-file", cx);
    assert_eq!(tree.edit_text(cx).as_deref(), Some(""));
    window.type_input("guide.md", cx);
    wait_until(cx, |cx| {
        tree.edit_message(cx).as_deref() == Some("guide.md already exists here")
    });
    window.press("enter", cx);
    assert!(tree.is_editing(cx), "the message blocks the commit");
    window.press("escape", cx);
    assert_eq!(command.finish(cx), CommandOutcome::Completed);
    assert!(!directory.path().join("app/docs/guide.md/x").exists());

    let command = start_edit(&explorer, "explorer.new-file", cx);
    window.type_input("deep/er/new.txt", cx);
    wait_until(cx, |cx| tree.edit_message(cx).is_none());
    window.press("enter", cx);
    assert_eq!(command.finish(cx), CommandOutcome::Completed);
    assert!(directory.path().join("app/docs/deep/er/new.txt").is_file());
    wait_until(cx, |cx| {
        window.document_tabs(cx) == [("new.txt".to_owned(), false)]
    });
    wait_until(cx, |cx| tree.selected(cx).as_deref() == Some("new.txt"));
    assert!(tree.labels(cx).contains(&"deep".to_owned()));
}

#[gpui::test]
fn new_folder_in_a_root_selects_the_folder(cx: &mut TestAppContext) {
    let directory = fixture();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(&["app", "docs", "src", "README.md"], cx);
    explorer.tree.gesture("app", "click", cx);
    let command = start_edit(&explorer, "explorer.new-folder", cx);
    explorer.window.type_input("assets", cx);
    explorer.window.press("enter", cx);
    assert_eq!(command.finish(cx), CommandOutcome::Completed);
    assert!(directory.path().join("app/assets").is_dir());
    wait_until(cx, |cx| {
        explorer.tree.selected(cx).as_deref() == Some("assets")
    });
    assert!(explorer.window.document_tabs(cx).is_empty());
}

#[gpui::test]
fn rename_selects_the_stem_and_open_documents_follow(cx: &mut TestAppContext) {
    let directory = fixture();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(&["app", "docs", "src", "README.md"], cx);
    let (window, tree) = (&explorer.window, &explorer.tree);
    window.open_path(&directory.path().join("app/README.md"), cx);
    wait_until(cx, |cx| tree.selected(cx).as_deref() == Some("README.md"));

    // F2 renames the selected row; typing replaces the stem only.
    tree.focus(cx);
    window.press("f2", cx);
    wait_until(cx, |cx| tree.is_editing(cx));
    assert_eq!(tree.edit_text(cx).as_deref(), Some("README.md"));
    window.type_input("NOTES", cx);
    assert_eq!(tree.edit_text(cx).as_deref(), Some("NOTES.md"));
    window.press("enter", cx);
    wait_until(cx, |_| directory.path().join("app/NOTES.md").is_file());
    assert!(!directory.path().join("app/README.md").exists());
    wait_until(cx, |cx| {
        window.document_tabs(cx) == [("NOTES.md".to_owned(), false)]
    });
    wait_until(cx, |cx| tree.selected(cx).as_deref() == Some("NOTES.md"));

    // A folder is selected whole, and an existing name is refused.
    tree.focus(cx);
    tree.gesture("src", "click", cx);
    tree.gesture("src", "rightClick", cx);
    window.press("f2", cx);
    wait_until(cx, |cx| tree.is_editing(cx));
    window.type_input("docs", cx);
    wait_until(cx, |cx| {
        tree.edit_message(cx).as_deref() == Some("docs already exists here")
    });
    window.press("escape", cx);
    wait_until(cx, |cx| !tree.is_editing(cx));
    assert!(directory.path().join("app/src").is_dir());
}

#[gpui::test]
fn delete_asks_first_then_moves_the_entry_to_the_trash(cx: &mut TestAppContext) {
    let directory = fixture();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(&["app", "docs", "src", "README.md"], cx);
    let (window, tree) = (&explorer.window, &explorer.tree);
    tree.gesture("README.md", "rightClick", cx);
    tree.focus(cx);

    window.press("delete", cx);
    wait_until(cx, |cx| cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(directory.path().join("app/README.md").exists());

    window.press("delete", cx);
    wait_until(cx, |cx| cx.has_pending_prompt());
    cx.simulate_prompt_answer("Move to Trash");
    wait_until(cx, |_| !directory.path().join("app/README.md").exists());
    explorer.wait_for_labels(&["app", "docs", "src"], cx);
}

#[gpui::test]
fn roots_cannot_be_renamed_or_deleted(cx: &mut TestAppContext) {
    let directory = fixture();
    let explorer = Explorer::start(&[&directory.path().join("app")], cx);
    explorer.wait_for_labels(&["app", "docs", "src", "README.md"], cx);
    explorer.tree.gesture("app", "rightClick", cx);
    explorer.tree.focus(cx);
    for command in ["explorer.rename", "explorer.delete"] {
        assert_eq!(
            explorer.window.run_command(command, cx),
            CommandOutcome::Unavailable,
            "{command}"
        );
    }
    assert!(!cx.has_pending_prompt());
}
