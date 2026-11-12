//! The version and commit compiled into this binary by `build.rs`.

pub(crate) const VERSION: &str = env!("KNOT_BUILD_VERSION");
pub(crate) const COMMIT: &str = env!("KNOT_BUILD_COMMIT");

/// The one-line identity `knot --version` prints and About shows.
pub(crate) fn summary() -> String {
    format!("Knot {VERSION} ({COMMIT})")
}
