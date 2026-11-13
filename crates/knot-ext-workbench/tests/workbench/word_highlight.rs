//! The bundled word highlight package, following a window's active editor.
use std::time::Duration;

use gpui::TestAppContext;

use super::bundled::only;
use knot::test_support::{TestEditor, TestProduct, wait_until};

const WORD: &str = "editor.wordHighlight";
const SELECTION: &str = "editor.selectionHighlight";
const DELAY: Duration = Duration::from_millis(75);

/// Advance the product's virtual clock and run the tasks it wakes.
fn advance(cx: &mut TestAppContext, delay: Duration) {
    cx.executor().advance_clock(delay);
    cx.run_until_parked();
}

/// Advance past the delay so the package's timer fires.
fn rest(cx: &mut TestAppContext) {
    advance(cx, DELAY);
}

fn painted(editor: &TestEditor, cx: &mut TestAppContext) -> Vec<(usize, usize, String)> {
    editor.decorations(cx)
}

#[gpui::test]
fn the_caret_word_is_highlighted_after_the_delay(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/word-highlight"]), cx);
    let editor = product.windowed_editor("foo bar\nFoo baz foo\n", cx);

    advance(cx, DELAY - Duration::from_millis(1));
    assert!(painted(&editor, cx).is_empty(), "the delay has not elapsed");
    advance(cx, Duration::from_millis(1));
    wait_until(cx, |cx| painted(&editor, cx).len() == 2);
    assert_eq!(
        painted(&editor, cx),
        [(0, 3, WORD.to_owned()), (16, 19, WORD.to_owned())]
    );

    // Moving within the word keeps its highlights, and the end of the word
    // still counts as the word.
    editor.select(2, 2, cx);
    editor.settle(cx);
    rest(cx);
    assert_eq!(painted(&editor, cx).len(), 2);
    editor.select(3, 3, cx);
    editor.settle(cx);
    rest(cx);
    assert_eq!(painted(&editor, cx).len(), 2);

    // A different word repaints, even though it occurs only once.
    editor.select(4, 4, cx);
    editor.settle(cx);
    rest(cx);
    wait_until(cx, |cx| painted(&editor, cx) == [(4, 7, WORD.to_owned())]);
}

#[gpui::test]
fn a_selection_highlights_its_other_occurrences_case_insensitively(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/word-highlight"]), cx);
    let editor = product.windowed_editor("foo bar Foo foo\n", cx);

    editor.select(0, 3, cx);
    editor.settle(cx);
    rest(cx);
    wait_until(cx, |cx| painted(&editor, cx).len() == 2);
    assert_eq!(
        painted(&editor, cx),
        [
            (8, 11, SELECTION.to_owned()),
            (12, 15, SELECTION.to_owned())
        ]
    );
}

#[gpui::test]
fn leaving_the_word_clears_the_highlights_after_the_delay(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/word-highlight"]), cx);
    let editor = product.windowed_editor("foo   bar\n", cx);
    rest(cx);
    wait_until(cx, |cx| painted(&editor, cx) == [(0, 3, WORD.to_owned())]);

    editor.select(4, 4, cx);
    editor.settle(cx);
    advance(cx, DELAY - Duration::from_millis(1));
    assert_eq!(
        painted(&editor, cx),
        [(0, 3, WORD.to_owned())],
        "the previous highlights survive until the delay elapses"
    );
    advance(cx, Duration::from_millis(1));
    wait_until(cx, |cx| painted(&editor, cx).is_empty());
}

#[gpui::test]
fn an_edit_repaints_the_highlights(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/word-highlight"]), cx);
    let editor = product.windowed_editor("aa bb aa\n", cx);
    rest(cx);
    wait_until(cx, |cx| painted(&editor, cx).len() == 2);

    // The typed 'a' makes the caret's word unique.
    editor.type_input("a", cx);
    editor.settle(cx);
    rest(cx);
    wait_until(cx, |cx| painted(&editor, cx) == [(0, 3, WORD.to_owned())]);

    // Deleting it brings the second occurrence back.
    editor.press("<bs>", cx);
    rest(cx);
    wait_until(cx, |cx| painted(&editor, cx).len() == 2);
}

#[gpui::test]
fn the_settings_turn_each_kind_off(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/word-highlight"]), cx);
    let editor = product.windowed_editor("foo bar foo\n", cx);
    rest(cx);
    wait_until(cx, |cx| painted(&editor, cx).len() == 2);

    product.set_setting("wordHighlight.enabled", false.into(), cx);
    editor.settle(cx);
    rest(cx);
    wait_until(cx, |cx| painted(&editor, cx).is_empty());

    // The selection kind still paints, without the caret word.
    editor.select(0, 3, cx);
    editor.settle(cx);
    rest(cx);
    wait_until(cx, |cx| {
        painted(&editor, cx) == [(8, 11, SELECTION.to_owned())]
    });

    product.set_setting("selectionHighlight.enabled", false.into(), cx);
    editor.settle(cx);
    rest(cx);
    wait_until(cx, |cx| painted(&editor, cx).is_empty());
}

#[gpui::test]
fn the_delay_setting_governs_the_wait(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/word-highlight"]), cx);
    let editor = product.windowed_editor("foo bar foo\n", cx);
    product.set_setting("wordHighlight.delay", 200.into(), cx);
    editor.settle(cx);

    advance(cx, Duration::from_millis(199));
    assert!(
        painted(&editor, cx).is_empty(),
        "the new delay has not elapsed"
    );
    advance(cx, Duration::from_millis(1));
    wait_until(cx, |cx| painted(&editor, cx).len() == 2);
}

#[gpui::test]
fn only_the_active_view_paints_the_highlights(cx: &mut TestAppContext) {
    let product = TestProduct::start(only(&["@knot/word-highlight"]), cx);
    let editor = product.windowed_editor("foo bar foo\n", cx);
    rest(cx);
    wait_until(cx, |cx| painted(&editor, cx).len() == 2);

    let split = editor.split(cx);
    rest(cx);
    wait_until(cx, |cx| painted(&split, cx).len() == 2);
    assert!(
        painted(&editor, cx).is_empty(),
        "the view that lost the caret"
    );
}
