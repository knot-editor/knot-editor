#![allow(clippy::single_range_in_vec_init)]

use gpui::{AppContext, TestAppContext};

use super::super::{BufferModel, Editor, Reveal};
use crate::app::settings::{ApplicationSettings, EDITOR_SOFT_WRAP, SettingsScope};

/// Ten lines, `l0` through `l9`, each three bytes with its terminator.
fn editor(cx: &mut TestAppContext) -> (gpui::Entity<BufferModel>, gpui::Entity<Editor>) {
    let text: String = (0..10).map(|line| format!("l{line}\n")).collect();
    let model = cx.new(|_| BufferModel::from_text(text.trim_end()));
    let editor = cx.new(|cx| Editor::new(model.clone(), crate::app::EDITOR_KEY_CONTEXT, cx));
    (model, editor)
}

fn caret(editor: &gpui::Entity<Editor>, cx: &mut TestAppContext) -> (usize, usize) {
    cx.read(|cx| {
        let editor = editor.read(cx);
        (editor.cursor_line, editor.cursor_col)
    })
}

fn rows(editor: &gpui::Entity<Editor>, cx: &mut TestAppContext) -> usize {
    cx.read(|cx| editor.read(cx).wrap.row_count())
}

fn place(editor: &gpui::Entity<Editor>, offset: usize, cx: &mut TestAppContext) {
    editor.update(cx, |editor, cx| {
        editor.set_selection(offset, offset, Reveal::Nearest, cx)
    });
}

fn command(editor: &gpui::Entity<Editor>, name: &str, cx: &mut TestAppContext) {
    cx.update(|cx| editor.update(cx, |editor, cx| editor.execute_editing_command(name, cx)));
}

fn fold(editor: &gpui::Entity<Editor>, lines: std::ops::Range<usize>, cx: &mut TestAppContext) {
    assert!(editor.update(cx, |editor, cx| editor.fold(lines, cx)));
}

#[gpui::test]
fn a_collapsed_fold_hides_its_lines_from_rows(cx: &mut TestAppContext) {
    let (_, editor) = editor(cx);
    fold(&editor, 2..6, cx);
    // Lines 3 through 5 are hidden.
    assert_eq!(rows(&editor, cx), 7);
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.row_start(2, cx), (2, 0));
        assert_eq!(editor.row_start(3, cx), (6, 0));
        assert_eq!(editor.row_of_position(6, 0, cx), Some(3));
        assert_eq!(editor.folds(), [2..6]);
        assert!(editor.is_fold_start(2) && !editor.is_fold_start(3));
    });
    // Folds need two lines inside the text.
    assert!(!editor.update(cx, |editor, cx| editor.fold(4..5, cx)));
    assert!(!editor.update(cx, |editor, cx| editor.fold(8..11, cx)));
    editor.update(cx, |editor, cx| editor.unfold(&(2..6), cx));
    assert_eq!(rows(&editor, cx), 10);
    cx.read(|cx| assert!(editor.read(cx).folds().is_empty()));
}

#[gpui::test]
fn folding_moves_a_caret_out_of_the_hidden_lines(cx: &mut TestAppContext) {
    let (_, editor) = editor(cx);
    place(&editor, 13, cx);
    assert_eq!(caret(&editor, cx), (4, 1));
    editor.update(cx, |editor, cx| {
        editor.set_selection(0, 13, Reveal::Nearest, cx)
    });
    fold(&editor, 2..6, cx);
    assert_eq!(caret(&editor, cx), (2, 1));
    cx.read(|cx| assert!(!editor.read(cx).has_selection()));
}

#[gpui::test]
fn vertical_and_horizontal_moves_step_over_a_fold(cx: &mut TestAppContext) {
    let (_, editor) = editor(cx);
    fold(&editor, 2..6, cx);
    place(&editor, 3, cx);
    command(&editor, "editor.move-down", cx);
    assert_eq!(caret(&editor, cx), (2, 0));
    command(&editor, "editor.move-down", cx);
    assert_eq!(caret(&editor, cx), (6, 0));
    command(&editor, "editor.move-up", cx);
    assert_eq!(caret(&editor, cx), (2, 0));
    command(&editor, "editor.move-down-logical", cx);
    assert_eq!(caret(&editor, cx), (6, 0));
    command(&editor, "editor.move-up-logical", cx);
    assert_eq!(caret(&editor, cx), (2, 0));
    command(&editor, "editor.move-line-end", cx);
    command(&editor, "editor.move-right", cx);
    assert_eq!(caret(&editor, cx), (6, 0));
    command(&editor, "editor.move-left", cx);
    assert_eq!(caret(&editor, cx), (2, 2));
    cx.read(|cx| assert_eq!(editor.read(cx).folds(), [2..6]));
}

