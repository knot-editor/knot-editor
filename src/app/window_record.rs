//! Native window hot exit: every window's roots and layout in one application
//! state record beside the tool-only snapshot.
//!
//! Recording starts once startup has restored and opened its windows, so
//! diagnostic windows and a launch whose config failed never write. Settled
//! window changes and window closure re-record every window through the store's
//! debounce. Quit captures the windows before closing them and stops recording,
//! so the closing windows are kept. Startup marks the record consumed durably
//! before restoring it, so a crash during restoration loses only the
//! arrangement and the next launch opens the default window.

use std::{cell::Cell, time::Duration};

use gpui::{App, Global, Subscription, Task};
use serde::{Deserialize, Serialize};

use super::{
    product::KnotWindow,
    record_store::{RecordStore, StateDatabase},
    window_layout::WindowLayout,
};

pub(crate) const WINDOW_RECORD_VERSION: u32 = 1;

const WRITE_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct WindowRecord {
    /// Set durably before a launch restores the record.
    pub(crate) consumed: bool,
    /// Index of the active window.
    pub(crate) active: Option<usize>,
    pub(crate) windows: Vec<RecordedWindow>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct RecordedWindow {
    pub(crate) roots: Vec<String>,
    pub(crate) layout: WindowLayout,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Recording {
    /// Before startup finishes opening windows.
    Waiting,
    Live,
    /// After quit captured the closing windows.
    Stopped,
}

pub(crate) struct ApplicationWindowRecord {
    store: RecordStore<WindowRecord>,
    recording: Cell<Recording>,
    _closed: Subscription,
}

impl Global for ApplicationWindowRecord {}

/// Load the record and observe window closure.
pub(crate) fn install(database: &StateDatabase, cx: &mut App) {
    let store = database.record("window_session", WINDOW_RECORD_VERSION, WRITE_DEBOUNCE);
    let closed = cx.on_window_closed(|cx, _| cx.defer(record));
    cx.set_global(ApplicationWindowRecord {
        store,
        recording: Cell::new(Recording::Waiting),
        _closed: closed,
    });
}

/// Mark an unconsumed record consumed and resolve with it once that mark is
/// durable. Resolves `None` without a record, or when the mark cannot be
/// written, so a record that keeps crashing startup is never retried.
pub(crate) fn take_for_restore(cx: &App) -> Task<Option<WindowRecord>> {
    let Some(state) = cx.try_global::<ApplicationWindowRecord>() else {
        return Task::ready(None);
    };
    let Some(record) = state
        .store
        .current()
        .filter(|record| !record.consumed && !record.windows.is_empty())
    else {
        return Task::ready(None);
    };
    state.store.publish(WindowRecord {
        consumed: true,
        ..record.clone()
    });
    let flush = state.store.flush();
    cx.background_executor()
        .spawn(async move { flush.await.ok().map(|()| record) })
}

/// Record every window from now on, starting with the current ones.
pub(crate) fn start_recording(cx: &mut App) {
    if let Some(state) = cx.try_global::<ApplicationWindowRecord>() {
        state.recording.set(Recording::Live);
    }
    record(cx);
}

/// Record the windows quit is about to close and stop recording.
pub(crate) fn record_for_quit(cx: &mut App) -> Option<Task<Result<(), String>>> {
    record(cx);
    let state = cx.try_global::<ApplicationWindowRecord>()?;
    state.recording.set(Recording::Stopped);
    Some(state.store.flush())
}

/// Re-record every window's roots and layout.
pub(crate) fn record(cx: &mut App) {
    if cx
        .try_global::<ApplicationWindowRecord>()
        .is_none_or(|record| record.recording.get() != Recording::Live)
    {
        return;
    }
    let active = crate::app::product::active_window(cx).map(|window| window.entity_id());
    let mut captured = Vec::new();
    for window in crate::app::product::windows(cx) {
        let layout = KnotWindow::capture_attached_layout(&window, cx);
        let state = window.read(cx);
        let recorded = RecordedWindow {
            roots: state.root_uris().iter().map(ToString::to_string).collect(),
            layout,
        };
        captured.push((
            state.creation_order(),
            active == Some(window.entity_id()),
            recorded,
        ));
    }
    captured.sort_by_key(|(order, _, _)| *order);
    let active_index = captured.iter().position(|(_, active, _)| *active);
    let windows = captured
        .into_iter()
        .map(|(_, _, window)| window)
        .collect::<Vec<_>>();
    cx.global::<ApplicationWindowRecord>()
        .store
        .publish(WindowRecord {
            consumed: false,
            active: active_index,
            windows,
        });
}

#[cfg(any(test, feature = "test-support"))]
impl ApplicationWindowRecord {
    pub(crate) fn publish_for_test(&self, record: WindowRecord) {
        self.store.publish(record);
    }
}

/// Stop recording as a crash would, without capturing the windows.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn stop_for_test(cx: &App) {
    if let Some(state) = cx.try_global::<ApplicationWindowRecord>() {
        state.recording.set(Recording::Stopped);
    }
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn current(cx: &App) -> Option<WindowRecord> {
    cx.try_global::<ApplicationWindowRecord>()
        .and_then(|state| state.store.current())
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn flush(cx: &App) -> Task<Result<(), String>> {
    cx.global::<ApplicationWindowRecord>().store.flush()
}
