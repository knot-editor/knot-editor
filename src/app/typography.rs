//! Application text defaults shared by native editor and terminal surfaces.

use gpui::{Font, FontFallbacks, FontFeatures, FontWeight, Pixels, px};
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