#[gpui::test]
fn revealing_a_hidden_position_expands_the_folds_hiding_it(cx: &mut TestAppContext) {
    let (_, editor) = editor(cx);
    fold(&editor, 1..8, cx);
    fold(&editor, 3..6, cx);
    assert_eq!(rows(&editor, cx), 4);
    // Line 7 is hidden by the outer fold only.
    place(&editor, 7 * 3, cx);
    cx.read(|cx| assert_eq!(editor.read(cx).folds(), [3..6]));
    assert_eq!(caret(&editor, cx), (7, 0));
    assert_eq!(rows(&editor, cx), 8);
    place(&editor, 4 * 3, cx);
    cx.read(|cx| assert!(editor.read(cx).folds().is_empty()));
    assert_eq!(rows(&editor, cx), 10);
}

#[gpui::test]
fn nested_folds_hide_their_union_and_unfold_independently(cx: &mut TestAppContext) {
    let (_, editor) = editor(cx);
    fold(&editor, 3..5, cx);
    fold(&editor, 2..8, cx);
    assert_eq!(rows(&editor, cx), 5);
    cx.read(|cx| assert_eq!(editor.read(cx).folds(), [2..8, 3..5]));
    editor.update(cx, |editor, cx| editor.unfold(&(2..8), cx));
    assert_eq!(rows(&editor, cx), 9);
    editor.update(cx, |editor, cx| editor.unfold(&(3..5), cx));
    assert_eq!(rows(&editor, cx), 10);
}

#[gpui::test]
fn folds_follow_edits_and_vanish_with_their_text(cx: &mut TestAppContext) {
    let (model, editor) = editor(cx);
    fold(&editor, 4..7, cx);
    cx.update(|cx| {
        model.update(cx, |model, cx| {
            model.replace(0..0, "new\nlines\n").unwrap();
            cx.notify();
        })
    });
    cx.read(|cx| assert_eq!(editor.read(cx).folds(), [6..9]));
    assert_eq!(rows(&editor, cx), 10);
    // Typing at the end of a fold's first line keeps the fold.
    let end_of_first = cx.read(|cx| {
        let model = model.read(cx);
        model.line_range(6).start + 2
    });
    place(&editor, end_of_first, cx);
    cx.update(|cx| editor.update(cx, |editor, cx| editor.type_text(None, "!", cx)));
    cx.read(|cx| {
        assert_eq!(editor.read(cx).folds(), [6..9]);
        assert!(model.read(cx).text().contains("l4!\n"));
    });
    // Deleting the folded text drops the fold.
    cx.update(|cx| {
        model.update(cx, |model, cx| {
            let len = model.len();
            model.replace(3..len, "").unwrap();
            cx.notify();
        })
    });
    cx.read(|cx| assert!(editor.read(cx).folds().is_empty()));
    assert_eq!(rows(&editor, cx), 1);
}

#[gpui::test]
fn line_commands_take_a_collapsed_folds_lines_with_its_first(cx: &mut TestAppContext) {
    let (model, editor) = editor(cx);
    fold(&editor, 2..5, cx);
    place(&editor, 6, cx);
    command(&editor, "editor.delete-lines", cx);
    cx.read(|cx| {
        assert_eq!(model.read(cx).text(), "l0\nl1\nl5\nl6\nl7\nl8\nl9");
        assert!(editor.read(cx).folds().is_empty());
    });
}

#[gpui::test]
fn folds_hide_rows_of_wrapped_lines_too(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut settings = ApplicationSettings::default();
        settings
            .0
            .set(EDITOR_SOFT_WRAP, true.into(), SettingsScope::default())
            .unwrap();
        cx.set_global(settings);
    });
    let model = cx.new(|_| BufferModel::from_text("aaa bbb ccc ddd\nx\ny\naaa bbb ccc ddd"));
    let editor = cx.new(|cx| Editor::new(model.clone(), crate::app::EDITOR_KEY_CONTEXT, cx));
    cx.update(|cx| editor.update(cx, |editor, cx| editor.lay_out_for_test(10., 8, cx)));
    assert_eq!(rows(&editor, cx), 2 + 1 + 1 + 2);
    fold(&editor, 0..3, cx);
    assert_eq!(rows(&editor, cx), 2 + 2);
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.row_start(2, cx), (3, 0));
        assert_eq!(editor.row_of_position(3, 8, cx), Some(3));
    });
}

