//! The application keymap and the toolkit's view of it.
//!
//! Knot matches keys itself (see `key_router`), so gpui holds bindings only
//! to label menus with their key equivalents, and gpui-component's bindings
//! for the keys its focused popups take.

use gpui::{App, Global, KeyBinding, KeyBindingContextPredicate};

pub(crate) use knot_model::commands::keymaps::*;

use crate::app::commands::dispatch::{QUIT_COMMAND, menu_action};

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

/// Give gpui the bindings it still needs: menu key equivalents, the config
/// error window's Quit, and gpui-component's own.
///
/// Menus show the key bound to their action. Binding the menu shortcuts to the
/// menu action in a context no element declares labels the menu without the
/// binding ever dispatching; a pressed key still reaches Knot's key routing.
pub(crate) fn rebuild(cx: &mut App) {
    cx.clear_key_bindings();
    let shortcuts = menu_shortcuts(&cx.global::<ApplicationKeymaps>().0);
    let bindings: Vec<_> = shortcuts
        .into_iter()
        .filter_map(|(name, key)| binding(&key, name.as_ref(), MENU_SHORTCUT_CONTEXT, cx))
        .chain(binding("secondary-q", QUIT_COMMAND, "config-error", cx))
        .chain(crate::app::components::key_bindings(cx))
        .collect();
    cx.bind_keys(bindings);
}

/// A gpui binding of `key` to a product command in `context`, resolving
/// `secondary` for the host.
fn binding(key: &str, command: &str, context: &str, cx: &App) -> Option<KeyBinding> {
    let context = KeyBindingContextPredicate::parse(context).ok()?;
    KeyBinding::load(
        key,
        menu_action(command),
        Some(context.into()),
        false,
        None,
        cx,
    )
    .ok()
}

mod presets;
#[cfg(test)]
mod tests;

pub(crate) use presets::*;
