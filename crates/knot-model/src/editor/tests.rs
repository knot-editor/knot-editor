use std::sync::Arc;

use super::*;
use crate::syntax::{LanguageResolver, SyntaxLanguage, SyntaxState};

mod brackets;
mod comments;
mod decorations;
mod editing;
mod folding;
mod indentation;
mod lines;
mod save_hygiene;

type Position = (usize, usize);

/// An editor of the `editor` view over its buffer, as a test drives them.
struct Fixture {
    buffer: BufferModel,
    editor: EditorState,
}

impl Fixture {
    fn new(text: &str) -> Self {
        Self::over(BufferModel::from_text(text), EditorSettings::default())
    }

    fn over(mut buffer: BufferModel, settings: EditorSettings) -> Self {
        let editor = EditorState::new("editor", settings, &mut buffer);
        Self { buffer, editor }
    }

    fn text(&self) -> String {
        self.buffer.text()
    }

    /// Run the editing command `command`, as the command dispatcher would.
    fn run(&mut self, command: &str) -> bool {
        self.editor
            .execute_editing_command(command, &mut self.buffer)
    }

    fn type_text(&mut self, text: &str) {
        self.editor.type_text(None, text, &mut self.buffer);
    }

    /// Select from `anchor` to `caret` without the side effects of a
    /// selection command.
    fn place(&mut self, anchor: Position, caret: Position) {
        let editor = &mut self.editor;
        (editor.anchor_line, editor.anchor_col) = anchor;
        (editor.cursor_line, editor.cursor_col) = caret;
        editor.has_selection = anchor != caret;
        editor.sync_view_position(&mut self.buffer);
    }

    /// The selection's anchor and caret.
    fn selection(&self) -> (Position, Position) {
        (self.editor.anchor(), self.editor.caret())
    }
}

/// Run `command` over `text` with the selection from `anchor` to `caret`,
/// check the text and selection it leaves, then check that undo restores the
/// original and redo the result.
fn check(
    insert_spaces: bool,
    (text, anchor, caret): (&str, Position, Position),
    command: &str,
    (expected, expected_anchor, expected_caret): (&str, Position, Position),
) {
    let settings = EditorSettings {
        insert_spaces,
        ..EditorSettings::default()
    };
    check_in(
        Fixture::over(BufferModel::from_text(text), settings),
        (anchor, caret),
        command,
        (expected, expected_anchor, expected_caret),
    );
}

/// [`check`] on the text of `fixture`.
fn check_in(
    mut fixture: Fixture,
    (anchor, caret): (Position, Position),
    command: &str,
    (expected, expected_anchor, expected_caret): (&str, Position, Position),
) {
    let text = fixture.text();
    fixture.place(anchor, caret);
    let context = format!("{command} on {text:?} from {anchor:?} to {caret:?}");

    assert!(fixture.run(command));
    assert_eq!(fixture.text(), expected, "{context}");
    assert_eq!(
        fixture.selection(),
        (expected_anchor, expected_caret),
        "{context}"
    );

    fixture.run("editor.undo");
    assert_eq!(fixture.text(), text, "undo of {context}");
    if expected != text {
        assert_eq!(fixture.selection(), (anchor, caret), "undo of {context}");
        fixture.run("editor.redo");
        assert_eq!(fixture.text(), expected, "redo of {context}");
        assert_eq!(
            fixture.selection(),
            (expected_anchor, expected_caret),
            "redo of {context}"
        );
    }
}

/// The queries and editing data of a test language.
#[derive(Default)]
struct Language<'a> {
    highlights: &'a str,
    injections: &'a str,
    indents: &'a str,
    folds: &'a str,
    editing: crate::editing::EditingDefinition,
}

impl Language<'_> {
    fn compile(self, grammar: &str) -> Arc<SyntaxLanguage> {
        Arc::new(
            SyntaxLanguage::new(grammar, self.highlights, self.injections)
                .unwrap()
                .with_indents(self.indents)
                .unwrap()
                .with_folds(self.folds)
                .unwrap()
                .with_editing(self.editing.resolve()),
        )
    }
}

/// Give `buffer` the syntax of `language`, whose injections resolve through
/// `resolver`.
fn set_syntax(buffer: &mut BufferModel, language: Arc<SyntaxLanguage>, resolver: LanguageResolver) {
    buffer.set_syntax(Some(SyntaxState::new(language, resolver)));
}
