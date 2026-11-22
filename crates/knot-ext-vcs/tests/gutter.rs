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
});
await (await vcs.client()).register("fixed");
"#;

fn packages() -> &'static [knot_bundled::BundledSource] {
    let vcs = *knot_ext_vcs::PACKAGES
        .iter()
        .find(|package| package.manifest.contains("\"name\": \"@knot/vcs\""))
        .unwrap();
    Box::leak(
        vec![
            vcs,
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
