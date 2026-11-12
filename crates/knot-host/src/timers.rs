//! Extension timers, independent of V8 mechanics.
//!
//! Each lifecycle owns one [`TimerTable`] of timer IDs and their armed waits.
//! A [`TimerDriver`] supplied by the embedder does the waiting; on expiry the
//! table's callback receives a [`TimerExpiry`], which the engine queues as a
//! root and accepts only while [`TimerTable::take`] still recognizes it.

use std::{collections::HashMap, sync::Arc, time::Duration};

/// Waits for a duration and then calls back, off any particular thread.
pub trait TimerDriver: Send + Sync {
    /// Calls `expire` once `delay` has passed, unless the returned guard is
    /// dropped first.
    fn start(&self, delay: Duration, expire: Box<dyn FnOnce() + Send>) -> TimerGuard;
}

/// Keeps one driver wait alive; dropping it cancels the wait.
pub struct TimerGuard(#[allow(dead_code, reason = "held for its drop")] Box<dyn Send>);

impl TimerGuard {
    pub fn new(inner: impl Send + 'static) -> Self {
        Self(Box::new(inner))
    }
}

/// One expiry of one arming of a timer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerExpiry {
    pub id: u64,
    arm: u64,
}

struct Armed {
    arm: u64,
    _guard: Option<TimerGuard>,
}

/// The timers of one extension lifecycle.
pub struct TimerTable {
    driver: Arc<dyn TimerDriver>,
    expire: Arc<dyn Fn(TimerExpiry) + Send + Sync>,
    next_id: u64,
    next_arm: u64,
    armed: HashMap<u64, Armed>,
}

impl TimerTable {
    pub fn new(
        driver: Arc<dyn TimerDriver>,
        expire: impl Fn(TimerExpiry) + Send + Sync + 'static,
    ) -> Self {
        Self {
            driver,
            expire: Arc::new(expire),
            next_id: 1,
            next_arm: 1,
            armed: HashMap::new(),
        }
    }

    /// Allocates a timer and arms it; `None` expires immediately.
    pub fn start(&mut self, delay: Option<Duration>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.restart(id, delay);
        id
    }

    /// Arms an allocated timer again, replacing any wait in progress.
    pub fn restart(&mut self, id: u64, delay: Option<Duration>) {
        if id == 0 || id >= self.next_id {
            return;
        }
        let arm = self.next_arm;
        self.next_arm += 1;
        let expiry = TimerExpiry { id, arm };
        let guard = match delay {
            Some(delay) => {
                let expire = Arc::clone(&self.expire);
                Some(self.driver.start(delay, Box::new(move || expire(expiry))))
            }
            None => None,
        };
        self.armed.insert(id, Armed { arm, _guard: guard });
        if delay.is_none() {
            (self.expire)(expiry);
        }
    }

    pub fn cancel(&mut self, id: u64) {
        self.armed.remove(&id);
    }

    /// Disarms the timer if `expiry` belongs to its current arming, which
    /// a cancel or restart since has superseded otherwise.
    pub fn take(&mut self, expiry: TimerExpiry) -> bool {
        let current = self
            .armed
            .get(&expiry.id)
            .is_some_and(|armed| armed.arm == expiry.arm);
        if current {
            self.armed.remove(&expiry.id);
        }
        current
    }

    /// Cancels every timer.
    pub fn clear(&mut self) {
        self.armed.clear();
    }
}

/// A driver advanced by hand, so tests never wait on the wall clock.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Default)]
pub struct ManualTimers {
    state: Arc<std::sync::Mutex<ManualState>>,
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
struct ManualState {
    now: Duration,
    next: u64,
    waits: std::collections::BTreeMap<(Duration, u64), Box<dyn FnOnce() + Send>>,
}

#[cfg(any(test, feature = "test-support"))]
impl ManualTimers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves time forward, then calls back every wait that is due, in
    /// deadline order and then in start order. A wait started meanwhile
    /// counts from the new time, so a callback re-arming from a worker
    /// cannot race this advance.
    pub fn advance(&self, by: Duration) {
        let due = {
            let mut state = self.state.lock().unwrap();
            state.now += by;
            let now = state.now;
            let later = state.waits.split_off(&(now, u64::MAX));
            std::mem::replace(&mut state.waits, later)
        };
        for expire in due.into_values() {
            expire();
        }
    }

    /// How many waits are in progress.
    pub fn pending(&self) -> usize {
        self.state.lock().unwrap().waits.len()
    }
}

#[cfg(any(test, feature = "test-support"))]
struct ManualWait {
    state: std::sync::Weak<std::sync::Mutex<ManualState>>,
    key: (Duration, u64),
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for ManualWait {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            let removed = state.lock().unwrap().waits.remove(&self.key);
            drop(removed);
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl TimerDriver for ManualTimers {
    fn start(&self, delay: Duration, expire: Box<dyn FnOnce() + Send>) -> TimerGuard {
        let mut state = self.state.lock().unwrap();
        let key = (state.now + delay, state.next);
        state.next += 1;
        state.waits.insert(key, expire);
        TimerGuard::new(ManualWait {
            state: Arc::downgrade(&self.state),
            key,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn table() -> (ManualTimers, TimerTable, Arc<Mutex<Vec<TimerExpiry>>>) {
        let driver = ManualTimers::new();
        let fired = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&fired);
        let table = TimerTable::new(Arc::new(driver.clone()), move |expiry| {
            sink.lock().unwrap().push(expiry)
        });
        (driver, table, fired)
    }

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn expiries_follow_deadline_then_start_order() {
        let (driver, mut table, fired) = table();
        let late = table.start(Some(ms(20)));
        let first = table.start(Some(ms(10)));
        let second = table.start(Some(ms(10)));
        driver.advance(ms(9));
        assert!(fired.lock().unwrap().is_empty());
        driver.advance(ms(11));
        let ids = fired
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, [first, second, late]);
    }

    #[test]
    fn take_accepts_only_the_current_arming() {
        let (driver, mut table, fired) = table();
        let id = table.start(Some(ms(10)));
        driver.advance(ms(10));
        let stale = fired.lock().unwrap()[0];
        table.restart(id, Some(ms(10)));
        assert!(!table.take(stale));
        driver.advance(ms(10));
        let current = fired.lock().unwrap()[1];
        assert!(table.take(current));
        assert!(!table.take(current));
    }

    #[test]
    fn cancel_and_clear_drop_driver_waits() {
        let (driver, mut table, fired) = table();
        let id = table.start(Some(ms(10)));
        table.start(Some(ms(10)));
        assert_eq!(driver.pending(), 2);
        table.cancel(id);
        assert_eq!(driver.pending(), 1);
        table.clear();
        assert_eq!(driver.pending(), 0);
        driver.advance(ms(20));
        assert!(fired.lock().unwrap().is_empty());
    }

    #[test]
    fn immediates_expire_without_the_driver() {
        let (driver, mut table, fired) = table();
        let id = table.start(None);
        assert_eq!(driver.pending(), 0);
        let expiry = fired.lock().unwrap()[0];
        assert_eq!(expiry.id, id);
        assert!(table.take(expiry));
    }

    #[test]
    fn restart_ignores_unallocated_ids() {
        let (driver, mut table, fired) = table();
        table.restart(7, Some(ms(1)));
        table.restart(0, None);
        driver.advance(ms(1));
        assert!(fired.lock().unwrap().is_empty());
    }
}
