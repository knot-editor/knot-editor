//! The application state database: the plain
//! [`knot_model::state_database`] with the executor that waits out debounces
//! and runs writes off the foreground.

use std::{path::PathBuf, time::Duration};

use gpui::{BackgroundExecutor, Global, Task};
use serde::{Serialize, de::DeserializeOwned};

use knot_model::state_database as plain;

/// SQLite's name for a private in-memory database, which lives as long as
/// its one connection.
#[cfg(any(test, feature = "test-support"))]
pub(crate) const IN_MEMORY: &str = plain::IN_MEMORY;

/// Shared handle to the application state database.
#[derive(Clone)]
pub(crate) struct StateDatabase {
    database: plain::StateDatabase,
    executor: BackgroundExecutor,
}

impl StateDatabase {
    pub(crate) fn user_path() -> Result<PathBuf, String> {
        directories::ProjectDirs::from("", "", "Knot")
            .map(|dirs| dirs.data_local_dir().join("state.sqlite3"))
            .ok_or_else(|| "cannot determine the user application-data directory".into())
    }

    /// The launch socket beside the state database, so one Knot per user
    /// answers command-line launches.
    #[cfg(unix)]
    pub(crate) fn user_socket_path() -> Result<PathBuf, String> {
        directories::ProjectDirs::from("", "", "Knot")
            .map(|dirs| dirs.data_local_dir().join("launch.sock"))
            .ok_or_else(|| "cannot determine the user application-data directory".into())
    }

    /// A database that lives only as long as this value, for tests that
    /// need no file.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn in_memory(executor: BackgroundExecutor) -> Self {
        Self::open(IN_MEMORY.into(), executor)
    }

    pub(crate) fn open(path: PathBuf, executor: BackgroundExecutor) -> Self {
        Self {
            database: plain::StateDatabase::open(path),
            executor,
        }
    }

    /// Load one record synchronously and return its handle.
    pub(crate) fn record<T>(
        &self,
        table: &'static str,
        version: u32,
        debounce: Duration,
    ) -> RecordStore<T>
    where
        T: Clone + PartialEq + Serialize + DeserializeOwned + Send + 'static,
    {
        RecordStore {
            record: self.database.record(table, version, debounce),
            executor: self.executor.clone(),
        }
    }

    /// Write every record's pending value now, off the foreground. Every
    /// record is attempted; the first failure is reported.
    pub(crate) fn flush(&self) -> Task<Result<(), String>> {
        let job = self.database.flush_job();
        self.executor.spawn(async move { job.run() })
    }
}

/// The application's state database, shared by every UI-state record.
pub(crate) struct ApplicationState(pub(crate) StateDatabase);

impl Global for ApplicationState {}

/// One record's handle in the [`StateDatabase`].
pub(crate) struct RecordStore<T> {
    record: plain::RecordStore<T>,
    executor: BackgroundExecutor,
}

impl<T> Clone for RecordStore<T> {
    fn clone(&self) -> Self {
        Self {
            record: self.record.clone(),
            executor: self.executor.clone(),
        }
    }
}

impl<T> RecordStore<T>
where
    T: Clone + PartialEq + Serialize + DeserializeOwned + Send + 'static,
{
    pub(crate) fn current(&self) -> Option<T> {
        self.record.current()
    }

    #[cfg(test)]
    pub(crate) fn last_error(&self) -> Option<String> {
        self.record.last_error()
    }

    /// Replace the record and schedule a coalesced write. Publishing an equal
    /// value is a no-op.
    pub(crate) fn publish(&self, value: T) {
        let Some(write) = self.record.publish(value) else {
            return;
        };
        let executor = self.executor.clone();
        self.executor
            .spawn(async move {
                executor.timer(write.delay()).await;
                write.run();
            })
            .detach();
    }

    /// Write this record's pending value now, off the foreground.
    pub(crate) fn flush(&self) -> Task<Result<(), String>> {
        let job = self.record.flush_job();
        self.executor.spawn(async move { job.run() })
    }
}

#[cfg(test)]
pub(crate) fn test_schema(table: &str) -> String {
    plain::test_schema(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
    struct Value(u32);

    const DEBOUNCE: Duration = Duration::from_millis(300);

    #[gpui::test]
    async fn a_publish_is_written_after_its_debounce_and_a_flush_writes_at_once(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let database = StateDatabase::open(path.clone(), cx.background_executor.clone());
        let debounced = database.record::<Value>("debounced", 1, DEBOUNCE);
        let flushed = database.record::<Value>("flushed", 1, DEBOUNCE);
        debounced.publish(Value(1));
        flushed.publish(Value(2));
        assert!(!path.exists());

        flushed.flush().await.unwrap();
        cx.background_executor.advance_clock(DEBOUNCE);
        cx.run_until_parked();
        assert_eq!(debounced.last_error(), None);

        let reopened = StateDatabase::open(path, cx.background_executor.clone());
        assert_eq!(
            reopened.record::<Value>("debounced", 1, DEBOUNCE).current(),
            Some(Value(1))
        );
        assert_eq!(
            reopened.record::<Value>("flushed", 1, DEBOUNCE).current(),
            Some(Value(2))
        );
    }
}
