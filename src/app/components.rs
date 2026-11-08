//! gpui-component, the library behind Knot's leaf widgets, and the bridge
//! that paints it with the application theme.
//!
//! Each application of a Knot theme switches gpui-component to the theme's
//! appearance, which loads that appearance's built-in colors, then sets
//! every field of [`COLORS`] from its native key. Fields without a fitting
//! key keep the built-in colors.
//!
//! A gpui-component popup that takes focus registers its focus handle. While
//! one holds focus, the keymap still matches keys first, and a key it leaves
//! goes to gpui's dispatch, where the popup's own actions run, rather than to
//! the surface with Knot-side focus.

use gpui::{App, FocusHandle, Global, Hsla, KeyBinding, WeakFocusHandle, Window};
use gpui_component::{Theme as ComponentTheme, ThemeColor, ThemeMode};

use super::{
    convert::ThemePaint,
    theme::{Appearance, color, theme},
};

macro_rules! component_colors {
    ($($field:ident = $key:expr;)*) => {
        /// Each gpui-component color field, by name, and the native key it takes.
        pub(crate) const COLORS: &[(&str, fn(&mut ThemeColor) -> &mut Hsla, &str)] =
            &[$((stringify!($field), |colors| &mut colors.$field, $key),)*];
    };
}

component_colors! {
    accent = color::LIST_HOVER_BACKGROUND;
    accent_foreground = color::TEXT;
    background = color::BACKGROUND;
    border = color::BORDER;
    caret = color::EDITOR_CURSOR;
    danger = color::ERROR;
    drop_target = color::DROP_TARGET_BACKGROUND;
    foreground = color::TEXT;
    info = color::INFO;
    link = color::TEXT_ACCENT;
    list_active = color::LIST_SELECTED_BACKGROUND;
    list_hover = color::LIST_HOVER_BACKGROUND;
    muted = color::BUTTON_BACKGROUND;
    muted_foreground = color::TEXT_MUTED;
    overlay = color::SCRIM;
    popover = color::POPOVER_BACKGROUND;
    popover_foreground = color::TEXT;
    scrollbar_thumb = color::SCROLLBAR_THUMB;
    secondary = color::BUTTON_BACKGROUND;
    secondary_foreground = color::TEXT;
    secondary_hover = color::BUTTON_HOVER_BACKGROUND;
    selection = color::EDITOR_SELECTION;
    status_bar = color::STATUS_BAR_BACKGROUND;
    status_bar_border = color::STATUS_BAR_BORDER;
    success = color::SUCCESS;
    tab = color::TAB_INACTIVE_BACKGROUND;
    tab_active = color::TAB_ACTIVE_BACKGROUND;
    tab_active_foreground = color::TAB_ACTIVE_FOREGROUND;
    tab_bar = color::TAB_BAR_BACKGROUND;
    tab_foreground = color::TAB_INACTIVE_FOREGROUND;
    warning = color::WARNING;
}

/// The focus handles of popups that may hold focus; dropped popups leave.
#[derive(Default)]
struct Popups(Vec<WeakFocusHandle>);

impl Global for Popups {}

/// Register the focus handle of a popup that takes focus.
pub(crate) fn track_popup(focus: &FocusHandle, cx: &mut App) {
    let popups = &mut cx.default_global::<Popups>().0;
    popups.retain(|popup| popup.upgrade().is_some());
    popups.push(focus.downgrade());
}

/// Whether a registered popup, or an element inside it, holds focus.
pub(crate) fn popup_focused(window: &Window, cx: &App) -> bool {
    cx.try_global::<Popups>().is_some_and(|popups| {
        popups
            .0
            .iter()
            .filter_map(WeakFocusHandle::upgrade)
            .any(|popup| popup.is_focused(window) || popup.contains_focused(window, cx))
    })
}

/// The key bindings gpui-component installs, which the keymap restores
/// whenever it rebuilds gpui's bindings.
#[derive(Default)]
struct Bindings(Vec<KeyBinding>);

impl Global for Bindings {}

/// Initialize gpui-component and paint it with the current theme.
pub(crate) fn init(cx: &mut App) {
    let before = cx.key_bindings().borrow().bindings().len();
    gpui_component::init(cx);
    let bindings = cx
        .key_bindings()
        .borrow()
        .bindings()
        .skip(before)
        .cloned()
        .collect();
    cx.set_global(Bindings(bindings));
    apply_theme(cx);
}

/// The key bindings of gpui-component's own actions, such as a popup
/// menu's arrows, Enter, and Escape in its key context.
pub(crate) fn key_bindings(cx: &App) -> Vec<KeyBinding> {
    cx.try_global::<Bindings>()
        .map(|bindings| bindings.0.clone())
        .unwrap_or_default()
}

/// Paint gpui-component with the current application theme; nothing before
/// [`init`].
pub(crate) fn apply_theme(cx: &mut App) {
    if !cx.has_global::<ComponentTheme>() {
        return;
    }
    let knot = theme(cx);
    let mode = match knot.appearance {
        Appearance::Dark => ThemeMode::Dark,
        Appearance::Light => ThemeMode::Light,
    };
    let colors = COLORS
        .iter()
        .map(|(_, field, key)| (*field, knot.paint_color(key)))
        .collect::<Vec<_>>();
    ComponentTheme::change(mode, None, cx);
    ComponentTheme::update(cx, |component| {
        for (field, value) in colors {
            *field(&mut component.colors) = value;
        }
    });
}

/// Each way gpui-component's theme differs from the application theme it
/// should follow: a mapped field, the popover token popups paint with, or
/// the appearance.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn drift(cx: &App) -> Vec<String> {
    let knot = theme(cx);
    let mut component = ComponentTheme::global(cx).clone();
    let mut drift = Vec::new();
    for (name, field, key) in COLORS {
        let (actual, expected) = (*field(&mut component.colors), knot.paint_color(key));
        if actual != expected {
            drift.push(format!("{name} is {actual:?}, {key} is {expected:?}"));
        }
    }
    if component.tokens.popover.color != component.colors.popover {
        drift.push("the popover token kept its previous color".to_owned());
    }
    if component.mode.is_dark() != (knot.appearance == Appearance::Dark) {
        drift.push(format!("mode is {:?}", component.mode));
    }
    drift
}
