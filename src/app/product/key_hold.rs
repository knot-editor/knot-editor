//! Ordered key delivery: while an extension command a key or the fallback
//! palette invoked is unfinished, later keys in the window and committed text in the held
//! editor wait, then replay in order, so each key sees the state the
//! previous one committed. Native commands need no hold because they run in
//! the dispatch turn. The window owns the hold, so it works with or
//! without a window.
//!
//! Accepting a picker or input an extension command shows holds later
//! input the same way until the command settles, so keys typed after Enter
//! reach what the command opens. Enter on supplied rows that are not yet
//! current holds from that Enter, with the waiting picker as the held view;
//! Escape then dismisses the picker and drops what was held. The timeout
//! starts once the picker confirms, since the frozen picker takes no keys
//! and Escape leaves it.
//!
//! The hold ends when the command completes, when a picker, input, or the
//! palette takes focus from the held view, when a key arrives after focus
//! left it otherwise, or after a timeout, so a stuck extension never keeps
//! the keyboard.

use std::collections::VecDeque;

use super::*;
use crate::app::commands::key_hold::{HeldInput, TypedTextRouter};
use crate::app::convert::keystroke_to_gpui;
use crate::app::editor::Editor;
use crate::app::keys::Keystroke;
use knot_protocol::CommandOutcome;
use knot_protocol::{Command, CommandArgumentValue, CommandInvocationId};

/// How long a command may hold later input before it is released anyway.
pub(super) const KEY_HOLD_TIMEOUT: Duration = Duration::from_secs(1);

pub(super) struct KeyHold {
    generation: u64,
    focus: FocusTarget,
    queue: VecDeque<HeldInput>,
    /// The editor whose committed text waits too.
    editor: Option<WeakEntity<Editor>>,
}

impl KnotWindow {
    /// Hold the window's later input until `completion` settles.
    pub(super) fn hold_keys_until(
        &mut self,
        completion: tokio::sync::oneshot::Receiver<CommandOutcome>,
        editor: Option<Entity<Editor>>,
        cx: &mut Context<Self>,
    ) {
        let focus = self.focus_target(cx);
        if let Some(generation) = self.start_key_hold(focus, editor.as_ref().map(Entity::downgrade))
        {
            self.release_key_hold_after(generation, completion, cx);
        }
    }

    /// Hold the window's later input until the command tree `invocation`
    /// belongs to settles, with `focus` as the held view. A waiting picker's
    /// hold starts its timeout when the picker confirms.
    pub(super) fn hold_keys_for_command(
        &mut self,
        invocation: CommandInvocationId,
        focus: FocusTarget,
        editor: Option<Entity<Editor>>,
        cx: &mut Context<Self>,
    ) {
        if self.key_hold.is_some() {
            return;
        }
        let Some(settled) = crate::app::extensions::host::entity(cx)
            .and_then(|host| host.update(cx, |host, _| host.watch_command_settle(invocation)))
        else {
            return;
        };
        if let Some(generation) = self.start_key_hold(focus, editor.as_ref().map(Entity::downgrade))
        {
            self.release_key_hold_on(generation, settled, cx);
            if focus.layer != Focus::Transient {
                self.release_key_hold_on_timeout(generation, cx);
            }
        }
    }

    /// Move the hold a waiting picker started to the view and editor focus
    /// returns to once the picker confirms, and start its timeout; false when
    /// no such hold is active.
    pub(super) fn retarget_picker_hold(
        &mut self,
        focus: FocusTarget,
        editor: Option<Entity<Editor>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(hold) = self
            .key_hold
            .as_mut()
            .filter(|hold| hold.focus.layer == Focus::Transient)
        else {
            return false;
        };
        hold.focus = focus;
        hold.editor = editor.as_ref().map(Entity::downgrade);
        let generation = hold.generation;
        self.release_key_hold_on_timeout(generation, cx);
        true
    }

