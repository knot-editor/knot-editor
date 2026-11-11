//! The bars an editor shows above its text: the text fields behind each
//! bar's fields, focus between them and the text, edit notifications, and
//! painting. What each bar shows and which field has focus is
//! [`EditorBars`](knot_model::editor::bars::EditorBars) state.

use knot_model::editor::bars::{BarFieldRef, BarSpec};
use knot_protocol::ExtensionKey;

use super::*;
use crate::app::convert::ThemePaint as _;
use crate::app::text_input::{TextInput, TextInputEvent};

/// A field's text, typed by the user, changed in `owner`'s bar `bar`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BarFieldEdited {
    pub owner: ExtensionKey,
    pub bar: String,
}

impl EventEmitter<BarFieldEdited> for Editor {}

/// The text field behind one bar field.
pub(super) struct BarInput {
    field: BarFieldRef,
    input: Entity<TextInput>,
    /// Changes the editor made itself and still to come back from the field,
    /// which are not edits to report.
    quiet: usize,
    _subscription: Subscription,
}

impl Editor {
    /// Open `owner`'s bar `id` below the others, or replace what it shows in
    /// place. A field that stays keeps its text; a new one starts empty.
    pub(crate) fn open_bar(
        &mut self,
        owner: ExtensionKey,
        id: &str,
        spec: BarSpec,
        cx: &mut Context<Self>,
    ) {
        self.state.bars_mut().open(owner, id, spec);
        self.sync_bar_inputs(cx);
    }

    /// Close `owner`'s bar `id`; false when it has none.
    pub(crate) fn close_bar(
        &mut self,
        owner: ExtensionKey,
        id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let closed = self.state.bars_mut().close(owner, id);
        self.sync_bar_inputs(cx);
        closed
    }

    /// Give `field` the editor's focus, first replacing its text with `text`
    /// selected; false when no open bar has the field.
    pub(crate) fn focus_bar_field(
        &mut self,
        field: BarFieldRef,
        text: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.state.bars_mut().focus_field(field.clone()) {
            return false;
        }
        if let Some(text) = text
            && let Some(bar_input) = self.bar_inputs.iter_mut().find(|bar| bar.field == field)
        {
            bar_input.quiet += 1;
            bar_input.input.update(cx, |input, cx| {
                input.set_text(text, cx);
                input.select_range(0..usize::MAX, cx);
            });
        }
        cx.notify();
        true
    }

    /// Return the editor's focus from a bar field to the text.
    pub(crate) fn focus_text(&mut self, cx: &mut Context<Self>) {
        if self.state.bars().focused().is_some() {
            self.state.bars_mut().focus_text();
            cx.notify();
        }
    }

    /// Move the editor's focus to the next or previous bar field.
    pub(crate) fn focus_adjacent_bar_field(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.state.bars_mut().focus_adjacent(forward);
        cx.notify();
    }

    /// The text field of the bar field with the editor's focus.
    pub(crate) fn focused_bar_input(&self) -> Option<&Entity<TextInput>> {
        let focused = self.state.bars().focused()?;
        self.bar_input(focused)
    }

    pub(crate) fn bar_input(&self, field: &BarFieldRef) -> Option<&Entity<TextInput>> {
        self.bar_inputs
            .iter()
            .find(|bar| &bar.field == field)
            .map(|bar| &bar.input)
    }

    /// The text of each field of `owner`'s bar `id`, in order; `None` when it
    /// has no such bar.
    pub(crate) fn bar_field_texts(
        &self,
        owner: ExtensionKey,
        id: &str,
        cx: &App,
    ) -> Option<Vec<(String, String)>> {
        let bar = self.state.bars().get(owner, id)?;
        Some(
            bar.spec
                .fields
                .iter()
                .map(|field| {
                    let reference = BarFieldRef::new(owner, id, field.id.clone());
                    let text = self
                        .bar_input(&reference)
                        .map(|input| input.read(cx).text().to_owned())
                        .unwrap_or_default();
                    (field.id.clone(), text)
                })
                .collect(),
        )
    }

