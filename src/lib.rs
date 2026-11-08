//! Knot editor library.

mod app;

pub use app::run;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
/// Handler declarations, for the linked-inventory test and declaration doc tests.
#[doc(hidden)]
pub use knot_protocol::declarations;
