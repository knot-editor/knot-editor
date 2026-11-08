#![allow(clippy::single_range_in_vec_init)]

use super::*;
use crate::editing::EditingDefinition;

/// Ten lines, `l0` through `l9`, each three bytes with its terminator.
fn editor() -> Fixture {
    let text: String = (0..10).map(|line| format!("l{line}\n")).collect();
    Fixture::new(text.trim_end())
}

fn rows(f: &Fixture) -> usize {
    f.editor.wrap.row_count()
}

fn place(f: &mut Fixture, offset: usize) {
    f.editor
        .set_selection(offset, offset, Reveal::Nearest, &mut f.buffer);
}

fn fold(f: &mut Fixture, lines: Range<usize>) {
    assert!(f.editor.fold(lines, &mut f.buffer));
}

fn regions(f: &Fixture) -> Vec<Range<usize>> {
    f.buffer.fold_regions().to_vec()
}

fn rust_regions_editor(text: &str) -> Fixture {
    let rust = Language {
        folds: "(block) @fold\n(declaration_list) @fold",
        editing: EditingDefinition::default(),
        ..Default::default()
    }
    .compile("rust");
    let mut buffer = BufferModel::from_text(text);
    set_syntax(&mut buffer, rust, Arc::new(|_| None));
    Fixture::over(buffer, EditorSettings::default())
}

#[test]
fn a_collapsed_fold_hides_its_lines_from_rows() {
    let mut f = editor();
    fold(&mut f, 2..6);
    // Lines 3 through 5 are hidden.
    assert_eq!(rows(&f), 7);
    assert_eq!(f.editor.row_start(2, &f.buffer), (2, 0));
    assert_eq!(f.editor.row_start(3, &f.buffer), (6, 0));
    assert_eq!(f.editor.row_of_position(6, 0, &f.buffer), Some(3));
    assert_eq!(f.editor.folds(), [2..6]);
    assert!(f.editor.is_fold_start(2) && !f.editor.is_fold_start(3));
    // Folds need two lines inside the text.
    assert!(!f.editor.fold(4..5, &mut f.buffer));
    assert!(!f.editor.fold(8..11, &mut f.buffer));
    f.editor.unfold(&(2..6), &mut f.buffer);
    assert_eq!(rows(&f), 10);
    assert!(f.editor.folds().is_empty());
}

#[test]
fn folding_moves_a_caret_out_of_the_hidden_lines() {
    let mut f = editor();
    place(&mut f, 13);
    assert_eq!(f.editor.caret(), (4, 1));
    f.editor
        .set_selection(0, 13, Reveal::Nearest, &mut f.buffer);
    fold(&mut f, 2..6);
    assert_eq!(f.editor.caret(), (2, 1));
    assert!(!f.editor.has_selection());
}

#[test]
fn vertical_and_horizontal_moves_step_over_a_fold() {
    let mut f = editor();
    fold(&mut f, 2..6);
    place(&mut f, 3);
    f.run("editor.move-down");
    assert_eq!(f.editor.caret(), (2, 0));
    f.run("editor.move-down");
    assert_eq!(f.editor.caret(), (6, 0));
    f.run("editor.move-up");
    assert_eq!(f.editor.caret(), (2, 0));
    f.run("editor.move-down-logical");
    assert_eq!(f.editor.caret(), (6, 0));
    f.run("editor.move-up-logical");
    assert_eq!(f.editor.caret(), (2, 0));
    f.run("editor.move-line-end");
    f.run("editor.move-right");
    assert_eq!(f.editor.caret(), (6, 0));
    f.run("editor.move-left");
    assert_eq!(f.editor.caret(), (2, 2));
    assert_eq!(f.editor.folds(), [2..6]);
}

#[test]
fn revealing_a_hidden_position_expands_the_folds_hiding_it() {
    let mut f = editor();
    fold(&mut f, 1..8);
    fold(&mut f, 3..6);
    assert_eq!(rows(&f), 4);
    // Line 7 is hidden by the outer fold only.
    place(&mut f, 7 * 3);
    assert_eq!(f.editor.folds(), [3..6]);
    assert_eq!(f.editor.caret(), (7, 0));
    assert_eq!(rows(&f), 8);
    place(&mut f, 4 * 3);
    assert!(f.editor.folds().is_empty());
    assert_eq!(rows(&f), 10);
}

#[test]
fn nested_folds_hide_their_union_and_unfold_independently() {
    let mut f = editor();
    fold(&mut f, 3..5);
    fold(&mut f, 2..8);
    assert_eq!(rows(&f), 5);
    assert_eq!(f.editor.folds(), [2..8, 3..5]);
    f.editor.unfold(&(2..8), &mut f.buffer);
    assert_eq!(rows(&f), 9);
    f.editor.unfold(&(3..5), &mut f.buffer);
    assert_eq!(rows(&f), 10);
}

#[test]
fn folds_follow_edits_and_vanish_with_their_text() {
    let mut f = editor();
    fold(&mut f, 4..7);
    f.buffer.replace(0..0, "new\nlines\n").unwrap();
    f.editor.buffer_changed(&mut f.buffer);
    assert_eq!(f.editor.folds(), [6..9]);
    assert_eq!(rows(&f), 10);
    // Typing at the end of a fold's first line keeps the fold.
    let end_of_first = f.buffer.line_range(6).start + 2;
    place(&mut f, end_of_first);
    f.type_text("!");
    assert_eq!(f.editor.folds(), [6..9]);
    assert!(f.text().contains("l4!\n"));
    // Deleting the folded text drops the fold.
    let len = f.buffer.len();
    f.buffer.replace(3..len, "").unwrap();
    f.editor.buffer_changed(&mut f.buffer);
    assert!(f.editor.folds().is_empty());
    assert_eq!(rows(&f), 1);
}

