//! Toggle line and block comment with the language at the selection start.

use gpui::{AppContext, Entity, TestAppContext};

use super::{BufferModel, Editor};
use crate::app::languages::{
    self, LanguageDefinition, LanguageOwner, LanguageQueries,
    editing::{CommentTokens, EditingDefinition},
};
use crate::host::protocol::{ExtensionId, ExtensionLifecycleId, LanguageInfo};

const LINE: &str = "editor.toggle-line-comment";
const BLOCK: &str = "editor.toggle-block-comment";

type Position = (usize, usize);

fn register(id: &str, injections: &str, comments: CommentTokens, cx: &mut gpui::App) {
    let definition = LanguageDefinition {
        info: LanguageInfo {
            id: id.into(),
            name: id.into(),
            grammar: id.into(),
            file_extensions: Vec::new(),
            file_names: Vec::new(),
            interpreters: Vec::new(),
        },
        queries: LanguageQueries {
            highlights: String::new(),
            injections: injections.into(),
            indents: String::new(),
        },
        editing: EditingDefinition {
            comments: Some(comments),
            ..Default::default()
        },
    };
    let owner = LanguageOwner {
        extension: ExtensionId::new(1),
        lifecycle: ExtensionLifecycleId::new(1),
    };
    languages::register(owner, definition.compile().unwrap(), cx);
}

/// An editor over `text` in `language`, with Rust and Markdown registered.
fn editor<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    language: Option<&str>,
) -> (Entity<Editor>, &'a mut TestAppContext) {
    cx.update(|cx| {
        register(
            "rust",
            "",
            CommentTokens {
                line: vec!["//".into()],
                block: Some(("/*".into(), "*/".into())),
            },
            cx,
        );
        register(
            "markdown",
            "(fenced_code_block (info_string (language) @injection.language) \
             (code_fence_content) @injection.content)",
            CommentTokens {
                line: Vec::new(),
                block: Some(("<!--".into(), "-->".into())),
            },
            cx,
        );
    });
    let model = cx.new(|_| BufferModel::from_text(text));
    if let Some(language) = language {
        cx.update(|cx| languages::set_language(&model, language.into(), cx));
    }
    let editor = cx.new(|cx| Editor::new(model, crate::app::EDITOR_KEY_CONTEXT, cx));
    (editor, cx)
}

fn select(view: &mut Editor, anchor: Position, caret: Position, cx: &mut gpui::Context<Editor>) {
    (view.anchor_line, view.anchor_col) = anchor;
    (view.cursor_line, view.cursor_col) = caret;
    view.has_selection = anchor != caret;
    view.sync_view_position(cx);
}

fn selection(view: &Editor) -> (Position, Position) {
    let caret = (view.cursor_line, view.cursor_col);
    let anchor = if view.has_selection {
        (view.anchor_line, view.anchor_col)
    } else {
        caret
    };
    (anchor, caret)
}

#[gpui::test]
fn line_comments_toggle_on_the_selected_lines_as_one_step(cx: &mut TestAppContext) {
    let text = "fn a() {\n    b();\n}\n";
    let (editor, cx) = editor(cx, text, Some("rust"));
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            select(view, (0, 0), (2, 0), cx);
            view.execute_editing_command(LINE, cx);
            assert_eq!(view.text(cx), "// fn a() {\n//     b();\n}\n");
            assert_eq!(selection(view), ((0, 0), (2, 0)));

            view.execute_editing_command(LINE, cx);
            assert_eq!(view.text(cx), text);
            view.execute_editing_command("editor.undo", cx);
            assert_eq!(view.text(cx), "// fn a() {\n//     b();\n}\n");
            view.execute_editing_command("editor.undo", cx);
            assert_eq!(view.text(cx), text);
            assert_eq!(selection(view), ((0, 0), (2, 0)));

            // A caret comments its own line and stays on the same text.
            select(view, (1, 6), (1, 6), cx);
            view.execute_editing_command(LINE, cx);
            assert_eq!(view.text(cx), "fn a() {\n    // b();\n}\n");
            assert_eq!(selection(view), ((1, 9), (1, 9)));
        })
    });
}

#[gpui::test]
fn block_comments_wrap_unwrap_and_insert_an_empty_pair(cx: &mut TestAppContext) {
    let (editor, cx) = editor(cx, "let a = b + c;", Some("rust"));
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            select(view, (0, 13), (0, 8), cx);
            view.execute_editing_command(BLOCK, cx);
            assert_eq!(view.text(cx), "let a = /* b + c */;");
            assert_eq!(selection(view), ((0, 16), (0, 11)));

            // The selection wrapping leaves unwraps again.
            view.execute_editing_command(BLOCK, cx);
            assert_eq!(view.text(cx), "let a = b + c;");
            assert_eq!(selection(view), ((0, 13), (0, 8)));

            // So does a selection of the whole comment.
            view.execute_editing_command(BLOCK, cx);
            select(view, (0, 8), (0, 19), cx);
            view.execute_editing_command(BLOCK, cx);
            assert_eq!(view.text(cx), "let a = b + c;");
            assert_eq!(selection(view), ((0, 8), (0, 13)));
            view.execute_editing_command("editor.undo", cx);
            assert_eq!(view.text(cx), "let a = /* b + c */;");
            assert_eq!(selection(view), ((0, 8), (0, 19)));

            select(view, (0, 3), (0, 3), cx);
            view.execute_editing_command(BLOCK, cx);
            assert_eq!(view.text(cx), "let/*  */ a = /* b + c */;");
            assert_eq!(selection(view), ((0, 6), (0, 6)));
            view.execute_editing_command("editor.undo", cx);
            assert_eq!(selection(view), ((0, 3), (0, 3)));
        })
    });
}

#[gpui::test]
fn a_code_fence_comments_in_its_own_language(cx: &mut TestAppContext) {
    let text = "Prose\n\n```rust\nlet a = 1;\n```\n";
    let (editor, cx) = editor(cx, text, Some("markdown"));
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            select(view, (3, 0), (3, 0), cx);
            view.execute_editing_command(LINE, cx);
            assert_eq!(view.text(cx), "Prose\n\n```rust\n// let a = 1;\n```\n");
            select(view, (3, 3), (3, 13), cx);
            view.execute_editing_command(BLOCK, cx);
            assert_eq!(
                view.text(cx),
                "Prose\n\n```rust\n// /* let a = 1; */\n```\n"
            );

            select(view, (0, 0), (0, 0), cx);
            view.execute_editing_command(LINE, cx);
            assert_eq!(
                view.text(cx),
                "<!-- Prose -->\n\n```rust\n// /* let a = 1; */\n```\n"
            );
        })
    });
}

#[gpui::test]
fn a_language_without_comment_tokens_changes_nothing(cx: &mut TestAppContext) {
    let (editor, cx) = editor(cx, "plain text", None);
    cx.update(|cx| {
        editor.update(cx, |view, cx| {
            select(view, (0, 0), (0, 5), cx);
            assert!(view.execute_editing_command(LINE, cx));
            assert!(view.execute_editing_command(BLOCK, cx));
            assert_eq!(view.text(cx), "plain text");
            assert_eq!(selection(view), ((0, 0), (0, 5)));
        })
    });
}
