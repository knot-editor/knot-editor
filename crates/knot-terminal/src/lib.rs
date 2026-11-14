//! The terminal engine, over the statically linked libghostty-vt.
//!
//! [`Terminal`] parses program output into screens and scrollback, reports
//! its effects as [`TerminalEvent`]s, answers mode, selection, text, and
//! hyperlink queries, and encodes keyboard and mouse input in the protocols
//! the program asked for; [`RenderState`] reads frames from it for painting.
//! [`Pty`] runs a program on a pseudo-terminal and feeds its output to a
//! terminal from its own threads. [`ffi`] is generated from `ghostty/vt.h`
//! by `scripts/libghostty-bindings.sh` (`just libghostty-bindings`); the
//! library itself creates no threads, and every handle it returns belongs
//! to the caller.

mod directory;
mod events;
#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    unsafe_op_in_unsafe_fn,
    clippy::all
)]
pub mod ffi;
mod grid;
mod input;
mod pty;
mod render;
mod terminal;

pub use directory::local_directory;
pub use events::TerminalEvent;
pub use grid::Point;
pub use input::{Key, KeyAction, KeyEvent, Modifiers, MouseAction, MouseButton, MouseEvent};
pub use pty::{ExitStatus, Pty, PtyEvent, PtyOptions, Shell};
pub use render::{
    Cell, CellWidth, Cells, Colors, Cursor, CursorStyle, Dirty, RenderState, Row, Rows, Style,
    Underline,
};
pub use terminal::{Mode, Rgb, Scroll, Scrollbar, Terminal, TerminalSize, default_palette};
