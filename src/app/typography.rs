//! Application text defaults shared by native editor and terminal surfaces.

use gpui::{Font, FontFallbacks, FontFeatures, FontWeight};
use knot_model::typography::FontSettings;

/// The reference text size, which chrome and text inputs scale from.
pub(crate) const TEXT_SIZE: f32 = 14.;

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

pub(crate) fn bold(font: &Font) -> Font {
    font.clone().bold()
}

pub(crate) fn italic(font: &Font) -> Font {
    font.clone().italic()
}
