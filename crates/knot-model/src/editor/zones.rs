//! Editor zones: toolbars over layouts of views shown in rows reserved
//! between an editor's lines. Each belongs to the lifecycle that opened it and
//! is named by an ID that lifecycle chose. The wrap map holds a zone's line and
//! row count under an opaque key; this model owns what the key stands for.
//! Presentation state, never recorded.

use knot_protocol::ExtensionKey;

/// What a zone shows where.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZoneSpec {
    /// The buffer line the zone sits above; the line count means below the
    /// last line, and larger lines clamp to it.
    pub line: usize,
    /// Whether the zone's focused member takes focus when it opens.
    pub focus: bool,
    /// The theme color key of the strip the gutter shows beside the zone's
    /// rows, if any.
    pub gutter: Option<String>,
}

/// One open zone.
#[derive(Clone, Debug)]
pub struct EditorZone {
    pub owner: ExtensionKey,
    pub id: String,
    pub spec: ZoneSpec,
    /// The key the wrap map knows the zone by.
    pub key: u64,
    /// The key context `zone:<id>` it holds while open.
    context: String,
}

/// A zone Knot closed, not yet reported to the lifecycle that opened it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosedZone {
    pub owner: ExtensionKey,
    pub id: String,
}

/// The zones of one editor, in opening order.
#[derive(Debug, Default)]
pub struct EditorZones {
    zones: Vec<EditorZone>,
    next_key: u64,
    closed: Vec<ClosedZone>,
}

impl EditorZones {
    pub fn zones(&self) -> &[EditorZone] {
        &self.zones
    }

    /// The key contexts of the open zones.
    pub fn contexts(&self) -> impl Iterator<Item = &str> {
        self.zones.iter().map(|zone| zone.context.as_str())
    }

    pub fn get(&self, owner: ExtensionKey, id: &str) -> Option<&EditorZone> {
        self.zones
            .iter()
            .find(|zone| zone.owner == owner && zone.id == id)
    }

    pub fn by_key(&self, key: u64) -> Option<&EditorZone> {
        self.zones.iter().find(|zone| zone.key == key)
    }

    /// Open `owner`'s zone `id`, or replace what it shows in place, keeping
    /// its key and place in the order. Returns the zone's key.
    pub fn open(&mut self, owner: ExtensionKey, id: &str, spec: ZoneSpec) -> u64 {
        if let Some(zone) = self
            .zones
            .iter_mut()
            .find(|zone| zone.owner == owner && zone.id == id)
        {
            zone.spec = spec;
            return zone.key;
        }
        let key = self.next_key;
        self.next_key += 1;
        self.zones.push(EditorZone {
            owner,
            id: id.to_owned(),
            spec,
            key,
            context: format!("zone:{id}"),
        });
        key
    }

    /// Close `owner`'s zone `id`, returning its key; none when it has none.
    pub fn close(&mut self, owner: ExtensionKey, id: &str) -> Option<u64> {
        let at = self
            .zones
            .iter()
            .position(|zone| zone.owner == owner && zone.id == id)?;
        Some(self.zones.remove(at).key)
    }

    /// Close the zone `key` because the user dismissed it, to report to its
    /// owner; returns whether there was one.
    pub fn dismiss(&mut self, key: u64) -> bool {
        let Some(at) = self.zones.iter().position(|zone| zone.key == key) else {
            return false;
        };
        let zone = self.zones.remove(at);
        self.closed.push(ClosedZone {
            owner: zone.owner,
            id: zone.id,
        });
        true
    }

    /// Close every zone `owner` opened, returning their keys.
    pub fn remove_owner(&mut self, owner: ExtensionKey) -> Vec<u64> {
        let mut keys = Vec::new();
        self.zones.retain(|zone| {
            let kept = zone.owner != owner;
            if !kept {
                keys.push(zone.key);
            }
            kept
        });
        keys
    }

    /// Close every zone, as the editor goes away.
    pub fn release(&mut self) {
        self.zones.clear();
    }

    /// The zones Knot closed since this was last called.
    pub fn take_closed(&mut self) -> Vec<ClosedZone> {
        std::mem::take(&mut self.closed)
    }
}

#[cfg(test)]
mod tests {
    use knot_protocol::{ExtensionId, ExtensionLifecycleId};

    use super::*;

    fn owner(id: u64) -> ExtensionKey {
        ExtensionKey::new(ExtensionId::new(id), ExtensionLifecycleId::new(1))
    }

    fn spec(line: usize) -> ZoneSpec {
        ZoneSpec {
            line,
            focus: false,
            gutter: None,
        }
    }

    fn ids(zones: &EditorZones) -> Vec<(ExtensionKey, &str)> {
        zones
            .zones()
            .iter()
            .map(|zone| (zone.owner, zone.id.as_str()))
            .collect()
    }

    #[test]
    fn opening_again_updates_in_place_and_keeps_the_key_and_order() {
        let mut zones = EditorZones::default();
        let a = zones.open(owner(1), "a", spec(2));
        let b = zones.open(owner(1), "b", spec(3));
        assert_ne!(a, b);
        let again = zones.open(
            owner(1),
            "a",
            ZoneSpec {
                line: 5,
                focus: true,
                gutter: None,
            },
        );
        assert_eq!(again, a);
        assert_eq!(ids(&zones), [(owner(1), "a"), (owner(1), "b")]);
        assert_eq!(zones.get(owner(1), "a").unwrap().spec.line, 5);
        // The same ID under another owner is another zone.
        assert_ne!(zones.open(owner(2), "a", spec(0)), a);
        assert_eq!(zones.by_key(b).unwrap().id, "b");
    }

    #[test]
    fn closing_by_owner_or_id_reports_nothing() {
        let mut zones = EditorZones::default();
        let a = zones.open(owner(1), "a", spec(0));
        let b = zones.open(owner(2), "a", spec(0));
        let c = zones.open(owner(2), "b", spec(0));
        assert_eq!(zones.close(owner(1), "a"), Some(a));
        assert_eq!(zones.close(owner(1), "a"), None);
        assert_eq!(zones.remove_owner(owner(2)), [b, c]);
        assert!(zones.zones().is_empty());
        assert!(zones.take_closed().is_empty());
    }

    #[test]
    fn dismissing_reports_the_zone_to_its_owner_once() {
        let mut zones = EditorZones::default();
        let a = zones.open(owner(1), "a", spec(0));
        zones.open(owner(1), "b", spec(0));
        assert!(zones.dismiss(a));
        assert!(!zones.dismiss(a));
        assert_eq!(
            zones.take_closed(),
            [ClosedZone {
                owner: owner(1),
                id: "a".into()
            }]
        );
        assert!(zones.take_closed().is_empty());
        zones.release();
        assert!(zones.zones().is_empty());
    }
}
