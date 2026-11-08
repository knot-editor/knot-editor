//! The application keymap and the toolkit's view of it.
//!
//! Knot matches keys itself (see `key_router`), so gpui holds bindings only
//! to label menus with their key equivalents.

use gpui::{App, Global, KeyBinding};

pub(crate) use knot_model::commands::keymaps::*;

use crate::app::commands::dispatch::{ProductCommandSource, QUIT_COMMAND};

/// A key context no element declares, for bindings that only label menus.
const MENU_SHORTCUT_CONTEXT: &str = "menu_shortcut";

pub(crate) struct ApplicationKeymaps(pub(crate) BindingRegistry);

impl Global for ApplicationKeymaps {}

pub(crate) fn install(cx: &mut App) {
    let mut registry = BindingRegistry::new();
    registry.replace_preset(&knot_preset(Platform::current()));
    cx.set_global(ApplicationKeymaps(registry));
    rebuild(cx);
}

/// Give gpui the bindings it still needs: menu key equivalents and the
/// config error window's Quit.
///
/// Menus show the key bound to their action. Binding the menu shortcuts to the
/// menu action in a context no element declares labels the menu without the
/// binding ever dispatching; a pressed key still reaches Knot's key routing.
pub(crate) fn rebuild(cx: &mut App) {
    cx.clear_key_bindings();
    let shortcuts = menu_shortcuts(&cx.global::<ApplicationKeymaps>().0);
    cx.bind_keys(shortcuts.into_iter().map(|(name, key)| {
        KeyBinding::new(
            &key,
            ProductCommandSource {
                name: name.to_string().into(),
            },
            Some(MENU_SHORTCUT_CONTEXT),
        )
    }));
    cx.bind_keys([KeyBinding::new(
        "secondary-q",
        ProductCommandSource::new(QUIT_COMMAND),
        Some("config-error"),
    )]);
}

mod presets;
#[cfg(test)]
mod tests;

pub(crate) use presets::*;
