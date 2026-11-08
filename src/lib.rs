//! Knot editor library.

mod app;

pub use app::run;
/// Handler declarations, for the linked-inventory test and declaration doc tests.
#[doc(hidden)]
pub use knot_protocol::declarations;
