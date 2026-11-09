//! The icon registry global and the element that paints an icon definition.
//!
//! The `icons.pack` setting selects the active pack and is followed for the
//! life of the application. A definition paints in one of two ways: a tinted
//! icon goes through gpui-component's [`Icon`] fed with the SVG bytes, which
//! gpui rasterizes as a single-color shape, and a full-color one through
//! gpui's image element. Both size to the surrounding text, so zoom needs no
//! extra work.

use std::sync::Arc;

use gpui::{
    App, Global, Image, ImageFormat, IntoElement, ObjectFit, RenderOnce, Styled, Window, div, img,
    prelude::*,
};
use gpui_component::Icon;
use gpui_kit_assets::AllAssets;

use super::{
    convert::ThemePaint,
    settings::{self, ApplicationSettings, ICONS_PACK},
    theme::{color, theme},
};
use knot_model::icons::{IconColor, IconDefinition, IconRegistry, IconSource};

#[derive(Default)]
pub(crate) struct ApplicationIcons(pub(crate) IconRegistry);

impl Global for ApplicationIcons {}

/// Select the pack the setting names and follow the setting.
pub(crate) fn apply_at_startup(cx: &mut App) {
    apply(cx);
    cx.observe_global::<ApplicationSettings>(apply).detach();
}

fn apply(cx: &mut App) {
    let pack = settings::text(ICONS_PACK, cx);
    if cx.default_global::<ApplicationIcons>().0.select_pack(&pack) {
        cx.refresh_windows();
    }
}

/// The definition of the icon `name`.
pub(crate) fn icon_definition(name: &str, cx: &App) -> IconDefinition {
    with_registry(cx, |registry| registry.icon(name))
}

/// The definition for a file of `language`.
pub(crate) fn file_definition(language: Option<&str>, cx: &App) -> IconDefinition {
    with_registry(cx, |registry| registry.file_icon(language))
}

/// The definition for a directory, in its expanded form when `open`.
pub(crate) fn folder_definition(open: bool, cx: &App) -> IconDefinition {
    with_registry(cx, |registry| registry.folder_icon(open))
}

/// Resolve against the application registry, or the native defaults before
/// one exists.
fn with_registry(
    cx: &App,
    resolve: impl FnOnce(&IconRegistry) -> IconDefinition,
) -> IconDefinition {
    match cx.try_global::<ApplicationIcons>() {
        Some(icons) => resolve(&icons.0),
        None => resolve(&IconRegistry::default()),
    }
}

/// The SVG document a definition draws, if its source exists.
fn svg_bytes(source: &IconSource) -> Option<std::borrow::Cow<'static, [u8]>> {
    match source {
        IconSource::Svg(text) => Some(text.clone().into_bytes().into()),
        IconSource::Lucide(name) => {
            AllAssets::get(&format!("icons/{name}.svg")).map(|file| file.data)
        }
    }
}

/// An icon at the size of the text around it.
#[derive(IntoElement)]
pub(crate) struct AppIcon {
    definition: Option<IconDefinition>,
}

impl AppIcon {
    pub(crate) fn new(definition: IconDefinition) -> Self {
        Self {
            definition: Some(definition),
        }
    }

    /// Empty space the size of an icon, to keep rows aligned.
    pub(crate) fn blank() -> Self {
        Self { definition: None }
    }
}

impl RenderOnce for AppIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = window.text_style().font_size.to_pixels(window.rem_size());
        let Some((definition, bytes)) = self
            .definition
            .and_then(|definition| Some((svg_bytes(&definition.source)?, definition)))
            .map(|(bytes, definition)| (definition, bytes))
        else {
            return div().size(size).flex_shrink_0().into_any_element();
        };
        let key = match &definition.color {
            IconColor::Original => {
                let image = Image::from_bytes(ImageFormat::Svg, bytes.into_owned());
                return img(Arc::new(image))
                    .size(size)
                    .flex_shrink_0()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element();
            }
            IconColor::Surface => color::ICON,
            IconColor::Key(key) => key,
        };
        let tint = theme(cx).paint_color(key);
        Icon::default()
            .data(&bytes)
            .size(size)
            .text_color(tint)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
