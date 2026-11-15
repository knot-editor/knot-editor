//! Times `TextBuffer` workloads: typing, bulk edits, snapshots, anchored-range
//! resolution, and line queries, at large document and piece counts.
//!
//! Usage: `cargo run --release -p knot-core --example buffer_bench -- [workload...]`
//!
//! With no arguments every workload runs; otherwise only the named ones
//! (`typing`, `replace_all`, `multi_cursor`, `snapshot`, `anchored`,
//! `last_screen`, `line_lookup`, `memory`). Workloads whose cost is expected
//! to be many seconds run once; the rest report percentiles or a mean.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use knot_core::anchored_range::AnchoredRangeStore;
use knot_core::buffer::TextBuffer;

/// Counts live heap bytes and allocation calls so memory per piece can be
/// measured without process RSS.
struct Counting;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards every call to `System` and only adds relaxed counters.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LIVE_BYTES.fetch_add(new_size, Ordering::Relaxed);
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// xorshift64*, so every run edits the same places.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as usize) % bound
    }
}

/// Rust-like text of `lines` lines, about 29 bytes per line, with the odd
/// multi-byte character so UTF-16 offsets differ from byte offsets.
fn fixture(lines: usize) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(lines * 30);
    for i in 0..lines {
        match i % 8 {
            0 => writeln!(text, "pub fn compute_{i}(input: u32) -> u32 {{ // {i}").unwrap(),
            1 => writeln!(text, "    let value_{i} = input * {i}; // {i}").unwrap(),
            2 => {
                if i % 64 == 2 {
                    writeln!(text, "    // naïve café — note {i}").unwrap()
                } else {
                    writeln!(text, "    // note {i}").unwrap()
                }
            }
            3 => writeln!(text, "    if value_{i} > 10 {{ // {i}").unwrap(),
            4 => writeln!(text, "        return value_{}; // {i}", i - 3).unwrap(),
            5 => writeln!(text, "    }}").unwrap(),
            6 => writeln!(text, "    value_{} // {i}", i - 5).unwrap(),
            _ => writeln!(text, "}}").unwrap(),
        }
    }
    text
}

fn micros(d: Duration) -> f64 {
    d.as_secs_f64() * 1e6
}

fn report_percentiles(name: &str, mut timings: Vec<Duration>) {
    timings.sort();
    let at = |p: usize| timings[(timings.len() * p / 100).min(timings.len() - 1)];
    println!(
        "{name}: n={} p50 {:.1} µs  p99 {:.1} µs  max {:.1} µs",
        timings.len(),
        micros(at(50)),
        micros(at(99)),
        micros(*timings.last().unwrap())
    );
}

fn report_total(name: &str, elapsed: Duration, count: usize) {
    println!(
        "{name}: {:.3} s total, {:.2} µs per item ({count} items)",
        elapsed.as_secs_f64(),
        micros(elapsed) / count as f64
    );
}

/// Mean time of `iterations` runs of `f`.
fn mean(name: &str, iterations: usize, mut f: impl FnMut()) {
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let elapsed = start.elapsed();
    println!(
        "{name}: mean {:.2} µs ({iterations} runs)",
        micros(elapsed) / iterations as f64
    );
}

/// A buffer over `lines` lines of fixture text with about `pieces` pieces,
/// made by inserting one character at random line starts. Pieces are not
/// public, so this counts edits: an insert inside the original text adds a
/// split and a piece, so `pieces / 2` edits reach the target.
///
/// The edit log is drained as it grows, as the application does, through
/// `store` when one observes the buffer.
fn buffer_with_pieces(
    lines: usize,
    pieces: usize,
    mut store: Option<&mut AnchoredRangeStore>,
) -> TextBuffer {
    let mut buffer = TextBuffer::from_text(fixture(lines));
    let mut rng = Rng::new(7);
    while buffer.edit_seq() < pieces / 2 {
        let line = rng.below(buffer.line_count());
        buffer.insert(buffer.line_start(line), "x");
        if buffer.edit_seq().is_multiple_of(1000) {
            if let Some(store) = store.as_deref_mut() {
                store.stabilize(&buffer);
            }
            buffer.discard_edits_before(buffer.edit_seq());
        }
    }
    buffer
}

fn typing() {
    let text = fixture(1_000_000);
    println!(
        "fixture: {} lines, {:.1} MiB",
        1_000_000,
        text.len() as f64 / 1048576.0
    );
    let mut buffer = TextBuffer::from_text(text);
    let mut rng = Rng::new(1);
    let mut timings = Vec::new();
    for _ in 0..200 {
        let line = rng.below(buffer.line_count());
        let mut at = buffer.line_start(line);
        for ch in "let x = 1; // typing".chars() {
            let mut utf8 = [0u8; 4];
            let start = Instant::now();
            buffer.insert(at, ch.encode_utf8(&mut utf8));
            timings.push(start.elapsed());
            at += ch.len_utf8();
        }
    }
    report_percentiles("typing bursts, 1M lines", timings);
}

fn replace_all() {
    for matches in [1_000usize, 10_000] {
        let mut buffer = TextBuffer::from_text(fixture(1_000_000));
        let lines = buffer.line_count();
        let offsets: Vec<usize> = (0..matches)
            .map(|i| buffer.line_start(i * (lines - 1) / matches))
            .collect();
        let start = Instant::now();
        // Descending order, so earlier offsets stay valid.
        for &at in offsets.iter().rev() {
            buffer.replace(at..at + 1, "replacement");
        }
        report_total(
            &format!("replace-all of {matches} matches, 1M lines"),
            start.elapsed(),
            matches,
        );
    }
}

