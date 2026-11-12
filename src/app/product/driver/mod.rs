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

/// Run until `shell` holds no keys and the extension pool is quiet: no
/// package has work in progress, and none finished a turn while the
/// foreground ran what it had.
pub(super) fn settle(shell: &Entity<KnotWindow>, cx: &mut TestAppContext) {
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
        let held = cx.read(|cx| shell.read(cx).holds_keys());
        if after == before && !after.busy && !held {
            return;
        }
        if after.finished_turns == before.finished_turns {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                pool.wait_for_turn(after.finished_turns, remaining),
                "the session never settled: {}",
                if held {
                    "it still holds keys"
                } else {
                    "a package is still busy"
                }
            );
        }
    }
}