    /// Stop holding input and replay what waited, because an interaction the
    /// running command started takes the keys now.
    pub(crate) fn release_key_hold_now(&mut self, cx: &mut Context<Self>) {
        let Some(generation) = self.key_hold.as_ref().map(|hold| hold.generation) else {
            return;
        };
        let window = cx.weak_entity();
        cx.defer(move |cx| release_key_hold(&window, generation, cx));
    }

    /// Begin holding input for the view `focus`; `None` when a hold is
    /// already active.
    fn start_key_hold(
        &mut self,
        focus: FocusTarget,
        editor: Option<WeakEntity<Editor>>,
    ) -> Option<u64> {
        if self.key_hold.is_some() {
            return None;
        }
        self.key_hold_generation += 1;
        self.key_hold = Some(KeyHold {
            generation: self.key_hold_generation,
            focus,
            queue: VecDeque::new(),
            editor,
        });
        Some(self.key_hold_generation)
    }

    /// Release hold `generation` when `completion` settles or the timeout
    /// passes, whichever comes first; the other finds a newer generation.
    fn release_key_hold_after(
        &mut self,
        generation: u64,
        completion: tokio::sync::oneshot::Receiver<CommandOutcome>,
        cx: &mut Context<Self>,
    ) {
        self.release_key_hold_on(generation, completion, cx);
        self.release_key_hold_on_timeout(generation, cx);
    }

    fn release_key_hold_on(
        &mut self,
        generation: u64,
        completion: tokio::sync::oneshot::Receiver<CommandOutcome>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let _ = completion.await;
            cx.update(|cx| release_key_hold(&this, generation, cx));
        })
        .detach();
    }

    fn release_key_hold_on_timeout(&mut self, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(KEY_HOLD_TIMEOUT).await;
            cx.update(|cx| release_key_hold(&this, generation, cx));
        })
        .detach();
    }

    /// Queue a key while the window holds its input; true when it did.
    /// A key arriving after focus left the held view releases the hold.
    /// Escape at a waiting picker drops the hold and reaches the picker.
    pub(super) fn hold_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let focused = self.focus_target(cx);
        let Some(hold) = self.key_hold.as_mut() else {
            return false;
        };
        // Only a waiting picker's hold has the transient layer as its view:
        // any other hold releases when an interaction takes focus.
        if hold.focus.layer == Focus::Transient
            && focused == hold.focus
            && keystroke.key.eq_ignore_ascii_case("escape")
        {
            self.key_hold = None;
            return false;
        }
        hold.queue.push_back(HeldInput::Key(keystroke.clone()));
        if focused != hold.focus {
            let generation = hold.generation;
            let window = cx.weak_entity();
            cx.defer(move |cx| release_key_hold(&window, generation, cx));
        }
        true
    }

    /// Take text committed into `editor` when the window holds it behind a
    /// command or a text binding routes it to one; false when the editor
    /// should insert it. A composition commits in place, since its preedit
    /// is already in the text.
    pub(super) fn take_typed_text(
        &mut self,
        editor: &Entity<Editor>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if editor.read(cx).is_composing() {
            return false;
        }
        if let Some(hold) = self.key_hold.as_mut() {
            if hold.editor.as_ref() != Some(&editor.downgrade()) {
                return false;
            }
            hold.queue.push_back(HeldInput::Text(text.to_owned()));
            return true;
        }
        let Some(command) = editor.read(cx).text_command(cx) else {
            return false;
        };
        let focus = self.focus_target(cx);
        let Some(generation) = self.start_key_hold(focus, Some(editor.downgrade())) else {
            return false;
        };
        // The command is dispatched once the caller is done with the editor,
        // since capturing a target reads it; the hold already queues input.
        let command = Command {
            name: command,
            arguments: CommandArgumentValue::Object(
                [(
                    "text".to_owned(),
                    CommandArgumentValue::String(text.to_owned()),
                )]
                .into(),
            ),
        };
        editor.update(cx, |editor, _| editor.record_command(&command));
        let window = cx.weak_entity();
        cx.defer(move |cx| {
            let Some(entity) = window.upgrade() else {
                return;
            };
            let dispatched = entity.update(cx, |this, cx| {
                let target = this.capture_command_target(cx);
                let execution = KnotWindow::command_dispatcher(cx).update(cx, |dispatcher, cx| {
                    dispatcher.dispatch(command, target, cx)
                });
                this.release_key_hold_after(generation, execution.completion, cx);
                Some(())
            });
            if dispatched.is_none() {
                release_key_hold(&window, generation, cx);
            }
        });
        true
    }

    /// Type `text` into `editor` as committed input, unless the window
    /// takes it.
    pub(super) fn type_into_editor(
        &mut self,
        editor: &Entity<Editor>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.take_typed_text(editor, text, cx) {
            editor.update(cx, |editor, cx| editor.type_input(None, text, cx));
        }
    }

    /// Release the hold once a picker, input, or the palette takes focus
    /// from the held view, so keys typed ahead reach it.
    pub(super) fn release_keys_for_focus(&mut self, cx: &mut Context<Self>) {
        let Some(hold) = self.key_hold.as_ref() else {
            return;
        };
        if hold.focus.layer == self.focus {
            return;
        }
        let generation = hold.generation;
        let window = cx.weak_entity();
        cx.defer(move |cx| release_key_hold(&window, generation, cx));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn holds_keys(&self) -> bool {
        self.key_hold.is_some()
    }
}

