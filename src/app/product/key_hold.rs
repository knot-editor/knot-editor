//! Ordered key delivery: while an extension command a key invoked is
//! unfinished, later key-downs in its window and committed text in its
//! editor wait, then replay in order, so each key sees the state the
//! previous one committed. Native commands need no hold because they run in
//! the dispatch turn.
//!
//! The hold ends when the command completes, when a key arrives after focus
//! left the held view (a command awaiting a picker), or after a timeout, so
//! a stuck extension never keeps the keyboard.

use super::*;
use crate::app::commands::key_hold::{HeldInput, HeldInputQueue, TextCommandRouter};
use crate::app::editor::EditorView;
use crate::host::protocol::CommandOutcome;
use crate::host::protocol::{Command, CommandArgumentValue, CommandName};

/// How long a command may hold later input before it is released anyway.
pub(super) const KEY_HOLD_TIMEOUT: Duration = Duration::from_secs(1);

pub(super) struct KeyHold {
    generation: u64,
    focus: FocusHandle,
    queue: HeldInputQueue,
    editor: Option<WeakEntity<EditorView>>,
}

impl ProductShell {
    /// Hold the focused view's later input until `completion` settles.
    pub(super) fn hold_keys_until(
        &mut self,
        completion: tokio::sync::oneshot::Receiver<CommandOutcome>,
        editor: Option<Entity<EditorView>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let queue = HeldInputQueue::default();
        let Some(generation) = self.start_key_hold(
            editor.as_ref().map(Entity::downgrade),
            queue.clone(),
            window,
            cx,
        ) else {
            return;
        };
        if let Some(editor) = &editor {
            editor.update(cx, |editor, _| editor.set_held_input(Some(queue)));
        }
        self.release_key_hold_after(generation, completion, window, cx);
    }

    /// Begin holding input into `queue`; the caller attaches the held editor
    /// to the same queue. `None` when a hold is already active or nothing
    /// has focus.
    fn start_key_hold(
        &mut self,
        editor: Option<WeakEntity<EditorView>>,
        queue: HeldInputQueue,
        window: &Window,
        cx: &App,
    ) -> Option<u64> {
        if self.key_hold.is_some() {
            return None;
        }
        let focus = window.focused(cx)?;
        self.key_hold_generation += 1;
        self.key_hold = Some(KeyHold {
            generation: self.key_hold_generation,
            focus,
            queue,
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let _ = completion.await;
            let _ = cx.update(|window, cx| release_key_hold(&this, generation, window, cx));
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(KEY_HOLD_TIMEOUT).await;
            let _ = cx.update(|window, cx| release_key_hold(&this, generation, window, cx));
        })
        .detach();
    }

    #[cfg(test)]
    pub(super) fn holds_keys(&self) -> bool {
        self.key_hold.is_some()
    }
}

/// Send text an editor's text binding claims to its command as
/// `{ text }`. The hold starts now, while the editor is still handling the
/// input, so later keys and text wait; the command is dispatched once the
/// editor is free, since capturing a target reads it.
fn route_text_command(
    editor: WeakEntity<EditorView>,
    command: CommandName,
    text: String,
    queue: HeldInputQueue,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let Some(Some(shell)) = window.root::<ProductShell>() else {
        return false;
    };
    let Some(generation) = shell.update(cx, |shell, cx| {
        shell.start_key_hold(Some(editor), queue, window, cx)
    }) else {
        return false;
    };
    let shell = shell.downgrade();
    window.defer(cx, move |window, cx| {
        let Some(shell_entity) = shell.upgrade() else {
            return;
        };
        let dispatched = shell_entity.update(cx, |this, cx| {
            let target = this.capture_command_target(window, cx)?;
            let command = Command {
                name: command,
                arguments: CommandArgumentValue::Object(
                    [("text".to_owned(), CommandArgumentValue::String(text))].into(),
                ),
            };
            let execution = ProductShell::command_dispatcher(cx).update(cx, |dispatcher, cx| {
                dispatcher.dispatch(command, target, cx)
            });
            this.release_key_hold_after(generation, execution.completion, window, cx);
            Some(())
        });
        if dispatched.is_none() {
            release_key_hold(&shell, generation, window, cx);
        }
    });
    true
}

/// Queue key-downs for product windows that hold their input. Interceptors
/// run before keybindings and text input, so a held key does neither.
pub(in crate::app) fn intercept_held_keys(cx: &mut App) {
    cx.set_global(TextCommandRouter(route_text_command));
    cx.intercept_keystrokes(|event, window, cx| {
        let Some(Some(shell)) = window.root::<ProductShell>() else {
            return;
        };
        let held = shell.update(cx, |shell, _| {
            let hold = shell.key_hold.as_ref()?;
            hold.queue.push(HeldInput::Key(event.keystroke.clone()));
            Some((hold.generation, hold.focus.is_focused(window)))
        });
        let Some((generation, focused)) = held else {
            return;
        };
        cx.stop_propagation();
        if !focused {
            let shell = shell.downgrade();
            window.defer(cx, move |window, cx| {
                release_key_hold(&shell, generation, window, cx)
            });
        }
    })
    .detach();
}

/// End the hold `generation` and replay its input. A replayed key that
/// starts another extension command holds whatever is still queued.
fn release_key_hold(
    shell: &WeakEntity<ProductShell>,
    generation: u64,
    window: &mut Window,
    cx: &mut App,
) {
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
    if let Some(editor) = &editor {
        editor.update(cx, |editor, _| editor.set_held_input(None));
    }
    replay(&shell, editor, hold.queue.take(), window, cx);
}

/// Replay the first of `pending`, then the rest once the effects it started
/// have run. Commands a key dispatches run deferred, so replaying every input
/// in one turn would insert later text before an earlier key's command. A
/// replayed key that starts another hold takes whatever is still queued.
fn replay(
    shell: &Entity<ProductShell>,
    editor: Option<Entity<EditorView>>,
    mut pending: std::collections::VecDeque<HeldInput>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(next) = &shell.read(cx).key_hold {
        next.queue.prepend(pending);
        return;
    }
    let Some(input) = pending.pop_front() else {
        return;
    };
    match input {
        HeldInput::Key(keystroke) => {
            window.dispatch_keystroke(keystroke, cx);
        }
        HeldInput::Text(text) => {
            if let Some(editor) = &editor {
                editor.update(cx, |editor, cx| {
                    editor.replace_text_in_range(None, &text, window, cx)
                });
            }
        }
    }
    if pending.is_empty() {
        return;
    }
    let shell = shell.clone();
    window.defer(cx, move |window, cx| {
        replay(&shell, editor, pending, window, cx)
    });
}
