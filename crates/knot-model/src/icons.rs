//! Icon definitions, packs, and the lookup that names resolve through.
//!
//! An icon is a flat dotted name (`file`, `folder.open`, `file.rust`). For one
//! name the first of these that defines it wins: an override, the active
//! pack, the defaults (the native table, then names extensions define), and
//! finally the generic fallback icon. Overrides and packs are registered
//! by extension lifecycles; the latest registration of a name wins, and
//! removing it reveals the previous one. Defining a name that already has a
//! default is an error, as for color keys.
//!
//! Native defaults name builtin Lucide glyphs. The application turns a
//! definition into painted bytes; this module only decides which definition
//! a name has.

use std::collections::BTreeMap;

use knot_protocol::{ExtensionId, ExtensionLifecycleId};

/// The name of the icon painted for a name no level defines.
pub const FALLBACK: &str = "fallback";

/// Where an icon's drawing comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconSource {
    /// An SVG document.
    Svg(String),
    /// A builtin Lucide glyph, by its kebab-case name.
    Lucide(String),
}

/// How an icon is colored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum IconColor {
    /// The surface's ordinary icon color.
    #[default]
    Surface,
    /// A single color taken from a theme color key.
    Key(String),
    /// The SVG's own colors.
    Original,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconDefinition {
    pub source: IconSource,
    pub color: IconColor,
}

impl IconDefinition {
    /// A builtin Lucide glyph in the surface's color.
    pub fn lucide(name: &str) -> Self {
        Self {
            source: IconSource::Lucide(name.to_owned()),
            color: IconColor::Surface,
        }
    }
}

/// The extension lifecycle that owns a registration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IconOwner {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
}

/// The native default table: each name and the Lucide glyph it paints.
pub const NATIVE_ICONS: &[(&str, &str)] = &[
    ("file", "file"),
    ("folder", "folder"),
    ("folder.open", "folder-open"),
    ("symbol", "box"),
    ("tree.collapsed", "chevron-right"),
    ("tree.expanded", "chevron-down"),
    (FALLBACK, "circle-question-mark"),
];

/// A name that native code or a live lifecycle already defines.
#[derive(Debug, PartialEq, Eq)]
pub struct DuplicateIcon;

struct Override {
    id: u64,
    owner: IconOwner,
    name: String,
    definition: IconDefinition,
}

struct Pack {
    id: u64,
    owner: IconOwner,
    name: String,
    icons: BTreeMap<String, IconDefinition>,
}

#[derive(Default)]
pub struct IconRegistry {
    overrides: Vec<Override>,
    packs: Vec<Pack>,
    defined: Vec<(IconOwner, String, IconDefinition)>,
    active_pack: String,
    next_id: u64,
}

impl IconRegistry {
    /// Define a name at the default level; it must not already have a default.
    pub fn define(
        &mut self,
        owner: IconOwner,
        name: &str,
        definition: IconDefinition,
    ) -> Result<(), DuplicateIcon> {
        if self.default_definition(name).is_some() {
            return Err(DuplicateIcon);
        }
        self.defined.push((owner, name.to_owned(), definition));
        Ok(())
    }

    /// Override a name above every pack, returning the registration's ID.
    pub fn register_override(
        &mut self,
        owner: IconOwner,
        name: &str,
        definition: IconDefinition,
    ) -> u64 {
        self.next_id += 1;
        self.overrides.push(Override {
            id: self.next_id,
            owner,
            name: name.to_owned(),
            definition,
        });
        self.next_id
    }

    /// Remove an owned override; false when it is not live.
    pub fn unregister_override(&mut self, owner: IconOwner, id: u64) -> bool {
        let before = self.overrides.len();
        self.overrides
            .retain(|registration| registration.id != id || registration.owner != owner);
        self.overrides.len() != before
    }

    /// Register a pack; the latest registration of a name defines it.
    pub fn register_pack(
        &mut self,
        owner: IconOwner,
        name: &str,
        icons: BTreeMap<String, IconDefinition>,
    ) -> u64 {
        self.next_id += 1;
        self.packs.push(Pack {
            id: self.next_id,
            owner,
            name: name.to_owned(),
            icons,
        });
        self.next_id
    }

    /// Remove an owned pack registration; false when it is not live.
    pub fn unregister_pack(&mut self, owner: IconOwner, id: u64) -> bool {
        let before = self.packs.len();
        self.packs
            .retain(|registration| registration.id != id || registration.owner != owner);
        self.packs.len() != before
    }

    pub fn remove_lifecycle(&mut self, extension: ExtensionId, lifecycle: ExtensionLifecycleId) {
        let owner = IconOwner {
            extension,
            lifecycle,
        };
        self.overrides
            .retain(|registration| registration.owner != owner);
        self.packs
            .retain(|registration| registration.owner != owner);
        self.defined.retain(|(defined, ..)| *defined != owner);
    }

    /// Select the pack by name; empty selects none. Returns whether the
    /// selection changed.
    pub fn select_pack(&mut self, name: &str) -> bool {
        let changed = self.active_pack != name;
        self.active_pack = name.to_owned();
        changed
    }

    /// Registered pack names, sorted.
    pub fn pack_names(&self) -> Vec<String> {
        let mut names = self
            .packs
            .iter()
            .map(|pack| pack.name.clone())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        names
    }

    fn default_definition(&self, name: &str) -> Option<IconDefinition> {
        NATIVE_ICONS
            .iter()
            .find(|(native, _)| *native == name)
            .map(|(_, lucide)| IconDefinition::lucide(lucide))
            .or_else(|| {
                self.defined
                    .iter()
                    .find(|(_, defined, _)| defined == name)
                    .map(|(_, _, definition)| definition.clone())
            })
    }

    /// The definition `name` has at the first level that defines it.
    pub fn lookup(&self, name: &str) -> Option<IconDefinition> {
        let overridden = self
            .overrides
            .iter()
            .rev()
            .find(|registration| registration.name == name)
            .map(|registration| registration.definition.clone());
        overridden
            .or_else(|| {
                self.packs
                    .iter()
                    .rev()
                    .find(|pack| pack.name == self.active_pack)
                    .and_then(|pack| pack.icons.get(name).cloned())
            })
            .or_else(|| self.default_definition(name))
    }

    /// The icon for `name`, or the generic fallback when nothing defines it.
    pub fn icon(&self, name: &str) -> IconDefinition {
        self.lookup(name)
            .or_else(|| self.lookup(FALLBACK))
            .expect("the fallback icon is native")
    }

    /// The first of `candidates` that has a definition, else the fallback.
    pub fn first_defined(&self, candidates: &[&str]) -> IconDefinition {
        candidates
            .iter()
            .find_map(|name| self.lookup(name))
            .unwrap_or_else(|| self.icon(FALLBACK))
    }

    /// The icon for a file: its language's `file.<id>`, else `file`.
    pub fn file_icon(&self, language: Option<&str>) -> IconDefinition {
        let language = language.map(|id| format!("file.{id}"));
        let candidates = language.iter().map(String::as_str).chain(["file"]);
        self.first_defined(&candidates.collect::<Vec<_>>())
    }

    /// The icon for a directory, in its expanded form when `open`.
    pub fn folder_icon(&self, open: bool) -> IconDefinition {
        let candidates: &[&str] = if open {
            &["folder.open", "folder"]
        } else {
            &["folder"]
        };
        self.first_defined(candidates)
    }
}

#[cfg(test)]
mod tests;
