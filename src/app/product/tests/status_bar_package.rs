//! The bundled status bar package, following the editor a window describes.
use gpui::TestAppContext;

use super::bundled::PACKAGES;
use crate::test_support::{TestProduct, wait_until};

#[gpui::test]
fn the_items_follow_the_selection_and_the_document_of_the_status_target(cx: &mut TestAppContext) {
    let product = TestProduct::start(PACKAGES, cx);
    let editor = product.windowed_editor("one\ntwo\nthree é\n", cx);
    let right = |cx: &mut TestAppContext| editor.status(cx).1;
    wait_until(cx, |cx| right(cx).len() == 3);
    assert_eq!(right(cx), ["Ln 1, Col 1", "Spaces: 4", "Plain Text"]);

    editor.select(6, 6, cx);
    wait_until(cx, |cx| right(cx)[0] == "Ln 2, Col 3");
    editor.select(0, 3, cx);
    wait_until(cx, |cx| right(cx)[0] == "Ln 1, Col 4 (3 selected)");

    // A document-scoped indentation write reaches the bar without a listener.
    editor.set_setting("editor.insertSpaces", false.into(), cx);
    wait_until(cx, |cx| right(cx)[1] == "Tab Size: 4");
}
