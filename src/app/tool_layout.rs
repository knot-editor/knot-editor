//! Automatic tool-layout memory: one versioned tool-only snapshot in application-owned SQLite.
//!
//! The snapshot records each dock's size, visibility, and split tree of
//! stable namespaced tool IDs with each pane's selection, and the dock each
//! hidden tool returns to. Document tabs, document split structure, terminal
//! contents, and empty placeholders are never serialized here.
//!
//! A fresh window reads the last published snapshot; already-open windows keep
//! their live arrangements. Only explicit user moves, resizes, and show/hide
//! changes publish a new default through [`ToolLayoutStore::publish`]. Restore,
//! window activation, and automatic presentation never publish. Writes run off
//! the foreground with trailing-edge debounce so divider dragging coalesces
//! into a single write; [`ToolLayoutStore::flush`] forces a pending write out,
//! which the quit path awaits before exiting.

use std::time::Duration;

use gpui::Global;
pub(crate) use knot_model::workbench::{TOOL_LAYOUT_VERSION, ToolLayoutSnapshot};

use super::record_store::{RecordStore, StateDatabase};

/// Trailing-edge debounce for automatic writes. Pointer-drag resizes publish
/// on every movement; only the settled arrangement reaches SQLite.
const WRITE_DEBOUNCE: Duration = Duration::from_millis(300);

/// Application-owned tool-layout persistence boundary.
///
/// The cached [`ToolLayoutSnapshot`] is updated synchronously on publish so
/// fresh windows immediately inherit user customizations; SQLite writes follow
/// off the foreground with debounce. A failed write keeps the live cache and
/// records the error without disturbing the window.
#[derive(Clone)]
pub(crate) struct ToolLayoutStore {
    record: RecordStore<ToolLayoutSnapshot>,
}

impl ToolLayoutStore {
    /// Load the persisted default synchronously; missing, corrupt, or
    /// versioned-away records fall back to product defaults without I/O noise.
    pub(crate) fn open(database: &StateDatabase) -> Self {
        Self {
            record: database.record("tool_layout", TOOL_LAYOUT_VERSION, WRITE_DEBOUNCE),
        }
    }

    pub(crate) fn current(&self) -> ToolLayoutSnapshot {
        self.record
            .current()
            .filter(|snapshot| snapshot.version == TOOL_LAYOUT_VERSION)
            .unwrap_or_else(ToolLayoutSnapshot::product_default)
    }

    #[cfg(test)]
    pub(crate) fn last_error(&self) -> Option<String> {
        self.record.last_error()
    }

    /// Record a user-customized arrangement and schedule a coalesced write.
    /// Equal snapshots are no-ops, so divider release without movement and
    /// repeated identical publishes never touch SQLite. Any publish marks the
    /// arrangement customized, including a return to product defaults.
    pub(crate) fn publish(&self, snapshot: ToolLayoutSnapshot) {
        let mut snapshot = snapshot;
        snapshot.customized = true;
        if self.current() != snapshot {
            self.record.publish(snapshot);
        }
    }

    /// Write the pending snapshot now, off the foreground; on failure the
    /// snapshot stays pending for a later retry. Quit flushes it through
    /// [`StateDatabase::flush`].
    #[cfg(test)]
    pub(crate) fn flush(&self) -> gpui::Task<Result<(), String>> {
        self.record.flush()
    }
}

pub(crate) struct ApplicationToolLayout(pub(crate) ToolLayoutStore);

impl Global for ApplicationToolLayout {}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    fn open_store(path: &Path, cx: &gpui::TestAppContext) -> ToolLayoutStore {
        ToolLayoutStore::open(&StateDatabase::open(
            path.to_owned(),
            cx.background_executor.clone(),
        ))
    }

    fn customized(size: f32) -> ToolLayoutSnapshot {
        let mut snapshot = ToolLayoutSnapshot::product_default();
        snapshot.docks[0].size = size;
        snapshot.docks.swap(0, 2);
        // Publish marks the arrangement customized; stored snapshots compare
        // against that form.
        snapshot.customized = true;
        snapshot
    }

    #[gpui::test]
    async fn snapshots_survive_database_reopen(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool-layout.sqlite3");
        let expected = customized(311.);
        let store = open_store(&path, cx);
        assert_eq!(store.current(), ToolLayoutSnapshot::product_default());
        store.publish(expected.clone());
        // The debounced write has not run yet: the cache is live but SQLite is untouched.
        assert!(!path.exists());
        store.flush().await.unwrap();
        assert!(path.exists());
        drop(store);
        let reopened = open_store(&path, cx);
        assert_eq!(reopened.current(), expected);
        assert_eq!(reopened.last_error(), None);
    }

    #[gpui::test]
    async fn coalesced_publishes_persist_only_the_latest(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("tool-layout.sqlite3");
        let store = open_store(&path, cx);
        store.publish(customized(300.));
        store.publish(customized(320.));
        assert!(!path.exists());
        store.flush().await.unwrap();
        drop(store);
        assert_eq!(open_store(&path, cx).current(), customized(320.));
    }

    #[gpui::test]
    async fn identical_republish_skips_writes(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool-layout.sqlite3");
        let store = open_store(&path, cx);
        assert!(!store.current().customized);
        store.publish(customized(320.));
        store.flush().await.unwrap();
        std::fs::remove_file(&path).unwrap();
        store.publish(customized(320.));
        store.flush().await.unwrap();
        assert!(!path.exists());
    }

    #[gpui::test]
    async fn publish_marks_customized_and_survives_reopen(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool-layout.sqlite3");
        let store = open_store(&path, cx);
        // Even confirming product defaults counts as a customization, so fresh
        // windows later inherit the confirmed arrangement.
        store.publish(ToolLayoutSnapshot::product_default());
        store.flush().await.unwrap();
        drop(store);
        let reopened = open_store(&path, cx);
        let mut expected = ToolLayoutSnapshot::product_default();
        expected.customized = true;
        assert_eq!(reopened.current(), expected);
    }

    #[gpui::test]
    async fn missing_corrupt_and_future_records_fall_back_to_defaults(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            open_store(&dir.path().join("absent.sqlite3"), cx).current(),
            ToolLayoutSnapshot::product_default()
        );
        let garbage = dir.path().join("garbage.sqlite3");
        std::fs::write(&garbage, "not sqlite").unwrap();
        assert_eq!(
            open_store(&garbage, cx).current(),
            ToolLayoutSnapshot::product_default()
        );
        let future = dir.path().join("future.sqlite3");
        let connection = rusqlite::Connection::open(&future).unwrap();
        connection
            .execute_batch(&crate::app::record_store::test_schema("tool_layout"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO tool_layout(state, version, payload) VALUES (0, 999, '{\"version\":999}')",
                [],
            )
            .unwrap();
        drop(connection);
        assert_eq!(
            open_store(&future, cx).current(),
            ToolLayoutSnapshot::product_default()
        );
    }

    #[gpui::test]
    async fn write_failure_keeps_live_state_and_falls_back_on_reload(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "not a directory").unwrap();
        let path = blocker.join("tool-layout.sqlite3");
        let store = open_store(&path, cx);
        let expected = customized(311.);
        store.publish(expected.clone());
        assert_eq!(store.current(), expected);
        assert!(store.flush().await.is_err());
        assert_eq!(store.current(), expected);
        assert!(store.last_error().is_some());
        // Nothing durable was written, so a fresh open falls back to defaults.
        assert_eq!(
            open_store(&path, cx).current(),
            ToolLayoutSnapshot::product_default()
        );
    }
}
