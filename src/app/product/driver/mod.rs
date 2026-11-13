//! The product test driver behind [`crate::test_support`]: the real
//! application started headless, driven and observed through plain values.

pub(crate) mod application;
pub(crate) mod editing;
pub(crate) mod editor;
pub(crate) mod product;
pub(in crate::app::product) mod setup;
pub(crate) mod window;

use std::time::{Duration, Instant};

use gpui::{Entity, TestAppContext};

use super::KnotWindow;

/// How long a run may take to settle before the test fails.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);

/// Run until `ready` holds, failing after ten seconds.
///
/// Between checks the thread blocks instead of spinning: on the extension
/// pool's next finished turn while a package works, and otherwise briefly on
/// the clock for events the pool does not report, such as the file watcher
/// and PTYs.
#[track_caller]
pub fn wait_until(cx: &mut TestAppContext, mut ready: impl FnMut(&mut TestAppContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        // The pool can appear while a test waits, so look it up each time.
        let pool = cx.read(crate::app::extensions::host::pool);
        let finished = pool.as_ref().map(|pool| pool.activity().finished_turns);
        cx.run_until_parked();
        if ready(cx) {
            return;
        }
        assert!(Instant::now() < deadline, "the condition never held");
        match (&pool, finished) {
            (Some(pool), Some(finished)) if pool.activity().busy => {
                pool.wait_for_turn(finished, Duration::from_millis(5));
            }
            _ => std::thread::sleep(Duration::from_millis(1)),
        }
    }
}

/// Run until `window` holds no keys and the extension pool is quiet: no
/// package has work in progress, and none finished a turn while the
/// foreground ran what it had.
pub(super) fn settle(window: &Entity<KnotWindow>, cx: &mut TestAppContext) {
    let pool = cx.read(crate::app::extensions::host::pool);
    let Some(pool) = pool else {
        cx.run_until_parked();
        return;
    };
    let deadline = Instant::now() + SETTLE_TIMEOUT;
    loop {
        let before = pool.activity();
        cx.run_until_parked();
        let after = pool.activity();
        let held = cx.read(|cx| window.read(cx).holds_keys());
        if after == before && !after.busy && !held {
            return;
        }
        if after.finished_turns == before.finished_turns {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                pool.wait_for_turn(after.finished_turns, remaining),
                "the window never settled: {}",
                if held {
                    "it still holds keys"
                } else {
                    "a package is still busy"
                }
            );
        }
    }
}
