//! The bundled workbench packages (projects, status bar, Go to Line, Go to File, themes), run in the real editor.

mod go_to_file;
mod go_to_line;
mod project_sessions;
mod projects;
mod status_bar_package;
mod themes;

mod bundled {
    pub const PACKAGES: &[knot_bundled::BundledSource] = knot_ext_workbench::PACKAGES;
}
