//! One editing action run in a bare editor, without a window.

use std::ops::Range;

use gpui::{AppContext, TestAppContext};

use crate::app::editor::{Editor, EditorViewState};
use crate::app::model::{Buffer, BufferModel};
use crate::test_support::cases::CARET;

/// In typed text, presses Backspace.
pub const BACKSPACE: char = '⌫';

/// What a run does at its selection.
pub enum Action<'a> {
    /// Run an editing command by name.
    Command(&'a str),
    /// Type the text one character at a time, pressing Backspace for `⌫`.
    Type(&'a str),
    /// Leave the view as it is.
    Inspect,
}

/// What a run left: the text, the selection, and the brackets the view
/// highlights.
pub struct Outcome {
    pub text: String,
    pub anchor: usize,
    pub caret: usize,
    pub matched: Option<[Range<usize>; 2]>,
    /// The fold regions as line ranges, first line to one past the last.
    pub regions: Vec<Range<usize>>,
    /// The first lines of the fold regions, asked for three lines at a time
    /// the way the gutter asks for its visible lines.
    pub fold_starts: Vec<usize>,
}

impl Outcome {
    /// The text with `‸` at the caret, or at both ends of the selection.
    pub fn marked(&self) -> String {
        let mut marked = self.text.clone();
        marked.insert(self.anchor.max(self.caret), CARET);
        if self.anchor != self.caret {
            marked.insert(self.anchor.min(self.caret), CARET);
        }
        marked
    }
}

/// Run `action` on `text` in `language` with the given selection, or with
/// everything selected, in an editor without a window.
pub fn run(
    cx: &mut TestAppContext,
    language: &str,
    text: &str,
    selection: Option<(usize, usize)>,
    action: Action,
) -> Outcome {
    let model = cx.new(|_| {
        let mut model = BufferModel::from_text(text);
        model.set_document(uuid::Uuid::new_v4());
        Buffer::new(model)
    });
    cx.update(|cx| crate::app::documents::detect_indentation(&model, cx));
    cx.update(|cx| crate::app::languages::set_language(&model, language.into(), cx));
    // The first parse that overruns the foreground budget finishes in the
    // background; the action needs its tree.
    cx.run_until_parked();
    let editor = cx.new(|cx| Editor::new(model.clone(), crate::app::EDITOR_KEY_CONTEXT, cx));
    let (anchor, caret) = selection.unwrap_or((0, text.len()));
    editor.update(cx, |editor, cx| {
        editor.restore_view_state(
            EditorViewState {
                anchor,
                caret,
                top: 0,
                scroll_x: 0.,
                folds: Vec::new(),
            },
            cx,
        );
        match action {
            Action::Command(command) => {
                editor.execute_editing_command(command, cx);
            }
            Action::Type(text) => {
                for character in text.chars() {
                    if character == BACKSPACE {
                        editor.execute_editing_command("editor.delete-backward", cx);
                    } else {
                        editor.type_text(None, &character.to_string(), cx);
                    }
                }
            }
            Action::Inspect => {}
        }
    });
    cx.run_until_parked();
    let (state, matched) = cx.read(|cx| {
        let editor = editor.read(cx);
        (editor.view_state(cx), editor.matched_brackets_for_test(cx))
    });
    Outcome {
        text: model.read_with(cx, |model, _| model.text()),
        anchor: state.anchor,
        caret: state.caret,
        matched,
        regions: model.read_with(cx, |model, _| model.fold_regions().to_vec()),
        fold_starts: model.read_with(cx, |model, _| {
            (0..model.line_count())
                .step_by(3)
                .flat_map(|first| model.fold_starts(first..first + 3))
                .collect()
        }),
    }
}
