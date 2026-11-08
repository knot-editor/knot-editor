//! The theme registry global and the selection that installs the application
//! theme from the `theme.*` settings.

use gpui::{App, Global, WindowAppearance};

use knot_protocol::{ExtensionId, ExtensionLifecycleId};

use super::{
    settings::{self, ApplicationSettings, THEME_DARK, THEME_LIGHT, THEME_MODE},
    theme::{Appearance, set_theme},
};

pub(crate) use knot_model::themes::*;

#[derive(Default)]
pub(crate) struct ApplicationThemes(pub(crate) ThemeRegistry);

impl Global for ApplicationThemes {}

/// The theme the selection settings name, for the current system appearance.
fn selection(cx: &App) -> Selection {
    let appearance = match settings::text(THEME_MODE, cx).as_str() {
        "light" => Appearance::Light,
        "system" => match cx.window_appearance() {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Appearance::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
        },
        _ => Appearance::Dark,
    };
    let name = settings::text(
        match appearance {
            Appearance::Dark => THEME_DARK,
            Appearance::Light => THEME_LIGHT,
        },
        cx,
    );
    Selection { name, appearance }
}

/// Resolve and install the selected theme. Startup calls this once every
/// theme is registered, and it starts following the selection settings.
pub(crate) fn apply_at_startup(cx: &mut App) {
    apply(cx, true);
    cx.observe_global::<ApplicationSettings>(|cx| apply(cx, false))
        .detach();
}

/// Install the selected theme again if the selection changed, such as after
/// a settings write or a change of system appearance.
pub(crate) fn selection_changed(cx: &mut App) {
    apply(cx, false);
}

fn apply(cx: &mut App, force: bool) {
    let selection = selection(cx);
    let registry = &mut cx.default_global::<ApplicationThemes>().0;
    if let Some(theme) = registry.select(selection, force) {
        set_theme(theme, cx);
    }
}

pub(crate) fn remove_lifecycle(
    extension: ExtensionId,
    lifecycle: ExtensionLifecycleId,
    cx: &mut App,
) {
    if cx.has_global::<ApplicationThemes>() {
        cx.global_mut::<ApplicationThemes>()
            .0
            .remove_lifecycle(extension, lifecycle);
    }
}

#[cfg(test)]
mod tests;
