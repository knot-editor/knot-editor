//! Runs the bracket cases each bundled language package keeps in
//! `test/brackets.jsonc` through an editor view on the registered language.
//!
//! Files group cases by language as the indentation cases do, with the same
//! `‸` markers. A `type` case types `text` one character at a time, `⌫`
//! pressing Backspace, and expects `after` with its caret or selection. A
//! `jump` case runs Jump to Matching Bracket and expects the caret. A `match`
//! case expects `after` to be the text with `«` and `»` around the brackets
//! the view highlights at the caret, or without markers when none match.
use serde::Deserialize;

use super::harness::start_bundled_packages;
use super::language_indents::{Action, IndentCase, TypeCase, case_files, read_cases, run, unmark};
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BracketCases {
    languages: Vec<String>,
    #[serde(default, rename = "type")]
    typing: Vec<TypeCase>,
    #[serde(default)]
    jump: Vec<IndentCase>,
    #[serde(default, rename = "match")]
    matching: Vec<IndentCase>,
}

/// `text` with `«` and `»` around each of `brackets`.
fn mark_brackets(text: &str, brackets: Option<[std::ops::Range<usize>; 2]>) -> String {
    let mut marked = text.to_owned();
    let mut ranges: Vec<_> = brackets.into_iter().flatten().collect();
    ranges.sort_by_key(|range| std::cmp::Reverse(range.start));
    for range in ranges {
        marked.insert(range.end, '»');
        marked.insert(range.start, '«');
    }
    marked
}

#[gpui::test]
fn bundled_bracket_cases_run_on_their_registered_languages(cx: &mut TestAppContext) {
    start_bundled_packages(cx);

    let mut tested = Vec::new();
    let mut failures = Vec::new();
    let mut check = |id: &str, name: &str, expected: String, actual: String| {
        if actual != expected {
            failures.push(format!(
                "{id}: {name}\n  expected:\n{expected}\n  actual:\n{actual}"
            ));
        }
    };
    for cases in case_files("brackets.jsonc")
        .iter()
        .flat_map(|path| read_cases::<BracketCases>(path))
    {
        for id in &cases.languages {
            tested.push(id.clone());
            for case in &cases.typing {
                let (before, selection) = unmark(&case.before);
                assert!(selection.is_some(), "{id}: {} marks no caret", case.name);
                let actual = run(cx, id, &before, selection, Action::Type(&case.text)).marked();
                check(id, &case.name, case.after.join("\n"), actual);
            }
            for case in &cases.jump {
                let (before, selection) = unmark(&case.before);
                assert!(selection.is_some(), "{id}: {} marks no caret", case.name);
                let actual = run(
                    cx,
                    id,
                    &before,
                    selection,
                    Action::Command("editor.jump-to-matching-bracket"),
                )
                .marked();
                check(id, &case.name, case.after.join("\n"), actual);
            }
            for case in &cases.matching {
                let (before, selection) = unmark(&case.before);
                assert!(selection.is_some(), "{id}: {} marks no caret", case.name);
                let outcome = run(cx, id, &before, selection, Action::Inspect);
                let actual = mark_brackets(&outcome.text, outcome.matched);
                check(id, &case.name, case.after.join("\n"), actual);
            }
        }
    }

    // A language that registers bracket pairs must have cases.
    let untested: Vec<String> = cx.read(|cx| {
        crate::app::languages::list(cx)
            .into_iter()
            .map(|language| language.id)
            .filter(|id| {
                crate::app::languages::get(id, cx).is_some_and(|language| {
                    language
                        .editing
                        .brackets
                        .as_ref()
                        .is_some_and(|brackets| !brackets.is_empty())
                })
            })
            .filter(|id| !tested.contains(id))
            .collect()
    });
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    assert!(untested.is_empty(), "no bracket cases for {untested:?}");
}
