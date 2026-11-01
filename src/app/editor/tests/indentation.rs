//! Enter, Tab, indent and outdent lines, and Backspace to a tab stop, in
//! both indentation styles.

use gpui::{AppContext, TestAppContext};

use super::{BufferModel, EditorView};
use crate::app::settings::{ApplicationSettings, EDITOR_INSERT_SPACES, SettingsScope};

type Position = (usize, usize);

/// Run `command` over `text` with the selection from `anchor` to `caret`,
/// check the text and selection it leaves, then check that undo restores the
/// original and redo the result.
fn check(
    cx: &mut TestAppContext,
    insert_spaces: bool,
    (text, anchor, caret): (&str, Position, Position),
    command: &str,
    (expected, expected_anchor, expected_caret): (&str, Position, Position),
) {
    cx.update(|cx| {
        let mut settings = ApplicationSettings::default();
        settings
            .0
            .set(
                EDITOR_INSERT_SPACES,
                insert_spaces.into(),
                SettingsScope::default(),
            )
            .unwrap();
        cx.set_global(settings);
    });
    let model = cx.new(|_| BufferModel::from_text(text));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    let selection = |view: &EditorView| {
        let caret = (view.cursor_line, view.cursor_col);
        let anchor = if view.has_selection {
            (view.anchor_line, view.anchor_col)
        } else {
            caret
        };
        (anchor, caret)
    };
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            (view.anchor_line, view.anchor_col) = anchor;
            (view.cursor_line, view.cursor_col) = caret;
            view.has_selection = anchor != caret;
            view.sync_view_position(cx);
            let context = format!("{command} on {text:?} from {anchor:?} to {caret:?}");

            assert!(view.execute_editing_command(command, window, cx));
            assert_eq!(view.text(cx), expected, "{context}");
            assert_eq!(
                selection(view),
                (expected_anchor, expected_caret),
                "{context}"
            );

            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), text, "undo of {context}");
            if expected != text {
                assert_eq!(selection(view), (anchor, caret), "undo of {context}");
                view.execute_editing_command("editor.redo", window, cx);
                assert_eq!(view.text(cx), expected, "redo of {context}");
                assert_eq!(
                    selection(view),
                    (expected_anchor, expected_caret),
                    "redo of {context}"
                );
            }
        })
    });
}

const NEWLINE: &str = "editor.insert-newline";
const TAB: &str = "editor.insert-tab";
const INDENT: &str = "editor.indent-lines";
const OUTDENT: &str = "editor.outdent-lines";
const BACKSPACE: &str = "editor.delete-backward";

#[gpui::test]
fn enter_keeps_the_indentation_before_the_caret(cx: &mut TestAppContext) {
    for spaces in [true, false] {
        check(
            cx,
            spaces,
            ("    foo", (0, 7), (0, 7)),
            NEWLINE,
            ("    foo\n    ", (1, 4), (1, 4)),
        );
        check(
            cx,
            spaces,
            ("\t\tfoo bar", (0, 6), (0, 9)),
            NEWLINE,
            ("\t\tfoo \n\t\t", (1, 2), (1, 2)),
        );
        check(
            cx,
            spaces,
            ("    foo", (0, 2), (0, 2)),
            NEWLINE,
            ("  \n    foo", (1, 2), (1, 2)),
        );
        check(
            cx,
            spaces,
            ("  a\r\nb", (0, 3), (0, 3)),
            NEWLINE,
            ("  a\r\n  \r\nb", (1, 2), (1, 2)),
        );
    }
}

#[gpui::test]
fn tab_inserts_one_unit_within_a_line(cx: &mut TestAppContext) {
    check(
        cx,
        true,
        ("ab", (0, 2), (0, 2)),
        TAB,
        ("ab  ", (0, 4), (0, 4)),
    );
    check(
        cx,
        true,
        ("abcd", (0, 1), (0, 3)),
        TAB,
        ("a   d", (0, 4), (0, 4)),
    );
    check(
        cx,
        false,
        ("abcd", (0, 3), (0, 1)),
        TAB,
        ("a\td", (0, 2), (0, 2)),
    );
}

#[gpui::test]
fn tab_and_indent_lines_indent_every_selected_line(cx: &mut TestAppContext) {
    // A selection across lines; the empty line is skipped and the start at
    // column 0 keeps covering the new indentation.
    check(
        cx,
        true,
        ("a\n\n  b\nc", (0, 0), (2, 3)),
        TAB,
        ("    a\n\n    b\nc", (0, 0), (2, 5)),
    );
    check(
        cx,
        false,
        ("a\n\n  b\nc", (2, 3), (0, 0)),
        TAB,
        ("\ta\n\n\t  b\nc", (2, 4), (0, 0)),
    );
    // A selection ending at column 0 leaves that line alone.
    check(
        cx,
        true,
        ("a\nb\nc", (0, 1), (2, 0)),
        INDENT,
        ("    a\n    b\nc", (0, 5), (2, 0)),
    );
    // With no selection, the caret's line, even an empty one.
    check(
        cx,
        true,
        ("a\nbc", (1, 1), (1, 1)),
        INDENT,
        ("a\n    bc", (1, 5), (1, 5)),
    );
    check(
        cx,
        false,
        ("", (0, 0), (0, 0)),
        INDENT,
        ("\t", (0, 1), (0, 1)),
    );
}

#[gpui::test]
fn outdent_removes_one_unit_from_every_selected_line(cx: &mut TestAppContext) {
    for spaces in [true, false] {
        check(
            cx,
            spaces,
            ("\t\ta\n      b\nc\n  d", (0, 0), (3, 3)),
            OUTDENT,
            ("\ta\n    b\nc\nd", (0, 0), (3, 1)),
        );
        check(
            cx,
            spaces,
            ("    ab", (0, 2), (0, 2)),
            OUTDENT,
            ("ab", (0, 0), (0, 0)),
        );
        check(
            cx,
            spaces,
            ("\ta\n\tb", (0, 2), (1, 0)),
            OUTDENT,
            ("a\n\tb", (0, 1), (1, 0)),
        );
        check(
            cx,
            spaces,
            ("a", (0, 1), (0, 1)),
            OUTDENT,
            ("a", (0, 1), (0, 1)),
        );
    }
}

#[gpui::test]
fn backspace_in_leading_spaces_deletes_to_the_previous_tab_stop(cx: &mut TestAppContext) {
    for spaces in [true, false] {
        check(
            cx,
            spaces,
            ("      a", (0, 6), (0, 6)),
            BACKSPACE,
            ("    a", (0, 4), (0, 4)),
        );
        check(
            cx,
            spaces,
            ("        a", (0, 8), (0, 8)),
            BACKSPACE,
            ("    a", (0, 4), (0, 4)),
        );
    }
    check(
        cx,
        true,
        ("  a  b", (0, 5), (0, 5)),
        BACKSPACE,
        ("  a b", (0, 4), (0, 4)),
    );
    check(
        cx,
        true,
        ("\t\ta", (0, 2), (0, 2)),
        BACKSPACE,
        ("\ta", (0, 1), (0, 1)),
    );
}

#[gpui::test]
fn detected_indentation_overrides_the_setting(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let model = cx.new(|_| {
        let mut model = BufferModel::from_text("a\n\tb\nc");
        model.detect_indent_style();
        model
    });
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.cursor_line = 2;
            view.execute_editing_command(TAB, window, cx);
            assert_eq!(view.text(cx), "a\n\tb\n\tc");
        })
    });
}
