//! Foreground-owned transient extension interactions.
//!
//! A picker and a text prompt are window-local overlays that own a shared
//! native text field and settle exactly once; the picker is the shared
//! [`Picker`]. The product shell keeps at most
//! one live extension interaction per window; confirmations and messages use
//! the native platform prompt instead of an overlay.

use crate::app::convert::ThemePaint as _;
use std::path::PathBuf;

use gpui::{prelude::*, *};

use super::keys::Keystroke;

use knot_protocol::{CommandInvocationId, ExtensionId, ExtensionLifecycleId, InputCompletion};

use super::picker::Picker;
use super::text_input::{TextInput, TextInputEvent};
use super::theme::{color, theme};

const PANEL_WIDTH: f32 = 560.;
const ROW_HEIGHT: f32 = 26.;
/// Completion rows visible at once; the highlight scrolls into view.
const MAX_VISIBLE_ROWS: usize = 8;

/// Identity of the extension operation that owns one active interaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TransientOwner {
    pub(crate) extension: ExtensionId,
    pub(crate) lifecycle: ExtensionLifecycleId,
    pub(crate) invocation: Option<CommandInvocationId>,
}

/// Title and optional body shared by native prompt interactions.
pub(crate) struct PromptText {
    pub(crate) title: String,
    pub(crate) message: Option<String>,
}

/// One rendered overlay surface owned by the window.
pub(crate) enum TransientSurface {
    Picker(Entity<Picker>),
    Input(Entity<InputView>),
}

/// One settled transient interaction result before it is mapped to a public
/// typed value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TransientOutcome {
    Picker(String),
    Input(String),
    Confirmed,
    Folder(PathBuf),
    Failed(String),
    Dismissed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum InputEvent {
    Accepted(String),
    Dismissed,
}

pub(crate) struct InputView {
    title: SharedString,
    /// Fixed text before the field, outside its value; with one, Backspace
    /// on an empty field dismisses.
    prompt: Option<SharedString>,
    input: Entity<TextInput>,
    /// A muted line under the field, set by the interaction's owner.
    message: Option<SharedString>,
    /// The completions under the field, in the caller's order.
    items: Vec<InputCompletion>,
    selected: usize,
    scroll: UniformListScrollHandle,
    _changes: Option<Subscription>,
}

impl InputView {
    /// With `changes`, the field's value is published as it changes,
    /// starting with the initial one.
    pub(crate) fn new(
        title: String,
        initial: String,
        prompt: Option<String>,
        changes: Option<tokio::sync::watch::Sender<Option<String>>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new(initial.clone(), cx));
        let subscription = changes.map(|changes| {
            changes.send_replace(Some(initial));
            cx.subscribe(&input, move |_, input, _: &TextInputEvent, cx| {
                let value = input.read(cx).text();
                changes.send_if_modified(|current| {
                    let changed = current.as_deref() != Some(value);
                    if changed {
                        *current = Some(value.to_string());
                    }
                    changed
                });
            })
        });
        Self {
            title: title.into(),
            prompt: prompt.map(Into::into),
            input,
            message: None,
            items: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            _changes: subscription,
        }
    }

    pub(crate) fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle()
    }

    /// The current field text.
    pub(crate) fn text(&self, cx: &App) -> String {
        self.input.read(cx).text().to_string()
    }

    /// Show the caller's message and completions, highlighting the first row.
    pub(crate) fn set_feedback(
        &mut self,
        message: Option<String>,
        items: Vec<InputCompletion>,
        cx: &mut Context<Self>,
    ) {
        self.message = message.map(Into::into);
        self.items = items;
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.items.len() as isize;
        if count == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }

    /// Replace the field with the highlighted completion's value. The field
    /// reports the change, so the caller answers with the next list.
    fn accept_completion(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected) else {
            return;
        };
        let value = item.value.clone();
        self.input.update(cx, |input, cx| input.set_text(value, cx));
    }

    #[cfg(test)]
    pub(crate) fn items(&self) -> &[InputCompletion] {
        &self.items
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    #[cfg(test)]
    pub(crate) fn message(&self) -> Option<&str> {
        self.message.as_ref().map(SharedString::as_ref)
    }

    #[cfg(test)]
    pub(crate) fn input(&self) -> &Entity<TextInput> {
        &self.input
    }

    /// Handle a key the keymap left unclaimed, the field first; false when
    /// the prompt ignores it.
    pub(crate) fn key_down(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let plain = !keystroke.modifiers.modified();
        if self.prompt.is_some()
            && plain
            && keystroke.key == "backspace"
            && self.input.read(cx).text().is_empty()
        {
            cx.emit(InputEvent::Dismissed);
            return true;
        }
        if self
            .input
            .update(cx, |input, cx| input.key_down(keystroke, cx))
        {
            return true;
        }
        let key = keystroke.key.to_lowercase();
        let control = keystroke.modifiers.control
            && !keystroke.modifiers.platform
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.shift;
        match key.as_str() {
            "up" if plain => self.move_selection(-1, cx),
            "down" if plain => self.move_selection(1, cx),
            "p" if control => self.move_selection(-1, cx),
            "n" if control => self.move_selection(1, cx),
            "tab" if plain => self.accept_completion(cx),
            "escape" => cx.emit(InputEvent::Dismissed),
            "enter" => {
                let value = self.input.read(cx).text().to_string();
                cx.emit(InputEvent::Accepted(value));
            }
            _ => return false,
        }
        true
    }

    /// Type `text` into the field.
    pub(crate) fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.input
            .update(cx, |input, cx| input.insert_text(None, text, cx));
    }
}

