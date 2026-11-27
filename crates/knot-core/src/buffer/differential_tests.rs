//! Property tests: random edit streams are applied to a `TextBuffer` and to a
//! plain `String`, and every public read, line, UTF-16, snapshot, and position
//! query is compared against the `String` model. Failing streams shrink to a
//! minimal sequence of edits. Chunks are small under test, so a few large
//! inserts seal add chunks.

use super::*;
use proptest::collection::vec;
use proptest::prelude::*;

const FRAGMENTS: &[&str] = &[
    "a",
    "b",
    "xyz",
    " ",
    "word ",
    "fn main() {",
    "\n",
    "\n",
    "\r\n",
    "é",
    "naïve",
    "日本語",
    "😀",
    "\t",
    "}",
    "\n\n",
];

const ORIGINAL: &str = "fn main() {\n    println!(\"héllo 日本\");\r\n}\n";

/// Text of a number of fragments in `count`.
fn text(count: std::ops::RangeInclusive<usize>) -> impl Strategy<Value = String> {
    vec(prop::sample::select(FRAGMENTS), count).prop_map(|parts| parts.concat())
}

fn initial_text() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(ORIGINAL.to_string()),
        Just(String::new()),
        Just("no newline at all 😀".to_string()),
        text(0..=20),
    ]
}

/// Where an edit lands, resolved against the current model so it is always a
/// char boundary.
#[derive(Clone, Debug)]
enum Place {
    Start,
    End,
    At(usize),
}

impl Place {
    fn resolve(&self, model: &str) -> usize {
        let raw = match self {
            Place::Start => 0,
            Place::End => model.len(),
            Place::At(n) => n % (model.len() + 1),
        };
        floor_boundary(model, raw)
    }
}

