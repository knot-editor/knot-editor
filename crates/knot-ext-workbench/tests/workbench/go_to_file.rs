//! The bundled Go to File package, driven from its key in a window over a
//! temporary tree.
use gpui::TestAppContext;

use super::bundled::PACKAGES;
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

    let product = TestProduct::launch(PACKAGES, cx);
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

    let product = TestProduct::launch(PACKAGES, cx);
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