impl EventEmitter<InputEvent> for InputView {}

impl Render for InputView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = crate::app::zoom::factor(cx);
        let rows = self.items.len();
        let entity = cx.entity();
        let list = (rows > 0).then(|| {
            uniform_list("input-completions", rows, move |range, _, cx| {
                let view = entity.read(cx);
                let theme = theme(cx);
                let muted = theme.paint_color(color::TEXT_MUTED);
                range
                    .map(|index| {
                        let item = &view.items[index];
                        let selected = index == view.selected;
                        div()
                            .id(("input-completion", index))
                            .h(px(ROW_HEIGHT * zoom))
                            .px_3()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .overflow_hidden()
                            .when(selected, |row| {
                                row.bg(theme.paint_color(color::LIST_SELECTED_BACKGROUND))
                                    .text_color(theme.paint_color(color::LIST_SELECTED_FOREGROUND))
                            })
                            .child(div().flex_shrink_0().child(item.label.clone()))
                            .when_some(item.detail.clone(), |row, detail| {
                                row.child(
                                    div()
                                        .flex_1()
                                        .truncate()
                                        .text_sm()
                                        .text_color(muted)
                                        .child(detail),
                                )
                            })
                    })
                    .collect()
            })
            .track_scroll(&self.scroll)
            .h(px(rows.min(MAX_VISIBLE_ROWS) as f32 * ROW_HEIGHT * zoom))
        });
        div()
            .id("extension-input")
            .w(crate::app::zoom::rems(PANEL_WIDTH))
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(theme(cx).paint_color(color::POPOVER_BORDER))
            .bg(theme(cx).paint_color(color::POPOVER_BACKGROUND))
            .shadow_lg()
            .key_context(super::TRANSIENT_KEY_CONTEXT)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(theme(cx).paint_color(color::TEXT))
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .px_3()
                    .pb_3()
                    .flex()
                    .flex_row()
                    .items_center()
                    .children(self.prompt.clone().map(|prompt| {
                        div()
                            .flex_none()
                            .pr_1()
                            .text_color(theme(cx).paint_color(color::TEXT_MUTED))
                            .child(prompt)
                    }))
                    .child(div().flex_1().child(self.input.clone())),
            )
            .children(self.message.clone().map(|message| {
                div()
                    .px_3()
                    .pb_2()
                    .text_sm()
                    .text_color(theme(cx).paint_color(color::TEXT_MUTED))
                    .child(message)
            }))
            .children(list)
    }
}
