use super::*;
use crate::buffer::TextPoint;
use proptest::collection::vec;
use proptest::prelude::*;

// Expected byte-offset transform for the randomized correctness oracle.
fn transform_offset(off: usize, edit: &BufferEdit) -> usize {
    match edit {
        BufferEdit::Insert {
            at, inserted_len, ..
        } => {
            if off < *at {
                off
            } else {
                off + inserted_len
            }
        }
        BufferEdit::Delete { range, .. } => {
            if off <= range.start {
                off
            } else if off >= range.end {
                off - (range.end - range.start)
            } else {
                range.start
            }
        }
    }
}

fn text() -> TextBuffer {
    TextBuffer::from_text("hello world")
}

#[test]
fn add_and_resolve_round_trips() {
    let b = text();
    let mut store = AnchoredRangeStore::new();
    let id = store.add(&b, 0, 5);
    assert_eq!(store.resolve(&b, id), Some(0..5));
    let id2 = store.add(&b, 6, 11);
    assert_eq!(store.resolve(&b, id2), Some(6..11));
}

#[test]
fn remove_drops_anchored_range() {
    let b = text();
    let mut store = AnchoredRangeStore::new();
    let id = store.add(&b, 0, 5);
    store.remove(id);
    assert!(store.resolve(&b, id).is_none());
    assert!(store.is_empty());
}

#[test]
fn insert_before_start_does_not_move_anchored_range() {
    let mut b = text();
    let mut store = AnchoredRangeStore::new();
    let id = store.add(&b, 6, 11);
    b.insert(0, ">>");
    store.stabilize(&b);
    // Both endpoints shifted by 2; the anchored range still covers "world".
    assert_eq!(store.resolve(&b, id), Some(8..13));
}

#[test]
fn insert_at_before_start_keeps_byte() {
    // start is `Before` (sticky-left): insert at the start offset keeps
    // the endpoint put (text lands before it).
    let mut b = text();
    let mut store = AnchoredRangeStore::new();
    let id = store.add(&b, 0, 5);
    b.insert(0, "X");
    store.stabilize(&b);
    // The `Before` start is sticky-left: it keeps pointing at the same
    // content ("h"), which the insert pushed to byte 1. The anchored range
    // still covers "hello", now at bytes 1..6 (it does NOT swallow "X").
    assert_eq!(store.resolve(&b, id), Some(1..6));
}

#[test]
fn insert_at_after_end_grows_anchored_range() {
    // end is `After` (sticky-right): insert at the end offset relocates
    // the endpoint past the inserted span.
    let mut b = text();
    let mut store = AnchoredRangeStore::new();
    let id = store.add(&b, 0, 5);
    b.insert(5, "X");
    store.stabilize(&b);
    assert_eq!(store.resolve(&b, id), Some(0..6));
}

#[test]
fn coalesced_insert_repairs_like_a_piece_boundary() {
    // Typing at the end of a typed span extends its piece instead of
    // adding one; boundary endpoints must move exactly as they do at the
    // end of an ordinary piece.
    let resolve_after_typing = |mut b: TextBuffer, mut store: AnchoredRangeStore| {
        let empty = store.add(&b, 3, 3);
        let ending = store.add(&b, 2, 3);
        let inside = store.add(&b, 1, 2);
        b.insert(3, "d");
        store.stabilize(&b);
        assert_eq!(b.read_range(0..b.len()), "abcd");
        [empty, ending, inside].map(|id| store.resolve(&b, id))
    };
    let mut typed = TextBuffer::from_text("ab");
    let mut typed_store = AnchoredRangeStore::new();
    typed.insert(2, "c");
    typed_store.stabilize(&typed);
    assert_eq!(
        resolve_after_typing(typed, typed_store),
        resolve_after_typing(TextBuffer::from_text("abc"), AnchoredRangeStore::new())
    );
}

#[test]
fn insert_at_after_end_extends() {
    let mut b = text();
    let mut store = AnchoredRangeStore::new();
    let id = store.add(&b, 0, 5);
    // "hello" then insert at offset 5 (right after "hello").
    b.insert(5, ">>>");
    store.stabilize(&b);
    // The After end relocates to 5 + 3 = 8.
    assert_eq!(store.resolve(&b, id), Some(0..8));
}

#[test]
fn delete_spanning_anchored_range_collapses_to_zero() {
    let mut b = text();
    let mut store = AnchoredRangeStore::new();
    let id = store.add(&b, 3, 8);
    // Delete [2, 9) which fully contains [3, 8).
    b.delete(2..9);
    store.stabilize(&b);
    // Fully deleted -> consumed, no extent; anchored range is removed entirely.
    assert_eq!(store.resolve(&b, id), None);
}

