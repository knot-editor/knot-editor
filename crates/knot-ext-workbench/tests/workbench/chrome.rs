//! The bundled chrome package, following the editor a window describes.
use gpui::TestAppContext;

use super::bundled::only;
use knot::test_support::{TestProduct, wait_until};

#[gpui::test]
fn the_items_follow_the_selection_and_the_document_of_the_status_target(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/chrome"]), cx);
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

#[gpui::test]
fn the_tab_menu_closes_and_splits_the_clicked_tab(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/chrome"]), cx);
    let editor = product.windowed_editor("one", cx);
    wait_until(cx, |cx| !editor.tab_menu(0, cx).is_empty());
    assert_eq!(
        editor.tab_menu(0, cx),
        [
            vec![
                "Close Tab",
                "Close Other Tabs",
                "Close Tabs to the Right",
                "Close All"
            ],
            vec!["Split Right", "Split Down"],
        ]
    );

    // Closing to the right of the first tab leaves it, though the last tab
    // had focus.
    editor.new_tab(cx);
    editor.new_tab(cx);
    assert_eq!(editor.tab_titles(cx).len(), 3);
    editor.right_click_tab(0, cx);
    editor.press_in_menu("down down down enter", cx);
    wait_until(cx, |cx| editor.tab_titles(cx).len() == 1);
}

#[gpui::test]
fn the_editor_menu_offers_the_clipboard_at_the_pointer_and_at_the_caret(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/chrome"]), cx);
    let editor = product.windowed_editor("one two", cx);
    wait_until(cx, |cx| !editor.content_menu(cx).is_empty());
    assert_eq!(editor.content_menu(cx), [vec!["Cut", "Copy", "Paste"]]);

    // A right-click outside the selection moves the caret to the click, then
    // Paste (the third item) acts on that editor.
    editor.select(0, 3, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("!".into()));
    editor.right_click_content(cx);
    assert_eq!(editor.caret(cx), 7);
    editor.press_in_menu("down down down enter", cx);
    assert_eq!(editor.active_text(cx), "one two!");

    // Shift+F10 opens the menu at the caret, and Cut removes the selection.
    editor.select(0, 3, cx);
    editor.press("<s-f10>", cx);
    editor.press_in_menu("down enter", cx);
    assert_eq!(editor.active_text(cx), " two!");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("one".to_owned())
    );
}
