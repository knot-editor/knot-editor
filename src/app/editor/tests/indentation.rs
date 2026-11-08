//! Indentation that follows the application: detected and converted styles
//! write the document's settings, and Enter waits for a background parse.

use std::time::Duration;

use gpui::{AppContext, Entity, TestAppContext};

use super::{Buffer, BufferModel, Editor, Reveal};
use crate::app::languages::{
    self, LanguageDefinition, LanguageOwner, LanguageQueries, editing::EditingDefinition,
};
use crate::app::settings::{self, ApplicationSettings};
use knot_protocol::{ExtensionId, ExtensionLifecycleId, LanguageInfo};

const BLOCKS: &str = "(block) @indent.begin\n\"}\" @indent.branch";

fn document_editor(text: &str, cx: &mut TestAppContext) -> Entity<Editor> {
    let model = cx.new(|_| {
        let mut model = BufferModel::from_text(text);
        model.set_document(uuid::Uuid::new_v4());
        Buffer::new(model)
    });
    cx.new(|cx| Editor::new(model, crate::app::EDITOR_KEY_CONTEXT, cx))
}

/// Whether `editor` indents with tabs, as its settings now resolve.
fn indents_with_tabs(editor: &Entity<Editor>, cx: &mut TestAppContext) -> bool {
    cx.read(|cx| {
        let editor = editor.read(cx);
        let scope = editor.settings_scope(editor.model().read(cx), cx);
        !settings::editor_insert_spaces(&scope, cx)
    })
}

fn text(editor: &Entity<Editor>, cx: &mut TestAppContext) -> String {
    cx.read(|cx| editor.read(cx).text(cx))
}

fn run(editor: &Entity<Editor>, command: &str, cx: &mut TestAppContext) -> bool {
    editor.update(cx, |editor, cx| editor.execute_editing_command(command, cx))
}

#[gpui::test]
fn detected_indentation_overrides_the_setting(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let editor = document_editor("a\n\tb\nc", cx);
    let model = cx.read(|cx| editor.read(cx).model().clone());
    cx.update(|cx| crate::app::documents::detect_indentation(&model, cx));
    editor.update(cx, |editor, cx| {
        editor.set_selection(5, 5, Reveal::Nearest, cx)
    });
    run(&editor, "editor.insert-tab", cx);
    assert_eq!(text(&editor, cx), "a\n\tb\n\tc");
}

#[gpui::test]
fn converting_indentation_sets_the_documents_style(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let editor = document_editor("a\n\tb", cx);
    assert!(run(&editor, "editor.convert-indentation-to-spaces", cx));
    assert_eq!(text(&editor, cx), "a\n    b");
    assert!(!indents_with_tabs(&editor, cx));
    assert!(run(&editor, "editor.convert-indentation-to-tabs", cx));
    assert_eq!(text(&editor, cx), "a\n\tb");
    assert!(indents_with_tabs(&editor, cx));
}

#[gpui::test]
fn detecting_indentation_replaces_the_documents_own_settings(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    // The user chose the opposite of what the first text shows and chose
    // tabs for the second, which shows no style.
    for (text, spaces, expect_tabs) in [
        ("a\n\tb\n\tc\n\td\n", true, true),
        ("a\nb\nc\n", false, false),
    ] {
        let editor = document_editor(text, cx);
        let document = cx.read(|cx| editor.read(cx).model().read(cx).document().unwrap());
        cx.update(|cx| settings::set_document_insert_spaces(document, spaces, cx));
        assert_eq!(indents_with_tabs(&editor, cx), !spaces);
        assert!(run(&editor, "editor.detect-indentation", cx));
        assert_eq!(indents_with_tabs(&editor, cx), expect_tabs);
    }
}

#[gpui::test]
fn enter_indents_once_a_background_parse_covers_it(cx: &mut TestAppContext) {
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
                indents: BLOCKS.into(),
                folds: String::new(),
            },
            editing: EditingDefinition::default(),
        };
        let owner = LanguageOwner {
            extension: ExtensionId::new(1),
            lifecycle: ExtensionLifecycleId::new(1),
        };
        languages::register(owner, rust.compile().unwrap(), cx);
    });
    // Large enough that a parse never finishes inside a zero budget.
    let rest = "fn b() { let x = 1; }\n".repeat(5_000);
    let source = format!("fn a() {{\n}}\n{rest}");
    let model = cx.new(|_| {
        let mut model = BufferModel::from_text(source.clone());
        model.set_syntax_budget(Duration::ZERO);
        Buffer::new(model)
    });
    cx.update(|cx| languages::set_language(&model, "rust".into(), cx));
    let editor = cx.new(|cx| Editor::new(model, crate::app::EDITOR_KEY_CONTEXT, cx));
    cx.run_until_parked();

    editor.update(cx, |editor, cx| {
        editor.set_selection(8, 8, Reveal::Nearest, cx)
    });
    run(&editor, "editor.insert-newline", cx);
    // The break keeps the line's indentation until the tree covers it.
    assert_eq!(text(&editor, cx), format!("fn a() {{\n\n}}\n{rest}"));
    cx.run_until_parked();
    assert_eq!(text(&editor, cx), format!("fn a() {{\n    \n}}\n{rest}"));
    assert_eq!(cx.read(|cx| editor.read(cx).caret()), (1, 4));
    run(&editor, "editor.undo", cx);
    assert_eq!(text(&editor, cx), source);
    assert_eq!(cx.read(|cx| editor.read(cx).caret()), (0, 8));

    // An edit before the parse finishes drops the request.
    run(&editor, "editor.insert-newline", cx);
    editor.update(cx, |editor, cx| editor.type_text(None, "x", cx));
    cx.run_until_parked();
    assert_eq!(text(&editor, cx), format!("fn a() {{\nx\n}}\n{rest}"));
}
