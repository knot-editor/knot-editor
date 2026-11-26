//! `@knot/vcs` and its backends, run in the launched product.

mod compare;
mod contracts;
mod decorations;
mod diff;
mod git;
mod gutter;
mod r#ref;
mod status;

/// The original and modified texts of the diff's composite tab, if one is open.
pub(crate) fn texts(
    window: &knot::test_support::TestWindow,
    cx: &mut gpui::TestAppContext,
) -> Vec<(String, String)> {
    if window.composite_titles(cx).is_empty() {
        return Vec::new();
    }
    let [original, modified] = <[_; 2]>::try_from(window.composite_editors(cx))
        .ok()
        .unwrap();
    vec![(original.text, modified.text)]
}
