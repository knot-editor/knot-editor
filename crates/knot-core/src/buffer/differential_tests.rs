//! Randomized differential test: a seeded stream of edits is applied to a
//! `TextBuffer` and to a plain `String`, and every public read, line, UTF-16,
//! snapshot, and position query is compared against the `String` model.

use super::*;

/// Dependency-free xorshift64* generator, so every seed replays identically.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in `0..bound`; `bound` must be positive.
    fn below(&mut self, bound: usize) -> usize {
        (self.next() >> 11) as usize % bound
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

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

fn random_text(rng: &mut Rng, max_fragments: usize) -> String {
    let count = 1 + rng.below(max_fragments);
    (0..count)
        .map(|_| FRAGMENTS[rng.below(FRAGMENTS.len())])
        .collect()
}

/// A random char boundary of `model`, biased toward the start, the end, and
/// the `hot` offsets (edges of earlier edits, which are piece boundaries).
fn random_offset(rng: &mut Rng, model: &str, hot: &[usize]) -> usize {
    let mut offset = match rng.below(10) {
        0 => 0,
        1 => model.len(),
        2..=4 if !hot.is_empty() => hot[rng.below(hot.len())].min(model.len()),
        _ => rng.below(model.len() + 1),
    };
    while !model.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
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

/// Compare every query against the model. `full` also compares every line
/// and a dense sample of offsets; otherwise a few random ones are checked.
fn check_buffer(b: &TextBuffer, model: &str, rng: &mut Rng, full: bool) {
    assert_eq!(b.len(), model.len());
    assert_eq!(b.is_empty(), model.is_empty());
    assert_eq!(b.read_range(0..b.len()), model);

    let starts = model_line_starts(model);
    assert_eq!(b.line_count(), starts.len());
    if full {
        for (line, &(byte, _)) in starts.iter().enumerate() {
            assert_eq!(b.line_start(line), byte, "line_start({line})");
        }
    } else {
        for _ in 0..4 {
            let line = rng.below(starts.len());
            assert_eq!(b.line_start(line), starts[line].0, "line_start({line})");
        }
    }

    let offsets: Vec<usize> = if full {
        (0..=model.len())
            .filter(|&o| model.is_char_boundary(o))
            .collect()
    } else {
        (0..12).map(|_| random_offset(rng, model, &[])).collect()
    };
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
    let utf16s: Vec<usize> = if full {
        (0..=total_utf16 + 2).collect()
    } else {
        (0..12).map(|_| rng.below(total_utf16 + 3)).collect()
    };
    for utf16 in utf16s {
        assert_eq!(
            b.offset_of_utf16(utf16),
            model_offset_of_utf16(model, utf16),
            "offset_of_utf16({utf16})"
        );
    }
    if !model.is_empty() {
        for offset in 0..model.len() {
            if !model.is_char_boundary(offset) {
                assert!(!b.is_char_boundary(offset), "mid-scalar {offset}");
                if !full {
                    break;
                }
            }
        }
    }

    for _ in 0..6 {
        let a = random_offset(rng, model, &[]);
        let c = random_offset(rng, model, &[]);
        let range = a.min(c)..a.max(c);
        assert_eq!(b.read_range(range.clone()), model[range]);
    }
}

fn check_snapshot(snapshot: &TextSnapshot, model: &str, rng: &mut Rng) {
    assert_eq!(snapshot.len(), model.len());
    assert_eq!(snapshot.read_range(0..model.len()), model);
    for _ in 0..4 {
        let a = random_offset(rng, model, &[]);
        let c = random_offset(rng, model, &[]);
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

fn issue_positions(b: &TextBuffer, model: &str, rng: &mut Rng, tracked: &mut Vec<Tracked>) {
    while tracked.len() < 24 {
        let at = random_offset(rng, model, &[]);
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

fn run(seed: u64, steps: usize, initial: &str) {
    let mut rng = Rng::new(seed);
    let mut b = TextBuffer::from_text(initial);
    let mut model = initial.to_string();
    let mut hot: Vec<usize> = Vec::new();
    let mut tracked: Vec<Tracked> = Vec::new();
    let mut snapshots: Vec<(TextSnapshot, String)> = Vec::new();
    let mut inserted_bytes = 0usize;

    for step in 0..steps {
        issue_positions(&b, &model, &mut rng, &mut tracked);
        let seq = b.edit_seq();
        match rng.below(10) {
            0..=4 => {
                let at = random_offset(&mut rng, &model, &hot);
                // Occasional large inserts so add chunks seal.
                let max = if rng.chance(10) { 600 } else { 12 };
                let text = random_text(&mut rng, max);
                b.insert(at, &text);
                model.insert_str(at, &text);
                inserted_bytes += text.len();
                hot.extend([at, at + text.len()]);
            }
            5..=7 => {
                let a = random_offset(&mut rng, &model, &hot);
                let c = if rng.chance(70) {
                    let span = rng.below(40);
                    let mut end = (a + span).min(model.len());
                    while !model.is_char_boundary(end) {
                        end -= 1;
                    }
                    end
                } else {
                    random_offset(&mut rng, &model, &hot)
                };
                let range = a.min(c)..a.max(c);
                b.delete(range.clone());
                model.replace_range(range.clone(), "");
                hot.push(range.start);
            }
            _ => {
                let a = random_offset(&mut rng, &model, &hot);
                let span = rng.below(30);
                let mut end = (a + span).min(model.len());
                while !model.is_char_boundary(end) {
                    end -= 1;
                }
                let text = random_text(&mut rng, 8);
                b.replace(a..end, &text);
                model.replace_range(a..end, &text);
                inserted_bytes += text.len();
                hot.extend([a, a + text.len()]);
                // A replace pushes two edits; its tokens are re-issued.
                tracked.clear();
            }
        }
        if hot.len() > 512 {
            hot.drain(..256);
        }

        // One edit-log entry per primitive: verify tokens against it.
        let edits = b.edits_since(seq);
        // Nothing to follow after a replace (tokens re-issued), on an empty
        // buffer, or when an empty delete logged nothing.
        if !tracked.is_empty() && !edits.is_empty() {
            assert_eq!(edits.len(), 1, "a primitive edit logs exactly one event");
            check_positions(&b, &mut tracked, &edits[0]);
        }

        check_buffer(&b, &model, &mut rng, step % 40 == 0);
        for (snapshot, expected) in &snapshots {
            if rng.chance(10) {
                check_snapshot(snapshot, expected, &mut rng);
            }
        }
        if step % 25 == 0 {
            let snapshot = b.snapshot();
            check_snapshot(&snapshot, &model, &mut rng);
            snapshots.push((snapshot, model.clone()));
            if snapshots.len() > 6 {
                snapshots.remove(0);
            }
        }
    }
    for (snapshot, expected) in &snapshots {
        check_snapshot(snapshot, expected, &mut rng);
    }
    assert!(
        initial.len() + inserted_bytes > 2 * ADD_CHUNK_SIZE,
        "seed {seed} inserted too little text ({inserted_bytes} bytes) to seal chunks"
    );
}

const ORIGINAL: &str = "fn main() {\n    println!(\"héllo 日本\");\r\n}\n";

#[test]
fn differential_seed_1() {
    run(1, 900, ORIGINAL);
}

#[test]
fn differential_seed_2() {
    run(2, 900, ORIGINAL);
}

#[test]
fn differential_seed_3() {
    run(3, 900, "");
}

#[test]
fn differential_seed_4() {
    run(4, 900, ORIGINAL);
}

#[test]
fn differential_seed_5() {
    run(5, 900, "no newline at all 😀");
}

#[test]
fn differential_seed_6() {
    run(6, 900, ORIGINAL);
}
