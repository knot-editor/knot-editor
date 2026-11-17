//! Notices the application tells the user without interrupting them, and the
//! history that records them.
//!
//! The store keeps the last [`HISTORY_LIMIT`] notices, each read or unread.
//! A notice is also a popup in each window that should show it; popups are
//! not the history, so hiding one leaves its notice unread.

use std::{collections::HashSet, time::SystemTime};

use crate::status_bar::StatusCommand;

/// How many notices the history keeps.
pub const HISTORY_LIMIT: usize = 100;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Severity {
    #[default]
    Info,
    Warning,
    Error,
}

/// A button on a notice that runs a command.
#[derive(Clone, Debug, PartialEq)]
pub struct NoticeAction {
    pub title: String,
    pub command: StatusCommand,
}

/// What a caller says when it raises a notice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoticeSpec {
    pub message: String,
    pub title: Option<String>,
    pub severity: Severity,
    /// Whether the popup hides on its own; `info` notices do by default.
    pub auto_dismiss: Option<bool>,
    pub actions: Vec<NoticeAction>,
    /// The window that shows the popup; every window when none.
    pub target: Option<String>,
    /// The package that raised the notice, or Knot for native ones.
    pub source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub id: u64,
    pub message: String,
    pub title: Option<String>,
    pub severity: Severity,
    pub auto_dismiss: bool,
    pub actions: Vec<NoticeAction>,
    pub target: Option<String>,
    pub source: String,
    pub time: SystemTime,
    pub read: bool,
    /// Windows whose popup for this notice timed out.
    hidden_in: HashSet<String>,
}

/// The popups one window shows.
#[derive(Debug, Default, PartialEq)]
pub struct Popups {
    /// The cards to show, newest first.
    pub shown: Vec<Notice>,
    /// How many more would show but for the limit.
    pub more: usize,
}

#[derive(Default)]
pub struct NotificationStore {
    /// Oldest first.
    notices: Vec<Notice>,
    next_id: u64,
}

impl NotificationStore {
    /// Record a notice, evicting the oldest past the limit, and return its ID.
    pub fn raise(&mut self, spec: NoticeSpec, time: SystemTime) -> u64 {
        self.next_id += 1;
        let auto_dismiss = spec.auto_dismiss.unwrap_or(spec.severity == Severity::Info);
        self.notices.push(Notice {
            id: self.next_id,
            message: spec.message,
            title: spec.title,
            severity: spec.severity,
            auto_dismiss,
            actions: spec.actions,
            target: spec.target,
            source: spec.source,
            time,
            read: false,
            hidden_in: HashSet::new(),
        });
        if self.notices.len() > HISTORY_LIMIT {
            self.notices.remove(0);
        }
        self.next_id
    }

    /// The history, oldest first.
    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }

    pub fn get(&self, id: u64) -> Option<&Notice> {
        self.notices.iter().find(|notice| notice.id == id)
    }

    pub fn unread(&self) -> usize {
        self.notices.iter().filter(|notice| !notice.read).count()
    }

    /// Mark notices read, which closes their popups; whether any changed.
    pub fn mark_read(&mut self, ids: &[u64]) -> bool {
        let mut changed = false;
        for notice in &mut self.notices {
            if ids.contains(&notice.id) && !notice.read {
                notice.read = true;
                changed = true;
            }
        }
        changed
    }

    pub fn mark_all_read(&mut self) -> bool {
        let ids = self
            .notices
            .iter()
            .map(|notice| notice.id)
            .collect::<Vec<_>>();
        self.mark_read(&ids)
    }

    /// Remove a notice from the history; whether it was there.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.notices.len();
        self.notices.retain(|notice| notice.id != id);
        self.notices.len() != before
    }

    /// Empty the history; whether it had notices.
    pub fn clear(&mut self) -> bool {
        let had = !self.notices.is_empty();
        self.notices.clear();
        had
    }

    /// Hide a notice's popup in one window, as its timeout does, leaving the
    /// notice unread. False when the notice is gone or already hidden there.
    pub fn hide_popup(&mut self, id: u64, window: &str) -> bool {
        self.notices
            .iter_mut()
            .find(|notice| notice.id == id)
            .is_some_and(|notice| notice.hidden_in.insert(window.to_owned()))
    }

    /// The popups `window` shows: unread notices aimed at it or at every
    /// window, not hidden there, newest first, at most `limit` of them.
    pub fn popups(&self, window: &str, limit: usize) -> Popups {
        let mut eligible = self
            .notices
            .iter()
            .rev()
            .filter(|notice| {
                !notice.read
                    && !notice.hidden_in.contains(window)
                    && notice
                        .target
                        .as_deref()
                        .is_none_or(|target| target == window)
            })
            .cloned()
            .collect::<Vec<_>>();
        let more = eligible.len().saturating_sub(limit);
        eligible.truncate(limit);
        Popups {
            shown: eligible,
            more,
        }
    }
}

#[cfg(test)]
mod tests;
