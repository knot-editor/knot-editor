//! The checkboxes and buttons of a layout's controls member, laid out in a
//! wrapping row.
//!
//! The controls hold no state of their own: a click runs the item's command
//! with the owner of the layout as target, and the package sends the items
//! again to change what they show.

use gpui::{prelude::*, *};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::{Disableable as _, Sizable as _};
use knot_protocol::{Command, CommandArgumentValue};

use super::view_layout::RunCommand;
use super::zoom;

/// One checkbox or button of a controls member.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum ControlItem {
    Checkbox {
        label: String,
        #[serde(default)]
        checked: bool,
        command: String,
        #[serde(default = "no_arguments")]
        arguments: CommandArgumentValue,
        #[serde(default)]
        tooltip: Option<String>,
    },
    Button {
        label: String,
        /// Drawn as the main action.
        #[serde(default)]
        primary: bool,
        /// Dimmed, and does nothing when clicked.
        #[serde(default)]
        disabled: bool,
        command: String,
        #[serde(default = "no_arguments")]
        arguments: CommandArgumentValue,
    },
}

fn no_arguments() -> CommandArgumentValue {
    CommandArgumentValue::Null
}

impl ControlItem {
    fn command(&self) -> Command {
        let (Self::Checkbox {
            command, arguments, ..
        }
        | Self::Button {
            command, arguments, ..
        }) = self;
        Command {
            name: command.as_str().into(),
            arguments: arguments.clone(),
        }
    }
}

/// The view of a controls member.
pub(crate) struct ControlsView {
    items: Vec<ControlItem>,
    run: RunCommand,
}

impl ControlsView {
    pub(crate) fn new(items: Vec<ControlItem>, run: RunCommand) -> Self {
        Self { items, run }
    }
}

impl Render for ControlsView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let items = self.items.iter().enumerate().map(|(index, item)| {
            let (run, command) = (self.run.clone(), item.command());
            let click = move |window: &mut Window, cx: &mut App| {
                let handle = window.window_handle();
                let (run, command) = (run.clone(), command.clone());
                cx.defer(move |cx| run(handle, command, cx));
            };
            let control = match item {
                ControlItem::Checkbox {
                    label,
                    checked,
                    tooltip,
                    ..
                } => {
                    let checkbox = Checkbox::new(("control", index))
                        .small()
                        .label(label.clone())
                        .checked(*checked)
                        .on_click(move |_, window, cx| click(window, cx));
                    match tooltip {
                        Some(tooltip) => checkbox.tooltip(tooltip.clone()),
                        None => checkbox,
                    }
                    .into_any_element()
                }
                ControlItem::Button {
                    label,
                    primary,
                    disabled,
                    ..
                } => {
                    let button = Button::new(("control", index))
                        .small()
                        .label(label.clone())
                        .disabled(*disabled)
                        .on_click(move |_, window, cx| click(window, cx));
                    if *primary { button.primary() } else { button }.into_any_element()
                }
            };
            // Buttons gather at the right of the row, after the options.
            let first_button = matches!(item, ControlItem::Button { .. })
                && !self.items[..index]
                    .iter()
                    .any(|item| matches!(item, ControlItem::Button { .. }));
            let slot = div().debug_selector(move || format!("control-{index}"));
            if first_button { slot.ml_auto() } else { slot }.child(control)
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(zoom::rems(8.))
            .px(zoom::rems(8.))
            .py(zoom::rems(4.))
            .children(items)
    }
}
