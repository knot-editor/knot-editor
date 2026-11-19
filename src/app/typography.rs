//! Application text defaults shared by native editor and terminal surfaces.

use std::{collections::HashSet, sync::Mutex};

use gpui::{App, Font, FontFallbacks, FontFeatures, FontWeight, Pixels, TextSystem, px};
use knot_model::typography::FontSettings;

/// The reference text size, which chrome and text inputs scale from.
pub(crate) const TEXT_SIZE: f32 = 14.;

/// Turns off macOS font smoothing for this application, so glyphs rasterize
/// at their natural stem weight like Ghostty's, not thickened by the
/// emulated CoreGraphics dilation. Must run before the first glyph paints.
#[cfg(target_os = "macos")]
pub(crate) fn disable_font_smoothing() {
    use core_foundation::{base::TCFType, number::CFNumber, string::CFString};
    use core_foundation_sys::preferences::{
        CFPreferencesSetAppValue, kCFPreferencesCurrentApplication,
    };

    let key = CFString::new("AppleFontSmoothing");
    let value = CFNumber::from(0i32);
    unsafe {
        CFPreferencesSetAppValue(
            key.as_concrete_TypeRef(),
            value.as_CFTypeRef(),
            kCFPreferencesCurrentApplication,
        );
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn disable_font_smoothing() {}

/// The regular-style font for `settings`; use [`bold`] and [`italic`] for
/// its variants.
pub(crate) fn font(settings: &FontSettings) -> Font {
    let mut font = gpui::font(settings.family.clone());
    font.weight = FontWeight(settings.weight);
    // The platform resolves missing families and glyphs through its font stack.
    font.fallbacks = Some(FontFallbacks::from_fonts(settings.fallbacks.clone()));
    font.features = FontFeatures(settings.features.clone().into());
    font
}

/// Distinct `(scope, family)` pairs already reported, so a view that lays out
/// every frame logs each missing family once.
static REPORTED_FAMILIES: Mutex<Option<HashSet<(&'static str, String)>>> = Mutex::new(None);

/// Log once per scope and family when the platform has no `font.family` font
/// and draws with its fallback stack instead. `scope` names the surface
/// (`editor` or `terminal`), which resolves its own settings.
pub(crate) fn report_missing_family(
    scope: &'static str,
    settings: &FontSettings,
    text: &TextSystem,
    cx: &mut App,
) {
    report_replaced(
        scope,
        &settings.family,
        || {
            let font = font(settings);
            text.get_font_for_id(text.resolve_font(&font))
                .map(|resolved| resolved.family.to_string())
        },
        cx,
    );
}

/// Log, and tell the user once, that `resolve`, which names the family the
/// platform drew with, differs from `family`.
pub(crate) fn report_replaced(
    scope: &'static str,
    family: &str,
    resolve: impl FnOnce() -> Option<String>,
    cx: &mut App,
) {
    let family = family.to_owned();
    if let Some(fallback) = warn_when_replaced(scope, &family, resolve) {
        // Painting calls this, so the notice waits until the frame is done.
        cx.defer(move |cx| {
            crate::app::notifications::raise_config_warning(
                format!("The {scope} font \"{family}\" is not installed; using {fallback}."),
                cx,
            );
        });
    }
}

/// Warn when `resolve`, which names the family the platform drew with, differs
/// from `family`; it runs only for the first check of a scope and family.
/// Returns the fallback family a warning named.
fn warn_when_replaced(
    scope: &'static str,
    family: &str,
    resolve: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let first_report = REPORTED_FAMILIES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get_or_insert_default()
        .insert((scope, family.to_owned()));
    if !first_report {
        return None;
    }
    let resolved = resolve();
    if resolved
        .as_deref()
        .is_none_or(|resolved| !resolved.eq_ignore_ascii_case(family))
    {
        let fallback = resolved.unwrap_or_else(|| "unknown".into());
        tracing::warn!(
            target: "knot::config",
            scope,
            family,
            fallback = fallback.as_str(),
            "font.family is not installed; using the fallback font",
        );
        return Some(fallback);
    }
    None
}

pub(crate) fn bold(font: &Font) -> Font {
    font.clone().bold()
}

pub(crate) fn italic(font: &Font) -> Font {
    font.clone().italic()
}

/// The row height for a font's `ascent` and `descent` (signed or not) times
/// `factor`, rounded up to whole physical pixels at `scale_factor`.
pub(crate) fn line_height(
    ascent: Pixels,
    descent: Pixels,
    factor: f32,
    scale_factor: f32,
) -> Pixels {
    let height = f32::from(ascent) + f32::from(descent).abs();
    px((height * factor * scale_factor).ceil().max(1.) / scale_factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct Captured(std::sync::Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_replaced_family_is_logged_once_per_scope() {
        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer({
                let captured = captured.clone();
                move || captured.clone()
            })
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            for _ in 0..3 {
                warn_when_replaced("editor", "Missing Test Family", || Some("Menlo".into()));
            }
            warn_when_replaced("terminal", "Missing Test Family", || Some("Menlo".into()));
            warn_when_replaced("editor", "Installed Test Family", || {
                Some("installed test family".into())
            });
        });
        let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        let lines: Vec<_> = log.lines().collect();
        assert_eq!(lines.len(), 2, "{log}");
        for (line, scope) in lines.iter().zip(["editor", "terminal"]) {
            assert!(line.contains("WARN knot::config"), "{line}");
            assert!(line.contains(&format!("scope=\"{scope}\"")), "{line}");
            assert!(line.contains(r#"family="Missing Test Family""#), "{line}");
            assert!(line.contains(r#"fallback="Menlo""#), "{line}");
        }
    }
}
