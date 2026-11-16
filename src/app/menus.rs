//! The application's menu registry, shared by every window. The items and
//! their ordering and scope rules live in [`knot_model::menus`].

use gpui::Global;
use knot_protocol::{ExtensionId, ExtensionLifecycleId};

pub(crate) use knot_model::menus::*;

#[derive(Default)]
pub(crate) struct ApplicationMenus(pub(crate) MenuRegistry);

impl Global for ApplicationMenus {}

pub(crate) fn remove_lifecycle(
    extension: ExtensionId,
    lifecycle: ExtensionLifecycleId,
    cx: &mut gpui::App,
) {
    if cx.has_global::<ApplicationMenus>() {
        cx.global_mut::<ApplicationMenus>()
            .0
            .remove_lifecycle(extension, lifecycle);
    }
}
