//! Runs the indentation cases each bundled language package keeps in
//! `test/cases/indents.jsonc` through an editor on the registered language,
//! in the [case](super::cases) format.
//!
//! A group lists the `languages` its cases run on. A `reindent` case
//! reindents the marked lines, or every line without a marker, and expects
//! `after`. An `enter` case presses Enter at the marked selection, and a
//! `type` case types `text` there one character at a time, each expecting
//! `after` with the caret where its marker is. Cases run in an editor
//! without a window.
use serde::Deserialize;

use super::cases::{Failures, read_groups, topic_files, unmark};
use super::harness::{Action, run, start_bundled_packages};
use super::*;

/// A case that expects `after` from `before`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IndentCase {
    pub(super) before: Vec<String>,
    pub(super) after: Vec<String>,
}

/// A case that types `text` at `before` and expects `after`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TypeCase {
    pub(super) before: Vec<String>,
    pub(super) text: String,
    pub(super) after: Vec<String>,
}

#[gpui::test]
fn bundled_indent_cases_indent_on_their_registered_languages(cx: &mut TestAppContext) {
    start_bundled_packages(cx);

    let mut tested = Vec::new();
    let mut failures = Failures::default();
    for path in topic_files("indents") {
        for group in read_groups(&path, &["languages", "reindent", "enter", "type"]) {
            for id in group.field::<Vec<String>>("languages") {
                tested.push(id.clone());
                for case in group.cases::<IndentCase>("reindent") {
                    let (before, selection) = unmark(&case.value.before);
                    let actual = run(
                        cx,
                        &id,
                        &before,
                        selection,
                        Action::Command("editor.reindent-selected-lines"),
                    )
                    .text;
                    failures.check(&case, &id, &case.value.after.join("\n"), &actual);
                }
                for case in group.cases::<TypeCase>("type") {
                    let (before, selection) = unmark(&case.value.before);
                    assert!(selection.is_some(), "{}: marks no caret", case.at);
                    let actual =
                        run(cx, &id, &before, selection, Action::Type(&case.value.text)).marked();
                    failures.check(&case, &id, &case.value.after.join("\n"), &actual);
                }
                for case in group.cases::<IndentCase>("enter") {
                    let (before, selection) = unmark(&case.value.before);
                    assert!(selection.is_some(), "{}: marks no caret", case.at);
                    let actual = run(
                        cx,
                        &id,
                        &before,
                        selection,
                        Action::Command("editor.insert-newline"),
                    )
                    .marked();
                    failures.check(&case, &id, &case.value.after.join("\n"), &actual);
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
    failures.assert_none();
    assert!(untested.is_empty(), "no indent cases for {untested:?}");
}
