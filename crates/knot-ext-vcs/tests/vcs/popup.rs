//! The hunk popup, a zone, in the launched product: `@knot/vcs` over a test backend
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
    "Previous Hunk",
    "Next Hunk",
    "Revert Hunk",
    "Compare with Last Commit",
];
const COPY: &str = "Copy Hunk";

#[gpui::test]
fn a_click_on_a_mark_opens_the_baseline_lines_under_the_hunk(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    let zones = window.zones(cx);
    assert_eq!(zones.len(), 1);
    assert_eq!(zones[0].id, "vcs.hunk");
    assert_eq!(zones[0].line, 0);
    assert_eq!(zones[0].buttons, [BUTTONS.as_slice(), &[COPY]].concat());
    assert_eq!(zones[0].texts, ["a"]);
    assert_eq!(zones[0].gutter.as_deref(), Some("diff.modified"));
}

#[gpui::test]
fn an_insertions_popup_has_only_the_toolbar(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(3, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    let zones = window.zones(cx);
    assert_eq!(zones[0].line, 3);
    assert_eq!(zones[0].gutter, None);
    assert_eq!(zones[0].buttons, BUTTONS);
    assert!(zones[0].texts.is_empty());
}

#[gpui::test]
fn copy_hunk_puts_the_baseline_lines_on_the_clipboard(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
    window.click_zone_button(COPY, cx);
    wait_until(cx, |cx| {
        cx.read_from_clipboard().and_then(|item| item.text()) == Some("a\n".to_owned())
    });
}

#[gpui::test]
fn a_removals_popup_sits_above_the_line_that_followed_it(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(2, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    let zones = window.zones(cx);
    assert_eq!(zones[0].line, 2);
    assert_eq!(zones[0].texts, ["c"]);
    assert_eq!(zones[0].gutter.as_deref(), Some("diff.removed"));
}

#[gpui::test]
fn next_and_previous_move_the_caret_and_the_popup_and_wrap(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.set_view(0, 0, 0, cx);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());

    for (button, line, zone_line) in [
        ("Next Hunk", 2, 2),
        ("Next Hunk", 3, 3),
        ("Next Hunk", 0, 0),
        ("Previous Hunk", 3, 3),
        ("Previous Hunk", 2, 2),
    ] {
        window.click_zone_button(button, cx);
        wait_until(cx, |cx| {
            caret_line(&window, cx) == line
                && window.zones(cx).first().map(|zone| zone.line) == Some(zone_line)
        });
    }
}

#[gpui::test]
fn alt_f5_moves_an_open_popup_and_does_not_open_one(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.set_view(0, 0, 0, cx);
    window.press("alt-f5", cx);
    wait_until(cx, |cx| caret_line(&window, cx) == 2);
    assert!(window.zones(cx).is_empty());

    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    window.press("alt-f5", cx);
    // The caret was already on the removal.
    wait_until(cx, |cx| {
        window.zones(cx).first().map(|zone| zone.line) == Some(3)
    });
}

#[gpui::test]
fn reverting_restores_the_baseline_in_one_undo_step_and_closes_the_popup(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    window.click_zone_button("Revert Hunk", cx);
    wait_until(cx, |cx| text(&window, cx) == "a\nb\nd\ne\n");
    wait_until(cx, |cx| window.zones(cx).is_empty());

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
        window.run_command("vcs.showHunk", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    assert_eq!(window.zones(cx)[0].line, 0);

    // The removal of `c`, and then the added line.
    window.set_view(5, 5, 0, cx);
    assert_eq!(
        window.run_command("vcs.revertHunk", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| text(&window, cx) == "A\nb\nc\nd\ne\n");
}

#[gpui::test]
fn a_caret_on_no_change_leaves_the_commands_unavailable(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.set_view(2, 2, 0, cx);
    for command in ["vcs.showHunk", "vcs.revertHunk"] {
        assert_eq!(window.run_command(command, cx), CommandOutcome::Unavailable);
    }
    assert!(window.zones(cx).is_empty());
}

#[gpui::test]
fn a_new_baseline_keeps_the_popup_on_a_hunk_it_still_has(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.append("f\n", cx);
    wait_until(cx, |cx| window.gutter_bars(0, cx).len() == 3);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    // Saving fetches the baseline again; the hunk is still there.
    assert_eq!(
        window.run_command("file.save", cx),
        CommandOutcome::Completed
    );
    wait_until(cx, |cx| window.gutter_bars(0, cx).len() == 3);
    assert_eq!(window.zones(cx).len(), 1);
    assert_eq!(window.zones(cx)[0].texts, ["a"]);
}

#[gpui::test]
fn a_removal_at_the_end_of_the_file_sits_below_the_last_line(cx: &mut TestAppContext) {
    // Against the baseline `a`, `b`, `c`, `d`, only `d` is gone.
    let shown = show_windowed("a\nb\nc\n", cx);
    wait_until(cx, |cx| shown.1.gutter_bars(0, cx).len() == 1);
    shown.1.focus_document(cx);
    shown.1.click_gutter_bar(3, cx);
    wait_until(cx, |cx| !shown.1.zones(cx).is_empty());
    let zones = shown.1.zones(cx);
    assert_eq!(zones[0].texts, ["d"]);
    assert_eq!(zones[0].line, text(&shown.1, cx).matches('\n').count());
}

#[gpui::test]
fn an_edit_closes_the_popup(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    window.append("f\n", cx);
    wait_until(cx, |cx| window.zones(cx).is_empty());
}

#[gpui::test]
fn a_removals_wedge_is_clickable_on_both_sides_of_its_boundary(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    // The removal of `c` points at the boundary above line 2.
    window.click_gutter_edge(2, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    assert_eq!(window.zones(cx)[0].line, 2);
}

#[gpui::test]
fn clicking_the_open_hunks_mark_again_closes_the_popup(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| window.zones(cx).is_empty());
}

#[gpui::test]
fn clicking_the_gutter_beside_the_zone_closes_it(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(2, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    for _ in 0..2 {
        cx.refresh().unwrap();
        cx.run_until_parked();
    }
    // Row 2 is the zone's first row.
    window.click_gutter_bar(2, cx);
    wait_until(cx, |cx| window.zones(cx).is_empty());
}

#[gpui::test]
fn escape_in_the_text_closes_the_popup(cx: &mut TestAppContext) {
    let (_product, window, _directory) = changed(cx);
    window.click_gutter_bar(0, cx);
    wait_until(cx, |cx| !window.zones(cx).is_empty());
    window.focus_document(cx);
    window.press("escape", cx);
    wait_until(cx, |cx| window.zones(cx).is_empty());
}
