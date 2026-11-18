//! The font a surface draws text with, resolved from the `font.*` settings.

use serde_json::Value;

use crate::settings::{
    FONT_FALLBACKS, FONT_FAMILY, FONT_FEATURES, FONT_LIGATURES, FONT_LINE_HEIGHT, FONT_WEIGHT,
    SettingsScope, SettingsStore,
};

/// The default primary family: the platform's standard code font, as in
/// VS Code.
pub const DEFAULT_FAMILY: &str = if cfg!(target_os = "macos") {
    "Menlo"
} else if cfg!(target_os = "windows") {
    "Consolas"
} else {
    "DejaVu Sans Mono"
};

/// The default fallbacks: other common code fonts, then the platform fonts
/// for Arabic and Hebrew, CJK, and emoji.
pub const DEFAULT_FALLBACKS: &[&str] = if cfg!(target_os = "macos") {
    &[
        "Monaco",
        "Courier New",
        "Geeza Pro",
        "PingFang SC",
        "Apple Color Emoji",
    ]
} else if cfg!(target_os = "windows") {
    &[
        "Courier New",
        "Segoe UI",
        "Microsoft YaHei",
        "Segoe UI Emoji",
    ]
} else {
    &[
        "Droid Sans Mono",
        "Liberation Mono",
        "Noto Sans Arabic",
        "Noto Sans CJK SC",
        "Noto Color Emoji",
    ]
};

/// The default `font.lineHeight` for the terminal scope, matching Ghostty's
/// stock look; every other scope defaults to 1.2.
pub const TERMINAL_LINE_HEIGHT: f32 = 1.0;

/// The default `font.lineHeight` outside the terminal scope.
pub const DEFAULT_LINE_HEIGHT: f32 = 1.2;

/// The resolved font choice of one settings scope.
#[derive(Clone, Debug, PartialEq)]
pub struct FontSettings {
    pub family: String,
    pub fallbacks: Vec<String>,
    /// Base weight, 100 to 900.
    pub weight: f32,
    /// Row height as a multiple of the font's ascent plus descent, 0.8 to 3.
    pub line_height: f32,
    /// Whether `font.ligatures` lets ligatures form.
    pub ligatures: bool,
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
                .unwrap_or(DEFAULT_FAMILY)
                .to_owned(),
            fallbacks: strings(FONT_FALLBACKS),
            weight: store
                .get(FONT_WEIGHT, scope)
                .and_then(Value::as_f64)
                .unwrap_or(400.)
                .clamp(100., 900.) as f32,
            line_height: store
                .find(FONT_LINE_HEIGHT, scope)
                .and_then(Value::as_f64)
                .unwrap_or(default_line_height(scope))
                .clamp(0.8, 3.) as f32,
            ligatures,
            features: features(ligatures, &strings(FONT_FEATURES)),
        }
    }
}

/// The `font.lineHeight` fallback for `scope`: an explicit value at any
/// matching scope wins, so only the terminal scope's own default applies here.
fn default_line_height(scope: &SettingsScope) -> f64 {
    if scope.view.as_deref() == Some("terminal") {
        f64::from(TERMINAL_LINE_HEIGHT)
    } else {
        f64::from(DEFAULT_LINE_HEIGHT)
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
        assert_eq!(font.family, DEFAULT_FAMILY);
        assert_eq!(font.fallbacks, DEFAULT_FALLBACKS);
        assert_eq!(font.weight, 400.);
        assert_eq!(font.line_height, 1.2);
        assert!(font.ligatures);
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
    fn terminal_line_height_defaults_to_one() {
        let store = SettingsStore::default();
        assert_eq!(
            FontSettings::resolve(&store, &terminal()).line_height,
            TERMINAL_LINE_HEIGHT
        );
        assert_eq!(
            FontSettings::resolve(&store, &SettingsScope::default()).line_height,
            DEFAULT_LINE_HEIGHT
        );
        assert_eq!(
            FontSettings::resolve(
                &store,
                &SettingsScope {
                    view: Some("editor".into()),
                    ..Default::default()
                }
            )
            .line_height,
            DEFAULT_LINE_HEIGHT
        );
    }

    #[test]
    fn explicit_line_height_wins_in_the_terminal_scope() {
        let mut store = SettingsStore::default();
        store
            .set(FONT_LINE_HEIGHT, json!(1.5), SettingsScope::default())
            .unwrap();
        assert_eq!(
            FontSettings::resolve(&store, &terminal()).line_height,
            1.5
        );
        store.set(FONT_LINE_HEIGHT, json!(1.4), terminal()).unwrap();
        assert_eq!(
            FontSettings::resolve(&store, &terminal()).line_height,
            1.4
        );
        assert_eq!(
            FontSettings::resolve(&store, &SettingsScope::default()).line_height,
            1.5
        );
    }

    #[test]
    fn line_height_is_clamped() {
        let mut store = SettingsStore::default();
        for (value, expected) in [(0.5, 0.8), (1.5, 1.5), (4., 3.)] {
            store
                .set(FONT_LINE_HEIGHT, json!(value), SettingsScope::default())
                .unwrap();
            assert_eq!(
                FontSettings::resolve(&store, &SettingsScope::default()).line_height,
                expected
            );
            store.set(FONT_LINE_HEIGHT, json!(value), terminal()).unwrap();
            assert_eq!(
                FontSettings::resolve(&store, &terminal()).line_height,
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