fn multi_cursor() {
    let mut buffer = TextBuffer::from_text(fixture(1_000_000));
    let lines = buffer.line_count();
    let bases: Vec<usize> = (0..50)
        .map(|i| buffer.line_start(i * (lines - 1) / 50))
        .collect();
    let mut timings = Vec::new();
    let start = Instant::now();
    for round in 0..100 {
        let round_start = Instant::now();
        for (i, base) in bases.iter().enumerate().rev() {
            // Cursor `i` has typed `round` characters, as have the `i`
            // cursors below it.
            buffer.insert(base + round * (i + 1), "a");
        }
        timings.push(round_start.elapsed());
    }
    report_total(
        "50 cursors x 100 characters, 1M lines",
        start.elapsed(),
        5000,
    );
    report_percentiles("  per keystroke round (50 edits)", timings);
}

fn snapshot() {
    for pieces in [10_000usize, 100_000] {
        let build = Instant::now();
        let buffer = buffer_with_pieces(100_000, pieces, None);
        println!(
            "built {pieces} pieces in {:.2} s",
            build.elapsed().as_secs_f64()
        );
        mean(&format!("snapshot at {pieces} pieces"), 200, || {
            black_box(buffer.snapshot());
        });
    }
}

fn anchored() {
    for pieces in [10_000usize, 100_000] {
        let mut store = AnchoredRangeStore::new();
        let mut buffer = buffer_with_pieces(100_000, pieces, Some(&mut store));
        let mut rng = Rng::new(3);
        let lines = buffer.line_count();
        let start = Instant::now();
        let ids: Vec<_> = (0..10_000)
            .map(|_| {
                let a = buffer.line_start(rng.below(lines));
                store.add(&buffer, a, a + 1)
            })
            .collect();
        println!(
            "add 10000 ranges at {pieces} pieces: {:.3} s",
            start.elapsed().as_secs_f64()
        );

        // Edits near the start shift every later endpoint and split pieces.
        for i in 0..50 {
            buffer.insert(buffer.line_start(i + 1) + 2, "edit");
        }
        let start = Instant::now();
        store.stabilize(&buffer);
        println!(
            "stabilize after 50 edits at {pieces} pieces: {:.3} ms",
            start.elapsed().as_secs_f64() * 1e3
        );

        let start = Instant::now();
        for &id in &ids {
            black_box(store.resolve(&buffer, id));
        }
        report_total(
            &format!("resolve 10000 ranges at {pieces} pieces"),
            start.elapsed(),
            ids.len(),
        );

        // A wide delete touches thousands of endpoints at once.
        let len = buffer.len();
        let start = Instant::now();
        buffer.delete(len / 4..len / 2);
        let consumed = store.stabilize(&buffer).consumed.len();
        println!(
            "stabilize after a delete of a quarter of the text at {pieces} pieces: {:.3} s ({consumed} consumed)",
            start.elapsed().as_secs_f64()
        );
    }
}

fn last_screen() {
    let buffer = buffer_with_pieces(100_000, 100_000, None);
    mean("read the last 50 lines at 100000 pieces", 2_000, || {
        let line = buffer.line_count().saturating_sub(50);
        let start = buffer.line_start(line);
        black_box(buffer.read_range(start..buffer.len()));
    });
}

fn line_lookup() {
    let buffer = buffer_with_pieces(100_000, 100_000, None);
    let near_end = buffer.len() - 20;
    let utf16_end = buffer.utf16_of_offset(near_end);
    mean(
        "line_of_offset near the end at 100000 pieces",
        2_000,
        || {
            black_box(buffer.line_of_offset(near_end));
        },
    );
    mean(
        "utf16_of_offset near the end at 100000 pieces",
        2_000,
        || {
            black_box(buffer.utf16_of_offset(near_end));
        },
    );
    mean(
        "offset_of_utf16 near the end at 100000 pieces",
        2_000,
        || {
            black_box(buffer.offset_of_utf16(utf16_end));
        },
    );
    mean("position_at near the end at 100000 pieces", 2_000, || {
        black_box(buffer.position_at(near_end));
    });
}

fn memory() {
    // Compare against an unedited buffer over the same text, so the original
    // text and the line index are subtracted out.
    let text = fixture(100_000);
    let before_bytes = LIVE_BYTES.load(Ordering::Relaxed);
    let before_allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let pristine = TextBuffer::from_text(text.as_str());
    let base_bytes = LIVE_BYTES.load(Ordering::Relaxed) - before_bytes;
    let base_allocations = ALLOCATIONS.load(Ordering::Relaxed) - before_allocations;
    drop(pristine);

    let before_bytes = LIVE_BYTES.load(Ordering::Relaxed);
    let before_allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let edited = buffer_with_pieces(100_000, 100_000, None);
    let bytes = LIVE_BYTES.load(Ordering::Relaxed) - before_bytes;
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before_allocations;
    let edits = edited.edit_seq();
    println!(
        "memory: {:.1} heap bytes and {:.3} allocations per edit over an unedited buffer ({edits} edits, about {} pieces)",
        (bytes - base_bytes) as f64 / edits as f64,
        (allocations - base_allocations) as f64 / edits as f64,
        edits * 2
    );
}

fn main() {
    let requested: Vec<String> = std::env::args().skip(1).collect();
    let workloads: [(&str, fn()); 8] = [
        ("typing", typing),
        ("replace_all", replace_all),
        ("multi_cursor", multi_cursor),
        ("snapshot", snapshot),
        ("anchored", anchored),
        ("last_screen", last_screen),
        ("line_lookup", line_lookup),
        ("memory", memory),
    ];
    for (name, run) in workloads {
        if requested.is_empty() || requested.iter().any(|r| r == name) {
            println!("== {name}");
            run();
        }
    }
}
