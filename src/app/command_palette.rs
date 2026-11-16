//! The fallback command palette: the catalog's commands by title in the
//! shared [`Picker`], confirmed against the target captured when it opened.
//!
//! Search Everywhere's Actions tab is the real palette. This one is the way
//! to reach commands such as the extension report when that package failed
//! to load, so it shows titles only: no descriptions, key bindings, or recent
//! commands, and commands that opt out of the palette are left out.

use gpui::{App, AppContext as _, Entity};

use knot_protocol::{Command, CommandArgumentValue};

use super::{
    PALETTE_KEY_CONTEXT,
    commands::catalog::CommandDefinition,
    picker::{Picker, PickerEntry},
};

pub(crate) struct CommandPalette<Origin> {
    picker: Entity<Picker>,
    commands: Vec<Command>,
    origin: Origin,
}

impl<Origin: Clone> CommandPalette<Origin> {
    pub(crate) fn new(
        definitions: impl IntoIterator<Item = CommandDefinition>,
        origin: Origin,
        cx: &mut App,
    ) -> Self {
        let mut definitions = definitions
            .into_iter()
            .filter(|definition| definition.palette)
            .collect::<Vec<_>>();
        definitions.sort_by(|left, right| left.title.cmp(&right.title));
        let entries = definitions
            .iter()
            .map(|definition| PickerEntry::new(definition.title.clone()))
            .collect();
        let commands = definitions
            .into_iter()
            .map(|definition| Command {
                name: definition.name,
                arguments: CommandArgumentValue::Null,
            })
            .collect();
        let picker = cx.new(|cx| {
            Picker::new(
                Some("Search commands".into()),
                entries,
                PALETTE_KEY_CONTEXT,
                cx,
            )
        });
        Self {
            picker,
            commands,
            origin,
        }
    }

    pub(crate) fn picker(&self) -> &Entity<Picker> {
        &self.picker
    }

    pub(crate) fn origin(&self) -> Origin {
        self.origin.clone()
    }

    /// The command of a confirmed picker entry.
    pub(crate) fn command(&self, entry: usize) -> Command {
        self.commands[entry].clone()
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::CommandPalette;
    use crate::app::commands::catalog::{CommandDefinition, CommandOwner};

    fn definition(name: &str, title: &str) -> CommandDefinition {
        CommandDefinition {
            name: name.into(),
            title: title.into(),
            description: Some("Described.".into()),
            palette: true,
            owner: CommandOwner::Native,
        }
    }

    #[gpui::test]
    fn lists_visible_commands_by_title_only(cx: &mut TestAppContext) {
        let hidden = CommandDefinition {
            palette: false,
            ..definition("editor.move-left", "Move Left")
        };
        let palette = cx.update(|cx| {
            CommandPalette::new(
                [
                    hidden,
                    definition("workspace.close", "Close Window"),
                    definition("copy", "Copy Selection"),
                ],
                (),
                cx,
            )
        });
        let picker = palette.picker().clone();
        picker.update(cx, |picker, cx| {
            let rows = picker
                .visible_entries()
                .into_iter()
                .map(|entry| {
                    let row = picker.entry(entry);
                    (row.label.clone(), row.description.clone(), row.hint.clone())
                })
                .collect::<Vec<_>>();
            assert_eq!(
                rows,
                [
                    ("Close Window".into(), None, None),
                    ("Copy Selection".into(), None, None),
                ]
            );
            picker.set_query("move", cx);
            assert!(picker.visible_entries().is_empty());
            picker.set_query("workspace.cl", cx);
            assert!(picker.visible_entries().is_empty());
            picker.set_query("selection", cx);
            let entry = picker.selected_entry().unwrap();
            assert_eq!(palette.command(entry).name.as_ref(), "copy");
        });
    }
}
