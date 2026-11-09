//! Recent-tab walks and the stack of recently closed tabs.

use super::{PaneId, TabId};

/// An uncommitted walk through one pane's recently used tabs. It starts on the
/// most recent tab and wraps in both directions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecentWalk {
    pane: PaneId,
    order: Vec<TabId>,
    index: usize,
}

impl RecentWalk {
    pub fn new(pane: PaneId, order: Vec<TabId>) -> Self {
        assert!(!order.is_empty(), "a walk needs a tab");
        Self {
            pane,
            order,
            index: 0,
        }
    }

    pub fn pane(&self) -> PaneId {
        self.pane
    }

    /// The tab the walk is on.
    pub fn current(&self) -> TabId {
        self.order[self.index]
    }

    /// Move one tab forward or back in recency order and return it.
    pub fn step(&mut self, forward: bool) -> TabId {
        let count = self.order.len();
        self.index = if forward {
            (self.index + 1) % count
        } else {
            (self.index + count - 1) % count
        };
        self.current()
    }
}

/// The most recently closed entries, oldest dropped first past `limit`.
#[derive(Clone, Debug)]
pub struct RecentlyClosed<T> {
    limit: usize,
    entries: Vec<T>,
}

impl<T> RecentlyClosed<T> {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: T) {
        self.entries.push(entry);
        if self.entries.len() > self.limit {
            self.entries.remove(0);
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        self.entries.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walk_wraps_in_both_directions() {
        let tabs = [TabId(1), TabId(2), TabId(3)];
        let mut walk = RecentWalk::new(PaneId(1), tabs.to_vec());
        assert_eq!(walk.current(), TabId(1));
        assert_eq!(walk.step(true), TabId(2));
        assert_eq!(walk.step(true), TabId(3));
        assert_eq!(walk.step(true), TabId(1));
        assert_eq!(walk.step(false), TabId(3));
        assert_eq!(walk.pane(), PaneId(1));
    }

    #[test]
    fn the_closed_stack_returns_newest_first_and_forgets_the_oldest() {
        let mut closed = RecentlyClosed::new(2);
        closed.push(1);
        closed.push(2);
        closed.push(3);
        assert_eq!(closed.pop(), Some(3));
        assert_eq!(closed.pop(), Some(2));
        assert_eq!(closed.pop(), None);
    }
}
