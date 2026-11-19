//! Foreground-owned transient extension interactions.
//!
//! A picker and a text prompt are window-local overlays that own a shared
//! native text field and settle exactly once; the picker is the shared
//! [`Picker`]. A window keeps at most one live extension interaction;
//! confirmations and messages use
//! the native platform prompt instead of an overlay.

use crate::app::convert::ThemePaint as _;
use std::path::PathBuf;

use gpui::{prelude::*, *};

use super::keys::Keystroke;

use knot_protocol::{CommandInvocationId, ExtensionId, ExtensionLifecycleId, InputCompletion};

use super::picker::{self, Picker, PickerEntry, PickerRowContent, picker_row};
use super::text_input::{TextInput, TextInputEvent};
use super::theme::{color, theme};
use knot_model::input_history::matching_entries;
use knot_model::matching::{byte_ranges, utf16_ranges};

use super::tokens::list::PANEL_WIDTH;
use super::{tokens, zoom};

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
    /// Accepted text and the index of the completion highlighted for it.
    Input(String, Option<usize>),
    Confirmed,
    Folder(PathBuf),
    Failed(String),
    Dismissed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum InputEvent {
    Accepted {
        value: String,
        highlighted: Option<usize>,
    },
    Dismissed,
}

/// What an input shows before it is typed into.
#[derive(Clone, Debug, Default)]
pub(crate) struct InputSpec {
    pub(crate) title: String,
    pub(crate) initial: String,
    /// Fixed text before the field, outside its value.
    pub(crate) prompt: Option<String>,
    /// Earlier entries, newest first.
    pub(crate) history: Vec<String>,
    /// Muted text in the empty field; Tab on an empty field accepts it.
    pub(crate) placeholder: Option<String>,
}

#[cfg(test)]
impl InputSpec {
    pub(crate) fn new(title: impl Into<String>, initial: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            initial: initial.into(),
            ..Self::default()
        }
    }

    pub(crate) fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    pub(crate) fn history(mut self, history: Vec<String>) -> Self {
        self.history = history;
        self
    }

    pub(crate) fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }
}

pub(crate) struct InputView {
    title: SharedString,
    /// Fixed text before the field, outside its value; with one, Backspace
    /// on an empty field dismisses.
    prompt: Option<SharedString>,
    input: Entity<TextInput>,
    /// What Tab puts into an empty field when no list is showing.
    placeholder: Option<String>,
    /// A muted line under the field, set by the interaction's owner.
    message: Option<SharedString>,
    /// Earlier entries, newest first, listed ahead of the caller's items.
    history: Vec<String>,
    /// What the caller's feedback last offered.
    caller_items: Vec<InputCompletion>,
    /// The completions under the field: the matching history rows, then the
    /// caller's items in its order.
    items: Vec<InputCompletion>,
    /// How many of `items` are history rows.
    history_rows: usize,
    selected: usize,
    scroll: UniformListScrollHandle,
    /// The field text the caller's feedback last applied to.
    answered: Option<String>,
    _changes: Option<Subscription>,
    _history: Option<Subscription>,
}

impl InputView {
    /// With `changes`, the field's value is published as it changes,
    /// starting with the initial one.
    pub(crate) fn new(
        spec: InputSpec,
        changes: Option<tokio::sync::watch::Sender<Option<String>>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let InputSpec {
            title,
            initial,
            prompt,
            history,
            placeholder,
        } = spec;
        let input = cx.new(|cx| TextInput::new(initial.clone(), cx));
        if let Some(placeholder) = &placeholder {
            input.update(cx, |input, cx| {
                input.set_placeholder(placeholder.clone(), cx)
            });
        }
        let has_history = !history.is_empty();
        let history_subscription = has_history.then(|| {
            cx.subscribe(&input, |this, _, _: &TextInputEvent, cx| {
                this.compose_items(cx)
            })
        });
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
        let mut view = Self {
            title: title.into(),
            prompt: prompt.map(Into::into),
            input,
            placeholder,
            message: None,
            history,
            caller_items: Vec::new(),
            items: Vec::new(),
            history_rows: 0,
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            answered: None,
            _changes: subscription,
            _history: history_subscription,
        };
        view.compose_items(cx);
        view
    }

