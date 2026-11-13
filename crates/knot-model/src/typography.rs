//! The font a surface draws text with, resolved from the `font.*` settings.

use serde_json::Value;

use crate::settings::{
    FONT_FALLBACKS, FONT_FAMILY, FONT_FEATURES, FONT_LIGATURES, FONT_WEIGHT, SettingsScope,
    SettingsStore,
};

/// The resolved font choice of one settings scope.
#[derive(Clone, Debug, PartialEq)]
pub struct FontSettings {
    pub family: String,
    pub fallbacks: Vec<String>,
    /// Base weight, 100 to 900.
    pub weight: f32,
    /// OpenType feature tags and values, the ligature toggle first.
    pub features: Vec<(String, u32)>,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self::resolve(&SettingsStore::default(), &SettingsScope::default())
    }
}

impl FontSettings {
    /// The font settings that apply to `scope`.
    pub fn resolve(store: &SettingsStore, scope: &SettingsScope) -> Self {
        let strings = |key: &str| -> Vec<String> {
            store
                .get(key, scope)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let ligatures = store
            .get(FONT_LIGATURES, scope)
            .and_then(Value::as_bool)
            .unwrap_or(true);
        Self {
            family: store
                .get(FONT_FAMILY, scope)
                .and_then(Value::as_str)
                .unwrap_or("Menlo")
                .to_owned(),
            fallbacks: strings(FONT_FALLBACKS),
            weight: store
                .get(FONT_WEIGHT, scope)
                .and_then(Value::as_f64)
                .unwrap_or(400.)
                .clamp(100., 900.) as f32,
            features: features(ligatures, &strings(FONT_FEATURES)),
        }
    }
}

/// The feature list for the ligature toggle and the `font.features` entries.
/// An entry is `tag` (on), `-tag` (off), or `tag=N`; malformed entries are
/// ignored, and a later entry for a tag replaces an earlier one.
pub fn features(ligatures: bool, entries: &[String]) -> Vec<(String, u32)> {
    let mut features: Vec<(String, u32)> = Vec::new();
    let mut set = |tag: &str, value: u32| match features.iter_mut().find(|(t, _)| t == tag) {
        Some(feature) => feature.1 = value,
        None => features.push((tag.to_owned(), value)),
    };
    if !ligatures {
        set("calt", 0);
        set("liga", 0);
    }
    for entry in entries {
        let entry = entry.trim();
        let (tag, value) = if let Some(tag) = entry.strip_prefix('-') {
            (tag, Some(0))
        } else if let Some((tag, value)) = entry.split_once('=') {
            (tag, value.trim().parse().ok())
        } else {
            (entry, Some(1))
        };
        let tag = tag.trim();
        if let Some(value) = value
            && tag.len() == 4
            && tag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            set(tag, value);
        }
    }
    features
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn entries(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    fn terminal() -> SettingsScope {
        SettingsScope {
            view: Some("terminal".into()),
            ..Default::default()
        }
    }

    #[test]
    fn defaults() {
        let font = FontSettings::resolve(&SettingsStore::default(), &SettingsScope::default());
        assert_eq!(font.family, "Menlo");
        assert_eq!(
            font.fallbacks,
            ["Geeza Pro", "PingFang SC", "Apple Color Emoji"]
        );
        assert_eq!(font.weight, 400.);
        assert!(font.features.is_empty());
    }

    #[test]
    fn scopes_resolve_like_other_settings() {
        let mut store = SettingsStore::default();
        store
            .set(FONT_FAMILY, json!("Fira Code"), SettingsScope::default())
            .unwrap();
        store.set(FONT_FAMILY, json!("Monaco"), terminal()).unwrap();
        store
            .set(
                FONT_WEIGHT,
                json!(300),
                SettingsScope {
                    language: Some("rust".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let editor = SettingsScope {
            language: Some("rust".into()),
            view: Some("editor".into()),
            ..Default::default()
        };
        assert_eq!(FontSettings::resolve(&store, &terminal()).family, "Monaco");
        let resolved = FontSettings::resolve(&store, &editor);
        assert_eq!(
            (resolved.family.as_str(), resolved.weight),
            ("Fira Code", 300.)
        );
    }

    #[test]
    fn weight_is_clamped() {
        let mut store = SettingsStore::default();
        for (value, expected) in [(50, 100.), (1200, 900.)] {
            store
                .set(FONT_WEIGHT, json!(value), SettingsScope::default())
                .unwrap();
            assert_eq!(
                FontSettings::resolve(&store, &SettingsScope::default()).weight,
                expected
            );
        }
    }

    #[test]
    fn feature_entries_parse() {
        assert_eq!(
            features(
                true,
                &entries(&["ss01", "-calt", "cv05=2", "bad", "toolong", "ss02=x", "-"])
            ),
            [("ss01".into(), 1), ("calt".into(), 0), ("cv05".into(), 2)]
        );
    }

    #[test]
    fn the_ligature_toggle_applies_before_features() {
        assert_eq!(
            features(false, &entries(&["liga", "ss01"])),
            [("calt".into(), 0), ("liga".into(), 1), ("ss01".into(), 1)]
        );
        assert_eq!(
            features(false, &[]),
            [("calt".into(), 0), ("liga".into(), 0)]
        );
    }
}
