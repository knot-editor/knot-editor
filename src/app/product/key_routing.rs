//! Key matching for a product session. A window's key-downs pass through one
//! interceptor, which first holds them behind an unfinished key-invoked
//! extension command and otherwise hands them to the session; a session
//! without a window takes keys and text directly. The session routes a key
//! through its `KeyRouter` against the focused view's kind and live key
//! contexts while the workbench has focus. A key the keymap does not claim
//! goes to the surface with Knot-side focus, and one that surface ignores
//! becomes typed text, as if no binding existed; the keys of a sequence that
//! stops matching are delivered that way after the fact.

use std::collections::VecDeque;

use super::*;
use crate::app::commands::key_router::{KeyAction, KeyRouter, SEQUENCE_TIMEOUT};
use crate::app::editor::EditorView;
use crate::app::transient::TransientSurface;
use crate::app::workbench::{CommandView, ExtensionView};
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

/// Marks the interceptor installed, so installing it again changes nothing.
struct KeyInterception;

impl Global for KeyInterception {}

/// Route key-downs in product windows. Interceptors run before gpui's
/// bindings, view key handlers, and text input, so a handled key reaches
/// none of them.
pub(in crate::app) fn intercept_keys(cx: &mut App) {
    if cx.has_global::<KeyInterception>() {
        return;
    }
    cx.set_global(KeyInterception);
    super::key_hold::install_text_router(cx);
    cx.intercept_keystrokes(|event, window, cx| {
        let Some(Some(shell)) = window.root::<ProductShell>() else {
            return;
        };
        if super::key_hold::hold_key(&shell, event, window, cx) {
            cx.stop_propagation();
            return;
        }
        // An unhandled key propagates to the window's text input.
        let handled = shell.update(cx, |shell, cx| {
            if !std::mem::take(&mut shell.keys.native)
                && shell.route_key(event.keystroke.clone(), Some(window), cx)
            {
                return true;
            }
            shell.deliver_key(&event.keystroke, cx)
        });
        if handled {
            cx.stop_propagation();
        }
    })
    .detach();
}

impl ProductShell {
    /// Press a key in a session without a window: it is routed, and when
    /// unclaimed delivered to the focused surface or typed.
    #[cfg(test)]
    pub(crate) fn press_key(&mut self, keystroke: Keystroke, cx: &mut Context<Self>) {
        let keystroke = keystroke.with_simulated_ime();
        if !self.route_key(keystroke.clone(), None, cx) {
            self.deliver_unclaimed(&keystroke, cx);
        }
    }

    /// Type `text` into the focused surface, as committed platform input.
    pub(crate) fn type_text(&mut self, text: &str, cx: &mut Context<Self>) {
        match self.focus {
            Focus::Workbench => match self.focused_command_view(cx) {
                Some(CommandView::Editor(view)) => {
                    let editor = view.read(cx).editor().clone();
                    editor.update(cx, |editor, cx| editor.type_text(None, text, cx));
                }
                Some(CommandView::Terminal(view)) => {
                    view.update(cx, |view, cx| view.insert_text(text, cx));
                }
                _ => {}
            },
            Focus::Palette => {
                if let Some(palette) = &self.command_palette {
                    let picker = palette.picker().clone();
                    picker.update(cx, |picker, cx| picker.insert_text(text, cx));
                }
            }
            Focus::Transient => match self
                .transient
                .as_ref()
                .and_then(|active| active.surface.as_ref())
            {
                Some(TransientSurface::Picker(picker)) => {
                    picker
                        .clone()
                        .update(cx, |picker, cx| picker.insert_text(text, cx));
                }
                Some(TransientSurface::Input(input)) => {
                    input
                        .clone()
                        .update(cx, |input, cx| input.insert_text(text, cx));
                }
                None => {}
            },
        }
    }

