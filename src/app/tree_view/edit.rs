//! Inline editing of a row's label or of a new row, in the shared text input.
//!
//! An edit settles through a channel the extension host pulls from: accepted
//! with the typed value, or dismissed. When the caller validates, each typed
//! value travels to it and its answer comes back as a message under the field;
//! a commit waits for the answer to the current text and is blocked by a
//! message.

use std::ops::Range;

use gpui::*;

use super::{TreeView, VisibleRow};
use crate::app::keys::Keystroke;
use crate::app::text_input::{TextInput, TextInputEvent};
use crate::app::transient::TransientOutcome;
use knot_protocol::IconRef;

/// What an edit changes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EditTarget {
    /// The label of the row with this ID.
    Label(String),
    /// A new row, shown first among the children of `parent`.
    New {
        parent: String,
        icon: Option<IconRef>,
    },
}

pub(super) struct TreeEdit {
    pub(super) target: EditTarget,
    pub(super) input: Entity<TextInput>,
    /// The caller's message about the answered text, shown under the field.
    pub(super) message: Option<SharedString>,
    /// Whether the caller validates the typed value.
    watching: bool,
    /// The text the caller last answered.
    answered: Option<String>,
    /// A commit waiting for the answer to the current text.
    commit_when_clean: bool,
    changes: tokio::sync::watch::Sender<Option<String>>,
    completion: Option<tokio::sync::oneshot::Sender<TransientOutcome>>,
    window: AnyWindowHandle,
    _subscriptions: Vec<Subscription>,
}

/// The channels an edit's caller pulls from.
pub(crate) struct EditSession {
    pub(crate) completion: tokio::sync::oneshot::Receiver<TransientOutcome>,
    pub(crate) changes: tokio::sync::watch::Receiver<Option<String>>,
}

