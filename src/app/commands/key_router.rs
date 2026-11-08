//! Matching typed keystrokes against the keymap, one key at a time.
//!
//! A key that begins a binding resolving to a command where it was typed is
//! held as pending; a key no such binding begins is delivered natively, as
//! text or to the focused view's own key handling. A sequence that stops
//! matching runs its longest bound prefix, or delivers its first key
//! natively, then routes the rest again. Whoever owns the router flushes a
//! pending sequence after `SEQUENCE_TIMEOUT`.

use std::time::Duration;

use gpui::Keystroke;

use knot_protocol::Command;

/// How long a pending sequence waits for its next key.
pub(crate) const SEQUENCE_TIMEOUT: Duration = Duration::from_secs(1);

/// How typed keystrokes match the bindings that resolve to a command where
/// they were typed.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TypedMatch {
    /// The command of a binding of exactly these keystrokes.
    pub(crate) exact: Option<Command>,
    /// Whether a longer binding begins with them.
    pub(crate) longer: bool,
}

/// What routing a key asks of the window, in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum KeyAction {
    Run(Command),
    /// Deliver the keystroke as if no binding existed.
    Native(Keystroke),
}

#[derive(Default)]
pub(crate) struct KeyRouter {
    pending: Vec<Keystroke>,
}

impl KeyRouter {
    pub(crate) fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Drop a pending sequence, as when focus moves elsewhere.
    pub(crate) fn clear(&mut self) {
        self.pending.clear();
    }

    /// Route a typed key. `matches` reports how keystrokes match the
    /// keymap where the key was typed.
    pub(crate) fn key(
        &mut self,
        keystroke: Keystroke,
        matches: impl Fn(&[Keystroke]) -> TypedMatch,
    ) -> Vec<KeyAction> {
        self.pending.push(keystroke);
        self.settle(&matches, false)
    }

    /// Resolve a pending sequence whose next key never came.
    pub(crate) fn flush(&mut self, matches: impl Fn(&[Keystroke]) -> TypedMatch) -> Vec<KeyAction> {
        self.settle(&matches, true)
    }

    fn settle(
        &mut self,
        matches: &impl Fn(&[Keystroke]) -> TypedMatch,
        flushing: bool,
    ) -> Vec<KeyAction> {
        let mut actions = Vec::new();
        while !self.pending.is_empty() {
            let typed = matches(&self.pending);
            if typed.longer && !flushing {
                break;
            }
            if let Some(command) = typed.exact {
                actions.push(KeyAction::Run(command));
                self.pending.clear();
                break;
            }
            // The sequence cannot complete: run its longest bound prefix,
            // or deliver its first key, and route what follows again.
            let prefix = (1..self.pending.len())
                .rev()
                .find_map(|length| Some((length, matches(&self.pending[..length]).exact?)));
            match prefix {
                Some((length, command)) => {
                    actions.push(KeyAction::Run(command));
                    self.pending.drain(..length);
                }
                None => actions.push(KeyAction::Native(self.pending.remove(0))),
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests;
