use gpui::{TestAppContext, WindowAppearance};

use super::*;
use crate::app::{
    style::Color,
    theme::{color, theme},
};

fn owner(lifecycle: u64) -> ThemeOwner {
    ThemeOwner {
        extension: ExtensionId::new(1),
        lifecycle: ExtensionLifecycleId::new(lifecycle),
    }
}

fn definition(name: &str, appearance: Appearance, background: &str) -> ThemeDefinition {
    ThemeDefinition {
        name: name.into(),
        appearance,
        colors: std::collections::BTreeMap::from([(color::BACKGROUND.into(), background.into())]),
        syntax: Default::default(),
    }
}

#[gpui::test]
fn startup_applies_the_selection_and_settings_writes_switch_it(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(ApplicationSettings::default());
        let registry = &mut cx.default_global::<ApplicationThemes>().0;
        registry.register(
            owner(1),
            definition("One Dark", Appearance::Dark, "#101010"),
        );
        registry.register(owner(1), definition("Paper", Appearance::Light, "#fefefe"));
        apply_at_startup(cx);
        assert_eq!(theme(cx).color(color::BACKGROUND), Color::rgb(0x101010));
    });
    cx.update(|cx| {
        let scope = settings::SettingsScope::default();
        let settings = &mut cx.global_mut::<ApplicationSettings>().0;
        settings
            .set(THEME_LIGHT, "Paper".into(), scope.clone())
            .unwrap();
        settings.set(THEME_MODE, "light".into(), scope).unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(theme(cx).appearance, Appearance::Light);
        assert_eq!(theme(cx).color(color::BACKGROUND), Color::rgb(0xfefefe));
        // A registration after startup changes nothing until the selection does.
        cx.global_mut::<ApplicationThemes>()
            .0
            .register(owner(1), definition("Paper", Appearance::Light, "#eeeeee"));
        selection_changed(cx);
        assert_eq!(theme(cx).color(color::BACKGROUND), Color::rgb(0xfefefe));
    });
}

#[gpui::test]
fn system_mode_follows_the_window_appearance(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut settings = ApplicationSettings::default();
        settings
            .0
            .set(THEME_MODE, "system".into(), Default::default())
            .unwrap();
        cx.set_global(settings);
        let expected = match cx.window_appearance() {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Appearance::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
        };
        apply_at_startup(cx);
        assert_eq!(theme(cx).appearance, expected);
    });
}
