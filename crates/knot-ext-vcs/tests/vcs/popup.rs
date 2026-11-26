//! The hunk popup in the launched product: `@knot/vcs` over a test backend
//! whose baseline is `a`, `b`, `c`, `d` on lines of their own.
use gpui::TestAppContext;
use knot::test_support::{CommandOutcome, TestWindow, wait_until};

use super::gutter::show_windowed;

/// Against the baseline: line 0 modified, `c` removed before line 2, and
/// line 3 added.
const CHANGED: &str = "A\nb\nd\ne\n";

fn text(window: &TestWindow, cx: &mut TestAppContext) -> String {
    window.editors(cx).remove(0).text
}

fn caret_line(window: &TestWindow, cx: &mut TestAppContext) -> usize {
    let editor = window.editors(cx).remove(0);
    editor.text[..editor.caret].matches('\n').count()
}

/// A window showing `CHANGED` with its marks painted.
fn changed(
    cx: &mut TestAppContext,
) -> (
    knot::test_support::TestProduct,
    TestWindow,
    tempfile::TempDir,
) {
    let shown = show_windowed(CHANGED, cx);
    wait_until(cx, |cx| shown.1.gutter_bars(0, cx).len() == 3);
    shown.1.focus_document(cx);
    shown
}

const BUTTONS: [&str; 4] = [
    "Previous Change",
    "Next Change",
    "Revert Change",
    "Compare with Last Commit",
];

#[gpui::test]
fn a_click_on_a_mark_opens_the_baseline_lines_under_the_hunk(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.popovers(cx).is_empty());
    let popovers = window.popovers(cx);
    assert_eq!(popovers.len(), 1);
    assert_eq!(popovers[0].id, "vcs.hunk");
    assert_eq!(popovers[0].anchor, 0..1);
    assert_eq!(popovers[0].buttons, BUTTONS);
    assert_eq!(popovers[0].texts, ["a\n"]);
}

#[gpui::test]
fn an_insertions_popup_shows_no_baseline_lines(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(3, cx);
    wait_until(cx, |cx| !window.popovers(cx).is_empty());
    let popovers = window.popovers(cx);
    assert_eq!(popovers[0].anchor, 3..4);
    assert_eq!(popovers[0].buttons, BUTTONS);
    assert_eq!(popovers[0].texts, [""]);
}

#[gpui::test]
fn a_removals_popup_sits_on_the_line_before_it(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(2, cx);
    wait_until(cx, |cx| !window.popovers(cx).is_empty());
    let popovers = window.popovers(cx);
    assert_eq!(popovers[0].anchor, 1..2);
    assert_eq!(popovers[0].texts, ["c\n"]);
}

#[gpui::test]
fn next_and_previous_move_the_caret_and_the_popup_and_wrap(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.set_view(0, 0, 0, cx);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.popovers(cx).is_empty());

    for (button, line, anchor) in [
        ("Next Change", 2, 1..2),
        ("Next Change", 3, 3..4),
        ("Next Change", 0, 0..1),
        ("Previous Change", 3, 3..4),
        ("Previous Change", 2, 1..2),
    ] {
        window.click_popover_button(button, cx);
        wait_until(cx, |cx| {
            caret_line(&window, cx) == line
                && window
                    .popovers(cx)
                    .first()
                    .map(|popover| popover.anchor.clone())
                    == Some(anchor.clone())
        });
    }
}

#[gpui::test]
fn alt_f5_moves_an_open_popup_and_does_not_open_one(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.set_view(0, 0, 0, cx);
    window.press("alt-f5", cx);
    wait_until(cx, |cx| caret_line(&window, cx) == 2);
    assert!(window.popovers(cx).is_empty());

    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.popovers(cx).is_empty());
    window.press("alt-f5", cx);
    // The caret was already on the removal.
    wait_until(cx, |cx| {
        window
            .popovers(cx)
            .first()
            .map(|popover| popover.anchor.clone())
            == Some(3..4)
    });
}

#[gpui::test]
fn reverting_restores_the_baseline_in_one_undo_step_and_closes_the_popup(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.popovers(cx).is_empty());
    window.click_popover_button("Revert Change", cx);
    wait_until(cx, |cx| text(&window, cx) == "a\nb\nd\ne\n");
    wait_until(cx, |cx| window.popovers(cx).is_empty());

    assert_eq!(
        window.run_command("editor.undo", cx),
        CommandOutcome::Completed
    );
    assert_eq!(text(&window, cx), CHANGED);
}

#[gpui::test]
fn the_commands_work_on_the_hunk_at_the_caret(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.set_view(0, 0, 0, cx);
    assert_eq!(
        window.run_command("vcs.showChange", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| !window.popovers(cx).is_empty());
    assert_eq!(window.popovers(cx)[0].anchor, 0..1);

    // The removal of `c`, and then the added line.
    window.set_view(5, 5, 0, cx);
    assert_eq!(
        window.run_command("vcs.revertChange", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| text(&window, cx) == "A\nb\nc\nd\ne\n");
}

#[gpui::test]
fn a_caret_on_no_change_leaves_the_commands_unavailable(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.set_view(2, 2, 0, cx);
    for command in ["vcs.showChange", "vcs.revertChange"] {
        assert_eq!(window.run_command(command, cx), CommandOutcome::Unavailable);
    }
    assert!(window.popovers(cx).is_empty());
}

#[gpui::test]
fn a_new_baseline_closes_the_popup(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.append("f\n", cx);
    wait_until(cx, |cx| window.gutter_bars(0, cx).len() == 3);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.popovers(cx).is_empty());
    // Saving fetches the baseline again.
    assert_eq!(
        window.run_command("file.save", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| window.popovers(cx).is_empty());
}
