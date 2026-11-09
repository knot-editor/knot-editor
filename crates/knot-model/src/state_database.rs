//! The application state database: one SQLite file holding versioned JSON
//! records, one table each.
//!
//! [`StateDatabase`] owns a single connection, so writes from every record
//! are serialized and never contend for the file. A [`RecordStore`] is one
//! record's handle: its cached value updates synchronously on publish so
//! readers see it immediately, and SQLite writes follow with a trailing-edge
//! debounce. The runtime that waits out the debounce and runs writes off the
//! foreground belongs to the caller: [`RecordStore::publish`] returns the
//! write to schedule and [`StateDatabase::flush_job`] the work to run. A
//! failed write keeps the cache, records the error, and leaves the value
//! pending for a later retry. Missing, corrupt, or versioned-away records
//! read as absent without creating the database.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};

const APPLICATION_ID: i64 = 0x4b4e4f54;

fn schema(table: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {table}(state INTEGER PRIMARY KEY CHECK(state = 0), version INTEGER NOT NULL, payload TEXT NOT NULL CHECK(json_valid(payload))) STRICT"
    )
}

/// A record whose pending value can be written by a database-wide flush.
pub trait PendingRecord: Send + Sync {
    fn generation(&self) -> u64;
    fn write_pending(&self) -> Result<(), String>;
}

struct Database {
    path: PathBuf,
    /// Opened on the first read of an existing file or the first write.
    connection: Mutex<Option<Connection>>,
    records: Mutex<Vec<Weak<dyn PendingRecord>>>,
}

/// SQLite's name for a private in-memory database, which lives as long as
/// its one connection.
pub const IN_MEMORY: &str = ":memory:";

/// Shared handle to the application state database.
#[derive(Clone)]
pub struct StateDatabase {
    inner: Arc<Database>,
}

impl StateDatabase {
    /// A database that lives only as long as this value, for tests that
    /// need no file.
    pub fn in_memory() -> Self {
        Self::open(IN_MEMORY.into())
    }

