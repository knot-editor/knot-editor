//! Key matching for a product window: every key-down passes through one
//! interceptor, which first holds it behind an unfinished key-invoked
//! extension command and otherwise routes it through the window's
//! `KeyRouter` against the focused view's kind and live key contexts. A key
//! the keymap does not claim propagates to the view and text input as if no
//! binding existed, and the keys of a sequence that stops matching are
//! delivered that way after the fact.

use std::collections::VecDeque;

use super::*;
use crate::app::commands::key_router::{KeyAction, KeyRouter, SEQUENCE_TIMEOUT};
use crate::app::editor::EditorView;
use crate::app::workbench::CommandView;
use crate::app::{PALETTE_KEY_CONTEXT, TRANSIENT_KEY_CONTEXT};
use crate::host::protocol::Command;

#[derive(Default)]
pub(super) struct KeyRouting {
    router: KeyRouter,
    /// What had focus when the pending sequence began.
    focus: Option<FocusTarget>,
    /// Bumped by every routed key, so a timer finds whether its sequence is
    /// still the pending one.
    generation: u64,
    /// The next key reaching the interceptor is one a failed sequence
    /// delivers natively.
    native: bool,
}

/// Route key-downs in product windows. Interceptors run before gpui's
/// bindings, view key handlers, and text input, so a claimed key reaches
/// none of them.
pub(in crate::app) fn intercept_keys(cx: &mut App) {
    super::key_hold::install_text_router(cx);
    cx.intercept_keystrokes(|event, window, cx| {
        let Some(Some(shell)) = window.root::<ProductShell>() else {
            return;
        };
        if super::key_hold::hold_key(&shell, event, window, cx) {
            cx.stop_propagation();
            return;
        }
        // The palette and transient interactions handle their own keys.
        if event.context_stack.iter().any(|context| {
            context.contains(PALETTE_KEY_CONTEXT) || context.contains(TRANSIENT_KEY_CONTEXT)
        }) {
            return;
        }
        let claimed = shell.update(cx, |shell, cx| {
            if std::mem::take(&mut shell.keys.native) {
                return false;
            }
            shell.route_key(event.keystroke.clone(), window, cx)
        });
        if claimed {
            cx.stop_propagation();
        }
    })
    .detach();
}

impl ProductShell {
    /// Route a typed key; false when it propagates natively.
    fn route_key(
        &mut self,
        keystroke: Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.command_palette.is_some() {
            return false;
        }
        let focus = Some(self.focus_target(cx));
        if self.keys.router.is_pending() && self.keys.focus != focus {
            self.keys.router.clear();
        }
        let was_pending = self.keys.router.is_pending();
        let (view, contexts) = self.key_scope(cx);
        let contexts = contexts.iter().map(String::as_str).collect::<Vec<_>>();
        let keymaps = &cx.global::<ApplicationKeymaps>().0;
        let actions = self.keys.router.key(keystroke.clone(), |typed| {
            keymaps.match_typed(typed, view.as_deref(), &contexts)
        });
        self.keys.generation += 1;
        if self.keys.router.is_pending() {
            self.keys.focus = focus;
            self.flush_keys_after_timeout(window, cx);
        }
        match actions.as_slice() {
            [] => true,
            [KeyAction::Native(native)] if !was_pending && *native == keystroke => false,
            [KeyAction::Run(command)] if !was_pending => {
                self.run_key_command(command.clone(), window, cx)
            }
            _ => {
                self.perform_key_actions(actions.into(), window, cx);
                true
            }
        }
    }

    /// The keymap view kind of the focused view and its live key contexts.
    /// The contexts are the view's own rather than gpui's, which reflect the
    /// last rendered frame, since the previous key may have changed them.
    fn key_scope(&mut self, cx: &mut Context<Self>) -> (Option<String>, Vec<String>) {
        let Some(target) = self.capture_command_target(cx) else {
            return (None, Vec::new());
        };
        let contexts = Self::target_editor(&target, cx)
            .map(|editor| {
                editor
                    .read(cx)
                    .editor()
                    .read(cx)
                    .key_contexts()
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        (Self::target_view_kind(&target, cx), contexts)
    }

    /// The editor a captured target's focus is in.
    fn target_editor(target: &ProductCommandTarget, cx: &App) -> Option<Entity<EditorView>> {
        Self::target_view_kind(target, cx)?;
        match target.command_view(cx)? {
            CommandView::Editor(editor) => Some(editor),
            _ => None,
        }
    }

    /// Run a key's command in the focused view. Later input waits for an
    /// extension command; native ones run in this turn. False when nothing
    /// can be targeted.
    fn run_key_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.capture_command_target(cx) else {
            return false;
        };
        let editor = Self::target_editor(&target, cx);
        let dispatcher = Self::command_dispatcher(cx);
        let native = matches!(
            dispatcher
                .read(cx)
                .catalog()
                .borrow()
                .resolve(command.name.as_ref()),
            Ok(crate::app::commands::catalog::CommandTargetKind::Native)
        );
        let execution = dispatcher.update(cx, |dispatcher, cx| {
            dispatcher.dispatch(command, target, cx)
        });
        if !native && self.key_hold.is_none() {
            self.hold_keys_until(execution.completion, editor, window, cx);
        }
        true
    }

    /// Perform routed actions in order. A key delivered natively goes
    /// through the window again once this turn ends, and the actions after
    /// it follow it.
    fn perform_key_actions(
        &mut self,
        mut actions: VecDeque<KeyAction>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        while let Some(action) = actions.pop_front() {
            match action {
                KeyAction::Run(command) => {
                    self.run_key_command(command, window, cx);
                }
                KeyAction::Native(keystroke) => {
                    let shell = cx.entity().downgrade();
                    window.defer(cx, move |window, cx| {
                        deliver_natively(&shell, keystroke, actions, window, cx)
                    });
                    return;
                }
            }
        }
    }

    fn flush_keys_after_timeout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.keys.generation;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SEQUENCE_TIMEOUT).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.keys.generation != generation || !this.keys.router.is_pending() {
                    return;
                }
                if this.keys.focus != Some(this.focus_target(cx)) {
                    this.keys.router.clear();
                    return;
                }
                let (view, contexts) = this.key_scope(cx);
                let contexts = contexts.iter().map(String::as_str).collect::<Vec<_>>();
                let keymaps = &cx.global::<ApplicationKeymaps>().0;
                let actions = this
                    .keys
                    .router
                    .flush(|typed| keymaps.match_typed(typed, view.as_deref(), &contexts));
                this.perform_key_actions(actions.into(), window, cx);
            });
        })
        .detach();
    }

    #[cfg(test)]
    pub(super) fn has_pending_keys(&self) -> bool {
        self.keys.router.is_pending()
    }
}

/// Deliver `keystroke` as if no binding existed, then perform `rest`.
fn deliver_natively(
    shell: &WeakEntity<ProductShell>,
    keystroke: Keystroke,
    rest: VecDeque<KeyAction>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(shell) = shell.upgrade() else {
        return;
    };
    shell.update(cx, |shell, _| shell.keys.native = true);
    window.dispatch_keystroke(keystroke, cx);
    shell.update(cx, |shell, cx| {
        shell.keys.native = false;
        shell.perform_key_actions(rest, window, cx);
    });
}