#[test]
fn persistent_range_collapses_at_a_full_deletion() {
    let mut b = text();
    let mut store = AnchoredRangeStore::new();
    let id = store.add_persistent(&b, 3, 8);

    b.delete(2..9);
    store.stabilize(&b);

    assert_eq!(store.resolve(&b, id), Some(2..2));
}

#[test]
fn delete_partial_left_collapses_start_to_surviving_edge() {
    let mut b = text();
    let mut store = AnchoredRangeStore::new();
    // "hello world": delete [0, 6) removes "hello " -> surviving "world".
    let id = store.add(&b, 2, 8);
    b.delete(0..6);
    store.stabilize(&b);
    // start (2) inside -> collapses to s=0; end (8) >= e=6 -> shifts to 2.
    assert_eq!(store.resolve(&b, id), Some(0..2));
}

#[test]
fn interior_split_stale_right_half_repaired() {
    // Build a buffer where an anchored range's end sits in the right half of
    // a piece, then split that piece with an insert and confirm the token
    // is repaired (content preserved), not invalidated.
    let mut b = TextBuffer::from_text("abcdefghij"); // single piece, len 10
    let mut store = AnchoredRangeStore::new();
    // Anchor [2, 7): end at 7 is in the right half of the original piece.
    let id = store.add(&b, 2, 7);
    assert_eq!(store.resolve(&b, id), Some(2..7));
    // Insert at offset 4 splits the original piece; "bcdefg" (the content
    // between the endpoints) is now preceded by an extra byte.
    b.insert(4, "X");
    store.stabilize(&b);
    // "abcdXefghij": original [2,7) ("cdefg") is now [2,8).
    assert_eq!(store.resolve(&b, id), Some(2..8));
}

#[test]
fn untouched_anchored_range_costs_nothing() {
    let mut b = text();
    let mut store = AnchoredRangeStore::new();
    let touched = store.add(&b, 0, 5);
    let _untouched = store.add(&b, 6, 11);
    // Edit at the start only affects the first anchored range.
    b.insert(0, "X");
    store.stabilize(&b);
    // `touched` start is `Before` (sticky-left): the "X" lands before it,
    // so "hello" shifts to 1..6.
    assert_eq!(store.resolve(&b, touched), Some(1..6));
    // The second anchored range shifted by the insert but was not specially
    // processed; its token stayed valid (stability, not O(all) work).
    assert_eq!(store.resolve(&b, _untouched), Some(7..12));
}

#[test]
fn zero_width_boundary_survives_empty_delete_and_reanchors_on_insert() {
    let mut b = TextBuffer::from_text("a");
    let mut store = AnchoredRangeStore::new();
    let id = store.add(&b, 1, 1);
    b.delete(0..1);
    store.stabilize(&b);
    assert_eq!(store.resolve(&b, id), Some(0..0));

    b.insert(0, "xy");
    store.stabilize(&b);
    assert_eq!(store.resolve(&b, id), Some(2..2));
}

// ---- Removal-on-consumption semantics ---------------------

#[test]
fn fully_consumed_anchored_range_removed_not_tombstoned() {
    let mut b = TextBuffer::from_text("hello world");
    let mut store = AnchoredRangeStore::new();
    let victim = store.add(&b, 6, 7);
    let other = store.add(&b, 0, 2);

    // Delete [5, 8) on "hello world" — fully contains [6, 7).
    b.delete(5..8);
    store.stabilize(&b);
    assert_eq!(b.read_range(0..b.len()), "hellorld");

    // Removed entirely — no tombstone and no iter_live entry.
    assert_eq!(store.resolve(&b, victim), None, "victim fully consumed");
    assert!(
        !store.iter_live().any(|(id, _)| *id == victim),
        "victim must not appear in iter_live"
    );
    assert_eq!(store.len(), 1, "only the unaffected anchored range remains");
    assert_eq!(store.resolve(&b, other), Some(0..2));

    // A later add re-uses no id; the victim's id is gone for good.
    let replacement = store.add(&b, 6, 7);
    assert_ne!(replacement, victim, "monotonic ids; victim id not reused");
    assert_eq!(store.resolve(&b, replacement), Some(6..7));
}

