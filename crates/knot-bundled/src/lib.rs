//! The vocabulary of compiled bundled extension packages, and the build-script
//! helper that produces them.

/// One bundled package: its manifest text and compiled modules, keyed by the
/// `dist/…` path they are admitted under.
#[derive(Clone, Copy)]
pub struct BundledSource {
    pub manifest: &'static str,
    pub sources: &'static [(&'static str, &'static str)],
}

pub mod build;
