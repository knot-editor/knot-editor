//! gpui-component, the library behind Knot's leaf widgets, and the bridge
//! that paints it with the application theme.
//!
//! Each application of a Knot theme switches gpui-component to the theme's
//! appearance, which loads that appearance's built-in colors, then sets
//! every field of [`COLORS`] from its native key. Fields without a fitting
//! key keep the built-in colors.

use gpui::{App, Hsla};
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

/// Initialize gpui-component and paint it with the current theme.
pub(crate) fn init(cx: &mut App) {
    gpui_component::init(cx);
    apply_theme(cx);
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
