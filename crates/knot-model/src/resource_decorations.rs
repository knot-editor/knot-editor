//! Resource decorations that extensions push and the surfaces that show
//! resources paint.
//!
//! A source belongs to one extension lifecycle and keeps its own map from
//! resource URI to decoration. When sources decorate one URI, the color and
//! the badge are each taken from the most severe decoration, then from the
//! latest source, and tooltips are joined. A decoration that propagates also
//! colors the ancestors of its URI that lack a color of their own.

use std::collections::HashMap;

use serde::Deserialize;
use unicode_segmentation::UnicodeSegmentation;

use knot_protocol::{ExtensionId, ExtensionLifecycleId};

use crate::{buffer::decoration_precedence, resource::ResourceUri};

/// The longest text badge, in grapheme clusters.
pub const MAX_BADGE_GRAPHEMES: usize = 2;

/// What a badge shows.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Badge {
    /// The name of an icon in the icon registry.
    Icon(String),
    /// One or two grapheme clusters.
    Text(String),
}

/// One source's decoration of one resource.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decoration {
    /// A theme color key that tints the label.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub badge: Option<Badge>,
    #[serde(default)]
    pub tooltip: Option<String>,
    /// Whether the color also tints the ancestors of the resource.
    #[serde(default)]
    pub propagate: bool,
}

impl Decoration {
    /// Whether the decoration is well formed: a text badge holds one or two
    /// grapheme clusters, and an icon badge a name.
    pub fn is_valid(&self) -> bool {
        match &self.badge {
            Some(Badge::Text(text)) => {
                (1..=MAX_BADGE_GRAPHEMES).contains(&text.graphemes(true).count())
            }
            Some(Badge::Icon(name)) => !name.is_empty(),
            None => true,
        }
    }
}

/// The extension lifecycle that owns a source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecorationOwner {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
}

/// What a surface paints for one resource.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolvedDecoration {
    pub color: Option<String>,
    pub badge: Option<Badge>,
    pub tooltip: Option<String>,
}

struct Source {
    id: u64,
    owner: DecorationOwner,
    entries: HashMap<ResourceUri, Decoration>,
}

/// A color or badge candidate: where it ranks, among its competitors.
type Rank = (u8, u64);

#[derive(Default)]
pub struct DecorationRegistry {
    sources: Vec<Source>,
    next_id: u64,
    /// The resolution of every decorated or propagated-to URI, rebuilt when
    /// a source changes so painting costs one lookup per row.
    resolved: HashMap<ResourceUri, ResolvedDecoration>,
}

impl DecorationRegistry {
    /// A new, empty source owned by `owner`, returning its ID.
    pub fn create_source(&mut self, owner: DecorationOwner) -> u64 {
        self.next_id += 1;
        self.sources.push(Source {
            id: self.next_id,
            owner,
            entries: HashMap::new(),
        });
        self.next_id
    }

    /// Change the named URIs of a source, where `None` removes a URI's
    /// decoration. Returns whether anything changed, or `None` when the
    /// owner has no such source.
    pub fn set(
        &mut self,
        owner: DecorationOwner,
        id: u64,
        changes: Vec<(ResourceUri, Option<Decoration>)>,
    ) -> Option<bool> {
        let source = self
            .sources
            .iter_mut()
            .find(|source| source.id == id && source.owner == owner)?;
        let mut changed = false;
        for (uri, decoration) in changes {
            match decoration {
                Some(decoration) => {
                    changed |= source.entries.insert(uri, decoration.clone()) != Some(decoration);
                }
                None => changed |= source.entries.remove(&uri).is_some(),
            }
        }
        if changed {
            self.rebuild();
        }
        Some(changed)
    }

    /// Remove every decoration of a source. Returns whether anything
    /// changed, or `None` when the owner has no such source.
    pub fn clear(&mut self, owner: DecorationOwner, id: u64) -> Option<bool> {
        let source = self
            .sources
            .iter_mut()
            .find(|source| source.id == id && source.owner == owner)?;
        let changed = !source.entries.is_empty();
        source.entries.clear();
        if changed {
            self.rebuild();
        }
        Some(changed)
    }

    /// Remove an owned source with its decorations; false when it is not
    /// live.
    pub fn remove_source(&mut self, owner: DecorationOwner, id: u64) -> bool {
        let before = self.sources.len();
        self.sources
            .retain(|source| source.id != id || source.owner != owner);
        let removed = self.sources.len() != before;
        if removed {
            self.rebuild();
        }
        removed
    }

    /// Remove a lifecycle's sources. Returns whether any decoration went.
    pub fn remove_lifecycle(
        &mut self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
    ) -> bool {
        let owner = DecorationOwner {
            extension,
            lifecycle,
        };
        let had_entries = self
            .sources
            .iter()
            .any(|source| source.owner == owner && !source.entries.is_empty());
        self.sources.retain(|source| source.owner != owner);
        if had_entries {
            self.rebuild();
        }
        had_entries
    }

    /// What to paint for `uri`, or `None` when nothing decorates it.
    pub fn lookup(&self, uri: &ResourceUri) -> Option<&ResolvedDecoration> {
        self.resolved.get(uri)
    }

    fn rebuild(&mut self) {
        struct Pending {
            color: Option<(Rank, String)>,
            badge: Option<(Rank, Badge)>,
            propagated: Option<(Rank, String)>,
            tooltips: Vec<String>,
        }
        fn better<T>(slot: &mut Option<(Rank, T)>, rank: Rank, value: &T)
        where
            T: Clone,
        {
            if slot.as_ref().is_none_or(|(current, _)| rank > *current) {
                *slot = Some((rank, value.clone()));
            }
        }
        let mut pending = HashMap::<ResourceUri, Pending>::new();
        let new = || Pending {
            color: None,
            badge: None,
            propagated: None,
            tooltips: Vec::new(),
        };
        // Sources are in creation order, so tooltips join in that order.
        for source in &self.sources {
            for (uri, decoration) in &source.entries {
                let rank = (
                    decoration.color.as_deref().map_or(0, decoration_precedence),
                    source.id,
                );
                let entry = pending.entry(uri.clone()).or_insert_with(new);
                if let Some(color) = &decoration.color {
                    better(&mut entry.color, rank, color);
                }
                if let Some(badge) = &decoration.badge {
                    better(&mut entry.badge, rank, badge);
                }
                if let Some(tooltip) = &decoration.tooltip {
                    entry.tooltips.push(tooltip.clone());
                }
                if let (true, Some(color)) = (decoration.propagate, &decoration.color) {
                    let mut ancestor = uri.parent();
                    while let Some(parent) = ancestor {
                        let entry = pending.entry(parent.clone()).or_insert_with(new);
                        better(&mut entry.propagated, rank, color);
                        ancestor = parent.parent();
                    }
                }
            }
        }
        self.resolved = pending
            .into_iter()
            .filter_map(|(uri, pending)| {
                let color = pending.color.or(pending.propagated).map(|(_, color)| color);
                let resolved = ResolvedDecoration {
                    color,
                    badge: pending.badge.map(|(_, badge)| badge),
                    tooltip: (!pending.tooltips.is_empty()).then(|| pending.tooltips.join("\n")),
                };
                (resolved != ResolvedDecoration::default()).then_some((uri, resolved))
            })
            .collect();
    }
}

#[cfg(test)]
mod tests;
