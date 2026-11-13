//! Runs the bracket cases each bundled language package keeps in
//! `test/cases/brackets.jsonc` through an editor on the registered language, in
//! the [case](knot::test_support::cases) format.
//!
//! Groups list their `languages` as the indentation cases do. A `type` case types `text` one character at a time, `⌫`
//! pressing Backspace, and expects `after` with its caret or selection. A
//! `jump` case runs Jump to Matching Bracket and expects the caret. A `match`
//! case expects `after` to be the text with `«` and `»` around the brackets
//! the view highlights at the caret, or without markers when none match.

use super::bundled::language_packages;
use super::indents::{IndentCase, TypeCase};
use gpui::TestAppContext;
use knot::test_support::TestProduct;
use knot::test_support::cases::{Failures, read_groups, topic_files, unmark};
use knot::test_support::editing::{Action, run};

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

/// Run every case of this topic on the started `product`.
pub fn check(product: &TestProduct, cx: &mut TestAppContext) {
    let mut tested = Vec::new();
    let mut failures = Failures::default();
    for path in topic_files(&language_packages(), "brackets") {
        for group in read_groups(&path, &["languages", "type", "jump", "match"]) {
            for id in group.field::<Vec<String>>("languages") {
                tested.push(id.clone());
                for case in group.cases::<TypeCase>("type") {
                    let (before, selection) = unmark(&case.value.before);
                    assert!(selection.is_some(), "{}: marks no caret", case.at);
                    let actual =
                        run(cx, &id, &before, selection, Action::Type(&case.value.text)).marked();
                    failures.check(&case, &id, &case.value.after.join("\n"), &actual);
                }
                for case in group.cases::<IndentCase>("jump") {
                    let (before, selection) = unmark(&case.value.before);
                    assert!(selection.is_some(), "{}: marks no caret", case.at);
                    let actual = run(
                        cx,
                        &id,
                        &before,
                        selection,
                        Action::Command("editor.jump-to-matching-bracket"),
                    )
                    .marked();
                    failures.check(&case, &id, &case.value.after.join("\n"), &actual);
                }
                for case in group.cases::<IndentCase>("match") {
                    let (before, selection) = unmark(&case.value.before);
                    assert!(selection.is_some(), "{}: marks no caret", case.at);
                    let outcome = run(cx, &id, &before, selection, Action::Inspect);
                    let actual = mark_brackets(&outcome.text, outcome.matched);
                    failures.check(&case, &id, &case.value.after.join("\n"), &actual);
                }
            }
        }
    }

    // A language that registers bracket pairs must have cases.
    let untested: Vec<String> = product
        .languages(cx)
        .into_iter()
        .filter(|language| language.brackets && !tested.contains(&language.id))
        .map(|language| language.id)
        .collect();
    failures.assert_none();
    assert!(untested.is_empty(), "no bracket cases for {untested:?}");
}
