//! The application zoom level, which scales every product window.
//!
//! One integer level applies to the whole application; each step scales by
//! [`STEP`]. Product windows set their rem size from it, which scales every
//! rem-styled surface: styles given in pixels at 100% use [`rems`], and
//! painting that measures in pixels multiplies by [`factor`] through
//! [`scaled`]. Editors and terminals
//! multiply their `font.size` by it. The level is one record in the
//! application state database, so it survives restarts.

use std::time::Duration;

use gpui::{App, Global, Pixels, Rems, px};
use serde::{Deserialize, Serialize};

use super::record_store::{RecordStore, StateDatabase};

const ZOOM_VERSION: u32 = 1;
const WRITE_DEBOUNCE: Duration = Duration::from_millis(300);
/// The scale of one zoom step.
const STEP: f32 = 1.2;
pub(crate) const MIN_LEVEL: i32 = -4;
pub(crate) const MAX_LEVEL: i32 = 6;
/// gpui's rem size at 100% zoom.
pub(crate) const BASE_REM: f32 = 16.;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
struct ZoomRecord {
    level: i32,
}

/// The current level, and the record it persists to when the application
/// has a state database.
#[derive(Default)]
pub(crate) struct ApplicationZoom {
    level: i32,
    record: Option<RecordStore<ZoomRecord>>,
}

impl Global for ApplicationZoom {}

impl ApplicationZoom {
    pub(crate) fn open(database: &StateDatabase) -> Self {
        let record = database.record("zoom", ZOOM_VERSION, WRITE_DEBOUNCE);
        let level = record
            .current()
            .map_or(0, |record: ZoomRecord| record.level)
            .clamp(MIN_LEVEL, MAX_LEVEL);
        Self {
            level,
            record: Some(record),
        }
    }
}

pub(crate) fn level(cx: &App) -> i32 {
    cx.try_global::<ApplicationZoom>()
        .map_or(0, |zoom| zoom.level)
}

/// How much the current level scales the application's 100% sizes.
pub(crate) fn factor(cx: &App) -> f32 {
    STEP.powi(level(cx))
}

/// A length given in pixels at 100% zoom, at the current zoom.
pub(crate) fn scaled(pixels: f32, cx: &App) -> Pixels {
    px(pixels * factor(cx))
}

/// A style length given in pixels at 100% zoom, which follows the rem size
/// of the window it lays out in.
pub(crate) fn rems(pixels: f32) -> Rems {
    gpui::rems(pixels / BASE_REM)
}

/// Set the level within its range, persist it, and refresh every window.
/// Returns whether the level changed.
pub(crate) fn set_level(level: i32, cx: &mut App) -> bool {
    let level = level.clamp(MIN_LEVEL, MAX_LEVEL);
    if self::level(cx) == level {
        return false;
    }
    let zoom = cx.default_global::<ApplicationZoom>();
    zoom.level = level;
    if let Some(record) = &zoom.record {
        record.publish(ZoomRecord { level });
    }
    cx.refresh_windows();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    async fn the_level_survives_reopen(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let database = StateDatabase::open(path.clone(), cx.background_executor.clone());
        cx.update(|cx| {
            cx.set_global(ApplicationZoom::open(&database));
            assert_eq!(level(cx), 0);
            assert!(set_level(2, cx));
            assert_eq!(level(cx), 2);
        });
        database.flush().await.unwrap();
        drop(database);

        let reopened = StateDatabase::open(path, cx.background_executor.clone());
        let zoom = ApplicationZoom::open(&reopened);
        assert_eq!(zoom.level, 2);
    }

    #[gpui::test]
    fn levels_clamp_and_scale_by_steps(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            assert_eq!(factor(cx), 1.);
            assert!(set_level(1, cx));
            assert!((factor(cx) - 1.2).abs() < 1e-6);
            assert_eq!(scaled(10., cx), px(12.));
            assert!(set_level(MAX_LEVEL + 3, cx));
            assert_eq!(level(cx), MAX_LEVEL);
            assert!(!set_level(MAX_LEVEL + 1, cx));
            assert!(set_level(MIN_LEVEL - 1, cx));
            assert_eq!(level(cx), MIN_LEVEL);
        });
    }
}
