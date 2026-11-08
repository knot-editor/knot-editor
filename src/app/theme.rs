//! The application theme: the installed global over the model's theme values.

use std::sync::LazyLock;

use gpui::{App, Global};

pub(crate) use knot_model::theme::*;

struct ApplicationTheme(Theme);

impl Global for ApplicationTheme {}

/// The application theme: the installed global, else the dark defaults.
pub(crate) fn theme(cx: &App) -> &Theme {
    static DEFAULTS: LazyLock<Theme> = LazyLock::new(Theme::default);
    cx.try_global::<ApplicationTheme>()
        .map_or(&DEFAULTS, |theme| &theme.0)
}

/// Replace the application theme, paint gpui-component with it, and repaint
/// every window.
pub(crate) fn set_theme(theme: Theme, cx: &mut App) {
    cx.set_global(ApplicationTheme(theme));
    super::components::apply_theme(cx);
    cx.refresh_windows();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every surface takes its colors from the theme; tests, the color type, and
    /// its conversions may build colors.
    #[test]
    fn no_color_literals_outside_the_theme() {
        let mut offenders = Vec::new();
        let mut pending = vec![
            std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app")),
            std::path::PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/crates/knot-model/src"
            )),
        ];
        while let Some(path) = pending.pop() {
            if path.is_dir() {
                pending.extend(
                    std::fs::read_dir(&path)
                        .unwrap()
                        .map(|entry| entry.unwrap().path()),
                );
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && !path.ends_with("theme.rs")
                && !path.ends_with("style.rs")
                && !path.ends_with("convert.rs")
                && !path.ends_with("tests.rs")
                && !path
                    .components()
                    .any(|component| component.as_os_str() == "tests")
            {
                let source = std::fs::read_to_string(&path).unwrap();
                for (index, line) in source.lines().enumerate() {
                    if ["rgb(0x", "rgba(0x", "hsla("]
                        .iter()
                        .any(|literal| line.contains(literal))
                    {
                        offenders.push(format!("{}:{}", path.display(), index + 1));
                    }
                }
            }
        }
        assert!(offenders.is_empty(), "color literals: {offenders:#?}");
    }

    #[gpui::test]
    fn installing_a_theme_repaints_windows_and_replaces_the_defaults(
        cx: &mut gpui::TestAppContext,
    ) {
        struct Probe(std::rc::Rc<std::cell::Cell<usize>>);
        impl gpui::Render for Probe {
            fn render(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl gpui::IntoElement {
                self.0.set(self.0.get() + 1);
                gpui::Empty
            }
        }
        let renders = std::rc::Rc::new(std::cell::Cell::new(0));
        cx.add_window({
            let renders = renders.clone();
            move |_, _| Probe(renders)
        });
        cx.run_until_parked();
        let before = renders.get();
        cx.update(|cx| {
            assert_eq!(theme(cx).appearance, Appearance::Dark);
            set_theme(
                Theme::defaults(Appearance::Light, &native_color_definitions()),
                cx,
            );
        });
        cx.run_until_parked();
        assert!(renders.get() > before);
        cx.update(|cx| assert_eq!(theme(cx).appearance, Appearance::Light));
    }
}
