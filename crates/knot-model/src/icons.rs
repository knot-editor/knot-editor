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

use serde::Deserialize;

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

/// An icon definition as an extension writes it: exactly one of `svg` and
/// `lucide`, and an optional `color`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IconSpec {
    #[serde(default)]
    svg: Option<String>,
    #[serde(default)]
    lucide: Option<String>,
    #[serde(default)]
    color: Option<String>,
}

impl IconSpec {
    /// The Lucide glyph this spec names, which the caller checks exists.
    pub fn lucide_name(&self) -> Option<&str> {
        self.lucide.as_deref()
    }

    /// The definition, or why the spec is invalid.
    pub fn definition(self) -> Result<IconDefinition, &'static str> {
        let source = match (self.svg, self.lucide) {
            (Some(svg), None) if looks_like_svg(&svg) => IconSource::Svg(svg),
            (Some(_), None) => return Err("icon svg must be an SVG document"),
            (None, Some(name)) if is_glyph_name(&name) => IconSource::Lucide(name),
            (None, Some(_)) => return Err("icon lucide must be a glyph name"),
            _ => return Err("an icon needs exactly one of svg and lucide"),
        };
        let color = match self.color.as_deref() {
            None => IconColor::Surface,
            Some("") => return Err("icon color must be nonempty"),
            Some("original") if matches!(source, IconSource::Svg(_)) => IconColor::Original,
            Some("original") => return Err("only an svg icon can keep its original colors"),
            Some(key) => IconColor::Key(key.to_owned()),
        };
        Ok(IconDefinition { source, color })
    }
}

fn is_glyph_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// A document that begins with an `<svg` element (after an optional XML
/// declaration) and ends with its close.
fn looks_like_svg(text: &str) -> bool {
    let text = text.trim();
    (text.starts_with("<svg") || text.starts_with("<?xml") && text.contains("<svg"))
        && (text.ends_with("</svg>") || text.ends_with("/>"))
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
    ("terminal", "square-terminal"),
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

/// Icon names claimed for file names, extensions (without the dot, any
/// case), and folder names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IconAssociation {
    pub files: BTreeMap<String, String>,
    pub extensions: BTreeMap<String, String>,
    pub folders: BTreeMap<String, String>,
}

struct Association {
    id: u64,
    owner: IconOwner,
    claims: IconAssociation,
}

#[derive(Default)]
pub struct IconRegistry {
    overrides: Vec<Override>,
    associations: Vec<Association>,
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

    /// Claim icon names for file and folder patterns, returning the
    /// registration's ID. The latest claim of a pattern wins.
    pub fn associate(&mut self, owner: IconOwner, claims: IconAssociation) -> u64 {
        self.next_id += 1;
        self.associations.push(Association {
            id: self.next_id,
            owner,
            claims,
        });
        self.next_id
    }

    /// Remove an owned association; false when it is not live.
    pub fn unassociate(&mut self, owner: IconOwner, id: u64) -> bool {
        let before = self.associations.len();
        self.associations
            .retain(|registration| registration.id != id || registration.owner != owner);
        self.associations.len() != before
    }

    pub fn remove_lifecycle(&mut self, extension: ExtensionId, lifecycle: ExtensionLifecycleId) {
        let owner = IconOwner {
            extension,
            lifecycle,
        };
        self.associations
            .retain(|registration| registration.owner != owner);
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
    /// In the expanded form each candidate `X` is tried as `X.open` before `X`.
    pub fn first_defined(&self, candidates: &[&str], open: bool) -> IconDefinition {
        candidates
            .iter()
            .find_map(|name| {
                let opened = open.then(|| self.lookup(&format!("{name}.open"))).flatten();
                opened.or_else(|| self.lookup(name))
            })
            .unwrap_or_else(|| self.icon(FALLBACK))
    }

    /// The latest association that claims a pattern.
    fn claimed(&self, claim: impl Fn(&IconAssociation) -> Option<&String>) -> Option<&str> {
        self.associations
            .iter()
            .rev()
            .find_map(|registration| claim(&registration.claims))
            .map(String::as_str)
    }

    /// The icon for a file: the name its file name or extension is associated
    /// with, its language's `file.<id>`, then `file`; expanded, each also as
    /// `<name>.open` first. A name nothing defines
    /// is skipped. The longest extension claimed wins, so `d.ts` beats `ts`.
    pub fn file_icon(
        &self,
        file_name: Option<&str>,
        language: Option<&str>,
        open: bool,
    ) -> IconDefinition {
        let mut candidates = Vec::new();
        if let Some(name) = file_name {
            candidates.extend(self.claimed(|claims| claims.files.get(name)));
            let lowercase = name.to_lowercase();
            for (index, _) in lowercase.match_indices('.') {
                let extension = &lowercase[index + 1..];
                candidates.extend(self.claimed(|claims| {
                    claims
                        .extensions
                        .iter()
                        .find(|(key, _)| key.to_lowercase() == extension)
                        .map(|(_, icon)| icon)
                }));
            }
        }
        let language = language.map(|id| format!("file.{id}"));
        candidates.extend(language.as_deref());
        candidates.push("file");
        self.first_defined(&candidates, open)
    }

    /// The icon for a directory: the name its folder name is associated
    /// with, then `folder`.
    pub fn folder_icon(&self, name: Option<&str>, open: bool) -> IconDefinition {
        let mut candidates = Vec::new();
        candidates.extend(name.and_then(|name| self.claimed(|claims| claims.folders.get(name))));
        candidates.push("folder");
        self.first_defined(&candidates, open)
    }
}

#[cfg(test)]
mod tests;