#[test]
fn line_commands_take_a_collapsed_folds_lines_with_its_first() {
    let mut f = editor();
    fold(&mut f, 2..5);
    place(&mut f, 6);
    f.run("editor.delete-lines");
    assert_eq!(f.text(), "l0\nl1\nl5\nl6\nl7\nl8\nl9");
    assert!(f.editor.folds().is_empty());
}

#[test]
fn folds_hide_rows_of_wrapped_lines_too() {
    let settings = EditorSettings {
        soft_wrap: true,
        ..EditorSettings::default()
    };
    let text = "aaa bbb ccc ddd\nx\ny\naaa bbb ccc ddd";
    let mut f = Fixture::over(BufferModel::from_text(text), settings);
    f.editor.lay_out_for_test(10., 8, &f.buffer);
    assert_eq!(rows(&f), 2 + 1 + 1 + 2);
    fold(&mut f, 0..3);
    assert_eq!(rows(&f), 2 + 2);
    assert_eq!(f.editor.row_start(2, &f.buffer), (3, 0));
    assert_eq!(f.editor.row_of_position(3, 8, &f.buffer), Some(3));
}

#[test]
fn text_inserted_before_a_folds_first_line_does_not_join_the_fold() {
    let mut f = editor();
    fold(&mut f, 4..7);
    let start = f.buffer.line_range(4).start;
    place(&mut f, start);
    f.type_text("new\n");
    assert_eq!(f.editor.folds(), [5..8]);
    assert_eq!(rows(&f), 9);
}

const INDENTED: &str = "a\n  b\n    c\n\n    d\n  e\nf\ng\n  h";

#[test]
fn text_without_a_folds_query_folds_by_indentation() {
    let f = Fixture::new(INDENTED);
    // `a` holds through `e`, `b` through `d` without its trailing blank line,
    // and `g` holds `h`.
    assert_eq!(regions(&f), [0..6, 1..5, 7..9]);
}

#[test]
fn fold_commands_collapse_the_innermost_region_at_the_caret() {
    let mut f = Fixture::new(INDENTED);
    let at = |f: &mut Fixture, line| {
        let offset = f.buffer.line_range(line).start;
        place(f, offset);
    };
    // Inside `b`'s region, away from a first line, the innermost encloses.
    at(&mut f, 2);
    f.run("editor.fold");
    assert_eq!(f.editor.folds(), [1..5]);
    // The caret moved to the fold's first line, so folding again takes the
    // region around it.
    assert_eq!(f.editor.caret().0, 1);
    f.run("editor.fold");
    assert_eq!(f.editor.folds(), [0..6, 1..5]);
    // Unfold expands the outermost fold on the caret's line.
    f.run("editor.unfold");
    assert_eq!(f.editor.folds(), [1..5]);
    at(&mut f, 1);
    f.run("editor.toggle-fold");
    assert!(f.editor.folds().is_empty());
    f.run("editor.toggle-fold");
    assert_eq!(f.editor.folds(), [1..5]);
    f.run("editor.unfold-all");
    assert!(f.editor.folds().is_empty());
    f.run("editor.fold-all");
    assert_eq!(f.editor.folds(), [0..6, 1..5, 7..9]);
    assert_eq!(rows(&f), 3);
}

#[test]
fn a_folds_query_gives_regions_that_leave_the_closing_line_visible() {
    let source = "impl T {\n    fn f() {\n        a;\n        b;\n    }\n\n    fn g() { c }\n}\n";
    let f = rust_regions_editor(source);
    // The impl's body ends at its closing brace, which stays visible, and so
    // does `f`'s; `g` fits one line.
    assert_eq!(regions(&f), [0..7, 1..4]);
}

#[test]
fn folds_round_trip_through_view_state() {
    let mut f = editor();
    fold(&mut f, 5..8);
    fold(&mut f, 2..6);
    let state = f.editor.view_state(&f.buffer);
    // From the start of each first line to the end of its last line's text.
    assert_eq!(state.folds, vec![6..17, 15..23]);

    let mut other = EditorState::new("editor", EditorSettings::default(), &mut f.buffer);
    other.restore_view_state(state.clone(), &mut f.buffer);
    assert_eq!(other.wrap.row_count(), 5);
    assert_eq!(other.folds(), vec![2..6, 5..8]);
    assert_eq!(other.view_state(&f.buffer).folds, state.folds);

    // Restoring a state without folds expands the ones there.
    let mut plain = state;
    plain.folds.clear();
    other.restore_view_state(plain, &mut f.buffer);
    assert_eq!(other.wrap.row_count(), 10);
}

#[test]
fn restored_folds_clamp_to_shorter_text_and_drop_when_empty() {
    let f = editor();
    let mut shorter = Fixture::new("l0\nl1\nl2");
    let state = EditorViewState {
        // The first runs past the end; the second lies within one line; the
        // third is wholly past the end.
        folds: vec![3..400, 7..8, 300..400],
        ..f.editor.view_state(&f.buffer)
    };
    shorter
        .editor
        .restore_view_state(state, &mut shorter.buffer);
    assert_eq!(shorter.editor.folds(), vec![1..3]);
    assert_eq!(rows(&shorter), 2);
}
