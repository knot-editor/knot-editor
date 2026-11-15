//! Ordering keys for pieces.
//!
//! A locator is a sequence of `u32` digits compared lexicographically, chosen
//! when a piece is created so that locators increase in text order. A key
//! between any two neighbors always exists, because a longer sequence can
//! always be squeezed between two shorter ones, so a piece never has to be
//! renumbered. The buffer keeps a map from piece id to locator, which turns
//! "where is this piece" into a seek by locator.

use std::sync::Arc;

/// Largest distance a new locator moves away from its lower neighbor within
/// one digit. Capping it keeps room above a locator that is repeatedly
/// followed by new pieces, as when text is appended in order, while halving
/// the gap when the room is small.
const MAX_STEP: u32 = 1 << 16;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Locator(Arc<[u32]>);

impl Locator {
    /// A locator strictly between `lower` and `upper`, either of which may be
    /// absent at the ends of the text. `lower` must be less than `upper`.
    ///
    /// Every locator ends in a nonzero digit, which guarantees there is always
    /// room after a locator.
    pub fn between(lower: Option<&Locator>, upper: Option<&Locator>) -> Locator {
        let lower = lower.map_or(&[][..], |l| &l.0);
        let mut upper_tight = upper.map(|u| &*u.0);
        let mut digits = Vec::new();
        for i in 0.. {
            let lo = lower.get(i).copied().unwrap_or(0);
            let hi = upper_tight.map_or(u32::MAX, |u| u.get(i).copied().unwrap_or(u32::MAX));
            let gap = hi - lo;
            if gap >= 2 {
                digits.push(lo + (gap / 2).min(MAX_STEP));
                break;
            }
            digits.push(lo);
            if gap == 1 {
                // Already below the upper neighbor, which no longer bounds
                // the later digits.
                upper_tight = None;
            }
        }
        Locator(digits.into())
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(lower: Option<&Locator>, upper: Option<&Locator>) -> Locator {
        let new = Locator::between(lower, upper);
        assert!(lower.is_none_or(|l| l < &new), "{lower:?} < {new:?}");
        assert!(upper.is_none_or(|u| &new < u), "{new:?} < {upper:?}");
        assert_ne!(new.0.last(), Some(&0));
        new
    }

    #[test]
    fn first_locator_has_room_on_both_sides() {
        let first = check(None, None);
        check(None, Some(&first));
        check(Some(&first), None);
    }

    #[test]
    fn appending_in_order_stays_one_digit_for_many_pieces() {
        let mut last = check(None, None);
        for _ in 0..10_000 {
            last = check(Some(&last), None);
        }
        assert_eq!(last.len(), 1);
    }

    #[test]
    fn inserting_between_neighbors_orders_correctly() {
        let a = check(None, None);
        let b = check(Some(&a), None);
        let mut keys = vec![a, b];
        // Always insert in the middle of the widest-nested gap: both the
        // squeeze-below and squeeze-above directions.
        for round in 0..500 {
            let at = if round % 2 == 0 { 0 } else { keys.len() - 2 };
            let new = check(Some(&keys[at]), Some(&keys[at + 1]));
            keys.insert(at + 1, new);
        }
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn repeated_inserts_at_one_point_grow_slowly() {
        let a = check(None, None);
        let end = check(Some(&a), None);
        // Each new piece goes directly after `a`, before the previous one.
        let mut upper = end;
        for _ in 0..1_000 {
            upper = check(Some(&a), Some(&upper));
        }
        assert!(upper.len() <= 100, "depth {}", upper.len());
        // Each new piece goes directly before `end`, after the previous one.
        let a = check(None, None);
        let end = check(Some(&a), None);
        let mut lower = a;
        for _ in 0..1_000 {
            lower = check(Some(&lower), Some(&end));
        }
        assert!(lower.len() <= 2, "depth {}", lower.len());
    }
}
