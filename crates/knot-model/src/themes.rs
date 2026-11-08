//! Theme registrations, extension color definitions, and selection of the
//! application theme.
//!
//! Extensions and personal config register themes by name; the latest
//! registration of a name defines it, and removing it reveals the previous
//! one. Extensions also define color keys beside the native ones, so a theme
//! and a contribution can name a key the native table does not know.
//!
//! The `theme.mode`, `theme.dark`, and `theme.light` settings select the
//! theme. It is resolved when startup completes and again whenever the
//! selection changes; registering or removing a theme later changes nothing
//! until the next selection change.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use knot_protocol::{ExtensionId, ExtensionLifecycleId};

use crate::{
    style::{Color, FontStyle, HighlightStyle, Underline},
    theme::{
        Appearance, ColorDefault, ColorDefinition, Highlight, Theme, markup_styles,
        native_color_definitions,
    },
};

/// The extension lifecycle that owns a registration or color definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeOwner {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
}

/// How a theme styles one capture name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyntaxStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_style: Option<FontStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strikethrough: Option<bool>,
}

impl SyntaxStyle {
    fn highlight(&self) -> HighlightStyle {
        HighlightStyle {
            color: self.color.as_deref().and_then(Color::parse),
            font_style: self.font_style,
            font_weight: self.font_weight,
            underline: self
                .underline
                .filter(|underline| *underline)
                .map(|_| Underline {
                    thickness: 1.,
                    color: None,
                    wavy: false,
                }),
            strikethrough: self.strikethrough.unwrap_or(false),
        }
    }
}

/// A theme as an extension registers it and `themes.get` returns it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeDefinition {
    pub name: String,
    pub appearance: Appearance,
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    #[serde(default)]
    pub syntax: BTreeMap<String, SyntaxStyle>,
}

impl ThemeDefinition {
    /// Why a color or font weight is invalid, if one is.
    pub fn invalid_value(&self) -> Option<&'static str> {
        let colors = self.colors.values().map(String::as_str).chain(
            self.syntax
                .values()
                .filter_map(|style| style.color.as_deref()),
        );
        for color in colors {
            if Color::parse(color).is_none() {
                return Some("theme colors must be #rrggbb or #rrggbbaa");
            }
        }
        self.syntax
            .values()
            .filter_map(|style| style.font_weight)
            .any(|weight| !(100..=900).contains(&weight))
            .then_some("theme font weights must be from 100 to 900")
    }

    /// A syntax key that names no capture or capture prefix; color keys
    /// stay open.
    pub fn unknown_capture(&self) -> Option<&str> {
        self.syntax
            .keys()
            .map(String::as_str)
            .find(|capture| !Highlight::styles_captures(capture))
    }

    fn theme(&self, definitions: &[ColorDefinition]) -> Theme {
        let values = self
            .colors
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), Color::parse(value)?)))
            .collect::<HashMap<_, _>>();
        let syntax = self
            .syntax
            .iter()
            .map(|(capture, style)| (capture.as_str(), style.highlight()))
            .chain(markup_styles())
            .collect::<Vec<_>>();
        Theme::new(self.appearance, &values, definitions, &syntax)
    }
}

/// A color default as written: `#rrggbb`, `#rrggbbaa`, or another key.
pub fn color_default(text: &str) -> Option<ColorDefault> {
    if text.starts_with('#') {
        Color::parse(text).map(ColorDefault::Color)
    } else if text.is_empty() {
        None
    } else {
        Some(ColorDefault::Key(text.to_owned()))
    }
}

/// A registered theme's name and appearance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ThemeInfo {
    pub name: String,
    pub appearance: Appearance,
}

struct Registration {
    id: u64,
    owner: ThemeOwner,
    definition: ThemeDefinition,
}

/// Which theme applies: a theme name and the appearance it was chosen for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub name: String,
    pub appearance: Appearance,
}

#[derive(Default)]
pub struct ThemeRegistry {
    /// In registration order, so a later index wins.
    registrations: Vec<Registration>,
    colors: Vec<(ThemeOwner, ColorDefinition)>,
    next_id: u64,
    /// The selection the installed theme was resolved from.
    applied: Option<Selection>,
}

impl ThemeRegistry {
    /// Add a validated registration, returning its ID.
    pub fn register(&mut self, owner: ThemeOwner, definition: ThemeDefinition) -> u64 {
        self.next_id += 1;
        self.registrations.push(Registration {
            id: self.next_id,
            owner,
            definition,
        });
        self.next_id
    }

    /// Remove an owned registration; false when it is not live.
    pub fn unregister(&mut self, owner: ThemeOwner, id: u64) -> bool {
        let before = self.registrations.len();
        self.registrations
            .retain(|registration| registration.id != id || registration.owner != owner);
        self.registrations.len() != before
    }

    /// Define a color key, which must not already be defined.
    pub fn define_color(
        &mut self,
        owner: ThemeOwner,
        definition: ColorDefinition,
    ) -> Result<(), DuplicateColor> {
        if self
            .color_definitions()
            .iter()
            .any(|existing| existing.key == definition.key)
        {
            return Err(DuplicateColor);
        }
        self.colors.push((owner, definition));
        Ok(())
    }

    pub fn remove_lifecycle(&mut self, extension: ExtensionId, lifecycle: ExtensionLifecycleId) {
        let owner = ThemeOwner {
            extension,
            lifecycle,
        };
        self.registrations
            .retain(|registration| registration.owner != owner);
        self.colors.retain(|(color_owner, _)| *color_owner != owner);
    }

    /// The latest registration of `name`.
    pub fn get(&self, name: &str) -> Option<&ThemeDefinition> {
        self.registrations
            .iter()
            .rev()
            .map(|registration| &registration.definition)
            .find(|definition| definition.name == name)
    }

    /// Each registered name's effective theme, sorted by name.
    pub fn list(&self) -> Vec<ThemeInfo> {
        let mut themes = BTreeMap::new();
        for registration in &self.registrations {
            themes.insert(
                registration.definition.name.clone(),
                registration.definition.appearance,
            );
        }
        themes
            .into_iter()
            .map(|(name, appearance)| ThemeInfo { name, appearance })
            .collect()
    }

    /// Record `selection` as the one the installed theme follows and return
    /// the theme to install, or `None` when it is already installed and
    /// `force` is not set.
    pub fn select(&mut self, selection: Selection, force: bool) -> Option<Theme> {
        if !force && self.applied.as_ref() == Some(&selection) {
            return None;
        }
        let theme = self.theme(&selection);
        self.applied = Some(selection);
        Some(theme)
    }

    /// Native color definitions followed by extension ones.
    fn color_definitions(&self) -> Vec<ColorDefinition> {
        native_color_definitions()
            .into_iter()
            .chain(self.colors.iter().map(|(_, definition)| definition.clone()))
            .collect()
    }

    /// The theme `selection` names, else the defaults for its appearance.
    pub fn theme(&self, selection: &Selection) -> Theme {
        let definitions = self.color_definitions();
        match self.get(&selection.name) {
            Some(definition) => definition.theme(&definitions),
            None => Theme::defaults(selection.appearance, &definitions),
        }
    }
}

/// A color key that native code or a live lifecycle already defines.
#[derive(Debug, PartialEq, Eq)]
pub struct DuplicateColor;

#[cfg(test)]
mod tests;
