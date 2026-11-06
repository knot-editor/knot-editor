//! Runs the indentation cases each bundled language package keeps in
//! `test/indents.jsonc` through an editor view on the registered language.
//!
//! A file holds one group of cases with the languages they run on, or a list
//! of such groups. Lines join with `\n`, and `‸` marks the caret: one marker places it, two
//! select from the first to the second. A `reindent` case reindents the
//! marked lines, or every line without a marker, and expects `after`. An
//! `enter` case presses Enter at the marked selection, and a `type` case
//! types `text` there one character at a time, each expecting `after` with
//! the caret where its marker is. The file reading, markers, and editor
//! runs are shared with the bracket cases.
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::{Deserialize, de::DeserializeOwned};

use super::language_editing::start_bundled_packages;
use super::*;
use gpui::EntityInputHandler;

use crate::app::editor::{EditorView, EditorViewState};

pub(super) const CARET: char = '‸';
/// In typed text, presses Backspace.
pub(super) const BACKSPACE: char = '⌫';

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndentCases {
    languages: Vec<String>,
    #[serde(default)]
    reindent: Vec<IndentCase>,
    #[serde(default)]
    enter: Vec<IndentCase>,
    #[serde(default, rename = "type")]
    typing: Vec<TypeCase>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TypeCase {
    pub(super) name: String,
    pub(super) before: Vec<String>,
    pub(super) text: String,
    pub(super) after: Vec<String>,
}

/// What a case does at its selection.
pub(super) enum Action<'a> {
    Command(&'a str),
    /// Type the text one character at a time, pressing Backspace for `⌫`.
    Type(&'a str),
    /// Leave the view as it is.
    Inspect,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IndentCase {
    pub(super) name: String,
    pub(super) before: Vec<String>,
    pub(super) after: Vec<String>,
}

/// What a run left: the text, the selection, and the brackets the view
/// highlights.
pub(super) struct Outcome {
    pub(super) text: String,
    pub(super) anchor: usize,
    pub(super) caret: usize,
    pub(super) matched: Option<[Range<usize>; 2]>,
}

impl Outcome {
    /// The text with `‸` at the caret, or at both ends of the selection.
    pub(super) fn marked(&self) -> String {
        let mut marked = self.text.clone();
        marked.insert(self.anchor.max(self.caret), CARET);
        if self.anchor != self.caret {
            marked.insert(self.anchor.min(self.caret), CARET);
        }
        marked
    }
}

/// Every bundled package's case file named `name`.
pub(super) fn case_files(name: &str) -> Vec<PathBuf> {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/@knot");
    let mut files: Vec<PathBuf> = std::fs::read_dir(packages)
        .unwrap()
        .map(|entry| entry.unwrap().path().join("test").join(name))
        .filter(|path| path.exists())
        .collect();
    files.sort();
    files
}

/// A case file: one group of cases, or several for different languages.
#[derive(Deserialize)]
#[serde(untagged)]
enum CaseFile<T> {
    One(T),
    Groups(Vec<T>),
}

pub(super) fn read_cases<T: DeserializeOwned>(path: &Path) -> Vec<T> {
    let text = std::fs::read_to_string(path).unwrap();
    let file: CaseFile<T> = jsonc_parser::parse_to_serde_value(&text, &Default::default())
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    match file {
        CaseFile::One(cases) => vec![cases],
        CaseFile::Groups(groups) => groups,
    }
}

/// `lines` joined, without markers, and the selection they mark.
pub(super) fn unmark(lines: &[String]) -> (String, Option<(usize, usize)>) {
    let marked = lines.join("\n");
    let markers: Vec<usize> = marked
        .match_indices(CARET)
        .enumerate()
        .map(|(index, (offset, _))| offset - index * CARET.len_utf8())
        .collect();
    let text = marked.replace(CARET, "");
    let selection = match markers[..] {
        [] => None,
        [caret] => Some((caret, caret)),
        [anchor, caret] => Some((anchor, caret)),
        _ => panic!("more than two carets: {marked:?}"),
    };
    (text, selection)
}

/// Run `action` on `text` in `language` with the marked selection, or with
/// everything selected.
pub(super) fn run(
    cx: &mut TestAppContext,
    language: &str,
    text: &str,
    selection: Option<(usize, usize)>,
    action: Action,
) -> Outcome {
    let model = cx.new(|_| {
        let mut model = BufferModel::from_text(text);
        model.set_document(uuid::Uuid::new_v4());
        model
    });
    cx.update(|cx| crate::app::documents::detect_indentation(&model, cx));
    cx.update(|cx| crate::app::languages::set_language(&model, language.into(), cx));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    let (anchor, caret) = selection.unwrap_or((0, text.len()));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.editor().clone().update(cx, |view, cx| {
                view.restore_view_state(
                    EditorViewState {
                        anchor,
                        caret,
                        top: 0,
                        scroll_x: 0.,
                    },
                    cx,
                )
            });
            match action {
                Action::Command(command) => {
                    view.editor()
                        .clone()
                        .update(cx, |view, cx| view.execute_editing_command(command, cx));
                }
                Action::Type(text) => {
                    for character in text.chars() {
                        if character == BACKSPACE {
                            view.editor().clone().update(cx, |view, cx| {
                                view.execute_editing_command("editor.delete-backward", cx)
                            });
                        } else {
                            view.replace_text_in_range(None, &character.to_string(), window, cx);
                        }
                    }
                }
                Action::Inspect => {}
            }
        })
    });
    cx.run_until_parked();
    let (state, matched) = cx.read(|cx| {
        let view = editor.read(cx);
        (
            view.editor().read(cx).view_state(cx),
            view.editor().read(cx).matched_brackets_for_test(cx),
        )
    });
    Outcome {
        text: model.read_with(cx, |model, _| model.text()),
        anchor: state.anchor,
        caret: state.caret,
        matched,
    }
}

