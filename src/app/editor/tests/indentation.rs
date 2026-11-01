//! Enter, Tab, indent and outdent lines, and Backspace to a tab stop, in
//! both indentation styles.

use gpui::{AppContext, EntityInputHandler, TestAppContext};

use std::time::Duration;

use super::{BufferModel, EditorView, check};
use crate::app::languages::{
    self, LanguageDefinition, LanguageOwner, LanguageQueries,
    editing::{BracketPair, EditingDefinition},
};
use crate::app::settings::ApplicationSettings;
use crate::host::protocol::{ExtensionId, ExtensionLifecycleId, LanguageInfo};

const NEWLINE: &str = "editor.insert-newline";
const TAB: &str = "editor.insert-tab";
const INDENT: &str = "editor.indent-lines";
const OUTDENT: &str = "editor.outdent-lines";
const BACKSPACE: &str = "editor.delete-backward";
const REINDENT: &str = "editor.reindent-lines";

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

/// Register Rust with `indents` and bracket pairs, then open an editor
/// over `text` in it whose parses all run in the background when
/// `background`.
fn rust_editor<'a>(
    cx: &'a mut TestAppContext,
    indents: &str,
    text: &str,
    background: bool,
) -> (
    gpui::Entity<EditorView>,
    gpui::Entity<BufferModel>,
    &'a mut gpui::VisualTestContext,
) {
    cx.update(|cx| {
        cx.set_global(ApplicationSettings::default());
        let pair = |open: &str, close: &str| BracketPair {
            open: open.into(),
            close: close.into(),
            auto_close: None,
            newline: None,
            not_in: None,
        };
        let rust = LanguageDefinition {
            info: LanguageInfo {
                id: "rust".into(),
                name: "Rust".into(),
                grammar: "rust".into(),
                file_extensions: Vec::new(),
                file_names: Vec::new(),
                interpreters: Vec::new(),
            },
            queries: LanguageQueries {
                highlights: String::new(),
                injections: String::new(),
                indents: indents.into(),
            },
            editing: EditingDefinition {
                brackets: Some(vec![pair("{", "}"), pair("(", ")"), pair("\"", "\"")]),
                ..Default::default()
            },
        };
        let owner = LanguageOwner {
            extension: ExtensionId::new(1),
            lifecycle: ExtensionLifecycleId::new(1),
        };
        languages::register(owner, rust.compile().unwrap(), cx);
    });
    let model = cx.new(|_| {
        let mut model = BufferModel::from_text(text);
        if background {
            model.set_syntax_budget(Duration::ZERO);
        }
        model
    });
    cx.update(|cx| languages::set_language(&model, "rust".into(), cx));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    (editor, model, cx)
}

const BLOCKS: &str = "(block) @indent.begin\n\"}\" @indent.branch";

type Selection = (usize, usize, usize, usize);

fn selection(view: &EditorView) -> Selection {
    if view.has_selection {
        (
            view.anchor_line,
            view.anchor_col,
            view.cursor_line,
            view.cursor_col,
        )
    } else {
        (
            view.cursor_line,
            view.cursor_col,
            view.cursor_line,
            view.cursor_col,
        )
    }
}

fn place(view: &mut EditorView, (line, col): (usize, usize), cx: &mut gpui::Context<EditorView>) {
    (view.cursor_line, view.cursor_col) = (line, col);
    view.has_selection = false;
    view.sync_view_position(cx);
}

#[gpui::test]
fn reindent_lines_is_one_step_that_keeps_the_selection_on_its_text(cx: &mut TestAppContext) {
    let text = "fn a() {\nb();\n  c();\n}";
    let (editor, _, cx) = rust_editor(cx, BLOCKS, text, false);
    let plain = cx.update(|_, cx| {
        let model = cx.new(|_| BufferModel::from_text(text));
        cx.new(|cx| EditorView::new(model, cx))
    });
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            (view.anchor_line, view.anchor_col) = (1, 1);
            (view.cursor_line, view.cursor_col) = (2, 3);
            view.has_selection = true;
            view.sync_view_position(cx);
            assert!(view.execute_editing_command(REINDENT, window, cx));
            assert_eq!(view.text(cx), "fn a() {\n    b();\n    c();\n}");
            assert_eq!(selection(view), (1, 5, 2, 5));

            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), text);
            assert_eq!(selection(view), (1, 1, 2, 3));
        });
        // Without a language, nothing changes.
        plain.update(cx, |view, cx| {
            view.has_selection = true;
            (view.cursor_line, view.cursor_col) = (3, 1);
            view.sync_view_position(cx);
            assert!(view.execute_editing_command(REINDENT, window, cx));
            assert_eq!(view.text(cx), text);
        });
    });
}

