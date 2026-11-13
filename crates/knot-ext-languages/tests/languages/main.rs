//! The bundled language packages, run in the real editor.

mod brackets;
mod editing;
mod folds;
mod highlights;
mod indents;

/// One product runs every topic: starting the language packages compiles
/// every query, which costs more than all the cases together.
#[gpui::test]
fn bundled_language_cases_run_on_their_registered_languages(cx: &mut gpui::TestAppContext) {
    let product = knot::test_support::TestProduct::start(bundled::PACKAGES, cx);
    brackets::check(&product, cx);
    editing::check(&product, cx);
    folds::check(&product, cx);
    highlights::check(&product, cx);
    indents::check(&product, cx);
}

mod bundled {
    use std::path::{Path, PathBuf};

    pub const PACKAGES: &[knot_bundled::BundledSource] = knot_ext_languages::PACKAGES;

    /// The directories of the bundled language packages.
    pub fn language_packages() -> Vec<PathBuf> {
        knot::test_support::cases::packages_in(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions/@knot"),
        )
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("language-"))
        })
        .collect()
    }
}