    pub fn open(path: PathBuf) -> Self {
        Self {
            inner: Arc::new(Database {
                path,
                connection: Mutex::new(None),
                records: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Load one record synchronously and return its handle.
    pub fn record<T>(&self, table: &'static str, version: u32, debounce: Duration) -> RecordStore<T>
    where
        T: Clone + PartialEq + Serialize + DeserializeOwned + Send + 'static,
    {
        let current = self.read(table, version);
        let inner = Arc::new(Inner {
            database: self.clone(),
            table,
            version,
            debounce,
            state: Mutex::new(State {
                current,
                pending: None,
                generation: 0,
                last_error: None,
            }),
        });
        let pending: Arc<dyn PendingRecord> = inner.clone();
        let mut records = self.inner.records.lock().unwrap();
        records.retain(|record| record.strong_count() > 0);
        records.push(Arc::downgrade(&pending));
        RecordStore { inner }
    }

    /// The write of every record's pending value, for the caller to run off
    /// the foreground. Every record is attempted; the first failure is
    /// reported.
    pub fn flush_job(&self) -> FlushJob {
        FlushJob(
            self.inner
                .records
                .lock()
                .unwrap()
                .iter()
                .filter_map(Weak::upgrade)
                .collect(),
        )
    }

    fn read<T: DeserializeOwned>(&self, table: &str, version: u32) -> Option<T> {
        let mut connection = self.inner.connection.lock().unwrap();
        if connection.is_none() {
            // Never create the database on read: a fresh install has no record,
            // and neither has an in-memory database before its first write.
            if !self.inner.path.is_file() {
                return None;
            }
            *connection = Some(Connection::open(&self.inner.path).ok()?);
        }
        let (stored, payload): (u32, String) = connection
            .as_ref()?
            .query_row(
                &format!("SELECT version, payload FROM {table} WHERE state = 0"),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok()?;
        (stored == version)
            .then(|| serde_json::from_str(&payload).ok())
            .flatten()
    }

    fn write(&self, table: &str, version: u32, payload: &str) -> Result<(), String> {
        let path = &self.inner.path;
        let failed = |error: &dyn std::fmt::Display| format!("{table} {}: {error}", path.display());
        let mut connection = self.inner.connection.lock().unwrap();
        if connection.is_none() {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent).map_err(|error| failed(&error))?;
            }
            *connection = Some(Connection::open(path).map_err(|error| failed(&error))?);
        }
        let connection = connection.as_ref().expect("connection opened above");
        connection
            .execute_batch(&format!(
                "PRAGMA application_id = {APPLICATION_ID}; {}",
                schema(table)
            ))
            .map_err(|error| failed(&error))?;
        connection
            .execute(
                &format!(
                    "INSERT INTO {table}(state, version, payload) VALUES (0, ?1, ?2) \
                     ON CONFLICT(state) DO UPDATE SET version = excluded.version, payload = excluded.payload"
                ),
                params![version, payload],
            )
            .map_err(|error| failed(&error))?;
        Ok(())
    }
}

struct State<T> {
    current: Option<T>,
    pending: Option<T>,
    generation: u64,
    last_error: Option<String>,
}

struct Inner<T> {
    database: StateDatabase,
    table: &'static str,
    version: u32,
    debounce: Duration,
    state: Mutex<State<T>>,
}

impl<T> Inner<T>
where
    T: Clone + Serialize + Send,
{
    fn commit(&self, value: T) -> Result<(), String> {
        let payload = serde_json::to_string(&value).expect("records serialize");
        match self.database.write(self.table, self.version, &payload) {
            Ok(()) => {
                self.state.lock().unwrap().last_error = None;
                Ok(())
            }
            Err(error) => {
                let mut state = self.state.lock().unwrap();
                state.last_error = Some(error.clone());
                // A newer publish keeps priority over the failed value.
                if state.pending.is_none() {
                    state.pending = Some(value);
                }
                Err(error)
            }
        }
    }
}

impl<T> PendingRecord for Inner<T>
where
    T: Clone + Serialize + Send,
{
    fn generation(&self) -> u64 {
        self.state.lock().unwrap().generation
    }

    fn write_pending(&self) -> Result<(), String> {
        let pending = self.state.lock().unwrap().pending.take();
        match pending {
            Some(value) => self.commit(value),
            None => Ok(()),
        }
    }
}

/// One record's handle in the [`StateDatabase`].
pub struct RecordStore<T> {
    inner: Arc<Inner<T>>,
}

impl<T> Clone for RecordStore<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> RecordStore<T>
where
    T: Clone + PartialEq + Serialize + DeserializeOwned + Send + 'static,
{
    pub fn current(&self) -> Option<T> {
        self.inner.state.lock().unwrap().current.clone()
    }

    pub fn last_error(&self) -> Option<String> {
        self.inner.state.lock().unwrap().last_error.clone()
    }

    /// Replace the record. Returns the debounced write to schedule, or
    /// `None` when the value equals the current one.
    pub fn publish(&self, value: T) -> Option<DebouncedWrite> {
        let generation = {
            let mut state = self.inner.state.lock().unwrap();
            if state.current.as_ref() == Some(&value) {
                return None;
            }
            state.current = Some(value.clone());
            state.pending = Some(value);
            state.generation += 1;
            state.generation
        };
        let inner = self.inner.clone();
        Some(DebouncedWrite {
            delay: inner.debounce,
            generation,
            record: inner,
        })
    }

    /// The write of this record's pending value, for the caller to run off
    /// the foreground.
    pub fn flush_job(&self) -> FlushJob {
        FlushJob(vec![self.inner.clone()])
    }
}

/// A coalesced write waiting out its debounce.
pub struct DebouncedWrite {
    delay: Duration,
    generation: u64,
    record: Arc<dyn PendingRecord>,
}

impl DebouncedWrite {
    /// How long to wait before [`run`](Self::run).
    pub fn delay(&self) -> Duration {
        self.delay
    }

    /// Write the record unless a newer publish or a flush superseded this
    /// write. Only one of the debounce and a flush can take the pending
    /// write; both persist the same value, so whichever runs first wins.
    pub fn run(self) {
        if self.record.generation() == self.generation {
            let _ = self.record.write_pending();
        }
    }
}

/// Pending writes to run together.
pub struct FlushJob(Vec<Arc<dyn PendingRecord>>);

impl FlushJob {
    /// Attempt every record and report the first failure.
    pub fn run(self) -> Result<(), String> {
        self.0
            .iter()
            .map(|record| record.write_pending())
            .fold(Ok(()), Result::and)
    }
}

pub fn test_schema(table: &str) -> String {
    schema(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
    struct Value(u32);

    const DEBOUNCE: Duration = Duration::from_millis(300);

    fn open(path: &std::path::Path) -> StateDatabase {
        StateDatabase::open(path.to_owned())
    }

    #[test]
    fn records_share_one_file_and_flush_together() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let database = open(&path);
        let first = database.record::<Value>("first", 1, DEBOUNCE);
        let second = database.record::<Value>("second", 1, DEBOUNCE);
        let write = first.publish(Value(1)).unwrap();
        assert_eq!(write.delay(), DEBOUNCE);
        second.publish(Value(2));
        // A debounced write racing the flush on one connection writes once.
        database.flush_job().run().unwrap();
        write.run();
        assert_eq!(first.last_error(), None);
        assert_eq!(second.last_error(), None);

        let reopened = open(&path);
        assert_eq!(
            reopened.record::<Value>("first", 1, DEBOUNCE).current(),
            Some(Value(1))
        );
        assert_eq!(
            reopened.record::<Value>("second", 1, DEBOUNCE).current(),
            Some(Value(2))
        );
    }

    #[test]
    fn publishing_an_equal_value_schedules_nothing_and_a_newer_one_supersedes() {
        let database = StateDatabase::in_memory();
        let record = database.record::<Value>("value", 1, DEBOUNCE);
        let first = record.publish(Value(1)).unwrap();
        assert!(record.publish(Value(1)).is_none());
        let second = record.publish(Value(2)).unwrap();
        first.run();
        assert!(record.last_error().is_none());
        // The superseded write left the value pending for the newer one.
        second.run();
        assert_eq!(record.current(), Some(Value(2)));
    }

    #[test]
    fn a_damaged_record_leaves_the_others_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let database = open(&path);
        let good = database.record::<Value>("good", 1, DEBOUNCE);
        let bad = database.record::<Value>("bad", 1, DEBOUNCE);
        good.publish(Value(7));
        bad.publish(Value(8));
        database.flush_job().run().unwrap();
        drop((database, good, bad));
        let connection = Connection::open(&path).unwrap();
        connection
            .execute("UPDATE bad SET payload = '\"not a number\"'", [])
            .unwrap();
        drop(connection);

        let reopened = open(&path);
        assert_eq!(reopened.record::<Value>("bad", 1, DEBOUNCE).current(), None);
        assert_eq!(
            reopened.record::<Value>("good", 1, DEBOUNCE).current(),
            Some(Value(7))
        );
    }

    #[test]
    fn flush_skips_dropped_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let database = open(&path);
        database.record::<Value>("dropped", 1, DEBOUNCE);
        let kept = database.record::<Value>("kept", 1, DEBOUNCE);
        kept.publish(Value(3));
        database.flush_job().run().unwrap();
        assert!(path.exists());
    }
}
