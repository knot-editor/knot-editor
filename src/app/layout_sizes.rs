//! The sizes the user dragged layout members to, remembered by name.
//!
//! A layout member that sets `resize` shows a handle on its leading edge.
//! Dragging it stores the member's size, in logical pixels, under that name
//! for the whole application, and the size is one record in the application
//! state database, so it survives restarts.

use std::collections::BTreeMap;
use std::time::Duration;

use gpui::{App, Global};

use super::record_store::{RecordStore, StateDatabase};

const VERSION: u32 = 1;
const WRITE_DEBOUNCE: Duration = Duration::from_millis(300);

/// The smallest and largest size a drag leaves a member, in logical pixels.
pub(crate) const MIN_SIZE: f32 = 40.;
pub(crate) const MAX_SIZE: f32 = 480.;

#[derive(Default)]
pub(crate) struct LayoutSizes {
    sizes: BTreeMap<String, f32>,
    record: Option<RecordStore<BTreeMap<String, f32>>>,
}

impl Global for LayoutSizes {}

impl LayoutSizes {
    pub(crate) fn open(database: &StateDatabase) -> Self {
        let record = database.record("layout_sizes", VERSION, WRITE_DEBOUNCE);
        Self {
            sizes: record.current().unwrap_or_default(),
            record: Some(record),
        }
    }
}

/// The remembered size under `name`, if the user dragged one.
pub(crate) fn remembered(name: &str, cx: &App) -> Option<f32> {
    cx.try_global::<LayoutSizes>()?.sizes.get(name).copied()
}

/// Remember `size` under `name`, within the allowed range, and return it.
pub(crate) fn remember(name: &str, size: f32, cx: &mut App) -> f32 {
    let size = size.clamp(MIN_SIZE, MAX_SIZE);
    let sizes = cx.default_global::<LayoutSizes>();
    sizes.sizes.insert(name.to_owned(), size);
    if let Some(record) = &sizes.record {
        record.publish(sizes.sizes.clone());
    }
    size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    async fn sizes_clamp_and_survive_reopen(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let database = StateDatabase::open(path.clone(), cx.background_executor.clone());
        cx.update(|cx| {
            cx.set_global(LayoutSizes::open(&database));
            assert_eq!(remembered("a", cx), None);
            assert_eq!(remember("a", 120., cx), 120.);
            assert_eq!(remember("b", 1., cx), MIN_SIZE);
            assert_eq!(remember("c", 9999., cx), MAX_SIZE);
        });
        database.flush().await.unwrap();
        drop(database);

        let reopened = StateDatabase::open(path, cx.background_executor.clone());
        let sizes = LayoutSizes::open(&reopened);
        assert_eq!(sizes.sizes.get("a"), Some(&120.));
        assert_eq!(sizes.sizes.get("b"), Some(&MIN_SIZE));
    }
}
