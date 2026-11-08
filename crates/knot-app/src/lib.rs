//! Every bundled package, joined across the domain crates that compile them.

use std::sync::OnceLock;

use knot_bundled::BundledSource;

/// The packages of all bundled domains.
pub fn packages() -> &'static [BundledSource] {
    static PACKAGES: OnceLock<Vec<BundledSource>> = OnceLock::new();
    PACKAGES.get_or_init(|| {
        [
            knot_ext_languages::PACKAGES,
            knot_ext_vim::PACKAGES,
            knot_ext_workbench::PACKAGES,
        ]
        .concat()
    })
}
