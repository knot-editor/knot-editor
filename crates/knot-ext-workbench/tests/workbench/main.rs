//! The bundled workbench packages (projects, status bar, Go to Line, Go to File, themes), run in the real editor.

mod explorer;
mod find;
mod go_to_file;
mod go_to_line;
mod project_inference;
mod project_sessions;
mod projects;
mod status_bar_package;
mod themes;
mod word_highlight;

mod bundled {
    pub const PACKAGES: &[knot_bundled::BundledSource] = knot_ext_workbench::PACKAGES;

    /// The packages named `names`, for a test that exercises only those.
    pub fn only(names: &[&str]) -> &'static [knot_bundled::BundledSource] {
        let chosen: Vec<_> = PACKAGES
            .iter()
            .filter(|package| {
                names
                    .iter()
                    .any(|name| package.manifest.contains(&format!("\"name\": \"{name}\"")))
            })
            .copied()
            .collect();
        assert_eq!(chosen.len(), names.len(), "unknown package in {names:?}");
        Box::leak(chosen.into_boxed_slice())
    }
}