impl TreeView {
    /// Start editing, replacing any edit in progress. `selection` is a byte
    /// range of `value`; omitted, the whole value is selected. None when the
    /// row, or the parent of a new row, is not in the tree.
    pub(crate) fn begin_edit(
        &mut self,
        target: EditTarget,
        value: String,
        selection: Option<Range<usize>>,
        watching: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<EditSession> {
        let row = match &target {
            EditTarget::Label(id) => id,
            EditTarget::New { parent, .. } => parent,
        };
        self.item(row)?;
        self.finish_edit(TransientOutcome::Dismissed, cx);
        if let EditTarget::New { parent, .. } = &target {
            self.expanded.insert(parent.clone());
            if !self.children.contains_key(&Some(parent.clone()))
                && let Some(provider) = self.provider
            {
                let _ = self.invalidate(provider.registration, Some(parent.clone()), cx);
            }
        }
        let length = value.len();
        let input = cx.new(|cx| TextInput::new(value.clone(), cx));
        input.update(cx, |input, cx| {
            input.select_range(selection.unwrap_or(0..length), cx);
        });
        let (changes, watched) = tokio::sync::watch::channel(None);
        if watching {
            changes.send_replace(Some(value));
        }
        let (completion, settled) = tokio::sync::oneshot::channel();
        let input_focus = input.read(cx).focus_handle();
        let subscriptions = vec![
            cx.subscribe(&input, |tree, input, _: &TextInputEvent, cx| {
                let value = input.read(cx).text().to_owned();
                if let Some(edit) = &tree.edit {
                    edit.changes.send_if_modified(|current| {
                        let changed = edit.watching && current.as_deref() != Some(value.as_str());
                        if changed {
                            *current = Some(value);
                        }
                        changed
                    });
                }
                cx.notify();
            }),
            cx.on_blur(&input_focus, window, |tree, _, cx| {
                tree.edit_commit(true, cx)
            }),
        ];
        input_focus.focus(window, cx);
        self.edit = Some(TreeEdit {
            target,
            input,
            message: None,
            watching,
            answered: None,
            commit_when_clean: false,
            changes,
            completion: Some(completion),
            window: window.window_handle(),
            _subscriptions: subscriptions,
        });
        self.scroll_to_edit();
        cx.notify();
        Some(EditSession {
            completion: settled,
            changes: watched,
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn is_editing(&self) -> bool {
        self.edit.is_some()
    }

    fn edit_text(&self, cx: &App) -> Option<String> {
        let edit = self.edit.as_ref()?;
        Some(edit.input.read(cx).text().to_owned())
    }

    fn scroll_to_edit(&self) {
        let rows = self.visible_rows();
        let position = rows.iter().position(|row| match (row, self.edit.as_ref()) {
            (VisibleRow::Draft { .. }, Some(_)) => true,
            (VisibleRow::Item { item, .. }, Some(edit)) => {
                matches!(&edit.target, EditTarget::Label(id) if *id == item.id)
            }
            _ => false,
        });
        if let Some(flat) = position {
            self.scroll.scroll_to_item(flat, ScrollStrategy::Nearest);
        }
    }

    /// Settle the edit, returning focus to the tree. A new row's empty value
    /// is a dismissal.
    fn finish_edit(&mut self, outcome: TransientOutcome, cx: &mut Context<Self>) {
        let Some(mut edit) = self.edit.take() else {
            return;
        };
        if let Some(completion) = edit.completion.take() {
            let _ = completion.send(outcome);
        }
        let focus = self.focus.clone();
        let window = edit.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| focus.focus(window, cx));
        });
        cx.notify();
    }

    pub(crate) fn edit_dismiss(&mut self, cx: &mut Context<Self>) {
        self.finish_edit(TransientOutcome::Dismissed, cx);
    }

    /// Commit the typed value. `lost_focus` is a commit the field did not ask
    /// for: a rejected value then dismisses instead of staying open.
    fn edit_commit(&mut self, lost_focus: bool, cx: &mut Context<Self>) {
        let Some(text) = self.edit_text(cx) else {
            return;
        };
        let Some(edit) = self.edit.as_mut() else {
            return;
        };
        if matches!(edit.target, EditTarget::New { .. }) && text.is_empty() {
            self.finish_edit(TransientOutcome::Dismissed, cx);
            return;
        }
        if !edit.watching {
            self.finish_edit(TransientOutcome::Input(text), cx);
            return;
        }
        if edit.answered.as_deref() != Some(text.as_str()) {
            edit.commit_when_clean = true;
            return;
        }
        if edit.message.is_none() {
            self.finish_edit(TransientOutcome::Input(text), cx);
        } else if lost_focus {
            self.finish_edit(TransientOutcome::Dismissed, cx);
        }
    }

    /// Show the caller's message about `delivered`, unless the field has
    /// changed since, and commit a waiting Enter the message does not block.
    pub(crate) fn set_edit_feedback(
        &mut self,
        delivered: Option<&str>,
        message: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.edit_text(cx) else {
            return;
        };
        let Some(edit) = self.edit.as_mut() else {
            return;
        };
        if delivered != Some(text.as_str()) {
            return;
        }
        edit.message = message
            .filter(|message| !message.is_empty())
            .map(Into::into);
        edit.answered = Some(text.clone());
        let commit = std::mem::take(&mut edit.commit_when_clean) && edit.message.is_none();
        if commit {
            self.finish_edit(TransientOutcome::Input(text), cx);
        }
        cx.notify();
    }

    /// Dismiss an edit whose row left the tree.
    pub(super) fn drop_orphaned_edit(&mut self, cx: &mut Context<Self>) {
        let row = match &self.edit {
            Some(TreeEdit {
                target: EditTarget::Label(id),
                ..
            }) => id,
            Some(TreeEdit {
                target: EditTarget::New { parent, .. },
                ..
            }) => parent,
            None => return,
        };
        if self.item(row).is_none() {
            self.finish_edit(TransientOutcome::Dismissed, cx);
        }
    }

    /// Handle a key while editing: Enter commits, Escape dismisses, and the
    /// field takes the editing keys and typed characters; false for any other
    /// key.
    pub(super) fn edit_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(edit) = &self.edit else {
            return false;
        };
        match keystroke.key.to_lowercase().as_str() {
            "enter" => self.edit_commit(false, cx),
            "escape" => self.edit_dismiss(cx),
            _ => {
                let input = edit.input.clone();
                // Editing keys act on the field, and a key that types a
                // character types it.
                if input.update(cx, |input, cx| input.key_down(keystroke, cx)) {
                    return true;
                }
                match super::typed_character(keystroke) {
                    Some(character) => self.edit_insert_text(&character.to_string(), cx),
                    None => return false,
                }
            }
        }
        true
    }

    /// Type text into the field of the edit in progress.
    pub(crate) fn edit_insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(edit) = &self.edit {
            let input = edit.input.clone();
            input.update(cx, |input, cx| input.insert_text(None, text, cx));
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn edit_text_for_test(&self, cx: &App) -> Option<String> {
        self.edit_text(cx)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn edit_message_for_test(&self) -> Option<String> {
        self.edit
            .as_ref()?
            .message
            .as_ref()
            .map(|message| message.to_string())
    }
}
