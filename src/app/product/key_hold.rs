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
use crate::app::commands::key_hold::{HeldInput, HeldInputQueue};
use crate::app::editor::EditorView;
use crate::host::protocol::CommandOutcome;

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
        let Some(focus) = window.focused(cx) else {
            return;
        };
        self.key_hold_generation += 1;
        let generation = self.key_hold_generation;
        let queue = HeldInputQueue::default();
        if let Some(editor) = &editor {
            editor.update(cx, |editor, _| editor.set_held_input(Some(queue.clone())));
        }
        self.key_hold = Some(KeyHold {
            generation,
            focus,
            queue,
            editor: editor.as_ref().map(Entity::downgrade),
        });
        // Whichever comes first releases; the other finds a newer generation.
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

/// Queue key-downs for product windows that hold their input. Interceptors
/// run before keybindings and text input, so a held key does neither.
pub(in crate::app) fn intercept_held_keys(cx: &mut App) {
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
    let mut pending = hold.queue.take();
    while let Some(input) = pending.pop_front() {
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
        if let Some(next) = &shell.read(cx).key_hold {
            next.queue.prepend(pending);
            return;
        }
    }
}
