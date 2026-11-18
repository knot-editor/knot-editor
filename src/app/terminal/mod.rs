//! Native terminal sessions and their views over `knot-terminal`.

/// The stable ID of the Terminal container tool window.
pub(crate) const TERMINAL_TOOL_ID: &str = "knot.terminal";
/// The slot the Terminal window docks in until the user moves it.
pub(crate) const TERMINAL_SLOT: crate::app::workbench::Slot =
    crate::app::workbench::Slot::BottomLeft;

pub(super) mod colors;
pub(super) mod input;
pub(super) mod layout;
pub(super) mod links;
pub(super) mod mouse;
mod process;
pub(super) mod session;
#[cfg(all(test, unix))]
mod session_tests;
pub(super) mod view;