    /// Give every open bar field a text field and drop the rest, then apply
    /// placeholders and notify.
    pub(super) fn sync_bar_inputs(&mut self, cx: &mut Context<Self>) {
        let fields = self.state.bars().fields().collect::<Vec<_>>();
        self.bar_inputs
            .retain(|bar_input| fields.contains(&bar_input.field));
        for field in fields {
            let placeholder = self
                .state
                .bars()
                .get(field.owner, &field.bar)
                .and_then(|bar| bar.spec.fields.iter().find(|f| f.id == field.field))
                .map(|f| f.placeholder.clone())
                .unwrap_or_default();
            let input = match self.bar_input(&field) {
                Some(input) => input.clone(),
                None => {
                    let input = cx.new(|cx| TextInput::new("", cx));
                    let subscription = cx.subscribe(&input, Self::bar_input_changed);
                    self.bar_inputs.push(BarInput {
                        field,
                        input: input.clone(),
                        quiet: 0,
                        _subscription: subscription,
                    });
                    input
                }
            };
            input.update(cx, |input, cx| input.set_placeholder(placeholder, cx));
        }
        cx.notify();
    }

    fn bar_input_changed(
        &mut self,
        input: Entity<TextInput>,
        _: &TextInputEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(bar_input) = self.bar_inputs.iter_mut().find(|bar| bar.input == input) else {
            return;
        };
        if bar_input.quiet > 0 {
            bar_input.quiet -= 1;
            return;
        }
        cx.emit(BarFieldEdited {
            owner: bar_input.field.owner,
            bar: bar_input.field.bar.clone(),
        });
    }
}

impl EditorView {
    /// The bars above the text, top to bottom.
    pub(super) fn render_bars(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let editor = self.editor.read(cx);
        let theme = theme(cx);
        let focused = editor.bars().focused();
        editor
            .bars()
            .bars()
            .iter()
            .enumerate()
            .map(|(index, bar)| {
                let fields = bar.spec.fields.iter().filter_map(|field| {
                    let reference = BarFieldRef::new(bar.owner, bar.id.clone(), field.id.clone());
                    let input = editor.bar_input(&reference)?.clone();
                    let has_focus = focused == Some(&reference);
                    let target = self.editor.clone();
                    Some(
                        div()
                            .w(crate::app::zoom::rems(240.))
                            .flex_shrink_0()
                            .px_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(theme.paint_color(if has_focus {
                                color::INPUT_FOCUSED_BORDER
                            } else {
                                color::INPUT_BORDER
                            }))
                            .bg(theme.paint_color(color::INPUT_BACKGROUND))
                            .capture_any_mouse_down(move |_, _, cx| {
                                target.update(cx, |editor, cx| {
                                    editor.focus_bar_field(reference.clone(), None, cx);
                                });
                            })
                            .child(input),
                    )
                });
                div()
                    .id(("editor-bar", index))
                    .flex()
                    .flex_row()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .bg(theme.paint_color(color::EDITOR_BAR_BACKGROUND))
                    .border_b_1()
                    .border_color(theme.paint_color(color::EDITOR_BAR_BORDER))
                    // A click in a bar never reaches the text below it.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(fields)
                    .when_some(bar.spec.message.clone(), |row, message| {
                        row.child(
                            div()
                                .truncate()
                                .text_sm()
                                .text_color(theme.paint_color(color::TEXT_MUTED))
                                .child(message),
                        )
                    })
                    .into_any_element()
            })
            .collect()
    }

    /// Have the window focus what has the editor's focus, a bar field or the
    /// text, while focus is in this view. A field the window focused stays in
    /// this view after its bar closes, so focus returns to the text.
    pub(super) fn follow_bar_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let closed_field = self
            .focused_field
            .take()
            .is_some_and(|field| field.is_focused(window));
        if !closed_field && !self.focus.contains_focused(window, cx) {
            return;
        }
        let desired = self
            .editor
            .read(cx)
            .focused_bar_input()
            .map(|input| input.read(cx).focus_handle())
            .unwrap_or_else(|| self.focus.clone());
        if desired != self.focus {
            self.focused_field = Some(desired.clone());
        }
        if !desired.is_focused(window) {
            window.defer(cx, move |window, cx| window.focus(&desired, cx));
        }
    }
}