/// Offer text committed into an editor of a window to that window.
fn route_typed_text(
    editor: &Entity<Editor>,
    text: &str,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let Some(Some(window)) = window.root::<KnotWindow>() else {
        return false;
    };
    window.update(cx, |window, cx| window.take_typed_text(editor, text, cx))
}

pub(super) fn install_text_router(cx: &mut App) {
    cx.set_global(TypedTextRouter(route_typed_text));
}

/// End the hold `generation` and replay its input. A replayed key that
/// starts another extension command holds whatever is still queued.
fn release_key_hold(window: &WeakEntity<KnotWindow>, generation: u64, cx: &mut App) {
    let Some(window) = window.upgrade() else {
        return;
    };
    let Some(hold) = window.update(cx, |window, _| {
        if window.key_hold.as_ref()?.generation != generation {
            return None;
        }
        window.key_hold.take()
    }) else {
        return;
    };
    let editor = hold.editor.as_ref().and_then(WeakEntity::upgrade);
    replay(&window, editor, hold.queue, cx);
}

/// Replay the first of `pending`, then the rest once the effects it started
/// have run. A replayed key that starts another hold takes whatever is
/// still queued. Keys go through the native window when it has one, so
/// they reach its text input as typed keys do.
fn replay(
    window: &Entity<KnotWindow>,
    editor: Option<Entity<Editor>>,
    mut pending: VecDeque<HeldInput>,
    cx: &mut App,
) {
    let held = window.update(cx, |window, _| {
        let next = window.key_hold.as_mut()?;
        pending.append(&mut next.queue);
        next.queue = std::mem::take(&mut pending);
        Some(())
    });
    if held.is_some() {
        return;
    }
    let Some(input) = pending.pop_front() else {
        return;
    };
    match input {
        HeldInput::Key(keystroke) => match window.read(cx).native_window {
            Some(window) => {
                let _ = window.update(cx, |_, native_window, cx| {
                    native_window.dispatch_keystroke(keystroke_to_gpui(&keystroke), cx)
                });
            }
            None => window.update(cx, |window, cx| window.press_key(keystroke, cx)),
        },
        HeldInput::Text(text) => {
            if let Some(editor) = &editor {
                window.update(cx, |window, cx| window.type_into_editor(editor, &text, cx));
            }
        }
    }
    if pending.is_empty() {
        return;
    }
    let window = window.clone();
    cx.defer(move |cx| replay(&window, editor, pending, cx));
}
