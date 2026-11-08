//! Keymap presets: named sets of default bindings that occupy the preset
//! slot beneath every extension and personal-config binding.
//!
//! The Knot preset is native data the registry receives, so editing works
//! before packages start and when they fail. Extensions and personal config
//! register further presets by name; the latest registration of a name
//! defines it, and removing it reveals the previous one. An unregistered
//! selection selects Knot.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::canonical_key;
use knot_protocol::{Command, CommandArgumentValue, ExtensionId, ExtensionLifecycleId};

/// The name of the native preset, which registrations cannot replace.
pub const KNOT_PRESET: &str = "knot";

/// One preset binding; a missing command unbinds the key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresetBinding {
    pub key: String,
    pub command: Option<Command>,
    #[serde(default)]
    pub view: Option<String>,
}

impl PresetBinding {
    pub fn new(key: String, command: Option<&str>, view: Option<&str>) -> Self {
        Self {
            key,
            command: command.map(|name| Command {
                name: name.into(),
                arguments: CommandArgumentValue::Null,
            }),
            view: view.map(ToOwned::to_owned),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeymapPreset {
    pub name: String,
    pub title: String,
    pub bindings: Vec<PresetBinding>,
}

impl KeymapPreset {
    /// Why the definition is invalid, if it is.
    pub fn invalid_value(&self) -> Option<&'static str> {
        if self.name.is_empty() {
            return Some("keymap preset name must be nonempty");
        }
        if self.name == KNOT_PRESET {
            return Some("the knot keymap preset cannot be replaced");
        }
        if self
            .bindings
            .iter()
            .any(|binding| canonical_key(&binding.key).is_err())
        {
            return Some("keymap preset binding has an invalid key sequence");
        }
        if self.bindings.iter().any(|binding| {
            binding
                .command
                .as_ref()
                .is_some_and(|command| command.name.as_ref().is_empty())
        }) {
            return Some("keymap preset binding command name is empty");
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeymapPresetInfo {
    pub name: String,
    pub title: String,
}

/// The extension lifecycle that owns a registration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetOwner {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
}

struct Registration {
    id: u64,
    owner: PresetOwner,
    preset: KeymapPreset,
}

pub struct PresetRegistry {
    /// The native Knot preset's bindings.
    knot: Vec<PresetBinding>,
    /// In registration order, so a later index wins.
    registrations: Vec<Registration>,
    next_id: u64,
    /// The selection the installed preset was resolved from.
    applied: Option<String>,
}

impl PresetRegistry {
    /// A registry whose native Knot preset binds `knot`.
    pub fn new(knot: Vec<PresetBinding>) -> Self {
        Self {
            knot,
            registrations: Vec::new(),
            next_id: 0,
            applied: None,
        }
    }

    /// Add a validated registration, returning its ID.
    pub fn register(&mut self, owner: PresetOwner, preset: KeymapPreset) -> u64 {
        self.next_id += 1;
        self.registrations.push(Registration {
            id: self.next_id,
            owner,
            preset,
        });
        self.next_id
    }

    /// Remove an owned registration; false when it is not live.
    pub fn unregister(&mut self, owner: PresetOwner, id: u64) -> bool {
        let before = self.registrations.len();
        self.registrations
            .retain(|registration| registration.id != id || registration.owner != owner);
        self.registrations.len() != before
    }

    pub fn remove_lifecycle(&mut self, owner: PresetOwner) {
        self.registrations
            .retain(|registration| registration.owner != owner);
    }

    /// The preset `name` selects: the native Knot preset or the latest
    /// registration of the name.
    pub fn get(&self, name: &str) -> Option<KeymapPreset> {
        if name == KNOT_PRESET {
            return Some(KeymapPreset {
                name: KNOT_PRESET.into(),
                title: "Knot".into(),
                bindings: self.knot.clone(),
            });
        }
        self.registrations
            .iter()
            .rev()
            .map(|registration| &registration.preset)
            .find(|preset| preset.name == name)
            .cloned()
    }

    /// Knot and each registered name's effective preset, sorted by name.
    pub fn list(&self) -> Vec<KeymapPresetInfo> {
        let mut presets = BTreeMap::from([(KNOT_PRESET.to_owned(), "Knot".to_owned())]);
        for registration in &self.registrations {
            presets.insert(
                registration.preset.name.clone(),
                registration.preset.title.clone(),
            );
        }
        presets
            .into_iter()
            .map(|(name, title)| KeymapPresetInfo { name, title })
            .collect()
    }

    /// The title of the installed preset.
    pub fn applied_title(&self) -> String {
        self.applied
            .as_deref()
            .and_then(|name| self.get(name))
            .or_else(|| self.get(KNOT_PRESET))
            .map(|preset| preset.title)
            .unwrap_or_default()
    }

    /// Select `selection` and return the bindings it names, falling back to
    /// Knot; nothing when it is already selected and `force` is false.
    pub fn select(&mut self, selection: String, force: bool) -> Option<Vec<PresetBinding>> {
        if !force && self.applied.as_ref() == Some(&selection) {
            return None;
        }
        let bindings = self
            .get(&selection)
            .or_else(|| self.get(KNOT_PRESET))
            .map(|preset| preset.bindings)
            .unwrap_or_default();
        self.applied = Some(selection);
        Some(bindings)
    }
}

/// The operating system whose conventions a preset follows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}
