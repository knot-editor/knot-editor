# D092: Piece tree text buffer

Status: 🚧 approved, checkpoint 1 complete, checkpoint 2 awaiting review. Roadmap item [D092](../roadmap.md). Follows
the [text buffer analysis](../reports/text-buffer-structure.md) (option B);
current behavior is owned by
[core and buffers](../architecture/core-and-buffers.md) and
`crates/knot-core/src/buffer.rs`, and the original rationale by
[decisions](../decisions/core-and-buffers.md#text-buffer-and-stable-positions).

## Goal

Bulk edits stop scaling with document size and piece count. Replace-all,
multi-cursor typing, formatting, and language-server edit lists can add
thousands of pieces without making each later edit, snapshot, line lookup, or
position resolve slower. Typing latency stays at or below today's.

Snapshots also become O(1). This matters beyond background parsing:
`BufferModel` takes a snapshot for every highlight query, bracket lookup,
reindent, and foreground parse, so painting pays today's O(pieces) copy too.

## Design

- **Unchanged semantics and API.** Stable piece IDs, the split rule (left half
  keeps its ID, right half gets a fresh one), trailing-piece coalescing, the
  append-only backing store, the `BufferEdit` log with its `Split` records,
  and the public `TextBuffer` and `TextSnapshot` surfaces stay as they are.
  `AnchoredRangeStore`, `EditTransaction`, syntax, history, and every
  `knot-model` caller need no changes.
- **Hand-rolled persistent B-tree, maintained in `knot-core`.** It is
  specialized to pieces rather than generic, and `knot-core` keeps zero
  dependencies. Nodes are `Arc`-shared and mutated through `Arc::make_mut`, so
  the foreground edits in place when no snapshot holds a node and a snapshot
  is a root clone. Each node summarizes its subtree's byte length, line-break
  count, UTF-16 length, and greatest locator. Operations: seek by byte offset,
  line, UTF-16 offset, and locator; insert a piece after a position; remove a
  range of pieces; update one piece's length; iterate pieces from a seek
  position. Leaves hold tens of pieces, so a freshly loaded file is one leaf.
- **Per-backing line indexes, as in VS Code.** The original text and each add
  chunk keep their own sorted `(byte, utf16)` line starts; the tail's index
  grows as text is appended and moves with the chunk when it is sealed. A
  piece's line breaks and UTF-16 length come from binary searches over its
  backing's index plus a scan of at most one partial line, so splitting the
  29 MiB original piece costs no full rescan. The document-wide `line_starts`
  index and its O(lines) shift on every edit are removed: `line_start`,
  `line_of_offset`, `utf16_of_offset`, and `offset_of_utf16` become one tree
  seek plus a lookup in the reached piece's backing index.
- **Locators for `resolve`.** Each piece carries a dense ordering key that
  increases in text order (a short inline sequence of `u32`s, Zed's
  technique), chosen between its neighbors when the piece is created; a split
  gives the right half a key between the left half and its old successor. The
  foreground buffer keeps a `HashMap<PieceId, Locator>` that inserts, splits,
  and deletions maintain; snapshots never resolve positions and do not carry
  it. `resolve` is a map lookup plus an O(log n) seek by locator, so anchored
  range stabilization becomes O(a log p) instead of O(a · p).
- **Snapshot sharing.** Sealed chunks are held as `Arc<[Arc<str>]>`, rebuilt
  only when a chunk is sealed, so a snapshot is a root clone, a chunk-list
  clone, and a copy of the unsealed tail (under 16 KiB).
- **Benchmark before and after.** A benchmark example measures the current
  buffer before any structural change, and the same harness is rerun on the
  same machine afterwards. The baseline numbers are recorded in this plan;
  the final comparison moves to decisions.

## Exclusions

- Batched bulk-edit primitives. Per-primitive edits at O(log n) make
  replace-all O(m log n), which the analysis judges acceptable; revisit only
  if the final benchmark disagrees.
- Piece compaction (merging adjacent pieces of one backing span). The tree
  removes the costs that motivated it.
- Changes to `AnchoredRangeStore`'s algorithm beyond what faster `resolve`
  gives for free.
- A benchmarking dependency such as criterion, and CI benchmark gating.

## Deferred decisions

- Leaf and internal node fan-out, tuned from benchmark results in
  checkpoint 2.
- Whether a locator is a fixed small array or a growable inline sequence,
  decided in checkpoint 3 from how deep keys grow under the randomized test.

## Baseline

Current `Vec`-based buffer, recorded by
`cargo run --release -p knot-core --example buffer_bench` (release profile,
no extra flags) on an Apple M1 Pro with 32 GiB, macOS 27.0.1, rustc 1.99.0.
Two full runs agreed within about 10%; the table is one of them. The
1M-line fixture is 27.8 MiB; the piece-count workloads use a 100,000-line
fixture (about 2.9 MiB) so that building 100k pieces stays practical. Piece
counts are approximate: each workload inserts one character at distinct line
starts, adding about two pieces per insert, and the edit log is drained as
it grows. The differential test and the benchmark's fixtures are
deterministic.

| Workload | Baseline |
| --- | --- |
| Typing bursts, 1M lines (4,000 keystrokes at 200 random places) | p50 145 µs, p99 307 µs, max 1.9 ms |
| Replace-all, 1,000 matches, 1M lines | 0.32 s (321 µs per match) |
| Replace-all, 10,000 matches, 1M lines | 3.43 s (343 µs per match) |
| 50 cursors × 100 characters, 1M lines | 0.83 s total; per keystroke round (50 edits) p50 8.2 ms, p99 11.3 ms |
| Snapshot, 10k pieces / 100k pieces | 21 µs / 141 µs |
| Build 10k / 100k pieces (one insert each) | 0.10 s / 2.6 s |
| Add 10,000 anchored ranges, 10k / 100k pieces | 54 ms / 449 ms |
| Resolve 10,000 anchored ranges, 10k / 100k pieces | 38 ms (3.8 µs each) / 330 ms (33 µs each) |
| Stabilize after 50 edits near the start, 10k / 100k pieces | 0.03 ms / 0.02 ms |
| Stabilize after deleting a quarter of the text (2,574 ranges consumed), 10k / 100k pieces | 18 ms / 137 ms |
| Read the last 50 lines, 100k pieces | 389 µs |
| `line_of_offset` near the end, 100k pieces | 0.02 µs |
| `utf16_of_offset` near the end, 100k pieces | 363 µs |
| `offset_of_utf16` near the end, 100k pieces | 339 µs |
| `position_at` near the end, 100k pieces | 47 µs |
| Heap per piece over an unedited buffer | about 44 bytes (88 bytes and 1.8 allocations per insert, about two pieces each), including spare `Vec` capacity |

Observations that shape the acceptance targets:

- A per-edit cost near 150–340 µs is the O(lines) line-index shift and the
  O(pieces) `Vec` splice at 1M lines; it is the dominant cost of every bulk
  edit, so bulk edits scale as m × document size.
- Replace-all of 10,000 matches takes 3.4 s today. The final target, tens of
  milliseconds, needs about 3–5 µs per replace (two primitive edits), well
  within reach of O(log n) edits.
- Snapshot, resolve, `position_at`, the last-screen read, and the UTF-16
  conversions grow linearly with piece count (the last two pay for a chunk
  walk from the line start plus the piece scan). `line_of_offset` is already
  logarithmic because the document-wide index exists.
- Typing p50 of 145 µs at 1M lines is the bar for "not slower"; it is
  dominated by the line-index shift that the tree removes.

## Checkpoints

### 1. Benchmark harness, baseline, and differential test ✅

Complete and approved. `crates/knot-core/examples/buffer_bench.rs` runs the
workloads (run it with `cargo run --release -p knot-core --example
buffer_bench -- [workload...]`), and `buffer/differential_tests.rs` checks
the buffer against a `String` model over fixed seeds, including position
tokens followed through the edit log's `Split` records. The baseline is
recorded above. The replace-all target of tens of milliseconds for 10,000
matches stands.

### 2. Piece tree with summaries and per-backing line indexes 🔍

Implemented and awaiting the review gate; see the intermediate results below.

Replace `pieces: Vec<Piece>` and `line_starts: Vec<LineStart>` with the
persistent B-tree and per-backing line indexes, and make snapshots share the
tree. `resolve` and `position_at` walk the tree in order for now, still
O(pieces) for `resolve` but no longer slower than today.

- **Deliverables:** the tree module; `split_at`, `insert`, `delete`,
  coalescing, `read_range`/`chunks`, `is_char_boundary`, `position_at`, and
  every line and UTF-16 query on top of tree seeks; per-backing line indexes
  for the original text, sealed chunks, and the tail; `TextSnapshot` holding
  a root clone and the shared chunk list, with `chunk_at` and `chunks` as
  tree seeks; `update_line_starts` and `scan_line_starts` over the whole
  document removed. Tree invariants (summaries equal the sum of children,
  leaves within fan-out bounds, uniform depth) are checked by a debug-only
  validator that the differential test calls after each step.
- **Validation:** existing `buffer/tests.rs`, `buffer/snapshot/tests.rs`,
  `anchored_range/tests.rs`, and `transaction.rs` tests pass, with internal
  piece inspections rewritten against a test-only piece iterator; the
  differential test passes with the tree validator; benchmark rerun noted in
  this plan as an intermediate result; `just check`.
- **Review gate:** confirm typing is not slower than the baseline and that
  edit, snapshot, line, and read costs no longer grow with piece or line
  count, and settle the node fan-out.
- **Intermediate results** (same machine and harness as the baseline; leaves
  hold up to 32 pieces and internal nodes up to 16, chosen because 16/8,
  64/16, and 64/32 measured the same within noise except for memory):

  | Workload | Baseline | Piece tree |
  | --- | --- | --- |
  | Typing bursts, 1M lines | p50 145 µs, p99 307 µs | p50 0.2 µs, p99 3.9 µs |
  | Replace-all, 10,000 matches | 3.43 s | 0.018 s |
  | 50 cursors × 100 characters | 0.83 s | 0.003 s |
  | Snapshot, 10k / 100k pieces | 21 µs / 141 µs | 0.13 µs / 0.08 µs |
  | Last 50 lines, 100k pieces | 389 µs | 0.47 µs |
  | `utf16_of_offset` / `offset_of_utf16` / `position_at`, 100k pieces | 363 / 339 / 47 µs | 0.11 / 0.14 / 0.04 µs |
  | Resolve one anchored range, 10k / 100k pieces | 3.8 / 33 µs | 10.9 / 135 µs |
  | Stabilize after deleting a quarter, 10k / 100k pieces | 18 / 137 ms | 47 / 461 ms |
  | Heap per edit over an unedited buffer | 88 bytes, 1.8 allocations | 140 bytes, 1.05 allocations |

  Everything but `resolve` is now logarithmic or constant. `resolve` still
  walks every piece (a scan of leaves by id) and is about 3–4 times slower
  than the old `Vec` scan, which also slows the stabilization that is
  dominated by it; checkpoint 3 replaces the walk. Memory per piece grew
  because a piece caches its line-break count and UTF-16 length (48 bytes
  against 40); narrowing those fields is possible if it matters.
- **Implementation context:** add `crates/knot-core/src/buffer/tree.rs` (the
  B-tree, summaries, seeks, cursor iteration) and a backing module (for
  example `buffer/backing.rs`) owning the original text, sealed chunks, tail,
  and their line indexes, replacing the `original`/`sealed`/`tail` fields and
  `backing_slice` in `buffer.rs`. `Piece`, `Backing`, and `SplitLoc` in
  `buffer.rs` stay as the leaf item and split result; `split_at` returns a
  tree position instead of a `Vec` index. `snapshot.rs` drops `pieces` and
  `piece_ends`. Tests that touch `b.pieces` and `line_starts` directly are in
  `crates/knot-core/src/buffer/tests.rs` (around lines 199–262). Snapshot
  callers in `crates/knot-model/src/buffer.rs` are unchanged.

### 3. Locators and logarithmic resolve

Give each piece a locator, keep the `PieceId → Locator` map on the buffer,
and make `resolve` a map lookup plus a locator seek.

- **Deliverables:** the locator type and the "key between two neighbors"
  function; locator assignment on insert, coalescing (unchanged key), and
  split; map maintenance on insert, split, and delete; node summaries carry
  the greatest locator; `resolve` seeks by locator and keeps today's `None`
  semantics for deleted pieces and stale right-of-split offsets.
- **Validation:** unit tests for key generation (between neighbors, at the
  ends, repeated inserts at one point keep keys short enough); the tree
  validator also checks locators increase in order and that the map matches
  the tree; the differential test's position checks pass; anchored-range
  tests pass; benchmark rerun for the resolve and stabilization workloads;
  `just check`.
- **Review gate:** confirm resolve and stabilization costs no longer grow
  with piece count beyond O(log n), and settle the locator representation.
- **Implementation context:** locator in `crates/knot-core/src/buffer/tree.rs`
  or its own `buffer/locator.rs`; map as a field on `TextBuffer` in
  `buffer.rs`, maintained in `split_at`, `insert`, and `delete`.
  `AnchoredRangeStore::stabilize` in `crates/knot-core/src/anchored_range.rs`
  calls `resolve` per moved endpoint and needs no change.

### 4. Final benchmark and documentation

Rerun the full benchmark on the same machine as the baseline and update the
durable docs.

- **Deliverables:** before/after table for every workload; acceptance check
  against the targets (typing at or below baseline p50, 10,000-match
  replace-all in tens of milliseconds, snapshot and resolve O(log n) or
  better in piece count); updated
  [core and buffers](../architecture/core-and-buffers.md) (the piece tree,
  per-backing line indexes, locators, O(1) snapshots; remove the eager
  document-wide line index and the deferred-compaction note);
  [decisions](../decisions/core-and-buffers.md#text-buffer-and-stable-positions)
  updated with the chosen structure, the rejected alternatives in one or two
  sentences, and the before/after evidence, with the
  [decisions index](../decisions.md) adjusted if its summary changes; the
  module documentation in `buffer.rs` and `snapshot.rs` describing the new
  structure; the analysis report removed or reduced as the docs guide
  requires; this plan and the D092 roadmap entry removed, with roadmap
  references to D092 as a dependency (D067, Find) updated.
- **Validation:** `just check`; the full benchmark output kept in the
  decisions entry in summarized form.
- **Review gate:** final review of numbers and docs before closing D092.
