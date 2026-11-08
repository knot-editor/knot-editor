//! Enter, Tab, indent and outdent lines, and Backspace to a tab stop, in
//! both indentation styles.

use super::*;
use crate::editing::{BracketPair, EditingDefinition};

const NEWLINE: &str = "editor.insert-newline";
const TAB: &str = "editor.insert-tab";
const INDENT: &str = "editor.indent-lines";
const OUTDENT: &str = "editor.outdent-lines";
const BACKSPACE: &str = "editor.delete-backward";
const REINDENT: &str = "editor.reindent-selected-lines";

/// An editor over `text` in Rust, whose indent query is `indents` and whose
/// bracket pairs are braces, parentheses, and double quotes.
pub(super) fn rust_editor(indents: &str, text: &str) -> Fixture {
    let pair = |open: &str, close: &str| BracketPair {
        open: open.into(),
        close: close.into(),
        auto_close: None,
        newline: None,
        not_in: None,
    };
    let rust = Language {
        indents,
        editing: EditingDefinition {
            brackets: Some(vec![pair("{", "}"), pair("(", ")"), pair("\"", "\"")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .compile("rust");
    let mut buffer = BufferModel::from_text(text);
    set_syntax(&mut buffer, rust, Arc::new(|_| None));
    Fixture::over(buffer, EditorSettings::default())
}

/// The selection as anchor line and column, then caret line and column.
fn selection(f: &Fixture) -> (usize, usize, usize, usize) {
    let ((anchor_line, anchor_col), (line, col)) = f.selection();
    (anchor_line, anchor_col, line, col)
}

#[test]
fn enter_keeps_the_indentation_before_the_caret() {
    for spaces in [true, false] {
        check(
            spaces,
            ("    foo", (0, 7), (0, 7)),
            NEWLINE,
            ("    foo\n    ", (1, 4), (1, 4)),
        );
        check(
            spaces,
            ("\t\tfoo bar", (0, 6), (0, 9)),
            NEWLINE,
            ("\t\tfoo \n\t\t", (1, 2), (1, 2)),
        );
        check(
            spaces,
            ("    foo", (0, 2), (0, 2)),
            NEWLINE,
            ("  \n    foo", (1, 2), (1, 2)),
        );
        check(
            spaces,
            ("  a\r\nb", (0, 3), (0, 3)),
            NEWLINE,
            ("  a\r\n  \r\nb", (1, 2), (1, 2)),
        );
    }
}

#[test]
fn tab_inserts_one_unit_within_a_line() {
    check(true, ("ab", (0, 2), (0, 2)), TAB, ("ab  ", (0, 4), (0, 4)));
    check(
        true,
        ("abcd", (0, 1), (0, 3)),
        TAB,
        ("a   d", (0, 4), (0, 4)),
    );
    check(
        false,
        ("abcd", (0, 3), (0, 1)),
        TAB,
        ("a\td", (0, 2), (0, 2)),
    );
}

#[test]
fn tab_and_indent_lines_indent_every_selected_line() {
    // A selection across lines; the empty line is skipped and the start at
    // column 0 keeps covering the new indentation.
    check(
        true,
        ("a\n\n  b\nc", (0, 0), (2, 3)),
        TAB,
        ("    a\n\n    b\nc", (0, 0), (2, 5)),
    );
    check(
        false,
        ("a\n\n  b\nc", (2, 3), (0, 0)),
        TAB,
        ("\ta\n\n\t  b\nc", (2, 4), (0, 0)),
    );
    // A selection ending at column 0 leaves that line alone.
    check(
        true,
        ("a\nb\nc", (0, 1), (2, 0)),
        INDENT,
        ("    a\n    b\nc", (0, 5), (2, 0)),
    );
    // With no selection, the caret's line, even an empty one.
    check(
        true,
        ("a\nbc", (1, 1), (1, 1)),
        INDENT,
        ("a\n    bc", (1, 5), (1, 5)),
    );
    check(false, ("", (0, 0), (0, 0)), INDENT, ("\t", (0, 1), (0, 1)));
}

#[test]
fn outdent_removes_one_unit_from_every_selected_line() {
    for spaces in [true, false] {
        check(
            spaces,
            ("\t\ta\n      b\nc\n  d", (0, 0), (3, 3)),
            OUTDENT,
            ("\ta\n    b\nc\nd", (0, 0), (3, 1)),
        );
        check(
            spaces,
            ("    ab", (0, 2), (0, 2)),
            OUTDENT,
            ("ab", (0, 0), (0, 0)),
        );
        check(
            spaces,
            ("\ta\n\tb", (0, 2), (1, 0)),
            OUTDENT,
            ("a\n\tb", (0, 1), (1, 0)),
        );
        check(
            spaces,
            ("a", (0, 1), (0, 1)),
            OUTDENT,
            ("a", (0, 1), (0, 1)),
        );
    }
}

#[test]
fn backspace_in_leading_spaces_deletes_to_the_previous_tab_stop() {
    for spaces in [true, false] {
        check(
            spaces,
            ("      a", (0, 6), (0, 6)),
            BACKSPACE,
            ("    a", (0, 4), (0, 4)),
        );
        check(
            spaces,
            ("        a", (0, 8), (0, 8)),
            BACKSPACE,
            ("    a", (0, 4), (0, 4)),
        );
    }
    check(
        true,
        ("  a  b", (0, 5), (0, 5)),
        BACKSPACE,
        ("  a b", (0, 4), (0, 4)),
    );
    check(
        true,
        ("\t\ta", (0, 2), (0, 2)),
        BACKSPACE,
        ("\ta", (0, 1), (0, 1)),
    );
}

const BLOCKS: &str = "(block) @indent.begin\n\"}\" @indent.branch";

#[test]
fn reindent_lines_is_one_step_that_keeps_the_selection_on_its_text() {
    let text = "fn a() {\nb();\n  c();\n}";
    let mut f = rust_editor(BLOCKS, text);
    f.place((1, 1), (2, 3));
    assert!(f.run(REINDENT));
    assert_eq!(f.text(), "fn a() {\n    b();\n    c();\n}");
    assert_eq!(selection(&f), (1, 5, 2, 5));

    f.run("editor.undo");
    assert_eq!(f.text(), text);
    assert_eq!(selection(&f), (1, 1, 2, 3));

    // Without a language, nothing changes.
    let mut plain = Fixture::new(text);
    plain.place((0, 0), (3, 1));
    assert!(plain.run(REINDENT));
    assert_eq!(plain.text(), text);
}

#[test]
fn enter_indents_from_the_query_in_the_same_undo_step() {
    let text = "fn a() {\n    b();\n}";
    let mut f = rust_editor(BLOCKS, text);
    f.place((1, 8), (1, 8));
    f.run(NEWLINE);
    assert_eq!(f.text(), "fn a() {\n    b();\n    \n}");
    assert_eq!(selection(&f), (2, 4, 2, 4));
    // Over-indented text corrects on the new line.
    f.place((0, 8), (0, 8));
    f.run(NEWLINE);
    assert_eq!(f.text(), "fn a() {\n    \n    b();\n    \n}");

    f.run("editor.undo");
    assert_eq!(f.text(), "fn a() {\n    b();\n    \n}");
    assert_eq!(selection(&f), (0, 8, 0, 8));
    f.run("editor.undo");
    assert_eq!(f.text(), text);
    assert_eq!(selection(&f), (1, 8, 1, 8));
    f.run("editor.redo");
    assert_eq!(f.text(), "fn a() {\n    b();\n    \n}");
    assert_eq!(selection(&f), (2, 4, 2, 4));
}

#[test]
fn enter_between_a_bracket_pair_opens_an_indented_middle_line() {
    // Without an indent query, the middle line is one unit deeper.
    let mut f = rust_editor("", "  f(a, {});");
    f.place((0, 8), (0, 8));
    f.run(NEWLINE);
    assert_eq!(f.text(), "  f(a, {\n      \n  });");
    assert_eq!(selection(&f), (1, 6, 1, 6));
    f.run("editor.undo");
    assert_eq!(f.text(), "  f(a, {});");
    assert_eq!(selection(&f), (0, 8, 0, 8));

    // Whitespace inside the pair goes; a mismatched pair or quotes
    // only break the line.
    f.type_text(" ");
    f.run(NEWLINE);
    assert_eq!(f.text(), "  f(a, {\n      \n  });");
    f.run("editor.undo");
    f.run("editor.undo");
    f.place((0, 7), (0, 7));
    f.run(NEWLINE);
    assert_eq!(f.text(), "  f(a, \n  {});");
}

#[test]
fn enter_on_a_whitespace_only_line_clears_it() {
    for spaces in [true, false] {
        check(
            spaces,
            ("a\n    \nb", (1, 4), (1, 4)),
            NEWLINE,
            ("a\n\n    \nb", (2, 4), (2, 4)),
        );
        check(
            spaces,
            ("a\n  \t  \nb", (1, 2), (1, 2)),
            NEWLINE,
            ("a\n\n  \nb", (2, 2), (2, 2)),
        );
    }
}

#[test]
fn a_typed_closer_outdents_within_the_typing_undo_step() {
    let text = "fn a() {\n    if x {\n        b();\n        \n}";
    let mut f = rust_editor(BLOCKS, text);
    f.place((3, 8), (3, 8));
    f.type_text("}");
    assert_eq!(f.text(), "fn a() {\n    if x {\n        b();\n    }\n}");
    assert_eq!(selection(&f), (3, 5, 3, 5));
    // Typing goes on in the same step.
    f.type_text(" ");
    f.run("editor.undo");
    assert_eq!(f.text(), text);
    assert_eq!(selection(&f), (3, 8, 3, 8));
}

#[test]
fn composition_never_outdents() {
    let text = "fn a() {\n    if x {\n        b();\n        \n}";
    let mut f = rust_editor(BLOCKS, text);
    f.place((3, 8), (3, 8));
    f.editor.mark_text(None, "}", None, &mut f.buffer);
    f.type_text("}");
    assert_eq!(f.text(), "fn a() {\n    if x {\n        b();\n        }\n}");
}

#[test]
fn reindent_buffer_covers_the_whole_document_regardless_of_the_selection() {
    let text = "fn a() {\nb();\n  c();\n}";
    let mut f = rust_editor(BLOCKS, text);
    f.place((1, 0), (1, 0));
    assert!(f.run("editor.reindent-buffer"));
    assert_eq!(f.text(), "fn a() {\n    b();\n    c();\n}");
    f.run("editor.undo");
    assert_eq!(f.text(), text);
    // The selected variant leaves the other lines alone.
    f.place((1, 0), (1, 0));
    assert!(f.run(REINDENT));
    assert_eq!(f.text(), "fn a() {\n    b();\n  c();\n}");
}
