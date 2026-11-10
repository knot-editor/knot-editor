//! Application settings: one in-memory store of values that personal config
//! and extensions write and native views read.
//!
//! Every writer shares the same entries, so a later write to a key and scope
//! replaces an earlier one regardless of who made it, and lifecycle teardown
//! removes no values. A scope is a set of facts that must all match the
//! context of a read; resolution picks the matching entry with the most facts
//! and breaks ties by the latest write. When nothing matches, the definition's
//! default applies.
//!
//! A definition gives a key its type, default, and description. Native keys
//! are defined when the store is created; extension definitions belong to the
//! lifecycle that made them. Values written before their key is defined are
//! kept as raw JSON and validated when the definition arrives. An undefined key
//! is inert: its values are kept but resolve to nothing.

use std::collections::HashMap;

use serde_json::Value;

use knot_protocol::{ExtensionId, ExtensionLifecycleId};

/// Base text size in pixels of editors, text frames, and terminals.
pub const FONT_SIZE: &str = "font.size";
/// Columns per tab stop in editor views.
pub const EDITOR_TAB_SIZE: &str = "editor.tabSize";
/// Whether indenting inserts spaces rather than a tab, unless the document's
/// own indentation shows otherwise.
pub const EDITOR_INSERT_SPACES: &str = "editor.insertSpaces";
/// Whether typing an opening bracket or quote inserts its closer, and typing
/// or deleting that closer steps over or deletes the pair.
pub const EDITOR_AUTO_CLOSING_BRACKETS: &str = "editor.autoClosingBrackets";
/// Whether typing an opening bracket or quote over a selection wraps it.
pub const EDITOR_AUTO_SURROUND: &str = "editor.autoSurround";
/// Whether editor views wrap long lines at the viewport width.
pub const EDITOR_SOFT_WRAP: &str = "editor.softWrap";
/// Line numbers in the editor gutter: `on`, `off`, or `relative`.
pub const EDITOR_LINE_NUMBERS: &str = "editor.lineNumbers";
/// Whether the editor gutter has a fold lane.
pub const EDITOR_FOLDING: &str = "editor.folding";
/// When the fold lane shows chevrons on foldable lines: `mouseover` or `always`.
pub const EDITOR_SHOW_FOLDING_CONTROLS: &str = "editor.showFoldingControls";
/// Whether saving trims trailing whitespace: `off` or `all`.
pub const FILES_TRIM_TRAILING_WHITESPACE: &str = "files.trimTrailingWhitespace";
/// Whether saving ends a non-empty file with a line terminator.
pub const FILES_INSERT_FINAL_NEWLINE: &str = "files.insertFinalNewline";
/// Which appearance's theme applies: `dark`, `light`, or `system`.
pub const THEME_MODE: &str = "theme.mode";
/// The theme name used in dark mode.
pub const THEME_DARK: &str = "theme.dark";
/// The theme name used in light mode.
pub const THEME_LIGHT: &str = "theme.light";
/// The icon pack that restyles icons; empty means no pack.
pub const ICONS_PACK: &str = "icons.pack";
/// The keymap preset that supplies default bindings.
pub const KEYMAP_PRESET: &str = "keymap.preset";
/// Whether documents browsed from the explorer and search open as previews.
pub const WORKBENCH_PREVIEW_TABS: &str = "workbench.previewTabs";
/// Where the bottom dock sits: `contained` between the side docks or `full` width.
pub const WORKBENCH_BOTTOM_DOCK_LAYOUT: &str = "workbench.bottomDockLayout";
/// Whether product windows show the tool stripes on the left and right edges.
pub const WORKBENCH_TOOL_STRIPES: &str = "workbench.toolStripes";
/// Whether product windows show the status bar.
pub const STATUS_BAR_VISIBLE: &str = "statusBar.visible";
/// IDs of status items the bar skips.
pub const STATUS_BAR_HIDDEN_ITEMS: &str = "statusBar.hiddenItems";

/// The match styles the picker, input completion, and `completion.match` use,
/// in order: `prefix`, `substring`, `flex`, or `orderless`.
pub const COMPLETION_STYLES: &str = "completion.styles";

/// The value type a definition accepts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingType {
    Boolean,
    Integer,
    Number,
    String,
    /// A list of strings.
    Strings,
}

