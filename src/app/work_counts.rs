//! Test-only counts of the work a step causes: window draws and extension host
//! requests by operation, so tests can guard what one key costs.

use std::cell::RefCell;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WorkCounts {
    pub(crate) draws: usize,
    pub(crate) requests: Vec<&'static str>,
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

/// The work counted since the previous call.
pub(crate) fn take() -> WorkCounts {
    COUNTS.with(|counts| std::mem::take(&mut *counts.borrow_mut()))
}
