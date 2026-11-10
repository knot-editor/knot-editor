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
    // The rows answering the query arrive asynchronously.
    wait_until(cx, |cx| window.picker_labels(cx) == ["button.rs"]);
    window.press("enter", cx);
    wait_until(cx, |cx| {
        !window.transient_open(cx) && window.document_texts(cx) == ["struct Button;".to_owned()]
    });
}
