//! Toggle line and block comment with the language at the selection start.

use super::*;
use crate::editing::{CommentTokens, EditingDefinition};

const LINE: &str = "editor.toggle-line-comment";
const BLOCK: &str = "editor.toggle-block-comment";

fn language(injections: &str, comments: CommentTokens) -> Arc<SyntaxLanguage> {
    Language {
        injections,
        editing: EditingDefinition {
            comments: Some(comments),
            ..Default::default()
        },
        ..Default::default()
    }
    .compile(if injections.is_empty() {
        "rust"
    } else {
        "markdown"
    })
}

/// An editor over `text` in `language`, with Rust and Markdown known.
fn editor(text: &str, language_id: Option<&str>) -> Fixture {
    let rust = language(
        "",
        CommentTokens {
            line: vec!["//".into()],
            block: Some(("/*".into(), "*/".into())),
        },
    );
    let markdown = language(
        "(fenced_code_block (info_string (language) @injection.language) \
         (code_fence_content) @injection.content)",
        CommentTokens {
            line: Vec::new(),
            block: Some(("<!--".into(), "-->".into())),
        },
    );
    let mut buffer = BufferModel::from_text(text);
    let resolver_rust = rust.clone();
    let resolver: LanguageResolver =
        Arc::new(move |id| (id == "rust").then(|| resolver_rust.clone()));
    match language_id {
        Some("rust") => set_syntax(&mut buffer, rust, resolver),
        Some(_) => set_syntax(&mut buffer, markdown, resolver),
        None => {}
    }
    Fixture::over(buffer, EditorSettings::default())
}

#[test]
fn line_comments_toggle_on_the_selected_lines_as_one_step() {
    let text = "fn a() {\n    b();\n}\n";
    let mut f = editor(text, Some("rust"));
    f.place((0, 0), (2, 0));
    f.run(LINE);
    assert_eq!(f.text(), "// fn a() {\n//     b();\n}\n");
    assert_eq!(f.selection(), ((0, 0), (2, 0)));

    f.run(LINE);
    assert_eq!(f.text(), text);
    f.run("editor.undo");
    assert_eq!(f.text(), "// fn a() {\n//     b();\n}\n");
    f.run("editor.undo");
    assert_eq!(f.text(), text);
    assert_eq!(f.selection(), ((0, 0), (2, 0)));

    // A caret comments its own line and stays on the same text.
    f.place((1, 6), (1, 6));
    f.run(LINE);
    assert_eq!(f.text(), "fn a() {\n    // b();\n}\n");
    assert_eq!(f.selection(), ((1, 9), (1, 9)));
}

#[test]
fn block_comments_wrap_unwrap_and_insert_an_empty_pair() {
    let mut f = editor("let a = b + c;", Some("rust"));
    f.place((0, 13), (0, 8));
    f.run(BLOCK);
    assert_eq!(f.text(), "let a = /* b + c */;");
    assert_eq!(f.selection(), ((0, 16), (0, 11)));

    // The selection wrapping leaves unwraps again.
    f.run(BLOCK);
    assert_eq!(f.text(), "let a = b + c;");
    assert_eq!(f.selection(), ((0, 13), (0, 8)));

    // So does a selection of the whole comment.
    f.run(BLOCK);
    f.place((0, 8), (0, 19));
    f.run(BLOCK);
    assert_eq!(f.text(), "let a = b + c;");
    assert_eq!(f.selection(), ((0, 8), (0, 13)));
    f.run("editor.undo");
    assert_eq!(f.text(), "let a = /* b + c */;");
    assert_eq!(f.selection(), ((0, 8), (0, 19)));

    f.place((0, 3), (0, 3));
    f.run(BLOCK);
    assert_eq!(f.text(), "let/*  */ a = /* b + c */;");
    assert_eq!(f.selection(), ((0, 6), (0, 6)));
    f.run("editor.undo");
    assert_eq!(f.selection(), ((0, 3), (0, 3)));
}

#[test]
fn a_code_fence_comments_in_its_own_language() {
    let text = "Prose\n\n```rust\nlet a = 1;\n```\n";
    let mut f = editor(text, Some("markdown"));
    f.place((3, 0), (3, 0));
    f.run(LINE);
    assert_eq!(f.text(), "Prose\n\n```rust\n// let a = 1;\n```\n");
    f.place((3, 3), (3, 13));
    f.run(BLOCK);
    assert_eq!(f.text(), "Prose\n\n```rust\n// /* let a = 1; */\n```\n");

    f.place((0, 0), (0, 0));
    f.run(LINE);
    assert_eq!(
        f.text(),
        "<!-- Prose -->\n\n```rust\n// /* let a = 1; */\n```\n"
    );
}

#[test]
fn a_language_without_comment_tokens_changes_nothing() {
    let mut f = editor("plain text", None);
    f.place((0, 0), (0, 5));
    assert!(f.run(LINE));
    assert!(f.run(BLOCK));
    assert_eq!(f.text(), "plain text");
    assert_eq!(f.selection(), ((0, 0), (0, 5)));
}