#[gpui::test]
fn enter_indents_from_the_query_in_the_same_undo_step(cx: &mut TestAppContext) {
    let text = "fn a() {\n    b();\n}";
    let (editor, _, cx) = rust_editor(cx, BLOCKS, text, false);
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            place(view, (1, 8), cx);
            view.execute_editing_command(NEWLINE, window, cx);
            assert_eq!(view.text(cx), "fn a() {\n    b();\n    \n}");
            assert_eq!(selection(view), (2, 4, 2, 4));
            // Over-indented text corrects on the new line.
            place(view, (0, 8), cx);
            view.execute_editing_command(NEWLINE, window, cx);
            assert_eq!(view.text(cx), "fn a() {\n    \n    b();\n    \n}");

            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), "fn a() {\n    b();\n    \n}");
            assert_eq!(selection(view), (0, 8, 0, 8));
            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), text);
            assert_eq!(selection(view), (1, 8, 1, 8));
            view.execute_editing_command("editor.redo", window, cx);
            assert_eq!(view.text(cx), "fn a() {\n    b();\n    \n}");
            assert_eq!(selection(view), (2, 4, 2, 4));
        });
    });
}

#[gpui::test]
fn enter_indents_once_a_background_parse_covers_it(cx: &mut TestAppContext) {
    // Large enough that a parse never finishes inside a zero budget.
    let rest = "fn b() { let x = 1; }\n".repeat(5_000);
    let text = &format!("fn a() {{\n}}\n{rest}");
    let (editor, _, cx) = rust_editor(cx, BLOCKS, text, true);
    cx.run_until_parked();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            place(view, (0, 8), cx);
            view.execute_editing_command(NEWLINE, window, cx);
            // The break keeps the line's indentation until the tree covers it.
            assert_eq!(view.text(cx), format!("fn a() {{\n\n}}\n{rest}"));
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            assert_eq!(view.text(cx), format!("fn a() {{\n    \n}}\n{rest}"));
            assert_eq!(selection(view), (1, 4, 1, 4));
            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), *text);
            assert_eq!(selection(view), (0, 8, 0, 8));

            // An edit before the parse finishes drops the request.
            view.execute_editing_command(NEWLINE, window, cx);
            view.replace_text_in_range(None, "x", window, cx);
        });
    });
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(
            editor.read(cx).text(cx),
            format!("fn a() {{\nx\n}}\n{rest}")
        )
    });
}

#[gpui::test]
fn enter_between_a_bracket_pair_opens_an_indented_middle_line(cx: &mut TestAppContext) {
    // Without an indent query, the middle line is one unit deeper.
    let (editor, _, cx) = rust_editor(cx, "", "  f(a, {});", false);
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            place(view, (0, 8), cx);
            view.execute_editing_command(NEWLINE, window, cx);
            assert_eq!(view.text(cx), "  f(a, {\n      \n  });");
            assert_eq!(selection(view), (1, 6, 1, 6));
            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), "  f(a, {});");
            assert_eq!(selection(view), (0, 8, 0, 8));

            // Whitespace inside the pair goes; a mismatched pair or quotes
            // only break the line.
            view.replace_text_in_range(None, " ", window, cx);
            view.execute_editing_command(NEWLINE, window, cx);
            assert_eq!(view.text(cx), "  f(a, {\n      \n  });");
            view.execute_editing_command("editor.undo", window, cx);
            view.execute_editing_command("editor.undo", window, cx);
            place(view, (0, 7), cx);
            view.execute_editing_command(NEWLINE, window, cx);
            assert_eq!(view.text(cx), "  f(a, \n  {});");
        });
    });
}

#[gpui::test]
fn enter_on_a_whitespace_only_line_clears_it(cx: &mut TestAppContext) {
    for spaces in [true, false] {
        check(
            cx,
            spaces,
            ("a\n    \nb", (1, 4), (1, 4)),
            NEWLINE,
            ("a\n\n    \nb", (2, 4), (2, 4)),
        );
        check(
            cx,
            spaces,
            ("a\n  \t  \nb", (1, 2), (1, 2)),
            NEWLINE,
            ("a\n\n  \nb", (2, 2), (2, 2)),
        );
    }
}
