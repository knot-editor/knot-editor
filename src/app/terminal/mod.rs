//! Native terminal sessions and their views over `knot-terminal`.

/// The stable ID of the Terminal container tool window the test fixtures
/// register as `@knot/chrome` does.
#[cfg(test)]
pub(crate) const TERMINAL_TOOL_ID: &str = "builtin.terminal";
/// The slot the test Terminal window docks in.
#[cfg(test)]
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
