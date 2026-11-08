//! Knot-owned extension host boundaries.
//!
//! V8 implementation details stay in [`engine`]. The scheduler and lifecycle
//! modules own policy independently of JavaScript mechanics, while
//! `knot_protocol` is the typed boundary shared with the application.

pub mod engine;
pub mod lifecycle;
pub mod module_graph;
pub mod pool;
pub mod scheduler;
