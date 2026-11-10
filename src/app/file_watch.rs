//! The application-wide registry of file watches.
//!
//! Watches are requested by their consumers, such as extensions through
//! `files.watch`; the registry shares provider watches between them.

use gpui::Global;
use knot_model::watch::WatchRegistry;

#[derive(Default)]
pub(crate) struct ApplicationFileWatches(pub(crate) WatchRegistry);

impl Global for ApplicationFileWatches {}