#[gpui::test]
fn bundled_indent_cases_indent_on_their_registered_languages(cx: &mut TestAppContext) {
    start_bundled_packages(cx);

    let mut tested = Vec::new();
    let mut failures = Vec::new();
    for cases in case_files("indents.jsonc")
        .iter()
        .flat_map(|path| read_cases::<IndentCases>(path))
    {
        for id in &cases.languages {
            tested.push(id.clone());
            for case in &cases.reindent {
                let (before, selection) = unmark(&case.before);
                let expected = case.after.join("\n");
                let actual = run(
                    cx,
                    id,
                    &before,
                    selection,
                    Action::Command("editor.reindent-selected-lines"),
                )
                .text;
                if actual != expected {
                    failures.push(format!(
                        "{id}: {}\n  expected:\n{expected}\n  actual:\n{actual}",
                        case.name
                    ));
                }
            }
            for case in &cases.typing {
                let (before, selection) = unmark(&case.before);
                assert!(selection.is_some(), "{id}: {} marks no caret", case.name);
                let expected = case.after.join("\n");
                let actual = run(cx, id, &before, selection, Action::Type(&case.text)).marked();
                if actual != expected {
                    failures.push(format!(
                        "{id}: {}\n  expected:\n{expected}\n  actual:\n{actual}",
                        case.name
                    ));
                }
            }
            for case in &cases.enter {
                let (before, selection) = unmark(&case.before);
                assert!(selection.is_some(), "{id}: {} marks no caret", case.name);
                let expected = case.after.join("\n");
                let actual = run(
                    cx,
                    id,
                    &before,
                    selection,
                    Action::Command("editor.insert-newline"),
                )
                .marked();
                if actual != expected {
                    failures.push(format!(
                        "{id}: {}\n  expected:\n{expected}\n  actual:\n{actual}",
                        case.name
                    ));
                }
            }
        }
    }

    // A language that registers an indent query must have cases.
    let untested: Vec<String> = cx.read(|cx| {
        crate::app::languages::list(cx)
            .into_iter()
            .map(|language| language.id)
            .filter(|id| {
                crate::app::languages::get(id, cx)
                    .is_some_and(|language| !language.queries.indents.trim().is_empty())
            })
            .filter(|id| !tested.contains(id))
            .collect()
    });
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    assert!(untested.is_empty(), "no indent cases for {untested:?}");
}
