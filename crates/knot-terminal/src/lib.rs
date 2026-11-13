//! The terminal engine, over the statically linked libghostty-vt.
//!
//! [`Terminal`] parses program output into screens and scrollback;
//! [`RenderState`] reads frames from it for painting. [`ffi`] is generated
//! from `ghostty/vt.h` by `scripts/libghostty-bindings.sh`
//! (`just libghostty-bindings`); the library itself creates no threads, and
//! every handle it returns belongs to the caller.

#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    unsafe_op_in_unsafe_fn,
    clippy::all
)]
pub mod ffi;
mod render;
mod terminal;

pub use render::{
    Cell, CellWidth, Cells, Colors, Cursor, CursorStyle, Dirty, RenderState, Row, Rows, Style,
    Underline,
};
pub use terminal::{Rgb, Scroll, Scrollbar, Terminal, TerminalSize, default_palette};
