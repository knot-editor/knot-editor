//! Commands recently confirmed in the command palette, most recent first,
//! kept in the application state database so their ranking survives
//! restarts.

use std::time::Duration;

use gpui::Global;
use knot_model::command_history::RecentCommands;

use super::record_store::{RecordStore, StateDatabase};

const COMMAND_HISTORY_VERSION: u32 = 1;
const WRITE_DEBOUNCE: Duration = Duration::from_millis(300);

/// The recent commands, persisted in the state database.
#[derive(Clone)]
pub(crate) struct CommandHistory {
    record: RecordStore<RecentCommands>,
}

impl CommandHistory {
    pub(crate) fn open(database: &StateDatabase) -> Self {
        Self {
            record: database.record("command_history", COMMAND_HISTORY_VERSION, WRITE_DEBOUNCE),
        }
    }

    /// Command names, most recent first.
    pub(crate) fn recent(&self) -> Vec<String> {
        self.record
            .current()
            .map(|history| history.recent().to_vec())
            .unwrap_or_default()
    }

    /// Move a confirmed command to the front; see [`RecentCommands::record`].
    pub(crate) fn record(&self, name: &str, registered: impl Fn(&str) -> bool) {
        let history = self.record.current().unwrap_or_default();
        self.record.publish(history.record(name, registered));
    }
}

pub(crate) struct ApplicationCommandHistory(pub(crate) CommandHistory);

impl Global for ApplicationCommandHistory {}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(path: &std::path::Path, cx: &gpui::TestAppContext) -> (StateDatabase, CommandHistory) {
        let database = StateDatabase::open(path.to_owned(), cx.background_executor.clone());
        let history = CommandHistory::open(&database);
        (database, history)
    }

    #[gpui::test]
    async fn records_most_recent_first_and_survives_reopen(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let (database, history) = open(&path, cx);
        assert!(history.recent().is_empty());
        history.record("file.save", |_| true);
        history.record("file.open", |_| true);
        history.record("file.save", |_| true);
        assert_eq!(history.recent(), ["file.save", "file.open"]);
        database.flush().await.unwrap();
        drop((database, history));

        let (_, reopened) = open(&path, cx);
        assert_eq!(reopened.recent(), ["file.save", "file.open"]);
    }
}
