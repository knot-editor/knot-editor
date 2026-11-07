//! Runs the fold cases each bundled language package keeps in
//! `test/cases/folds.jsonc` through an editor on the registered language, in
//! the [case](super::cases) format.
//!
//! A group lists the `languages` its cases run on. A `regions` case expects
//! `after` to list the fold regions of `before`, one `first-last` pair of
//! 0-based lines, both included, per entry, ordered by first line and then
//! outermost first.
use super::cases::{Failures, read_groups, topic_files, unmark};
use super::harness::{Action, run, start_bundled_packages};
use super::language_indents::IndentCase;
use super::*;

#[gpui::test]
fn bundled_fold_cases_find_regions_on_their_registered_languages(cx: &mut TestAppContext) {
    start_bundled_packages(cx);

    let mut tested = Vec::new();
    let mut failures = Failures::default();
    for path in topic_files("folds") {
        for group in read_groups(&path, &["languages", "regions"]) {
            for id in group.field::<Vec<String>>("languages") {
                tested.push(id.clone());
                for case in group.cases::<IndentCase>("regions") {
                    let (before, _) = unmark(&case.value.before);
                    let outcome = run(cx, &id, &before, None, Action::Inspect);
                    let actual = outcome
                        .regions
                        .iter()
                        .map(|lines| format!("{}-{}", lines.start, lines.end - 1))
                        .collect::<Vec<_>>()
                        .join("\n");
                    failures.check(&case, &id, &case.value.after.join("\n"), &actual);
                }
            }
        }
    }

    // A language that registers a folds query must have cases.
    let untested: Vec<String> = cx.read(|cx| {
        crate::app::languages::list(cx)
            .into_iter()
            .map(|language| language.id)
            .filter(|id| {
                crate::app::languages::get(id, cx)
                    .is_some_and(|language| !language.queries.folds.is_empty())
            })
            .filter(|id| !tested.contains(id))
            .collect()
    });
    failures.assert_none();
    assert!(untested.is_empty(), "no fold cases for {untested:?}");
}
