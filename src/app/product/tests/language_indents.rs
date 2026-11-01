//! Runs the indentation cases each bundled language package keeps in
//! `test/indents.jsonc` through an editor view on the registered language.
//!
//! Lines join with `\n`, and `‸` marks the caret: one marker places it, two
//! select from the first to the second. A `reindent` case reindents the
//! marked lines, or every line without a marker, and expects `after`. An
//! `enter` case presses Enter at the marked selection, and a `type` case
//! types `text` there one character at a time, each expecting `after` with
//! the caret where its marker is.
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::language_editing::start_bundled_packages;
use super::*;
use gpui::EntityInputHandler;

use crate::app::editor::{EditorView, EditorViewState};

const CARET: char = '‸';

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
struct TypeCase {
    name: String,
    before: Vec<String>,
    text: String,
    after: Vec<String>,
}

/// What a case does at its selection.
enum Action<'a> {
    Command(&'a str),
    Type(&'a str),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndentCase {
    name: String,
    before: Vec<String>,
    after: Vec<String>,
}

/// Every bundled package's case file.
fn case_files() -> Vec<PathBuf> {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/@knot");
    let mut files: Vec<PathBuf> = std::fs::read_dir(packages)
        .unwrap()
        .map(|entry| entry.unwrap().path().join("test/indents.jsonc"))
        .filter(|path| path.exists())
        .collect();
    files.sort();
    files
}

fn read_cases(path: &Path) -> IndentCases {
    let text = std::fs::read_to_string(path).unwrap();
    jsonc_parser::parse_to_serde_value(&text, &Default::default())
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// `lines` joined, without markers, and the selection they mark.
fn unmark(lines: &[String]) -> (String, Option<(usize, usize)>) {
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

/// `text` with `‸` at `caret`.
fn mark(text: &str, caret: usize) -> String {
    let mut marked = text.to_owned();
    marked.insert(caret, CARET);
    marked
}

/// Run `command` on `text` in `language` with the marked selection, or with
/// everything selected, and return the resulting text and caret.
fn run(
    cx: &mut TestAppContext,
    language: &str,
    text: &str,
    selection: Option<(usize, usize)>,
    action: Action,
) -> (String, usize) {
    let model = cx.new(|_| {
        let mut model = BufferModel::from_text(text);
        model.detect_indent_style();
        model
    });
    cx.update(|cx| crate::app::languages::set_language(&model, language.into(), cx));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    let (anchor, caret) = selection.unwrap_or((0, text.len()));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.restore_view_state(
                EditorViewState {
                    anchor,
                    caret,
                    top: 0,
                    scroll_x: 0.,
                },
                cx,
            );
            match action {
                Action::Command(command) => {
                    view.execute_editing_command(command, window, cx);
                }
                Action::Type(text) => {
                    for character in text.chars() {
                        view.replace_text_in_range(None, &character.to_string(), window, cx);
                    }
                }
            }
        })
    });
    cx.run_until_parked();
    let caret = cx.read(|cx| editor.read(cx).view_state(cx).caret);
    (model.read_with(cx, |model, _| model.text()), caret)
}

#[gpui::test]
fn bundled_indent_cases_indent_on_their_registered_languages(cx: &mut TestAppContext) {
    start_bundled_packages(cx);

    let mut tested = Vec::new();
    let mut failures = Vec::new();
    for path in case_files() {
        let cases = read_cases(&path);
        for id in &cases.languages {
            tested.push(id.clone());
            for case in &cases.reindent {
                let (before, selection) = unmark(&case.before);
                let expected = case.after.join("\n");
                let (actual, _) = run(
                    cx,
                    id,
                    &before,
                    selection,
                    Action::Command("editor.reindent-lines"),
                );
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
                let (text, caret) = run(cx, id, &before, selection, Action::Type(&case.text));
                let actual = mark(&text, caret);
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
                let (text, caret) = run(
                    cx,
                    id,
                    &before,
                    selection,
                    Action::Command("editor.insert-newline"),
                );
                let actual = mark(&text, caret);
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
    assert!(untested.is_empty(), "no indent cases for {untested:?}");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
