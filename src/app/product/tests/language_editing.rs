//! Runs the comment toggling cases each bundled language package keeps in
//! `test/cases/comments.jsonc` through the native line and block toggles, on the
//! configuration the package registers, in the [case](super::cases) format.
use serde::Deserialize;

use super::cases::{Failures, read_groups, topic_files};
use super::harness::start_bundled_packages;
use super::*;
use crate::app::languages::editing::EditingConfiguration;

/// Toggling comments on `before` gives `after`.
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

/// Toggle comments on `lines` and apply the edits.
fn toggle(configuration: &EditingConfiguration, lines: &[String]) -> Vec<String> {
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut result = lines.to_vec();
    for edit in configuration.toggle_comments(&borrowed).into_iter().rev() {
        result[edit.line].replace_range(edit.range, &edit.text);
    }
    result
}

/// Toggle a block comment around `text` and apply the edits.
fn toggle_block(configuration: &EditingConfiguration, text: &str) -> String {
    let mut result = text.to_owned();
    for (range, text) in configuration.toggle_block_comment(text).into_iter().rev() {
        result.replace_range(range, &text);
    }
    result
}

#[gpui::test]
fn bundled_comment_cases_toggle_on_their_registered_languages(cx: &mut TestAppContext) {
    start_bundled_packages(cx);

    let mut tested = Vec::new();
    let mut failures = Failures::default();
    for path in topic_files("comments") {
        for group in read_groups(&path, &["languages", "cases", "block"]) {
            let line_cases = group.cases::<CommentCase>("cases");
            let block_cases = group.cases::<BlockCase>("block");
            for id in group.field::<Vec<String>>("languages") {
                let configuration =
                    cx.read(|cx| crate::app::languages::editing_configuration(&id, cx));
                let Some(configuration) = configuration else {
                    panic!("{}: {id} is not registered", path.display());
                };
                for case in &line_cases {
                    let actual = toggle(&configuration, &case.value.before).join("\n");
                    failures.check(case, &id, &case.value.after.join("\n"), &actual);
                }
                for case in &block_cases {
                    let actual = toggle_block(&configuration, &case.value.before);
                    failures.check(case, &id, &case.value.after, &actual);
                }
                if configuration.block_comment.is_some()
                    && group.cases::<BlockCase>("block").is_empty()
                    && std::env::var("KNOT_CASES").is_err()
                {
                    panic!("{}: {id} has no block comment cases", path.display());
                }
                tested.push(id);
            }
        }
    }

    // A language that declares comment tokens must have cases.
    let untested: Vec<String> = cx.read(|cx| {
        crate::app::languages::list(cx)
            .into_iter()
            .map(|language| language.id)
            .filter(|id| {
                crate::app::languages::editing_configuration(id, cx).is_some_and(|editing| {
                    !editing.line_comments.is_empty() || editing.block_comment.is_some()
                })
            })
            .filter(|id| !tested.contains(id))
            .collect()
    });
    failures.assert_none();
    assert!(untested.is_empty(), "no comment cases for {untested:?}");
}
