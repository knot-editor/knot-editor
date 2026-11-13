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
pub use crate::app::driver::editor::{TestEditor, Work};
pub use crate::app::driver::product::{Language, LoadEntry, ServedFiles, TestProduct};
pub use crate::app::driver::window::{EditorTab, TestCommand, TestTree, TestWindow};
pub use knot_protocol::CommandOutcome;

/// One editing action run in a bare editor, without a session or window.
pub mod editing {
    pub use crate::app::driver::editing::{Action, BACKSPACE, Outcome, run};
}

pub use crate::app::driver::wait_until;
