//! Runs the fold cases each bundled language package keeps in
//! `test/cases/folds.jsonc` through an editor on the registered language, in
//! the [case](knot::test_support::cases) format.
//!
//! A group lists the `languages` its cases run on. A `regions` case expects
//! `after` to list the fold regions of `before`, one `first-last` pair of
//! 0-based lines, both included, per entry, ordered by first line and then
//! outermost first.
use super::bundled::{PACKAGES, language_packages};
use super::indents::IndentCase;
use gpui::TestAppContext;
use knot::test_support::TestProduct;
use knot::test_support::cases::{Failures, read_groups, topic_files, unmark};
use knot::test_support::editing::{Action, run};

#[gpui::test]
fn bundled_fold_cases_find_regions_on_their_registered_languages(cx: &mut TestAppContext) {
    let product = TestProduct::start(PACKAGES, cx);

    let mut tested = Vec::new();
    let mut failures = Failures::default();
    for path in topic_files(&language_packages(), "folds") {
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
    let untested: Vec<String> = product
        .languages(cx)
        .into_iter()
        .filter(|language| language.folds && !tested.contains(&language.id))
        .map(|language| language.id)
        .collect();
    failures.assert_none();
    assert!(untested.is_empty(), "no fold cases for {untested:?}");
}
