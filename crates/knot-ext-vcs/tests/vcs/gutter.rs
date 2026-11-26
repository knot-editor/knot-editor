//! The change gutter in the launched product: `@knot/vcs` over a test
//! backend whose baseline is fixed text.
use gpui::TestAppContext;
use knot::test_support::{TestProduct, wait_until};

/// A backend `@knot/test-fixed` covering every root, whose baseline for any
/// file is `a`, `b`, `c`, `d` on lines of their own.
const BACKEND_MANIFEST: &str = r#"{ "name": "@knot/test-fixed", "apiVersion": 1, "version": "0.1.0", "main": "dist/main.js", "requires": ["@knot/vcs"] }"#;
const BACKEND: &str = r#"
import { backendContract, vcs } from "@knot/vcs";
await backendContract("fixed").serve({
  repositories: (root) => [root],
  repositoryOf: () => null,
  baseline: () => "a\nb\nc\nd\n",
  status: () => [],
  ref: () => null,
});
await (await vcs.client()).register("fixed");
"#;

fn packages() -> &'static [knot_bundled::BundledSource] {
    let find = |name: &str| {
        *knot_ext_vcs::PACKAGES
            .iter()
            .find(|package| package.manifest.contains(&format!("\"name\": \"{name}\"")))
            .unwrap()
    };
    Box::leak(
        vec![
            find("@knot/diff"),
            find("@knot/vcs"),
            knot_bundled::BundledSource {
                manifest: BACKEND_MANIFEST,
                sources: &[("dist/main.js", BACKEND)],
            },
        ]
        .into_boxed_slice(),
    )
}

fn uri(path: &std::path::Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

/// A window rooted at a temporary directory showing `text` as `file.txt`.
fn show(
    text: &str,
    cx: &mut TestAppContext,
) -> (
    TestProduct,
    knot::test_support::TestWindow,
    tempfile::TempDir,
) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("file.txt"), text).unwrap();
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    window.set_roots(&[&uri(directory.path())], cx);
    window.open_path(&directory.path().join("file.txt"), cx);
    (product, window, directory)
}

fn bars(
    window: &knot::test_support::TestWindow,
    cx: &mut TestAppContext,
) -> Vec<(usize, &'static str, String)> {
    window.gutter_bars(0, cx)
}

#[gpui::test]
fn modified_added_and_removed_lines_are_marked(cx: &mut TestAppContext) {
    let (_product, window, _directory) = show("a\nB\nc\nd\ne\n", cx);
    wait_until(cx, |cx| !bars(&window, cx).is_empty());
    assert_eq!(
        bars(&window, cx),
        [
            (1, "bar", "diff.modified".to_owned()),
            (4, "bar", "diff.added".to_owned()),
        ]
    );
}

#[gpui::test]
fn a_removed_line_is_a_wedge_above_the_line_that_followed_it(cx: &mut TestAppContext) {
    let (_product, window, _directory) = show("a\nc\nd\n", cx);
    wait_until(cx, |cx| !bars(&window, cx).is_empty());
    assert_eq!(
        bars(&window, cx),
        [(1, "deletion", "diff.removed".to_owned())]
    );
}

#[gpui::test]
fn a_file_equal_to_its_baseline_shows_nothing_until_it_is_edited(cx: &mut TestAppContext) {
    let (_product, window, _directory) = show("a\nb\nc\nd\n", cx);
    // Let the package see the document before checking that it painted nothing.
    wait_until(cx, |cx| window.document_texts(cx).len() == 1);
    assert!(bars(&window, cx).is_empty());

    window.append("e\n", cx);
    wait_until(cx, |cx| !bars(&window, cx).is_empty());
    assert_eq!(bars(&window, cx), [(4, "bar", "diff.added".to_owned())]);
}

/// The line holding the caret of the first editor.
fn caret_line(window: &knot::test_support::TestWindow, cx: &mut TestAppContext) -> usize {
    let editor = window.editors(cx).remove(0);
    editor.text[..editor.caret].matches('\n').count()
}

#[gpui::test]
fn alt_f5_moves_between_changes_and_wraps(cx: &mut TestAppContext) {
    // Against a, b, c, d: line 0 modified, c removed before line 2, line 3 added.
    let (_product, window, _directory) = show("A\nb\nd\ne\n", cx);
    wait_until(cx, |cx| !bars(&window, cx).is_empty());
    window.focus_document(cx);
    window.set_view(0, 0, 0, cx);

    // A key runs the extension's command, which finishes after the key.
    for (key, expected) in [
        ("alt-f5", 2),
        ("alt-f5", 3),
        ("alt-f5", 0),
        ("alt-shift-f5", 3),
        ("alt-shift-f5", 2),
        ("alt-shift-f5", 0),
    ] {
        window.press(key, cx);
        wait_until(cx, |cx| caret_line(&window, cx) == expected);
    }
}

#[gpui::test]
fn navigation_does_nothing_without_changes(cx: &mut TestAppContext) {
    let (_product, window, _directory) = show("a\nb\nc\nd\n", cx);
    window.focus_document(cx);
    window.set_view(2, 2, 0, cx);
    window.press("alt-f5", cx);
    assert_eq!(window.editors(cx)[0].caret, 2);
}

#[gpui::test]
fn show_changes_opens_the_baseline_against_the_file_and_follows_edits(cx: &mut TestAppContext) {
    let (_product, window, _directory) = show("a\nB\nc\nd\n", cx);
    wait_until(cx, |cx| !bars(&window, cx).is_empty());
    window.focus_document(cx);

    assert_eq!(
        window.run_command("vcs.showChanges", cx),
        knot::test_support::CommandOutcome::Completed
    );
    wait_until(cx, |cx| !super::texts(&window, cx).is_empty());
    assert_eq!(
        super::texts(&window, cx),
        [("a\nb\nc\nd\n".to_owned(), "a\nB\nc\nd\n".to_owned())]
    );
}