    /// Give an unclaimed key to the surface with Knot-side focus; false when
    /// it ignores the key.
    fn deliver_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        match self.focus {
            Focus::Workbench => match self.focused_command_view(cx) {
                Some(CommandView::Terminal(view)) => {
                    view.update(cx, |view, cx| view.key_down(keystroke, cx))
                }
                Some(CommandView::Workspace(tree)) => {
                    tree.update(cx, |tree, cx| tree.key_down(keystroke, cx))
                }
                Some(CommandView::Extension(ExtensionView::Tree(tree))) => {
                    tree.update(cx, |tree, cx| tree.key_down(keystroke, cx))
                }
                Some(CommandView::Editor(_) | CommandView::Extension(ExtensionView::Text(_)))
                | None => false,
            },
            Focus::Palette => {
                let Some(picker) = self
                    .command_palette
                    .as_ref()
                    .map(|palette| palette.picker().clone())
                else {
                    return false;
                };
                picker.update(cx, |picker, cx| picker.key_down(keystroke, cx))
            }
            Focus::Transient => match self
                .transient
                .as_ref()
                .and_then(|active| active.surface.as_ref())
            {
                Some(TransientSurface::Picker(picker)) => picker
                    .clone()
                    .update(cx, |picker, cx| picker.key_down(keystroke, cx)),
                Some(TransientSurface::Input(input)) => input
                    .clone()
                    .update(cx, |input, cx| input.key_down(keystroke, cx)),
                None => false,
            },
        }
    }

    /// Deliver an unclaimed key without a window: to the focused surface,
    /// otherwise as the text it types, as a window's text input would.
    fn deliver_unclaimed(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        if self.deliver_key(keystroke, cx) {
            return;
        }
        let modifiers = keystroke.modifiers;
        if let Some(text) = &keystroke.key_char
            && !modifiers.platform
            && !modifiers.control
        {
            self.type_text(text, cx);
        }
    }

    /// The view of the focused pane's active tab.
    fn focused_command_view(&self, cx: &App) -> Option<CommandView> {
        self.workbench
            .read(cx)
            .focused_pane()
            .and_then(|pane| pane.active_tab().command_view())
    }

    /// Route a typed key while the workbench has focus; false when the
    /// keymap leaves it unclaimed. Without a window, extension commands do
    /// not hold later input.
    fn route_key(
        &mut self,
        keystroke: Keystroke,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.focus != Focus::Workbench {
            return false;
        }
        let focus = Some(self.focus_target(cx));
        if self.keys.router.is_pending() && self.keys.focus != focus {
            self.keys.router.clear();
        }
        let was_pending = self.keys.router.is_pending();
        let (view, contexts) = self.key_scope(cx);
        let contexts = contexts.iter().map(String::as_str).collect::<Vec<_>>();
        let Some(ApplicationKeymaps(keymaps)) = cx.try_global::<ApplicationKeymaps>() else {
            return false;
        };
        let actions = self.keys.router.key(keystroke.clone(), |typed| {
            keymaps.match_typed(typed, view.as_deref(), &contexts)
        });
        self.keys.generation += 1;
        if self.keys.router.is_pending() {
            self.keys.focus = focus;
            self.flush_keys_after_timeout(cx);
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
        window: Option<&mut Window>,
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
        if let Some(window) = window
            && !native
            && self.key_hold.is_none()
        {
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
        mut window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        while let Some(action) = actions.pop_front() {
            match action {
                KeyAction::Run(command) => {
                    self.run_key_command(command, window.as_deref_mut(), cx);
                }
                KeyAction::Native(keystroke) => {
                    let Some(window) = window.as_deref_mut() else {
                        self.deliver_unclaimed(&keystroke, cx);
                        continue;
                    };
                    let shell = cx.entity().downgrade();
                    window.defer(cx, move |window, cx| {
                        deliver_natively(&shell, keystroke, actions, window, cx)
                    });
                    return;
                }
            }
        }
    }

    fn flush_keys_after_timeout(&mut self, cx: &mut Context<Self>) {
        let generation = self.keys.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEQUENCE_TIMEOUT).await;
            let _ = cx.update(|cx| {
                let Some(this) = this.upgrade() else {
                    return;
                };
                let flush =
                    |this: &mut Self, window: Option<&mut Window>, cx: &mut Context<Self>| {
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
                    };
                match this.read(cx).window {
                    Some(window) => {
                        let _ = window.update(cx, |_, window, cx| {
                            this.update(cx, |this, cx| flush(this, Some(window), cx))
                        });
                    }
                    None => this.update(cx, |this, cx| flush(this, None, cx)),
                }
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
        shell.perform_key_actions(rest, Some(window), cx);
    });
}