fn floor_boundary(model: &str, mut offset: usize) -> usize {
    while !model.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn place() -> impl Strategy<Value = Place> {
    prop_oneof![
        1 => Just(Place::Start),
        1 => Just(Place::End),
        6 => any::<usize>().prop_map(Place::At),
    ]
}

#[derive(Clone, Debug)]
enum Op {
    Insert(Place, String),
    Delete(Place, Place),
    /// Replace up to the given number of bytes after the place.
    Replace(Place, usize, String),
}

/// An edit mix where one insert in six is large enough to seal an add chunk.
/// `replace` adds replaces, which push two edits.
fn op(replace: bool) -> BoxedStrategy<Op> {
    let insert = prop_oneof![
        5 => (place(), text(1..=8)),
        1 => (place(), text(40..=150)),
    ]
    .prop_map(|(at, text)| Op::Insert(at, text));
    let delete = (place(), place()).prop_map(|(a, b)| Op::Delete(a, b));
    if replace {
        let replace = (place(), 0..40usize, text(1..=8))
            .prop_map(|(at, span, text)| Op::Replace(at, span, text));
        prop_oneof![5 => insert, 3 => delete, 2 => replace].boxed()
    } else {
        prop_oneof![5 => insert, 3 => delete].boxed()
    }
}

/// Apply an op to both sides; returns the bytes it inserted.
fn apply(b: &mut TextBuffer, model: &mut String, op: &Op) -> usize {
    match op {
        Op::Insert(at, text) => {
            let at = at.resolve(model);
            b.insert(at, text);
            model.insert_str(at, text);
            text.len()
        }
        Op::Delete(a, c) => {
            let (a, c) = (a.resolve(model), c.resolve(model));
            let range = a.min(c)..a.max(c);
            b.delete(range.clone());
            model.replace_range(range, "");
            0
        }
        Op::Replace(at, span, text) => {
            let at = at.resolve(model);
            let end = floor_boundary(model, (at + span).min(model.len()));
            b.replace(at..end, text);
            model.replace_range(at..end, text);
            text.len()
        }
    }
}

fn model_line_starts(model: &str) -> Vec<(usize, usize)> {
    let mut starts = vec![(0, 0)];
    let mut utf16 = 0;
    for (i, ch) in model.char_indices() {
        utf16 += ch.len_utf16();
        if ch == '\n' {
            starts.push((i + 1, utf16));
        }
    }
    starts
}

fn model_utf16_of_offset(model: &str, offset: usize) -> usize {
    model[..offset].encode_utf16().count()
}

fn model_offset_of_utf16(model: &str, utf16: usize) -> usize {
    let mut units = 0;
    let mut offset = 0;
    for ch in model.chars() {
        if units >= utf16 {
            break;
        }
        units += ch.len_utf16();
        offset += ch.len_utf8();
    }
    offset
}

/// `count` evenly spread char boundaries of `model`, including both ends.
fn sample_offsets(model: &str, count: usize) -> Vec<usize> {
    let mut offsets: Vec<usize> = (0..=count)
        .map(|i| floor_boundary(model, model.len() * i / count))
        .collect();
    offsets.dedup();
    offsets
}

/// Compare every query against the model: every line start, a spread of
/// offsets and UTF-16 indexes, every non-boundary byte, and ranges between
/// the sampled offsets.
fn check_buffer(b: &TextBuffer, model: &str) {
    assert_eq!(b.len(), model.len());
    assert_eq!(b.is_empty(), model.is_empty());
    assert_eq!(b.read_range(0..b.len()), model);

    let starts = model_line_starts(model);
    assert_eq!(b.line_count(), starts.len());
    for (line, &(byte, _)) in starts.iter().enumerate() {
        assert_eq!(b.line_start(line), byte, "line_start({line})");
    }

    let offsets = sample_offsets(model, 40);
    for &offset in &offsets {
        let line = starts.partition_point(|&(byte, _)| byte <= offset) - 1;
        assert_eq!(b.line_of_offset(offset), line, "line_of_offset({offset})");
        assert_eq!(
            b.point_of_offset(offset),
            TextPoint {
                line,
                column: offset - starts[line].0
            },
            "point_of_offset({offset})"
        );
        assert_eq!(
            b.utf16_of_offset(offset),
            model_utf16_of_offset(model, offset),
            "utf16_of_offset({offset})"
        );
        assert!(b.is_char_boundary(offset));
    }
    let total_utf16 = model.encode_utf16().count();
    for i in 0..=40 {
        let utf16 = (total_utf16 + 2) * i / 40;
        assert_eq!(
            b.offset_of_utf16(utf16),
            model_offset_of_utf16(model, utf16),
            "offset_of_utf16({utf16})"
        );
    }
    for offset in 0..model.len() {
        if !model.is_char_boundary(offset) {
            assert!(!b.is_char_boundary(offset), "mid-scalar {offset}");
        }
    }
    for (i, &a) in offsets.iter().enumerate() {
        let c = offsets[(i * 7 + 3) % offsets.len()];
        let range = a.min(c)..a.max(c);
        assert_eq!(b.read_range(range.clone()), model[range]);
    }
}

fn check_snapshot(snapshot: &TextSnapshot, model: &str) {
    assert_eq!(snapshot.len(), model.len());
    assert_eq!(snapshot.read_range(0..model.len()), model);
    let offsets = sample_offsets(model, 12);
    for (i, &a) in offsets.iter().enumerate() {
        let c = offsets[(i * 5 + 1) % offsets.len()];
        let range = a.min(c)..a.max(c);
        assert_eq!(snapshot.read_range(range.clone()), model[range.clone()]);
        let streamed: String = snapshot.chunks(range.clone()).collect();
        assert_eq!(streamed, model[range]);
    }
    // Successive chunks from the start read the whole text.
    let mut offset = 0;
    let mut streamed = String::new();
    while offset < model.len() {
        let chunk = snapshot.chunk_at(offset);
        assert!(!chunk.is_empty(), "chunk_at({offset}) empty before the end");
        streamed.push_str(chunk);
        offset += chunk.len();
    }
    assert_eq!(streamed, model);
    assert_eq!(snapshot.chunk_at(model.len()), "");
}

/// A position token and the offset it should resolve to.
struct Tracked {
    position: Position,
    expected: usize,
}

/// Apply the edit log's `Split` records to a token, as the anchored-range
/// store does.
fn follow_splits(position: Position, splits: &[Split]) -> Position {
    let mut position = position;
    for split in splits {
        if position.piece() == split.old_piece && position.offset() as usize > split.split_offset {
            position = Position::new(
                split.new_piece,
                position.offset() - split.split_offset as u32,
            );
        }
    }
    position
}

fn splits_of(edit: &BufferEdit) -> &[Split] {
    match edit {
        BufferEdit::Insert { splits, .. } | BufferEdit::Delete { splits, .. } => splits,
    }
}

/// Re-resolve every tracked token after one primitive edit and compare with
/// the model. A token exactly at an edit boundary may land on either side,
/// and a token inside a deleted span must stop resolving.
fn check_positions(b: &TextBuffer, tracked: &mut Vec<Tracked>, edit: &BufferEdit) {
    let splits = splits_of(edit);
    tracked.retain_mut(|t| {
        t.position = follow_splits(t.position, splits);
        let resolved = b.resolve(t.position);
        match edit {
            BufferEdit::Insert {
                at, inserted_len, ..
            } => {
                let resolved = resolved.expect("an insert never deletes a piece");
                let shifted = t.expected + inserted_len;
                match t.expected.cmp(at) {
                    std::cmp::Ordering::Less => assert_eq!(resolved, t.expected),
                    std::cmp::Ordering::Greater => assert_eq!(resolved, shifted),
                    std::cmp::Ordering::Equal => assert!(
                        resolved == t.expected || resolved == shifted,
                        "boundary token resolved to {resolved}, expected {} or {shifted}",
                        t.expected
                    ),
                }
                t.expected = resolved;
                true
            }
            BufferEdit::Delete { range, .. } => {
                let removed = range.len();
                if t.expected < range.start {
                    assert_eq!(resolved, Some(t.expected));
                    true
                } else if t.expected > range.end {
                    assert_eq!(resolved, Some(t.expected - removed));
                    t.expected -= removed;
                    true
                } else if t.expected == range.start || t.expected == range.end {
                    assert!(
                        resolved.is_none() || resolved == Some(range.start),
                        "boundary token resolved to {resolved:?}, expected {}",
                        range.start
                    );
                    t.expected = range.start;
                    resolved.is_some()
                } else {
                    assert_eq!(resolved, None, "token inside a deleted span");
                    false
                }
            }
        }
    });
}

/// Top the tracked tokens up to 24, spread over the model.
fn issue_positions(b: &TextBuffer, model: &str, tracked: &mut Vec<Tracked>) {
    let mut i = 0;
    while tracked.len() < 24 {
        let at = floor_boundary(model, model.len() * i / 23);
        i += 1;
        match b.position_at(at) {
            Some(position) => {
                assert_eq!(b.resolve(position), Some(at), "fresh token at {at}");
                tracked.push(Tracked {
                    position,
                    expected: at,
                });
            }
            None => {
                assert!(model.is_empty());
                break;
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Every query matches the model after every edit, and the piece tree's
    /// invariants hold.
    #[test]
    fn edits_match_a_string_model(
        initial in initial_text(),
        ops in vec(op(true), 1..=40),
    ) {
        let mut b = TextBuffer::from_text(initial.as_str());
        let mut model = initial;
        for op in &ops {
            apply(&mut b, &mut model, op);
            b.validate();
            check_buffer(&b, &model);
        }
    }

    /// Large inserts seal add chunks without disturbing text, lines, or
    /// pieces.
    #[test]
    fn large_inserts_seal_chunks(
        initial in initial_text(),
        ops in vec(
            prop_oneof![
                (place(), text(80..=200)).prop_map(|(at, text)| Op::Insert(at, text)),
                (place(), place()).prop_map(|(a, b)| Op::Delete(a, b)),
            ],
            4..=12,
        ),
    ) {
        let mut b = TextBuffer::from_text(initial.as_str());
        let mut model = initial;
        let mut inserted = 0;
        for op in &ops {
            inserted += apply(&mut b, &mut model, op);
            b.validate();
            check_buffer(&b, &model);
        }
        if inserted >= ADD_CHUNK_SIZE {
            prop_assert!(!b.backings.sealed.is_empty(), "{inserted} bytes inserted, none sealed");
        }
    }

    /// A snapshot keeps reading the text it was taken from, whatever edits
    /// follow.
    #[test]
    fn snapshots_are_immutable(
        initial in initial_text(),
        steps in vec((op(true), any::<bool>()), 1..=30),
    ) {
        let mut b = TextBuffer::from_text(initial.as_str());
        let mut model = initial;
        let mut snapshots = vec![(b.snapshot(), model.clone())];
        for (op, snap) in &steps {
            apply(&mut b, &mut model, op);
            if *snap {
                let snapshot = b.snapshot();
                check_snapshot(&snapshot, &model);
                snapshots.push((snapshot, model.clone()));
            }
        }
        check_snapshot(&b.snapshot(), &model);
        for (snapshot, expected) in &snapshots {
            check_snapshot(snapshot, expected);
        }
    }

    /// Position tokens follow primitive edits through the edit log: shifted
    /// past inserts and deletes, stopping to resolve inside a deleted span.
    #[test]
    fn positions_follow_edits(
        initial in initial_text(),
        ops in vec(op(false), 1..=30),
    ) {
        let mut b = TextBuffer::from_text(initial.as_str());
        let mut model = initial;
        let mut tracked: Vec<Tracked> = Vec::new();
        for op in &ops {
            issue_positions(&b, &model, &mut tracked);
            let seq = b.edit_seq();
            apply(&mut b, &mut model, op);
            let edits = b.edits_since(seq);
            // An empty delete logs nothing.
            if !tracked.is_empty() && !edits.is_empty() {
                assert_eq!(edits.len(), 1, "a primitive edit logs exactly one event");
                check_positions(&b, &mut tracked, &edits[0]);
            }
        }
    }

    /// Each logged edit carries the line and column of its offsets in the
    /// text it saw, edit by edit.
    #[test]
    fn edit_points_describe_the_text_each_edit_saw(
        initial in initial_text(),
        ops in vec(op(true), 1..=30),
    ) {
        fn point(text: &str, offset: usize) -> TextPoint {
            let before = &text[..offset];
            let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
            TextPoint {
                line: before.matches('\n').count(),
                column: offset - line_start,
            }
        }

        let mut b = TextBuffer::from_text(initial.as_str());
        let mut text = initial;
        for op in &ops {
            let seq = b.edit_seq();
            let mut model = text.clone();
            apply(&mut b, &mut model, op);
            let inserted = match op {
                Op::Insert(_, t) | Op::Replace(_, _, t) => t.as_str(),
                Op::Delete(..) => "",
            };
            // Replay the log against the text the edits saw.
            for edit in b.edits_since(seq) {
                match edit {
                    BufferEdit::Delete { range, start_point, end_point, .. } => {
                        assert_eq!(*start_point, point(&text, range.start));
                        assert_eq!(*end_point, point(&text, range.end));
                        text.replace_range(range.clone(), "");
                    }
                    BufferEdit::Insert { at, inserted_len, at_point, end_point, .. } => {
                        assert_eq!(*at_point, point(&text, *at));
                        text.insert_str(*at, inserted);
                        assert_eq!(*end_point, point(&text, at + inserted_len));
                    }
                }
            }
            assert_eq!(text, model);
        }
    }
}
