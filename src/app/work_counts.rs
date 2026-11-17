//! Test-only counts of the work a step causes: window draws, extension host
//! requests by operation, and the time spent in background syntax parses, so
//! tests can guard what one key costs and benchmarks can time it.

use std::{cell::RefCell, time::Duration};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WorkCounts {
    pub(crate) draws: usize,
    pub(crate) requests: Vec<&'static str>,
    /// The thread time of background syntax parses and of freeing the layers
    /// they replace, which a test executor runs on the foreground thread.
    pub(crate) background_parses: Duration,
}

thread_local! {
    static COUNTS: RefCell<WorkCounts> = RefCell::default();
}

pub(crate) fn draw() {
    COUNTS.with(|counts| counts.borrow_mut().draws += 1);
}

pub(crate) fn request(operation: &'static str) {
    COUNTS.with(|counts| counts.borrow_mut().requests.push(operation));
}

/// Run background syntax work, counting its thread time.
pub(crate) fn background_parse<T>(parse: impl FnOnce() -> T) -> T {
    let started = thread_time();
    let parsed = parse();
    let elapsed = thread_time() - started;
    COUNTS.with(|counts| counts.borrow_mut().background_parses += elapsed);
    parsed
}

/// The CPU time the current thread has used, which leaves out time it spent
/// blocked, such as waiting for the extension pool.
#[cfg(unix)]
pub(crate) fn thread_time() -> Duration {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `time` is a valid timespec for the call to fill.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
    assert_eq!(result, 0, "the thread clock is unavailable");
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

/// Wall time since the first call, where no thread clock is available.
#[cfg(not(unix))]
pub(crate) fn thread_time() -> Duration {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed()
}

/// The work counted since the previous call.
pub(crate) fn take() -> WorkCounts {
    COUNTS.with(|counts| std::mem::take(&mut *counts.borrow_mut()))
}
