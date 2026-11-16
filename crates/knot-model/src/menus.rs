//! Menu items that extensions add to the context menus and the application
//! menu bar, and the menu a surface shows from them.
//!
//! An item names a command, so the registry holds no menu contents of its
//! own. Items belong to the lifecycle that added them. A context menu item
//! may be scoped to a view kind and a key context by the keymap's rule: a
//! scoped item shows only where the surface has that kind and, when it names
//! one, that live context.

use knot_protocol::{CommandArgumentValue, ExtensionId, ExtensionLifecycleId};
use serde::Deserialize;

/// Where an item can be added.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuLocation {
    /// The right-click menu of a tab header.
    Tab,
    /// The right-click menu inside a surface.
    Content,
    MenubarFile,
    MenubarEdit,
    MenubarView,
    MenubarGo,
}

impl MenuLocation {
    /// The location a name selects.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "tab" => Self::Tab,
            "content" => Self::Content,
            "menubar.file" => Self::MenubarFile,
            "menubar.edit" => Self::MenubarEdit,
            "menubar.view" => Self::MenubarView,
            "menubar.go" => Self::MenubarGo,
            _ => return None,
        })
    }

    /// Whether the location is in the application menu bar, which is global
    /// and so takes no scope.
    pub fn is_menubar(self) -> bool {
        !matches!(self, Self::Tab | Self::Content)
    }
}

/// A command a menu runs, with what it shows and where it sits.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuItem {
    pub command: String,
    /// The label; the command's title when absent.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default = "no_arguments")]
    pub arguments: CommandArgumentValue,
    /// Items of one group sit together, groups in alphabetical order and
    /// ungrouped items last, with a separator between groups.
    #[serde(default)]
    pub group: Option<String>,
    /// Within a group, lower values come first.
    #[serde(default)]
    pub order: i64,
    /// The view kind a surface must have.
    #[serde(default)]
    pub view: Option<String>,
    /// A key context the surface must have; only with a view.
    #[serde(default)]
    pub context: Option<String>,
}

fn no_arguments() -> CommandArgumentValue {
    CommandArgumentValue::Null
}

impl MenuItem {
    /// Whether the item may be added at `location`.
    pub fn validate(&self, location: MenuLocation) -> Result<(), MenuError> {
        if self.command.is_empty() {
            return Err(MenuError::Command);
        }
        if location.is_menubar() && (self.view.is_some() || self.context.is_some()) {
            return Err(MenuError::MenubarScope);
        }
        if self.context.as_deref().is_some_and(str::is_empty)
            || (self.context.is_some() && self.view.is_none())
        {
            return Err(MenuError::Context);
        }
        Ok(())
    }
}

/// Why an item cannot be added.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuError {
    /// The command name is empty.
    Command,
    /// A menu bar item names a view or a context.
    MenubarScope,
    /// A context without a view.
    Context,
}

/// The lifecycle that owns an item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MenuOwner {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
}

/// A surface a menu opens on: its view kind and live key contexts.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuScope<'a> {
    pub view: Option<&'a str>,
    pub contexts: &'a [&'a str],
}

struct Registration {
    id: u64,
    owner: MenuOwner,
    location: MenuLocation,
    item: MenuItem,
}

/// An item a menu shows, with its label resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuEntry {
    pub title: String,
    pub command: String,
    pub arguments: CommandArgumentValue,
}

#[derive(Default)]
pub struct MenuRegistry {
    registrations: Vec<Registration>,
    next_id: u64,
}

impl MenuRegistry {
    /// Add `item` at `location`, returning the registration's ID.
    pub fn add(
        &mut self,
        owner: MenuOwner,
        location: MenuLocation,
        item: MenuItem,
    ) -> Result<u64, MenuError> {
        item.validate(location)?;
        self.next_id += 1;
        self.registrations.push(Registration {
            id: self.next_id,
            owner,
            location,
            item,
        });
        Ok(self.next_id)
    }

    /// Remove an owned item; false when it is not live.
    pub fn remove(&mut self, owner: MenuOwner, id: u64) -> bool {
        let before = self.registrations.len();
        self.registrations
            .retain(|registration| registration.id != id || registration.owner != owner);
        self.registrations.len() != before
    }

    pub fn remove_lifecycle(&mut self, extension: ExtensionId, lifecycle: ExtensionLifecycleId) {
        let owner = MenuOwner {
            extension,
            lifecycle,
        };
        self.registrations
            .retain(|registration| registration.owner != owner);
    }

    /// The groups of a menu at `location` for a surface in `scope`, in
    /// display order; a separator sits between groups. `title` gives an
    /// item's label when it has none of its own.
    pub fn groups(
        &self,
        location: MenuLocation,
        scope: MenuScope<'_>,
        title: impl Fn(&str) -> String,
    ) -> Vec<Vec<MenuEntry>> {
        let mut items = self
            .registrations
            .iter()
            .filter(|registration| {
                registration.location == location && applies(&registration.item, scope)
            })
            .map(|registration| {
                let item = &registration.item;
                let label = item.title.clone().unwrap_or_else(|| title(&item.command));
                (item, label)
            })
            .collect::<Vec<_>>();
        items.sort_by(|(a, a_title), (b, b_title)| {
            (a.group.is_none(), &a.group, a.order, a_title).cmp(&(
                b.group.is_none(),
                &b.group,
                b.order,
                b_title,
            ))
        });
        let mut groups: Vec<(Option<&String>, Vec<MenuEntry>)> = Vec::new();
        for (item, label) in items {
            let entry = MenuEntry {
                title: label,
                command: item.command.clone(),
                arguments: item.arguments.clone(),
            };
            match groups.last_mut() {
                Some((group, entries)) if *group == item.group.as_ref() => entries.push(entry),
                _ => groups.push((item.group.as_ref(), vec![entry])),
            }
        }
        groups.into_iter().map(|(_, entries)| entries).collect()
    }
}

/// The keymap's scope rule: an item names the view kind it needs, and a key
/// context that the surface must hold.
fn applies(item: &MenuItem, scope: MenuScope<'_>) -> bool {
    item.view
        .as_deref()
        .is_none_or(|view| scope.view == Some(view))
        && item
            .context
            .as_deref()
            .is_none_or(|context| scope.contexts.contains(&context))
}

#[cfg(test)]
mod tests;
