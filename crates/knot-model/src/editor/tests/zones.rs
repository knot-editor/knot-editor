use knot_protocol::{ExtensionId, ExtensionKey, ExtensionLifecycleId};

use super::*;
use crate::editor::zones::{ClosedZone, ZoneSpec};

fn owner(extension: u64) -> ExtensionKey {
    ExtensionKey::new(ExtensionId::new(extension), ExtensionLifecycleId::new(1))
}

fn spec(line: usize) -> ZoneSpec {
    ZoneSpec { line, focus: false }
}

fn caret_line(f: &Fixture) -> usize {
    f.editor.caret().0
}

fn frames(f: &Fixture) -> Vec<Range<usize>> {
    f.editor
        .wrap()
        .zone_frames()
        .map(|(_, rows)| rows)
        .collect()
}

/// Lines `l0` through `l3` with a two-row zone above `l1` and a one-row zone
/// below the last line.
fn editor() -> Fixture {
    let mut f = Fixture::new("l0\nl1\nl2\nl3");
    let above = f.editor.open_zone(owner(1), "above", spec(1), &f.buffer);
    f.editor.set_zone_rows(above, 2, &f.buffer);
    let below = f.editor.open_zone(owner(1), "below", spec(4), &f.buffer);
    f.editor.set_zone_rows(below, 1, &f.buffer);
    f.editor.lay_out_for_test(10., 40, &f.buffer);
    f
}

#[test]
fn zone_rows_count_in_the_rows_but_hold_no_line() {
    let f = editor();
    // l0, z, z, l1, l2, l3, z
    assert_eq!(f.editor.wrap().row_count(), 7);
    assert_eq!(frames(&f), [1..3, 6..7]);
    assert_eq!(f.editor.row(&f.buffer, 3).line, 1);
    assert_eq!(f.editor.row(&f.buffer, 3).index, 3);
    assert_eq!(f.editor.row_start(2, &f.buffer), (1, 0));
    assert_eq!(f.editor.visible_lines(), 0..4);
}

#[test]
fn vertical_movement_steps_over_zone_rows() {
    let mut f = editor();
    for line in [1, 2, 3, 3] {
        assert!(f.run("editor.move-down"));
        assert_eq!(caret_line(&f), line);
    }
    for line in [2, 1, 0, 0] {
        assert!(f.run("editor.move-up"));
        assert_eq!(caret_line(&f), line);
    }
}

#[test]
#[allow(clippy::single_range_in_vec_init)]
fn zones_follow_edits_and_hide_with_folds() {
    let mut f = editor();
    f.place((0, 0), (0, 0));
    f.type_text("new\n");
    f.editor.lay_out_for_test(10., 40, &f.buffer);
    let lines: Vec<_> = f
        .editor
        .wrap()
        .zones()
        .iter()
        .map(|zone| zone.line)
        .collect();
    assert_eq!(lines, [2, 5]);
    assert_eq!(f.editor.wrap().row_count(), 8);
    // Folding under a line hides the zone above the lines it hides, and
    // hiding the last line hides the zone below it.
    assert!(f.editor.fold(1..3, &mut f.buffer));
    assert_eq!(f.editor.wrap().row_count(), 5);
    assert_eq!(frames(&f), [4..5]);
    assert!(f.editor.fold(3..5, &mut f.buffer));
    assert_eq!(f.editor.wrap().row_count(), 3);
    assert!(frames(&f).is_empty());
}

#[test]
fn a_zone_whose_line_is_deleted_moves_to_the_start_of_the_edit() {
    let mut f = editor();
    f.place((1, 0), (3, 0));
    assert!(f.run("editor.delete-backward"));
    f.editor.lay_out_for_test(10., 40, &f.buffer);
    assert_eq!(f.text(), "l0\nl3");
    let lines: Vec<_> = f
        .editor
        .wrap()
        .zones()
        .iter()
        .map(|zone| zone.line)
        .collect();
    assert_eq!(lines, [1, 2]);
}

#[test]
fn resizing_a_zone_above_the_viewport_keeps_the_top_text_in_place() {
    let mut f = Fixture::new("l0\nl1\nl2\nl3\nl4\nl5");
    let key = f.editor.open_zone(owner(1), "z", spec(1), &f.buffer);
    f.editor.set_zone_rows(key, 2, &f.buffer);
    f.editor.lay_out_for_test(3., 40, &f.buffer);
    // l0, z, z, l1, l2, ...: l2 on top.
    f.editor.scroll_to_for_test(4.);
    let top = |f: &Fixture| f.editor.row(&f.buffer, f.editor.scroll_top() as usize).line;
    assert_eq!(top(&f), 2);
    f.editor.set_zone_rows(key, 5, &f.buffer);
    assert_eq!((top(&f), f.editor.scroll_top()), (2, 7.));
    f.editor.set_zone_rows(key, 0, &f.buffer);
    assert_eq!((top(&f), f.editor.scroll_top()), (2, 2.));
}

#[test]
fn the_first_rows_of_a_zone_above_the_top_line_show() {
    let mut f = Fixture::new("l0\nl1\nl2\nl3\nl4\nl5");
    f.editor.lay_out_for_test(3., 40, &f.buffer);
    f.editor.scroll_to_for_test(2.);
    // Above the top line, `l2`: the zone shows, and the text moves down.
    let key = f.editor.open_zone(owner(1), "z", spec(2), &f.buffer);
    f.editor.set_zone_rows(key, 2, &f.buffer);
    assert_eq!(f.editor.scroll_top(), 2.);
    // Once it has rows, the top text stays where it is.
    f.editor.set_zone_rows(key, 3, &f.buffer);
    assert_eq!(f.editor.scroll_top(), 5.);
    // A zone above the top line stays out of view.
    let above = f.editor.open_zone(owner(1), "a", spec(1), &f.buffer);
    f.editor.set_zone_rows(above, 2, &f.buffer);
    assert_eq!(f.editor.scroll_top(), 7.);
}

#[test]
fn opening_updates_in_place_and_closing_frees_the_rows() {
    let mut f = editor();
    let key = f.editor.zones().get(owner(1), "above").unwrap().key;
    // Moving a zone keeps its rows.
    assert_eq!(
        f.editor.open_zone(owner(1), "above", spec(2), &f.buffer),
        key
    );
    assert_eq!(f.editor.wrap().row_count(), 7);
    assert_eq!(frames(&f), [2..4, 6..7]);
    assert!(f.editor.close_zone(owner(1), "above", &f.buffer));
    assert!(!f.editor.close_zone(owner(1), "above", &f.buffer));
    assert_eq!(f.editor.wrap().row_count(), 5);
    // Escape reports the dismissed zone; teardown does not.
    let below = f.editor.zones().get(owner(1), "below").unwrap().key;
    assert!(f.editor.dismiss_zone(below, &f.buffer));
    assert_eq!(
        f.editor.take_closed_zones(),
        [ClosedZone {
            owner: owner(1),
            id: "below".into()
        }]
    );
    let again = f.editor.open_zone(owner(2), "x", spec(0), &f.buffer);
    f.editor.set_zone_rows(again, 1, &f.buffer);
    assert_eq!(f.editor.wrap().row_count(), 5);
    f.editor.remove_owner_state(owner(2), &mut f.buffer);
    assert_eq!(f.editor.wrap().row_count(), 4);
    assert!(f.editor.zones().zones().is_empty());
}