#[test]
fn consumed_anchored_range_stays_removed_through_undo_redo() {
    // The text-edit inverse (re-insert) does not revive a consumed
    // anchored range; the provider must re-publish under a fresh id.
    let mut b = TextBuffer::from_text("hello world");
    let mut store = AnchoredRangeStore::new();
    let victim = store.add(&b, 6, 7);

    b.delete(5..8); // "hellorld", victim fully consumed
    store.stabilize(&b);
    assert_eq!(store.resolve(&b, victim), None);

    // Undo the delete: the log re-emits a forward insert at byte 5;
    // stabilize re-runs through it, but the victim is already removed
    // and is not re-added.
    b.insert(5, " wo");
    store.stabilize(&b);
    assert_eq!(b.read_range(0..b.len()), "hello world");
    assert_eq!(
        store.resolve(&b, victim),
        None,
        "undo does not revive victim"
    );

    // Redo: delete again. The victim is still gone.
    b.delete(5..8);
    store.stabilize(&b);
    assert_eq!(b.read_range(0..b.len()), "hellorld");
    assert_eq!(
        store.resolve(&b, victim),
        None,
        "redo still finds no victim"
    );
}

// ---- Property: ranges follow edits like the offset-transform oracle ----

fn edit_insert(at: usize, len: usize) -> BufferEdit {
    BufferEdit::Insert {
        at,
        inserted_len: len,
        at_point: TextPoint::default(),
        end_point: TextPoint::default(),
        inserted_end: Position::new(0, len as u32),
        left_piece: None,
        pre_first_piece: None,
        splits: vec![],
    }
}
fn edit_delete(s: usize, e: usize) -> BufferEdit {
    BufferEdit::Delete {
        range: s..e,
        start_point: TextPoint::default(),
        end_point: TextPoint::default(),
        deleted_pieces: vec![],
        deleted_piece_lens: vec![],
        left_survivor: None,
        right_survivor: None,
        splits: vec![],
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// After every insert, delete, or replace, each anchored range resolves
    /// to what transforming its endpoints through the edit predicts. A range
    /// a delete swallowed from the inside stays removed.
    #[test]
    fn ranges_follow_edits_like_the_oracle(
        src in "[a-z ]{1,40}",
        ranges in vec((any::<usize>(), any::<usize>()), 1..=10),
        edits in vec((0..3u8, any::<usize>(), 1..=4usize), 1..=30),
    ) {
        let mut b = TextBuffer::from_text(src.as_str());
        let mut store = AnchoredRangeStore::new();
        let mut text = src.clone();

        let mut o_start = Vec::new();
        let mut o_end = Vec::new();
        let mut o_collapsed = vec![false; ranges.len()];
        let mut ids = Vec::new();
        for &(s, e) in &ranges {
            let (s, e) = (s % (b.len() + 1), e % (b.len() + 1));
            let (s, e) = (s.min(e), s.max(e));
            ids.push(store.add(&b, s, e));
            o_start.push(s);
            o_end.push(e);
        }

        for &(pick, at, span) in &edits {
            let len = b.len();
            let at = at % (len + 1);
            // Delete and replace need a byte to remove; at the end of the
            // buffer they fall back to an append.
            let pick = if pick != 0 && at >= len { 0 } else { pick };
            // The edit as primitives, each with the endpoints it consumes.
            let mut primitives = Vec::new();
            match pick {
                0 => {
                    b.insert(at, "xy");
                    text.insert_str(at, "xy");
                    primitives.push(edit_insert(at, 2));
                }
                1 => {
                    let end = (at + span).min(len);
                    b.delete(at..end);
                    text.replace_range(at..end, "");
                    primitives.push(edit_delete(at, end));
                }
                _ => {
                    let end = (at + span).min(len);
                    b.replace(at..end, "zz");
                    text.replace_range(at..end, "zz");
                    primitives.push(edit_delete(at, end));
                    primitives.push(edit_insert(at, 2));
                }
            }
            for i in 0..ranges.len() {
                if o_collapsed[i] {
                    continue;
                }
                for edit in &primitives {
                    // A delete consumes a range whose endpoint lies strictly
                    // inside it and leaves it empty; an empty range the
                    // delete doesn't touch stays valid at its point.
                    let swallowed = match edit {
                        BufferEdit::Delete { range, .. } => {
                            (o_start[i] > range.start && o_start[i] < range.end)
                                || (o_end[i] > range.start && o_end[i] < range.end)
                        }
                        BufferEdit::Insert { .. } => false,
                    };
                    o_start[i] = transform_offset(o_start[i], edit);
                    o_end[i] = transform_offset(o_end[i], edit);
                    if swallowed && o_start[i] >= o_end[i] {
                        o_collapsed[i] = true;
                        break;
                    }
                }
            }

            store.stabilize(&b);

            for i in 0..ranges.len() {
                let expected = if o_collapsed[i] {
                    None
                } else {
                    Some(o_start[i].min(o_end[i])..o_start[i].max(o_end[i]))
                };
                prop_assert_eq!(
                    store.resolve(&b, ids[i]),
                    expected,
                    "range {} diverges after edit at={} pick={} (buffer={:?})",
                    i, at, pick, text
                );
            }
            prop_assert_eq!(b.read_range(0..b.len()), text.as_str());
        }
    }
}