impl SettingType {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "boolean" => Self::Boolean,
            "integer" => Self::Integer,
            "number" => Self::Number,
            "string" => Self::String,
            "strings" => Self::Strings,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::String => "string",
            Self::Strings => "strings",
        }
    }

    fn accepts(self, value: &Value) -> bool {
        match self {
            Self::Boolean => value.is_boolean(),
            Self::Integer => value.is_i64(),
            Self::Number => value.is_number(),
            Self::String => value.is_string(),
            Self::Strings => value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingDefinition {
    pub key: String,
    pub value_type: SettingType,
    pub default: Value,
    pub description: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingOwner {
    Native,
    Extension {
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
    },
}

/// Facts about where a value applies, or about the context reading it. An
/// absent fact in a write's scope matches any context; an absent fact in a
/// read's context matches only scopes that leave it absent too.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SettingsScope {
    /// A document's effective language ID.
    pub language: Option<String>,
    /// A native surface kind (`editor`, `terminal`) or a contributed view ID.
    pub view: Option<String>,
    /// A document's UUID, as text. Entries carrying it belong to the document
    /// and are dropped when it is released.
    pub document: Option<String>,
}

impl SettingsScope {
    fn fact_count(&self) -> usize {
        usize::from(self.language.is_some())
            + usize::from(self.view.is_some())
            + usize::from(self.document.is_some())
    }

    /// The scope's facts for a message, `every context` when it has none.
    pub fn describe(&self) -> String {
        let facts = [
            ("language", &self.language),
            ("view", &self.view),
            ("document", &self.document),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.as_ref().map(|value| format!("{name} {value}")))
        .collect::<Vec<_>>();
        if facts.is_empty() {
            "every context".into()
        } else {
            facts.join(", ")
        }
    }

    fn matches(&self, context: &SettingsScope) -> bool {
        fn fact(scope: &Option<String>, context: &Option<String>) -> bool {
            scope.is_none() || scope == context
        }
        fact(&self.language, &context.language)
            && fact(&self.view, &context.view)
            && fact(&self.document, &context.document)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingsError {
    /// The key is already defined by native code or a live lifecycle.
    DuplicateSetting,
    /// A definition's default, or a write to a defined key, has the wrong type.
    TypeMismatch { expected: SettingType },
}

struct Definition {
    definition: SettingDefinition,
    owner: SettingOwner,
}

struct Entry {
    scope: SettingsScope,
    value: Value,
    sequence: u64,
}

pub struct SettingsStore {
    definitions: HashMap<String, Definition>,
    entries: HashMap<String, Vec<Entry>>,
    next_sequence: u64,
}

impl Default for SettingsStore {
    fn default() -> Self {
        let mut store = Self {
            definitions: HashMap::new(),
            entries: HashMap::new(),
            next_sequence: 0,
        };
        for definition in native_definitions() {
            store
                .define(SettingOwner::Native, definition)
                .expect("native setting definitions are valid and distinct");
        }
        store
    }
}

fn native_definitions() -> Vec<SettingDefinition> {
    let text = |key: &str, default: &str, description: &str| SettingDefinition {
        key: key.into(),
        value_type: SettingType::String,
        default: default.into(),
        description: description.into(),
    };
    vec![
        SettingDefinition {
            key: FONT_SIZE.into(),
            value_type: SettingType::Number,
            default: 14.into(),
            description: "Text size in pixels; scope it to { view: \"terminal\" } to size terminals separately".into(),
        },
        SettingDefinition {
            key: EDITOR_TAB_SIZE.into(),
            value_type: SettingType::Integer,
            default: 4.into(),
            description: "Columns per tab stop in editor views".into(),
        },
        SettingDefinition {
            key: EDITOR_INSERT_SPACES.into(),
            value_type: SettingType::Boolean,
            default: true.into(),
            description: "Indent with spaces rather than tabs; a document whose text shows a clear style gets its own value".into(),
        },
        SettingDefinition {
            key: EDITOR_AUTO_CLOSING_BRACKETS.into(),
            value_type: SettingType::Boolean,
            default: true.into(),
            description: "Insert the closer of a typed bracket or quote where the language allows, and step over or delete it with the opener".into(),
        },
        SettingDefinition {
            key: EDITOR_AUTO_SURROUND.into(),
            value_type: SettingType::Boolean,
            default: true.into(),
            description: "Wrap the selection when typing an opening bracket or quote over it".into(),
        },
        SettingDefinition {
            key: EDITOR_SOFT_WRAP.into(),
            value_type: SettingType::Boolean,
            default: false.into(),
            description: "Wrap long lines at the viewport width; Toggle Soft Wrap sets it for one document".into(),
        },
        text(
            EDITOR_LINE_NUMBERS,
            "on",
            "Line numbers in the editor gutter: \"on\", \"off\", or \"relative\" to number lines by their distance from the cursor",
        ),
        SettingDefinition {
            key: EDITOR_FOLDING.into(),
            value_type: SettingType::Boolean,
            default: true.into(),
            description: "Show the fold lane in the editor gutter".into(),
        },
        text(
            EDITOR_SHOW_FOLDING_CONTROLS,
            "mouseover",
            "When the fold lane shows chevrons on foldable lines: \"mouseover\" while the pointer is over the gutter, or \"always\"; collapsed folds always show one",
        ),
        text(
            FILES_TRIM_TRAILING_WHITESPACE,
            "all",
            "Trim trailing spaces and tabs from every line except the cursor's when saving: \"all\" or \"off\"",
        ),
        SettingDefinition {
            key: FILES_INSERT_FINAL_NEWLINE.into(),
            value_type: SettingType::Boolean,
            default: true.into(),
            description: "End a non-empty file with a line terminator when saving".into(),
        },
        text(
            THEME_MODE,
            "dark",
            "Which theme applies: \"dark\", \"light\", or \"system\" to follow the system appearance",
        ),
        text(THEME_DARK, "One Dark", "The theme used in dark mode"),
        text(THEME_LIGHT, "One Light", "The theme used in light mode"),
        text(
            ICONS_PACK,
            "",
            "The name of the icon pack that restyles icons; empty uses the default icons",
        ),
        text(
            KEYMAP_PRESET,
            "knot",
            "The keymap preset that supplies default key bindings",
        ),
        SettingDefinition {
            key: WORKBENCH_PREVIEW_TABS.into(),
            value_type: SettingType::Boolean,
            default: true.into(),
            description: "Open documents browsed from the explorer or search as preview tabs that the next preview replaces".into(),
        },
        text(
            WORKBENCH_BOTTOM_DOCK_LAYOUT,
            "contained",
            "Where the bottom dock sits: \"contained\" under the center between the side docks, or \"full\" across the window width under them",
        ),
        SettingDefinition {
            key: WORKBENCH_TOOL_STRIPES.into(),
            value_type: SettingType::Boolean,
            default: true.into(),
            description: "Show a stripe of tool buttons on the left and right window edges".into(),
        },
        SettingDefinition {
            key: STATUS_BAR_VISIBLE.into(),
            value_type: SettingType::Boolean,
            default: true.into(),
            description: "Show the status bar below the workbench".into(),
        },
        SettingDefinition {
            key: STATUS_BAR_HIDDEN_ITEMS.into(),
            value_type: SettingType::Strings,
            default: Value::Array(Vec::new()),
            description: "IDs of status bar items the bar skips; hidden items keep running".into(),
        },
        SettingDefinition {
            key: COMPLETION_STYLES.into(),
            value_type: SettingType::Strings,
            default: Value::Array(vec!["prefix".into(), "flex".into()]),
            description: "How pickers and completion lists match, in order: prefix, substring, flex, or orderless".into(),
        },
    ]
}

impl SettingsStore {
    /// Define a key and validate the values already stored for it. Returns the
    /// scopes of values dropped because they do not match the type.
    pub fn define(
        &mut self,
        owner: SettingOwner,
        definition: SettingDefinition,
    ) -> Result<Vec<SettingsScope>, SettingsError> {
        if self.definitions.contains_key(&definition.key) {
            return Err(SettingsError::DuplicateSetting);
        }
        let value_type = definition.value_type;
        if !value_type.accepts(&definition.default) {
            return Err(SettingsError::TypeMismatch {
                expected: value_type,
            });
        }
        let mut dropped = Vec::new();
        if let Some(entries) = self.entries.get_mut(&definition.key) {
            entries.retain(|entry| {
                let keep = value_type.accepts(&entry.value);
                if !keep {
                    dropped.push(entry.scope.clone());
                }
                keep
            });
        }
        self.definitions
            .insert(definition.key.clone(), Definition { definition, owner });
        Ok(dropped)
    }

    /// Remove every definition one lifecycle made, keeping their values.
    pub fn remove_lifecycle(
        &mut self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
    ) -> bool {
        let before = self.definitions.len();
        let owner = SettingOwner::Extension {
            extension,
            lifecycle,
        };
        self.definitions
            .retain(|_, definition| definition.owner != owner);
        self.definitions.len() != before
    }

    pub fn definition(&self, key: &str) -> Option<&SettingDefinition> {
        self.definitions
            .get(key)
            .map(|definition| &definition.definition)
    }

    /// Whether one lifecycle owns any definition.
    pub fn has_lifecycle(&self, extension: ExtensionId, lifecycle: ExtensionLifecycleId) -> bool {
        let owner = SettingOwner::Extension {
            extension,
            lifecycle,
        };
        self.definitions
            .values()
            .any(|definition| definition.owner == owner)
    }

    /// Check that `value` fits `key`'s definition, if it has one.
    pub fn check(&self, key: &str, value: &Value) -> Result<(), SettingsError> {
        match self.definitions.get(key) {
            Some(definition) if !definition.definition.value_type.accepts(value) => {
                Err(SettingsError::TypeMismatch {
                    expected: definition.definition.value_type,
                })
            }
            _ => Ok(()),
        }
    }

    /// Write the value for exactly `key` and `scope`, as the latest write.
    pub fn set(
        &mut self,
        key: &str,
        value: Value,
        scope: SettingsScope,
    ) -> Result<(), SettingsError> {
        self.check(key, &value)?;
        self.next_sequence += 1;
        let entries = self.entries.entry(key.to_owned()).or_default();
        entries.retain(|entry| entry.scope != scope);
        entries.push(Entry {
            scope,
            value,
            sequence: self.next_sequence,
        });
        Ok(())
    }

    /// Write every value in `values` at `scope`, or none: the first value
    /// that does not fit its definition rejects the whole call, naming its
    /// key.
    pub fn set_all(
        &mut self,
        values: Vec<(String, Value)>,
        scope: &SettingsScope,
    ) -> Result<Vec<String>, (String, SettingsError)> {
        for (key, value) in &values {
            self.check(key, value)
                .map_err(|error| (key.clone(), error))?;
        }
        let mut keys = Vec::with_capacity(values.len());
        for (key, value) in values {
            self.set(&key, value, scope.clone())
                .expect("checked setting value");
            keys.push(key);
        }
        Ok(keys)
    }

    /// Remove the value for exactly `key` and `scope`. Returns false when
    /// there was none.
    pub fn reset(&mut self, key: &str, scope: &SettingsScope) -> bool {
        let Some(entries) = self.entries.get_mut(key) else {
            return false;
        };
        let before = entries.len();
        entries.retain(|entry| &entry.scope != scope);
        entries.len() != before
    }

    /// Drop every entry scoped to the document with this UUID.
    pub fn release_document(&mut self, document: &str) {
        for entries in self.entries.values_mut() {
            entries.retain(|entry| entry.scope.document.as_deref() != Some(document));
        }
    }

    /// The value of a defined `key` for `context`, or its default when no
    /// entry matches. `None` for an undefined key.
    pub fn get(&self, key: &str, context: &SettingsScope) -> Option<&Value> {
        let definition = &self.definitions.get(key)?.definition;
        let resolved = self
            .entries
            .get(key)
            .into_iter()
            .flatten()
            .filter(|entry| entry.scope.matches(context))
            .max_by_key(|entry| (entry.scope.fact_count(), entry.sequence));
        Some(resolved.map_or(&definition.default, |entry| &entry.value))
    }
}

/// A JSON value from JavaScript, whose numbers all arrive as doubles, with
/// integral numbers restored as integers so they satisfy `integer`.
pub fn restore_integers(value: Value) -> Value {
    match value {
        Value::Number(number) => match number.as_f64() {
            Some(float) if float.fract() == 0.0 && float.abs() <= 9_007_199_254_740_991.0 => {
                (float as i64).into()
            }
            _ => Value::Number(number),
        },
        Value::Array(items) => items.into_iter().map(restore_integers).collect(),
        Value::Object(fields) => fields
            .into_iter()
            .map(|(key, value)| (key, restore_integers(value)))
            .collect(),
        other => other,
    }
}

#[cfg(test)]
mod tests;
