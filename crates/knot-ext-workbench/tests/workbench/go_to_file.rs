//! The bundled Go to File package, driven from its key in a window over a
//! temporary tree.
use gpui::TestAppContext;

use super::bundled::only;
use super::projects::file_uri;
use knot::test_support::{TestProduct, wait_until};

const KEY: &str = if cfg!(target_os = "macos") {
    "cmd-p"
} else {
    "ctrl-p"
};

#[gpui::test]
fn the_key_lists_matching_files_and_enter_opens_the_highlighted_one(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("src/ui")).unwrap();
    std::fs::write(directory.path().join("src/main.rs"), "fn main() {}").unwrap();
    std::fs::write(directory.path().join("src/ui/button.rs"), "struct Button;").unwrap();
    std::fs::write(directory.path().join("README.md"), "# readme").unwrap();

    let product = TestProduct::launch(only(&["@knot/go-to-file"]), cx);
    let window = product.active_window(cx);
    window.set_roots(&[&file_uri(directory.path())], cx);

    window.press(KEY, cx);
    wait_until(cx, |cx| window.transient_open(cx));
    window.type_input("butt", cx);
    // Enter right after typing waits for the rows answering the query.
    window.press("enter", cx);
    wait_until(cx, |cx| {
        !window.transient_open(cx) && window.document_texts(cx) == ["struct Button;".to_owned()]
    });
}

#[gpui::test]
fn files_of_every_root_are_ranked_as_one_list(cx: &mut TestAppContext) {
    let first = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(first.path().join("button")).unwrap();
    std::fs::write(first.path().join("button/readme.md"), "# readme").unwrap();
    let second = tempfile::tempdir().unwrap();
    std::fs::write(second.path().join("button.rs"), "struct Button;").unwrap();

    let product = TestProduct::launch(only(&["@knot/go-to-file"]), cx);
    let window = product.active_window(cx);
    window.set_roots(&[&file_uri(first.path()), &file_uri(second.path())], cx);

    window.press(KEY, cx);
    wait_until(cx, |cx| window.transient_open(cx));
    window.type_input("butt", cx);
    // The name match in the second root outranks the directory match in the
    // first.
    wait_until(cx, |cx| {
        window.picker_labels(cx) == ["button.rs", "readme.md"]
    });
    window.press("enter", cx);
    wait_until(cx, |cx| {
        !window.transient_open(cx) && window.document_texts(cx) == ["struct Button;".to_owned()]
    });
}

#[gpui::test]
fn the_key_then_enter_returns_to_the_previous_file(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("first.rs"), "first").unwrap();
    std::fs::write(directory.path().join("second.rs"), "second").unwrap();

    let product = TestProduct::launch(only(&["@knot/go-to-file"]), cx);
    let window = product.active_window(cx);
    window.set_roots(&[&file_uri(directory.path())], cx);
    window.open_path(&directory.path().join("first.rs"), cx);
    window.open_path(&directory.path().join("second.rs"), cx);

    window.press(KEY, cx);
    wait_until(cx, |cx| window.transient_open(cx));
    // The current file is left out, so the first row is the previous one.
    wait_until(cx, |cx| {
        window.picker_labels(cx).first().map(String::as_str) == Some("first.rs")
    });
    window.press("enter", cx);
    wait_until(cx, |cx| !window.transient_open(cx));

    // First is now current, so the list leads with second.
    window.press(KEY, cx);
    wait_until(cx, |cx| {
        window.picker_labels(cx).first().map(String::as_str) == Some("second.rs")
    });
}

const RECENT_KEY: &str = if cfg!(target_os = "macos") {
    "cmd-e"
} else {
    "ctrl-e"
};

#[gpui::test]
fn open_recent_lists_active_files_newest_first_and_drops_deleted_ones(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    for name in ["first.rs", "second.rs", "third.rs"] {
        std::fs::write(directory.path().join(name), name).unwrap();
    }

    let product = TestProduct::launch(only(&["@knot/go-to-file"]), cx);
    product.watch_files(cx);
    let window = product.active_window(cx);
    window.set_roots(&[&file_uri(directory.path())], cx);
    for name in ["first.rs", "second.rs", "third.rs"] {
        window.open_path(&directory.path().join(name), cx);
    }
    std::fs::remove_file(directory.path().join("second.rs")).unwrap();

    // The workspace index learns of the deletion from the file watcher, so
    // reopen the list until it no longer confirms the file. The current file
    // leads.
    wait_until(cx, |cx| {
        window.press(RECENT_KEY, cx);
        let listed =
            window.transient_open(cx) && window.picker_labels(cx) == ["third.rs", "first.rs"];
        if !listed && window.transient_open(cx) {
            window.press("escape", cx);
        }
        listed
    });
    window.press("down", cx);
    window.press("enter", cx);
    wait_until(cx, |cx| {
        !window.transient_open(cx) && window.document_texts(cx).contains(&"first.rs".to_owned())
    });
}
