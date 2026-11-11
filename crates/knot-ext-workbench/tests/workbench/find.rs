//! The bundled Find package, driven from its keys in an editor.
use gpui::TestAppContext;

use super::bundled::PACKAGES;
use knot::test_support::{TestEditor, TestProduct, wait_until};

const FIND: &str = if cfg!(target_os = "macos") {
    "<d-f>"
} else {
    "<c-f>"
};
const NEXT: &str = if cfg!(target_os = "macos") {
    "<d-g>"
} else {
    "<f3>"
};
const PREVIOUS: &str = if cfg!(target_os = "macos") {
    "<s-d-g>"
} else {
    "<s-f3>"
};
const MATCH: &str = "editor.findMatch";
const CURRENT: &str = "editor.findMatchCurrent";

fn message(editor: &TestEditor, cx: &mut TestAppContext) -> Option<String> {
    editor.first_bar(cx).and_then(|(message, _)| message)
}

fn wait_for_message(editor: &TestEditor, expected: &str, cx: &mut TestAppContext) {
    wait_until(cx, |cx| message(editor, cx).as_deref() == Some(expected));
}

fn open_with(editor: &TestEditor, query: &str, cx: &mut TestAppContext) {
    editor.press_unsettled(FIND, cx);
    wait_until(cx, |cx| editor.first_bar(cx).is_some());
    editor.type_input(query, cx);
}

#[gpui::test]
fn typing_highlights_matches_and_the_keys_step_between_them(cx: &mut TestAppContext) {
    let product = TestProduct::start(PACKAGES, cx);
    let editor = product.windowed_editor("foo bar\nFoo baz foo\n", cx);

    open_with(&editor, "foo", cx);
    wait_for_message(&editor, "1 of 3", cx);
    assert_eq!(editor.caret(cx), 3);
    let colors = |cx: &mut TestAppContext| {
        editor
            .decorations(cx)
            .into_iter()
            .map(|(start, _, color)| (start, color))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        colors(cx),
        [
            (0, CURRENT.to_owned()),
            (8, MATCH.to_owned()),
            (16, MATCH.to_owned()),
            // The current match is also underlined.
            (0, CURRENT.to_owned())
        ]
    );

    // Enter and the next key step from the bar, ⇧Enter and the previous key
    // step back, wrapping at the ends.
    editor.press_unsettled("<cr>", cx);
    wait_for_message(&editor, "2 of 3", cx);
    assert_eq!(editor.caret(cx), 11);
    editor.press_unsettled(NEXT, cx);
    wait_for_message(&editor, "3 of 3", cx);
    editor.press_unsettled(NEXT, cx);
    wait_for_message(&editor, "1 of 3", cx);
    editor.press_unsettled("<s-cr>", cx);
    wait_for_message(&editor, "3 of 3", cx);
    editor.press_unsettled(PREVIOUS, cx);
    wait_for_message(&editor, "2 of 3", cx);
    assert_eq!(editor.caret(cx), 11);

    // The bar keeps its text; the keys went to the bar, not the text.
    assert_eq!(editor.first_bar(cx).unwrap().1, "foo");
    assert_eq!(editor.marked(cx).replace('‸', ""), "foo bar\nFoo baz foo\n");
}

#[gpui::test]
fn escape_closes_the_bar_and_clears_the_highlights(cx: &mut TestAppContext) {
    let product = TestProduct::start(PACKAGES, cx);
    let editor = product.windowed_editor("foo bar\nbaz\n", cx);

    open_with(&editor, "baz", cx);
    wait_for_message(&editor, "1 of 1", cx);
    editor.press_unsettled("<esc>", cx);
    wait_until(cx, |cx| {
        editor.first_bar(cx).is_none() && editor.decorations(cx).is_empty()
    });
    // The match stays selected, and typing edits the text again.
    assert_eq!(editor.marked(cx), "foo bar\n‸baz‸\n");

    // With the bar closed, the next key finds from the caret with no bar.
    editor.select(0, 0, cx);
    editor.press_unsettled(NEXT, cx);
    wait_until(cx, |cx| editor.caret(cx) == 11);
    assert!(editor.first_bar(cx).is_none());
    assert!(editor.decorations(cx).is_empty());
}

#[gpui::test]
fn edits_while_the_bar_is_open_search_again(cx: &mut TestAppContext) {
    let product = TestProduct::start(PACKAGES, cx);
    let editor = product.windowed_editor("ab\nab\n", cx);

    open_with(&editor, "ab", cx);
    wait_for_message(&editor, "1 of 2", cx);
    // Deleting the first match leaves one, and the current match is gone.
    editor.run_command("editor.delete-forward", cx);
    wait_for_message(&editor, "1 result", cx);
    assert_eq!(editor.decorations(cx).len(), 1);
}

#[gpui::test]
fn more_than_a_thousand_matches_highlight_a_thousand(cx: &mut TestAppContext) {
    let product = TestProduct::start(PACKAGES, cx);
    let editor = product.windowed_editor(&"x\n".repeat(1500), cx);

    open_with(&editor, "x", cx);
    wait_for_message(&editor, "1000+", cx);
    // A thousand matches, and the current one underlined.
    assert_eq!(editor.decorations(cx).len(), 1001);
    // Stepping past the highlighted matches still finds the rest.
    editor.select(2 * 1100, 2 * 1100, cx);
    editor.press_unsettled(NEXT, cx);
    wait_until(cx, |cx| editor.caret(cx) == 2 * 1100 + 1);
}

#[gpui::test]
fn the_key_fills_the_field_from_a_selection_or_the_last_query(cx: &mut TestAppContext) {
    let product = TestProduct::start(PACKAGES, cx);
    let editor = product.windowed_editor("one two\nthree\n", cx);

    editor.select(4, 7, cx);
    editor.press_unsettled(FIND, cx);
    wait_until(cx, |cx| {
        editor.first_bar(cx).is_some_and(|(_, text)| text == "two")
    });
    editor.press_unsettled("<esc>", cx);
    wait_until(cx, |cx| editor.first_bar(cx).is_none());

    // A multi-line selection falls back to the last query.
    editor.select(0, 10, cx);
    editor.press_unsettled(FIND, cx);
    wait_until(cx, |cx| {
        editor.first_bar(cx).is_some_and(|(_, text)| text == "two")
    });
}
