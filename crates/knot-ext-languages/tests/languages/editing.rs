//! Runs the comment toggling cases each bundled language package keeps in
//! `test/cases/comments.jsonc` through the editor's line and block toggles on
//! the registered language, in the [case](knot::test_support::cases) format.
use serde::Deserialize;

use super::bundled::language_packages;
use gpui::TestAppContext;
use knot::test_support::TestProduct;
use knot::test_support::cases::{Failures, read_groups, topic_files};
use knot::test_support::editing::{Action, run};

/// Toggling line comments on every line of `before` gives `after`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommentCase {
    before: Vec<String>,
    after: Vec<String>,
}

/// Toggling a block comment around the selected text `before` gives `after`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockCase {
    before: String,
    after: String,
}

/// Run every case of this topic on the started `product`.
pub fn check(product: &TestProduct, cx: &mut TestAppContext) {
    let languages = product.languages(cx);

    let mut tested = Vec::new();
    let mut failures = Failures::default();
    for path in topic_files(&language_packages(), "comments") {
        for group in read_groups(&path, &["languages", "cases", "block"]) {
            let line_cases = group.cases::<CommentCase>("cases");
            let block_cases = group.cases::<BlockCase>("block");
            for id in group.field::<Vec<String>>("languages") {
                let Some(language) = languages.iter().find(|language| language.id == id) else {
                    panic!("{}: {id} is not registered", path.display());
                };
                for case in &line_cases {
                    let before = case.value.before.join("\n");
                    let toggle = Action::Command("editor.toggle-line-comment");
                    let actual = run(cx, &id, &before, None, toggle).text;
                    failures.check(case, &id, &case.value.after.join("\n"), &actual);
                }
                for case in &block_cases {
                    let toggle = Action::Command("editor.toggle-block-comment");
                    let actual = run(cx, &id, &case.value.before, None, toggle).text;
                    failures.check(case, &id, &case.value.after, &actual);
                }
                if language.block_comments
                    && block_cases.is_empty()
                    && std::env::var("KNOT_CASES").is_err()
                {
                    panic!("{}: {id} has no block comment cases", path.display());
                }
                tested.push(id);
            }
        }
    }

    // A language that declares comment tokens must have cases.
    let untested: Vec<&str> = languages
        .iter()
        .filter(|language| {
            (language.line_comments || language.block_comments) && !tested.contains(&language.id)
        })
        .map(|language| language.id.as_str())
        .collect();
    failures.assert_none();
    assert!(untested.is_empty(), "no comment cases for {untested:?}");
}