#[gpui::test]
fn text_inserted_before_a_folds_first_line_does_not_join_the_fold(cx: &mut TestAppContext) {
    let (model, editor) = editor(cx);
    fold(&editor, 4..7, cx);
    let start = cx.read(|cx| model.read(cx).line_range(4).start);
    place(&editor, start, cx);
    cx.update(|cx| editor.update(cx, |editor, cx| editor.type_text(None, "new\n", cx)));
    cx.read(|cx| assert_eq!(editor.read(cx).folds(), [5..8]));
    assert_eq!(rows(&editor, cx), 9);
}

fn regions(editor: &gpui::Entity<Editor>, cx: &mut TestAppContext) -> Vec<std::ops::Range<usize>> {
    cx.read(|cx| editor.read(cx).fold_regions(cx).to_vec())
}

fn indented_editor(
    text: &str,
    cx: &mut TestAppContext,
) -> (gpui::Entity<BufferModel>, gpui::Entity<Editor>) {
    let model = cx.new(|_| BufferModel::from_text(text));
    let editor = cx.new(|cx| Editor::new(model.clone(), crate::app::EDITOR_KEY_CONTEXT, cx));
    (model, editor)
}

const INDENTED: &str = "a\n  b\n    c\n\n    d\n  e\nf\ng\n  h";

#[gpui::test]
fn text_without_a_folds_query_folds_by_indentation(cx: &mut TestAppContext) {
    let (_, editor) = indented_editor(INDENTED, cx);
    // `a` holds through `e`, `b` through `d` without its trailing blank line,
    // and `g` holds `h`.
    assert_eq!(regions(&editor, cx), [0..6, 1..5, 7..9]);
}

#[gpui::test]
fn fold_commands_collapse_the_innermost_region_at_the_caret(cx: &mut TestAppContext) {
    let (model, editor) = indented_editor(INDENTED, cx);
    let at = |line, cx: &mut TestAppContext| {
        let offset = cx.read(|cx| model.read(cx).line_range(line).start);
        place(&editor, offset, cx);
    };
    // Inside `b`'s region, away from a first line, the innermost encloses.
    at(2, cx);
    command(&editor, "editor.fold", cx);
    cx.read(|cx| assert_eq!(editor.read(cx).folds(), [1..5]));
    // The caret moved to the fold's first line, so folding again takes the
    // region around it.
    assert_eq!(caret(&editor, cx).0, 1);
    command(&editor, "editor.fold", cx);
    cx.read(|cx| assert_eq!(editor.read(cx).folds(), [0..6, 1..5]));
    // Unfold expands the outermost fold on the caret's line.
    command(&editor, "editor.unfold", cx);
    cx.read(|cx| assert_eq!(editor.read(cx).folds(), [1..5]));
    at(1, cx);
    command(&editor, "editor.toggle-fold", cx);
    cx.read(|cx| assert!(editor.read(cx).folds().is_empty()));
    command(&editor, "editor.toggle-fold", cx);
    cx.read(|cx| assert_eq!(editor.read(cx).folds(), [1..5]));
    command(&editor, "editor.unfold-all", cx);
    cx.read(|cx| assert!(editor.read(cx).folds().is_empty()));
    command(&editor, "editor.fold-all", cx);
    cx.read(|cx| assert_eq!(editor.read(cx).folds(), [0..6, 1..5, 7..9]));
    assert_eq!(rows(&editor, cx), 3);
}

fn rust_regions_editor(cx: &mut TestAppContext, text: &str) -> gpui::Entity<Editor> {
    use crate::app::languages::editing::EditingDefinition;
    use crate::app::languages::{self, LanguageDefinition, LanguageOwner, LanguageQueries};
    use crate::host::protocol::{ExtensionId, ExtensionLifecycleId, LanguageInfo};
    cx.update(|cx| {
        cx.set_global(ApplicationSettings::default());
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
                indents: String::new(),
                folds: "(block) @fold\n(declaration_list) @fold".into(),
            },
            editing: EditingDefinition::default(),
        };
        let owner = LanguageOwner {
            extension: ExtensionId::new(1),
            lifecycle: ExtensionLifecycleId::new(1),
        };
        languages::register(owner, rust.compile().unwrap(), cx);
    });
    let model = cx.new(|_| BufferModel::from_text(text));
    cx.update(|cx| languages::set_language(&model, "rust".into(), cx));
    cx.new(|cx| Editor::new(model.clone(), crate::app::EDITOR_KEY_CONTEXT, cx))
}

#[gpui::test]
fn a_folds_query_gives_regions_that_leave_the_closing_line_visible(cx: &mut TestAppContext) {
    let source = "impl T {\n    fn f() {\n        a;\n        b;\n    }\n\n    fn g() { c }\n}\n";
    let editor = rust_regions_editor(cx, source);
    // The impl's body ends at its closing brace, which stays visible, and so
    // does `f`'s; `g` fits one line.
    assert_eq!(regions(&editor, cx), [0..7, 1..4]);
}
