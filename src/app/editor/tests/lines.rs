//! Copy, delete, move, and join lines, and Smart Home.

use gpui::TestAppContext;

use super::check;

const COPY_UP: &str = "editor.copy-lines-up";
const COPY_DOWN: &str = "editor.copy-lines-down";
const MOVE_UP: &str = "editor.move-lines-up";
const MOVE_DOWN: &str = "editor.move-lines-down";
const DELETE: &str = "editor.delete-lines";
const JOIN: &str = "editor.join-lines";

#[gpui::test]
fn copy_lines_duplicates_the_selected_lines(cx: &mut TestAppContext) {
    check(
        cx,
        true,
        ("a\nbc\nd", (1, 1), (1, 1)),
        COPY_DOWN,
        ("a\nbc\nbc\nd", (2, 1), (2, 1)),
    );
    check(
        cx,
        true,
        ("a\nbc\nd", (1, 1), (1, 1)),
        COPY_UP,
        ("a\nbc\nbc\nd", (1, 1), (1, 1)),
    );
    // A selection over two lines, ending at column 0 of a third.
    check(
        cx,
        true,
        ("a\nb\nc", (0, 0), (2, 0)),
        COPY_DOWN,
        ("a\nb\na\nb\nc", (2, 0), (4, 0)),
    );
    // The last line without a terminator, and CRLF text.
    check(
        cx,
        true,
        ("a\r\nb", (1, 1), (1, 0)),
        COPY_DOWN,
        ("a\r\nb\r\nb", (2, 1), (2, 0)),
    );
    check(
        cx,
        true,
        ("a\r\nb", (0, 0), (0, 0)),
        COPY_UP,
        ("a\r\na\r\nb", (0, 0), (0, 0)),
    );
}

#[gpui::test]
fn delete_lines_removes_the_lines_and_their_terminator(cx: &mut TestAppContext) {
    check(
        cx,
        true,
        ("ab\ncdef\ngh", (1, 3), (1, 3)),
        DELETE,
        ("ab\ngh", (1, 2), (1, 2)),
    );
    check(
        cx,
        true,
        ("ab\ncd\nef", (0, 1), (1, 1)),
        DELETE,
        ("ef", (0, 1), (0, 1)),
    );
    // The last line takes the previous line's terminator.
    check(
        cx,
        true,
        ("ab\r\ncd", (1, 1), (1, 1)),
        DELETE,
        ("ab", (0, 1), (0, 1)),
    );
    check(
        cx,
        true,
        ("only", (0, 2), (0, 2)),
        DELETE,
        ("", (0, 0), (0, 0)),
    );
    // A selection ending at column 0 keeps that line.
    check(
        cx,
        true,
        ("a\nb\nc", (0, 0), (1, 0)),
        DELETE,
        ("b\nc", (0, 0), (0, 0)),
    );
}

#[gpui::test]
fn move_lines_swaps_with_the_neighbouring_line(cx: &mut TestAppContext) {
    check(
        cx,
        true,
        ("a\nbc\nd", (1, 1), (1, 1)),
        MOVE_DOWN,
        ("a\nd\nbc", (2, 1), (2, 1)),
    );
    check(
        cx,
        true,
        ("a\nbc\nd", (1, 1), (1, 1)),
        MOVE_UP,
        ("bc\na\nd", (0, 1), (0, 1)),
    );
    check(
        cx,
        true,
        ("a\nb\nc\nd", (0, 1), (1, 1)),
        MOVE_DOWN,
        ("c\na\nb\nd", (1, 1), (2, 1)),
    );
    check(
        cx,
        true,
        ("a\nb\nc\nd", (2, 0), (1, 0)),
        MOVE_UP,
        ("b\na\nc\nd", (1, 0), (0, 0)),
    );
    // A selection ending at column 0 ends after the moved lines.
    check(
        cx,
        true,
        ("a\nb\nc\nd", (1, 0), (3, 0)),
        MOVE_UP,
        ("b\nc\na\nd", (0, 0), (2, 0)),
    );
    check(
        cx,
        true,
        ("a\nb\nc\nd", (0, 0), (2, 0)),
        MOVE_DOWN,
        ("c\na\nb\nd", (1, 0), (3, 0)),
    );
    // Nothing moves past the document edges.
    check(
        cx,
        true,
        ("a\nb", (0, 1), (0, 1)),
        MOVE_UP,
        ("a\nb", (0, 1), (0, 1)),
    );
    check(
        cx,
        true,
        ("a\nb", (1, 1), (1, 1)),
        MOVE_DOWN,
        ("a\nb", (1, 1), (1, 1)),
    );
    // The last line, without a terminator, and CRLF text.
    check(
        cx,
        true,
        ("a\r\nb\r\nc", (1, 1), (1, 1)),
        MOVE_DOWN,
        ("a\r\nc\r\nb", (2, 1), (2, 1)),
    );
    check(
        cx,
        true,
        ("a\r\nb\r\nc", (2, 1), (2, 1)),
        MOVE_UP,
        ("a\r\nc\r\nb", (1, 1), (1, 1)),
    );
}

#[gpui::test]
fn join_lines_replaces_breaks_and_indentation_with_one_space(cx: &mut TestAppContext) {
    check(
        cx,
        true,
        ("fn a() {  \n    b();\n}", (0, 2), (0, 2)),
        JOIN,
        ("fn a() { b();\n}", (0, 8), (0, 8)),
    );
    check(
        cx,
        true,
        ("a\n  b\n\n  c\nd", (0, 0), (3, 3)),
        JOIN,
        ("a b c\nd", (0, 0), (0, 5)),
    );
    check(
        cx,
        true,
        ("a\r\n\r\nb", (0, 1), (0, 1)),
        JOIN,
        ("a\r\nb", (0, 1), (0, 1)),
    );
    check(
        cx,
        true,
        ("a\nb", (1, 1), (1, 1)),
        JOIN,
        ("a\nb", (1, 1), (1, 1)),
    );
}

#[gpui::test]
fn smart_home_toggles_between_the_first_character_and_column_0(cx: &mut TestAppContext) {
    for (from, to) in [(7, 4), (4, 0), (0, 4), (2, 4)] {
        check(
            cx,
            true,
            ("    foo", (0, from), (0, from)),
            "editor.move-smart-home",
            ("    foo", (0, to), (0, to)),
        );
    }
    check(
        cx,
        true,
        ("\tfoo", (0, 4), (0, 4)),
        "editor.select-smart-home",
        ("\tfoo", (0, 4), (0, 1)),
    );
    check(
        cx,
        true,
        ("   ", (0, 3), (0, 3)),
        "editor.move-smart-home",
        ("   ", (0, 0), (0, 0)),
    );
}
