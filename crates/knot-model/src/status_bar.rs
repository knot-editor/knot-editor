//! Status bar items that extensions register and the bar a window shows from
//! them.
//!
//! An ID names one item. Several lifecycles may register it, the latest
//! registration defines it, and removing that one reveals the previous, as
//! for languages and themes. A registration holds one application-wide value
//! and may hold a replacement for individual windows.

use std::collections::HashMap;

use serde::Deserialize;

use knot_protocol::{CommandArgumentValue, ExtensionId, ExtensionLifecycleId};

/// Which end of the bar an item sits at.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusSide {
    #[default]
    Left,
    Right,
}

/// A command a click on an item runs.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusCommand {
    pub name: String,
    #[serde(default = "no_arguments")]
    pub arguments: CommandArgumentValue,
}

fn no_arguments() -> CommandArgumentValue {
    CommandArgumentValue::Null
}

/// What the bar shows for one item.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusItem {
    pub text: String,
    /// An icon painted before the text, in `color` when one is set.
    #[serde(default)]
    pub icon: Option<knot_protocol::IconRef>,
    #[serde(default)]
    pub tooltip: Option<String>,
    #[serde(default)]
    pub command: Option<StatusCommand>,
    /// A theme color key for the text.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub side: StatusSide,
    /// Higher values sit further left.
    #[serde(default)]
    pub priority: i64,
}

/// The extension lifecycle that owns a registration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatusOwner {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
}

struct Registration {
    id: u64,
    owner: StatusOwner,
    item_id: String,
    value: StatusItem,
    /// Replacements for single windows, by window ID.
    windows: HashMap<String, StatusItem>,
}

/// An item the bar shows.
#[derive(Clone, Debug, PartialEq)]
pub struct BarItem {
    pub id: String,
    pub item: StatusItem,
}

/// The items of one window's bar, in display order.
#[derive(Debug, Default, PartialEq)]
pub struct BarItems {
    pub left: Vec<BarItem>,
    pub right: Vec<BarItem>,
}

#[derive(Default)]
pub struct StatusRegistry {
    registrations: Vec<Registration>,
    next_id: u64,
}

impl StatusRegistry {
    /// Register `item_id` with its application-wide value, returning the
    /// registration's ID.
    pub fn register(&mut self, owner: StatusOwner, item_id: String, value: StatusItem) -> u64 {
        self.next_id += 1;
        self.registrations.push(Registration {
            id: self.next_id,
            owner,
            item_id,
            value,
            windows: HashMap::new(),
        });
        self.next_id
    }

    /// Replace a registration's application-wide value, or with `window` its
    /// replacement for that window, where `None` clears it. False when the
    /// owner has no such registration.
    pub fn update(
        &mut self,
        owner: StatusOwner,
        id: u64,
        window: Option<String>,
        value: Option<StatusItem>,
    ) -> bool {
        let Some(registration) = self
            .registrations
            .iter_mut()
            .find(|registration| registration.id == id && registration.owner == owner)
        else {
            return false;
        };
        match (window, value) {
            (None, Some(value)) => registration.value = value,
            (Some(window), Some(value)) => {
                registration.windows.insert(window, value);
            }
            (Some(window), None) => {
                registration.windows.remove(&window);
            }
            (None, None) => return false,
        }
        true
    }

    /// Whether `update` with these arguments would leave the registration
    /// as it is: the owner has it and already holds `value` there.
    pub fn holds(
        &self,
        owner: StatusOwner,
        id: u64,
        window: Option<&str>,
        value: Option<&StatusItem>,
    ) -> bool {
        let Some(registration) = self
            .registrations
            .iter()
            .find(|registration| registration.id == id && registration.owner == owner)
        else {
            return false;
        };
        match (window, value) {
            (None, Some(value)) => registration.value == *value,
            (Some(window), value) => registration.windows.get(window) == value,
            (None, None) => false,
        }
    }

    /// Remove an owned registration; false when it is not live.
    pub fn unregister(&mut self, owner: StatusOwner, id: u64) -> bool {
        let before = self.registrations.len();
        self.registrations
            .retain(|registration| registration.id != id || registration.owner != owner);
        self.registrations.len() != before
    }

    pub fn remove_lifecycle(&mut self, extension: ExtensionId, lifecycle: ExtensionLifecycleId) {
        let owner = StatusOwner {
            extension,
            lifecycle,
        };
        self.registrations
            .retain(|registration| registration.owner != owner);
    }

    /// The items a window's bar shows: each ID's latest registration, minus
    /// the hidden IDs and the items without text, ordered by priority and then
    /// by registration.
    pub fn items(&self, window: &str, hidden: &[String]) -> BarItems {
        let mut effective = Vec::<&Registration>::new();
        for registration in &self.registrations {
            match effective
                .iter_mut()
                .find(|existing| existing.item_id == registration.item_id)
            {
                Some(existing) => *existing = registration,
                None => effective.push(registration),
            }
        }
        effective.retain(|registration| !hidden.contains(&registration.item_id));
        effective.sort_by_key(|registration| registration.id);
        let mut items = BarItems::default();
        for registration in effective {
            let item = registration
                .windows
                .get(window)
                .unwrap_or(&registration.value)
                .clone();
            if item.text.is_empty() {
                continue;
            }
            let side = match item.side {
                StatusSide::Left => &mut items.left,
                StatusSide::Right => &mut items.right,
            };
            side.push(BarItem {
                id: registration.item_id.clone(),
                item,
            });
        }
        for side in [&mut items.left, &mut items.right] {
            side.sort_by_key(|bar_item| std::cmp::Reverse(bar_item.item.priority));
        }
        items
    }
}

#[cfg(test)]
mod tests;
