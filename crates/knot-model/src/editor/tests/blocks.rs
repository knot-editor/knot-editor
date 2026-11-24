use super::*;

fn block(line: usize, texts: &[&str]) -> Block {
    Block {
        line,
        lines: texts
            .iter()
            .map(|text| BlockLine {
                text: (*text).into(),
                highlights: Vec::new(),
                key: None,
            })
            .collect(),
    }
}

/// Lines `l0` through `l3`, with two block rows above `l1` and one above `l3`.
fn editor() -> Fixture {
    let mut f = Fixture::new("l0\nl1\nl2\nl3");
    let blocks = vec![block(1, &["x", "y"]), block(3, &["z"])];
    f.editor.set_blocks(blocks, &f.buffer);
    f
}

fn caret_line(f: &Fixture) -> usize {
    f.editor.caret().0
}

#[test]
fn block_rows_count_in_the_rows_but_hold_no_line() {
    let mut f = editor();
    f.editor.lay_out_for_test(10., 40, &f.buffer);
    assert_eq!(f.editor.wrap().row_count(), 7);
    assert_eq!(f.editor.caret_row(), 0);
    // l0, x, y, l1, l2, z, l3
    assert_eq!(f.editor.row(&f.buffer, 1).line, 1);
    assert_eq!(f.editor.row(&f.buffer, 1).index, 3);
    assert_eq!(f.editor.row_start(2, &f.buffer), (1, 0));
    assert_eq!(f.editor.visible_lines(), 0..4);
}

#[test]
fn vertical_movement_steps_over_block_rows() {
    let mut f = editor();
    for line in [1, 2, 3] {
        assert!(f.run("editor.move-down"));
        assert_eq!(caret_line(&f), line);
    }
    assert!(f.run("editor.move-down"));
    assert_eq!(caret_line(&f), 3);
    for line in [2, 1, 0] {
        assert!(f.run("editor.move-up"));
        assert_eq!(caret_line(&f), line);
    }
    // The first line has a block above it too: up stays put.
    let mut f = Fixture::new("l0\nl1");
    f.editor.set_blocks(vec![block(0, &["x"])], &f.buffer);
    assert!(f.run("editor.move-up"));
    assert_eq!(f.editor.caret(), (0, 0));
    assert!(f.run("editor.move-down"));
    assert_eq!(f.editor.caret(), (1, 0));
    assert!(f.run("editor.move-up"));
    assert_eq!(f.editor.caret(), (0, 0));
}

#[test]
fn page_movement_and_selection_skip_block_rows() {
    let mut f = editor();
    f.editor.set_viewport_rows_for_test(3.);
    assert!(f.run("editor.move-page-down"));
    // Three rows down from row 0 is a text row, l1's row.
    assert_eq!(caret_line(&f), 1);
    f.place((0, 0), (0, 0));
    assert!(f.run("editor.select-down"));
    assert!(f.run("editor.select-down"));
    assert_eq!(f.selection(), ((0, 0), (2, 0)));
    // The selection covers the text only; blocks hold none.
    let range = f.editor.selected_byte_range(&f.buffer).unwrap();
    assert_eq!(f.buffer.read(range), "l0\nl1\n");
}

#[test]
fn blocks_follow_edits_and_folds() {
    let mut f = editor();
    // A line typed above shifts the blocks down with their lines.
    f.place((0, 0), (0, 0));
    f.type_text("new\n");
    f.editor.lay_out_for_test(10., 40, &f.buffer);
    let lines: Vec<_> = f.editor.wrap().blocks().map(|block| block.line).collect();
    assert_eq!(lines, [2, 4]);
    assert_eq!(f.editor.wrap().row_count(), 8);
    // Folding the lines under a block hides it.
    assert!(f.editor.fold(1..3, &mut f.buffer));
    assert_eq!(f.editor.wrap().row_count(), 5);
}
