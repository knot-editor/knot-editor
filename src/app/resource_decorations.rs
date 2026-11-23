//! The application's resource decorations, shared by every window. The
//! sources and their merge rules live in [`knot_model::resource_decorations`].

use gpui::Global;
use knot_protocol::{ExtensionId, ExtensionLifecycleId};

pub(crate) use knot_model::resource_decorations::*;

#[derive(Default)]
pub(crate) struct ApplicationResourceDecorations(pub(crate) DecorationRegistry);

impl Global for ApplicationResourceDecorations {}

/// Repaint every window after decorations changed.
pub(crate) fn refresh(cx: &mut gpui::App) {
    cx.refresh_windows();
}

pub(crate) fn remove_lifecycle(
    extension: ExtensionId,
    lifecycle: ExtensionLifecycleId,
    cx: &mut gpui::App,
) {
    if cx.has_global::<ApplicationResourceDecorations>()
        && cx
            .global_mut::<ApplicationResourceDecorations>()
            .0
            .remove_lifecycle(extension, lifecycle)
    {
        refresh(cx);
    }
}

/// The decoration painted for `uri`, if a source decorates it.
pub(crate) fn decoration_of(
    uri: &knot_model::resource::ResourceUri,
    cx: &gpui::App,
) -> Option<ResolvedDecoration> {
    cx.try_global::<ApplicationResourceDecorations>()?
        .0
        .lookup(uri)
        .cloned()
}
