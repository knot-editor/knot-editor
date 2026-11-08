//! The command catalog, dispatch, keymaps, extension invocation, and native
//! handler declarations. Command bodies live beside the state they act on.

pub(crate) use knot_model::commands::{catalog, key_router};
pub(super) mod declarations;
pub(super) mod dispatch;
pub(super) mod extension;
pub(crate) mod key_hold;
pub(super) mod keymaps;