    /// List the history entries matching the field's text, then the caller's
    /// items, highlighting the first row.
    fn compose_items(&mut self, cx: &mut Context<Self>) {
        let text = self.text(cx);
        let mut items: Vec<InputCompletion> = matching_entries(&self.history, &text)
            .into_iter()
            .map(|(index, ranges)| {
                let entry = self.history[index].clone();
                InputCompletion {
                    matches: Some(utf16_ranges(&entry, &ranges))
                        .filter(|ranges| !ranges.is_empty()),
                    label: entry.clone(),
                    value: entry,
                    description: None,
                    hint: None,
                    icon: None,
                    description_matches: None,
                }
            })
            .collect();
        self.history_rows = items.len();
        items.extend(self.caller_items.iter().cloned());
        self.items = items;
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
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
        self.answered = Some(self.text(cx));
        self.caller_items = items;
        self.compose_items(cx);
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

    /// Highlight the completion at `index` and accept it.
    fn accept_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = index;
        self.accept_completion(cx);
    }

    /// Replace the field with the highlighted completion's value. The field
    /// reports the change, so the caller answers with the next list.
    fn accept_completion(&mut self, cx: &mut Context<Self>) {
        let value = match self.items.get(self.selected) {
            Some(item) => item.value.clone(),
            // With no list, Tab on an empty field takes the placeholder.
            None => match &self.placeholder {
                Some(placeholder) if self.input.read(cx).text().is_empty() => placeholder.clone(),
                _ => return,
            },
        };
        self.input.update(cx, |input, cx| input.set_text(value, cx));
    }

    /// Whether the caller watches the field and has not yet answered its
    /// current text.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn awaiting_feedback(&self, cx: &App) -> bool {
        self._changes.is_some() && self.answered.as_deref() != Some(self.text(cx).as_str())
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
                // The list answers the text only once the caller has seen it.
                let highlighted = (self.answered.as_deref() == Some(value.as_str()))
                    .then(|| self.selected.checked_sub(self.history_rows))
                    .flatten()
                    .filter(|&index| index < self.caller_items.len());
                cx.emit(InputEvent::Accepted { value, highlighted });
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
        let row_height = picker::row_height(false, zoom);
        let list = (rows > 0).then(|| {
            uniform_list("input-completions", rows, move |range, _, cx| {
                let view = entity.read(cx);
                range
                    .map(|index| {
                        let completion = &view.items[index];
                        let entry = PickerEntry {
                            label: completion.label.clone(),
                            description: completion.description.clone(),
                            hint: completion.hint.clone(),
                            icon: completion.icon.clone(),
                            ..PickerEntry::default()
                        };
                        let ranges = |text: &str, ranges: &Option<Vec<[usize; 2]>>| {
                            ranges
                                .as_deref()
                                .map_or_else(Vec::new, |ranges| byte_ranges(text, ranges))
                        };
                        let label_matches = ranges(&completion.label, &completion.matches);
                        let description_matches = ranges(
                            completion.description.as_deref().unwrap_or_default(),
                            &completion.description_matches,
                        );
                        let entity = entity.clone();
                        picker_row(
                            PickerRowContent {
                                entry: &entry,
                                label_matches: &label_matches,
                                description_matches: &description_matches,
                            },
                            index,
                            index == view.selected,
                            row_height,
                            move |_, _, cx| {
                                entity.update(cx, |view, cx| view.accept_row(index, cx));
                            },
                            cx,
                        )
                    })
                    .collect()
            })
            .track_scroll(&self.scroll)
            .h(px(rows.min(MAX_VISIBLE_ROWS) as f32 * row_height))
        });
        div()
            .id("extension-input")
            .w(crate::app::zoom::rems(PANEL_WIDTH))
            .flex()
            .flex_col()
            .rounded(crate::app::zoom::rems(crate::app::tokens::radius::MD))
            .border_1()
            .border_color(theme(cx).paint_color(color::POPOVER_BORDER))
            .bg(theme(cx).paint_color(color::POPOVER_BACKGROUND))
            .shadow_lg()
            .text_size(zoom::rems(tokens::list::TEXT_SIZE))
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
