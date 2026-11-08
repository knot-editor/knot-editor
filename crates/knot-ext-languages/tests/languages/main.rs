//! The bundled language packages, run in the real editor.

mod brackets;
mod editing;
mod folds;
mod highlights;
mod indents;

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
