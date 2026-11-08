//! Ordered key delivery: while an extension command a key invoked is
//! unfinished, later keys in the session and committed text in the held
//! editor wait, then replay in order, so each key sees the state the
//! previous one committed. Native commands need no hold because they run in
//! the dispatch turn. The session owns the hold, so it works with or
//! without a window.
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
use knot_protocol::{Command, CommandArgumentValue};

/// How long a command may hold later input before it is released anyway.
pub(super) const KEY_HOLD_TIMEOUT: Duration = Duration::from_secs(1);

pub(super) struct KeyHold {
    generation: u64,
    focus: FocusTarget,
    queue: VecDeque<HeldInput>,
    /// The editor whose committed text waits too.
    editor: Option<WeakEntity<Editor>>,
}

impl ProductShell {
    /// Hold the session's later input until `completion` settles.
    pub(super) fn hold_keys_until(
        &mut self,
        completion: tokio::sync::oneshot::Receiver<CommandOutcome>,
        editor: Option<Entity<Editor>>,
        cx: &mut Context<Self>,
    ) {
        if let Some(generation) = self.start_key_hold(editor.as_ref().map(Entity::downgrade), cx) {
            self.release_key_hold_after(generation, completion, cx);
        }
    }

    /// Begin holding input; `None` when a hold is already active.
    fn start_key_hold(&mut self, editor: Option<WeakEntity<Editor>>, cx: &App) -> Option<u64> {
        if self.key_hold.is_some() {
            return None;
        }
        let focus = self.focus_target(cx);
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
        cx.spawn(async move |this, cx| {
            let _ = completion.await;
            let _ = cx.update(|cx| release_key_hold(&this, generation, cx));
        })
        .detach();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(KEY_HOLD_TIMEOUT).await;
            let _ = cx.update(|cx| release_key_hold(&this, generation, cx));
        })
        .detach();
    }

    /// Queue a key while the session holds its input; true when it did.
    /// A key arriving after focus left the held view releases the hold.
    pub(super) fn hold_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let focused = self.focus_target(cx);
        let Some(hold) = self.key_hold.as_mut() else {
            return false;
        };
        hold.queue.push_back(HeldInput::Key(keystroke.clone()));
        if focused != hold.focus {
            let generation = hold.generation;
            let shell = cx.weak_entity();
            cx.defer(move |cx| release_key_hold(&shell, generation, cx));
        }
        true
    }

    /// Take text committed into `editor` when the session holds it behind a
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
        let Some(generation) = self.start_key_hold(Some(editor.downgrade()), cx) else {
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
        let shell = cx.weak_entity();
        cx.defer(move |cx| {
            let Some(entity) = shell.upgrade() else {
                return;
            };
            let dispatched = entity.update(cx, |this, cx| {
                let target = this.capture_command_target(cx)?;
                let execution = ProductShell::command_dispatcher(cx)
                    .update(cx, |dispatcher, cx| {
                        dispatcher.dispatch(command, target, cx)
                    });
                this.release_key_hold_after(generation, execution.completion, cx);
                Some(())
            });
            if dispatched.is_none() {
                release_key_hold(&shell, generation, cx);
            }
        });
        true
    }

    /// Type `text` into `editor` as committed input, unless the session
    /// takes it.
    pub(super) fn type_into_editor(
        &mut self,
        editor: &Entity<Editor>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.take_typed_text(editor, text, cx) {
            editor.update(cx, |editor, cx| editor.type_text(None, text, cx));
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
        let shell = cx.weak_entity();
        cx.defer(move |cx| release_key_hold(&shell, generation, cx));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn holds_keys(&self) -> bool {
        self.key_hold.is_some()
    }
}

/// Offer text committed into an editor of a product window to its session.
fn route_typed_text(
    editor: &Entity<Editor>,
    text: &str,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let Some(Some(shell)) = window.root::<ProductShell>() else {
        return false;
    };
    shell.update(cx, |shell, cx| shell.take_typed_text(editor, text, cx))
}

pub(super) fn install_text_router(cx: &mut App) {
    cx.set_global(TypedTextRouter(route_typed_text));
}

/// End the hold `generation` and replay its input. A replayed key that
/// starts another extension command holds whatever is still queued.
fn release_key_hold(shell: &WeakEntity<ProductShell>, generation: u64, cx: &mut App) {
    let Some(shell) = shell.upgrade() else {
        return;
    };
    let Some(hold) = shell.update(cx, |shell, _| {
        if shell.key_hold.as_ref()?.generation != generation {
            return None;
        }
        shell.key_hold.take()
    }) else {
        return;
    };
    let editor = hold.editor.as_ref().and_then(WeakEntity::upgrade);
    replay(&shell, editor, hold.queue, cx);
}

/// Replay the first of `pending`, then the rest once the effects it started
/// have run. A replayed key that starts another hold takes whatever is
/// still queued. Keys go through the session's window when it has one, so
/// they reach its text input as typed keys do.
fn replay(
    shell: &Entity<ProductShell>,
    editor: Option<Entity<Editor>>,
    mut pending: VecDeque<HeldInput>,
    cx: &mut App,
) {
    let held = shell.update(cx, |shell, _| {
        let next = shell.key_hold.as_mut()?;
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
        HeldInput::Key(keystroke) => match shell.read(cx).window {
            Some(window) => {
                let _ = window.update(cx, |_, window, cx| {
                    window.dispatch_keystroke(keystroke_to_gpui(&keystroke), cx)
                });
            }
            None => shell.update(cx, |shell, cx| shell.press_key(keystroke, cx)),
        },
        HeldInput::Text(text) => {
            if let Some(editor) = &editor {
                shell.update(cx, |shell, cx| shell.type_into_editor(editor, &text, cx));
            }
        }
    }
    if pending.is_empty() {
        return;
    }
    let shell = shell.clone();
    cx.defer(move |cx| replay(&shell, editor, pending, cx));
}
