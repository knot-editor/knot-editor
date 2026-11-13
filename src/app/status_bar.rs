//! The application's status bar registry, shared by every window. The items
//! and their merge rules live in [`knot_model::status_bar`].

use gpui::Global;
use knot_protocol::{ExtensionId, ExtensionLifecycleId};

pub(crate) use knot_model::status_bar::*;

#[derive(Default)]
pub(crate) struct ApplicationStatusBar(pub(crate) StatusRegistry);

impl Global for ApplicationStatusBar {}

pub(crate) fn remove_lifecycle(
    extension: ExtensionId,
    lifecycle: ExtensionLifecycleId,
    cx: &mut gpui::App,
) {
    if cx.has_global::<ApplicationStatusBar>() {
        cx.global_mut::<ApplicationStatusBar>()
            .0
            .remove_lifecycle(extension, lifecycle);
    }
}
