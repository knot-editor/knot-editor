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
use knot_protocol::{ExtensionId, ExtensionLifecycleId};

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

/// Repaint every window after the registry changed.
pub(crate) fn refresh_icons(cx: &mut App) {
    cx.refresh_windows();
}

/// Drop what a torn-down lifecycle registered.
pub(crate) fn remove_lifecycle(
    extension: ExtensionId,
    lifecycle: ExtensionLifecycleId,
    cx: &mut App,
) {
    if cx.has_global::<ApplicationIcons>() {
        cx.global_mut::<ApplicationIcons>()
            .0
            .remove_lifecycle(extension, lifecycle);
        refresh_icons(cx);
    }
}

/// Whether `name` is a glyph of the builtin Lucide set.
pub(crate) fn glyph_exists(name: &str) -> bool {
    AllAssets::get(&format!("icons/{name}.svg")).is_some()
}

/// The definition of the icon `name`.
pub(crate) fn icon_definition(name: &str, cx: &App) -> IconDefinition {
    with_registry(cx, |registry| registry.icon(name))
}

/// The definition for a file called `file_name` of `language`.
pub(crate) fn file_definition(
    file_name: Option<&str>,
    language: Option<&str>,
    cx: &App,
) -> IconDefinition {
    with_registry(cx, |registry| registry.file_icon(file_name, language))
}

/// The definition for a file known only by its name: the language is the one
/// that name detects.
pub(crate) fn named_file_definition(file_name: &str, cx: &App) -> IconDefinition {
    let language = super::languages::detect_file_name(file_name, cx);
    file_definition(Some(file_name), language.as_deref(), cx)
}

/// The definition for a directory called `name`, in its expanded form when
/// `open`.
pub(crate) fn folder_definition(name: Option<&str>, open: bool, cx: &App) -> IconDefinition {
    with_registry(cx, |registry| registry.folder_icon(name, open))
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
