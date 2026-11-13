//! The bundled Go to Line package, driven from its key in an editor.
use gpui::TestAppContext;

use super::bundled::only;
use knot::test_support::{TestProduct, wait_until};

#[gpui::test]
fn ctrl_g_previews_and_keeps_a_typed_line_in_the_editor(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/go-to-line"]), cx);
    let editor = product.windowed_editor("one\ntwo\nthree é\nfour\n", cx);
    let target = "one\ntwo\nthree ".len();
    // The picker keeps the package's command waiting, so keys go in unsettled
    // and the test waits for what each one shows.

    editor.press_unsettled("<c-g>", cx);
    wait_until(cx, |cx| editor.transient_open(cx));
    editor.type_input("3:7", cx);
    // The caret moves while typing, after "three ".
    wait_until(cx, |cx| editor.caret(cx) == target);
    editor.press_unsettled("<cr>", cx);
    wait_until(cx, |cx| !editor.transient_open(cx));
    assert_eq!(editor.caret(cx), target);

    // Escape after a preview restores the caret.
    editor.press_unsettled("<c-g>", cx);
    wait_until(cx, |cx| editor.transient_open(cx));
    editor.type_input("1", cx);
    wait_until(cx, |cx| editor.caret(cx) == 0);
    editor.press_unsettled("<esc>", cx);
    wait_until(cx, |cx| {
        !editor.transient_open(cx) && editor.caret(cx) == target
    });
}
