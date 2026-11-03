//! Status bar items that extensions register and the bar a product window
//! shows from them.
//!
//! An ID names one item. Several lifecycles may register it, the latest
//! registration defines it, and removing that one reveals the previous, as
//! for languages and themes. A registration holds one application-wide value
//! and may hold a replacement for individual windows.

use std::collections::HashMap;

use gpui::Global;
use serde::Deserialize;

use crate::host::protocol::{CommandArgumentValue, ExtensionId, ExtensionLifecycleId};

/// Which end of the bar an item sits at.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum StatusSide {
    #[default]
    Left,
    Right,
}

/// A command a click on an item runs.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StatusCommand {
    pub(crate) name: String,
    #[serde(default = "no_arguments")]
    pub(crate) arguments: CommandArgumentValue,
}

fn no_arguments() -> CommandArgumentValue {
    CommandArgumentValue::Null
}

/// What the bar shows for one item.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StatusItem {
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) tooltip: Option<String>,
    #[serde(default)]
    pub(crate) command: Option<StatusCommand>,
    /// A theme color key for the text.
    #[serde(default)]
    pub(crate) color: Option<String>,
    #[serde(default)]
    pub(crate) side: StatusSide,
    /// Higher values sit further left.
    #[serde(default)]
    pub(crate) priority: i64,
}

/// The extension lifecycle that owns a registration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StatusOwner {
    pub(crate) extension: ExtensionId,
    pub(crate) lifecycle: ExtensionLifecycleId,
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
pub(crate) struct BarItem {
    pub(crate) id: String,
    pub(crate) item: StatusItem,
}

/// The items of one window's bar, in display order.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct BarItems {
    pub(crate) left: Vec<BarItem>,
    pub(crate) right: Vec<BarItem>,
}

#[derive(Default)]
pub(crate) struct StatusRegistry {
    registrations: Vec<Registration>,
    next_id: u64,
}

impl StatusRegistry {
    /// Register `item_id` with its application-wide value, returning the
    /// registration's ID.
    pub(crate) fn register(
        &mut self,
        owner: StatusOwner,
        item_id: String,
        value: StatusItem,
    ) -> u64 {
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
    pub(crate) fn update(
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

    /// Remove an owned registration; false when it is not live.
    pub(crate) fn unregister(&mut self, owner: StatusOwner, id: u64) -> bool {
        let before = self.registrations.len();
        self.registrations
            .retain(|registration| registration.id != id || registration.owner != owner);
        self.registrations.len() != before
    }

    pub(crate) fn remove_lifecycle(
        &mut self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
    ) {
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
    pub(crate) fn items(&self, window: &str, hidden: &[String]) -> BarItems {
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

#[derive(Default)]
pub(crate) struct ApplicationStatusBar(pub(crate) StatusRegistry);

impl Global for ApplicationStatusBar {}

pub(crate) fn remove_lifecycle(
    extension: ExtensionId,
    lifecycle: ExtensionLifecycleId,
    cx: &mut gpui::App,
) {
    if cx.has_global::<ApplicationStatusBar>() {
        cx.global_mut::<ApplicationStatusBar>()
            .0
            .remove_lifecycle(extension, lifecycle);
    }
}

#[cfg(test)]
mod tests;
