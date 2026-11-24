//! A driver for testing the real product from outside the library.
//!
//! [`TestProduct`] starts the application with the packages a test chooses,
//! and [`TestEditor`] drives one editor session through keys, commands, and
//! documents. A [launched](TestProduct::launch) product has windows, which
//! [`TestWindow`] drives through keys and commands without a native window
//! unless the test [asks for one](TestProduct::launch_windowed), and durable
//! state that a relaunch reads back. Both report what the product shows as plain data, so tests
//! exercise the real editor and packages without reaching into the
//! library's internals. [`cases`] reads the case files packages keep beside
//! their sources, and [`editing`] runs one editing action in a bare editor.

pub mod cases;

pub use crate::app::driver::application::{HeldReads, Retained};
pub use crate::app::driver::editor::{KeyCost, TestEditor, Work};
pub use crate::app::driver::product::{Language, LoadEntry, ServedFiles, TestProduct};
pub use crate::app::driver::window::{EditorTab, TestCommand, TestTree, TestWindow};
pub use knot_model::large_documents;
pub use knot_protocol::CommandOutcome;

/// One editing action run in a bare editor, without a session or window.
pub mod editing {
    pub use crate::app::driver::editing::{Action, BACKSPACE, Outcome, run};
}

pub use crate::app::driver::wait_until;

/// A temporary directory named by its resolved path, for a test that watches
/// a Git repository made in it. The system's temporary directory can be
/// reached through a symlink (on macOS `/var` is `/private/var`), while Git
/// reports resolved paths: a repository's metadata would then lie outside the
/// workspace root as spelled, and each watch of it would start an operating
/// system watch of its own instead of sharing the root's.
pub fn resolved_tempdir() -> tempfile::TempDir {
    let parent = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    tempfile::tempdir_in(parent).unwrap()
}
