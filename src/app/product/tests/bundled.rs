//! The bundled packages and their directories, for the tests that run them.
use std::path::{Path, PathBuf};

pub(super) const PACKAGES: &[knot_bundled::BundledSource] = knot_bundled::PACKAGES;

/// The directory of the bundled package `@knot/<name>`.
pub(super) fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("extensions/@knot")
        .join(name)
}

/// The directories of the bundled language packages.
pub(super) fn language_packages() -> Vec<PathBuf> {
    crate::test_support::cases::packages_in(&package(""))
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("language-"))
        })
        .collect()
}
