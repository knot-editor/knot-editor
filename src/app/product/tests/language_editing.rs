//! Runs the comment toggling cases each bundled language package keeps in
//! `test/comments.jsonc` through the native line and block toggles, on the
//! configuration the package registers.
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::*;
use crate::app::extensions::load::LoadResult;
use crate::app::languages::editing::EditingConfiguration;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommentCases {
    languages: Vec<String>,
    cases: Vec<CommentCase>,
    #[serde(default)]
    block: Vec<BlockCase>,
}

/// Toggling comments on `before` gives `after`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommentCase {
    name: String,
    before: Vec<String>,
    after: Vec<String>,
}

/// Toggling a block comment around the selected text `before` gives `after`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockCase {
    name: String,
    before: String,
    after: String,
}

/// Every bundled package's case file.
fn case_files() -> Vec<PathBuf> {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/@knot");
    let mut files: Vec<PathBuf> = std::fs::read_dir(packages)
        .unwrap()
        .map(|entry| entry.unwrap().path().join("test/comments.jsonc"))
        .filter(|path| path.exists())
        .collect();
    files.sort();
    files
}

fn read_cases(path: &Path) -> CommentCases {
    let text = std::fs::read_to_string(path).unwrap();
    jsonc_parser::parse_to_serde_value(&text, &Default::default())
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Start every bundled package with an empty personal config and no
/// installed packages, and require that each one loaded.
pub(super) fn start_bundled_packages(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let extensions = tempfile::tempdir().unwrap();
    install_globals(cx);
    install_launch_gate(cx);
    let host = cx.update(crate::app::extensions::host::install);
    host.update(cx, |host, cx| {
        host.start_product_startup(
            Some(config.path().to_path_buf()),
            Ok(extensions.path().to_path_buf()),
            cx,
        )
    });
    assert!(wait_for_product_startup(cx));
    let report = cx
        .read(crate::app::extensions::host::startup_report)
        .unwrap();
    let failures: Vec<_> = report
        .report
        .entries
        .iter()
        .filter(|entry| entry.result != LoadResult::Loaded)
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
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
    let mut failures = Vec::new();
    for path in case_files() {
        let cases = read_cases(&path);
        for id in &cases.languages {
            let configuration = cx.read(|cx| crate::app::languages::editing_configuration(id, cx));
            let Some(configuration) = configuration else {
                failures.push(format!("{}: {id} is not registered", path.display()));
                continue;
            };
            for case in &cases.cases {
                let actual = toggle(&configuration, &case.before);
                if actual != case.after {
                    failures.push(format!(
                        "{id}: {}\n  before: {:?}\n  expected: {:?}\n  actual: {:?}",
                        case.name, case.before, case.after, actual
                    ));
                }
            }
            for case in &cases.block {
                let actual = toggle_block(&configuration, &case.before);
                if actual != case.after {
                    failures.push(format!(
                        "{id}: {}\n  before: {:?}\n  expected: {:?}\n  actual: {:?}",
                        case.name, case.before, case.after, actual
                    ));
                }
            }
            if configuration.block_comment.is_some() && cases.block.is_empty() {
                failures.push(format!("{id}: no block comment cases"));
            }
            tested.push(id.clone());
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
    assert!(untested.is_empty(), "no comment cases for {untested:?}");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
