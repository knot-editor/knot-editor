# Core and buffers decisions

Text storage, stable positions, anchored ranges, and history. The
[decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Text buffer and stable positions

Use a hand-built stable-ID piece tree: a persistent, copy-on-write B-tree of
pieces whose nodes summarize bytes, line breaks, and UTF-16 length, with a line
index per backing and an ordering locator per piece. It gives direct control
over edit events and stable positions, and bulk edits, snapshots, and position
resolution stop depending on document size or piece count. A single-threaded
UTF-8 core matches foreground ownership; presentation handles graphemes and the
extension boundary adapts strings. The structure is in
[core and buffers](../architecture/core-and-buffers.md#stable-positions-and-edit-behavior).

Rejected: patching the flat `Vec` and document-wide line index piece by piece,
which left the piece walk, the splice, and the O(pieces) snapshot each needing
its own index; a rope crate, which loses piece identity, so anchors would be
remapped through the edit stream or rebuilt on an ID layer, which is the piece
tree again; and a gap buffer or CRDT, which offer no stable positions or
snapshots, or pay for collaboration, a [design non-goal](../design.md#non-goals).
Batched bulk-edit primitives and piece compaction were left out because
per-primitive O(log n) edits already make replace-all fast.

Measured with `cargo run --release -p knot-core --example buffer_bench` on an
Apple M1 Pro, macOS, with a 1M-line, 27.8 MiB fixture and 100,000-line fixtures
for the piece-count workloads (about two pieces per insert):

| Workload | `Vec` and flat line index | Piece tree |
| --- | --- | --- |
| Typing p50 / p99, 1M lines | 145 / 307 µs | 0.2 / 4.4 µs |
| Replace-all, 10,000 matches | 3.43 s | 0.069 s |
| 50 cursors × 100 characters | 0.83 s | 0.004 s |
| Snapshot, 10k / 100k pieces | 21 / 141 µs | 0.12 / 0.05 µs |
| Build 10k / 100k pieces | 0.10 / 2.6 s | 0.01 / 0.09 s |
| Resolve per anchored range, 10k / 100k pieces | 3.8 / 33 µs | 0.27 / 0.65 µs |
| Stabilize after deleting a quarter of the text, 10k / 100k pieces | 18 / 137 ms | 3 / 7 ms |
| Last 50 lines, 100k pieces | 389 µs | 0.50 µs |
| `utf16_of_offset` / `position_at`, 100k pieces | 363 / 47 µs | 0.12 / 0.04 µs |
| `line_of_offset`, 100k pieces | 0.02 µs | 0.09 µs |
| Heap per edit | 88 B, 1.8 allocations | 294 B, 6.4 allocations |

Resolve scales with the logarithm of the piece count because of the locators.
Their cost shows in the last row and in replace-all, which was 0.018 s before
locators: a map insert and a key allocation per piece, accepted for
logarithmic resolve. Typing deepens a locator by about one digit per 17–32
pieces inserted at one point, and appends stay one digit.

The line index is per backing because views read lines from the model rather
than keeping a private copy, which had made every keystroke cost O(document)
per view. It also records UTF-16 line starts, because platform text input
speaks document-wide UTF-16 offsets and every IME callback had scanned from the
start of the document. The core stays UTF-8: the rejected whole-document UTF-16
table was a JavaScript adapter that exceeded the isolate heap (see
[extension runtime](extension-host.md#extension-runtime)), not one integer per
line in Rust.

Contiguous typing extends the trailing add piece and the consumed edit log is
discarded, so long editing runs do not degrade every buffer operation. Inserted
newlines follow the buffer's first line terminator rather than a dominant one,
which would need a whole-document scan for a rare mixed-ending case.

Reference: [core and buffers](../architecture/core-and-buffers.md).

## Anchored ranges and contributions

Keep stable geometry in core and feature, source, lifecycle, and presentation
policy in the application. Independent features can share one edit stream
without privileged coordination. Query indexes remain derived caches.

On Apple M4 Pro/macOS 15 release builds with 10,000 annotations on a million-line
buffer, token stabilization measured p50 12.2–12.4 µs per edit versus
69.5–69.7 µs for offset remapping across three and six sources. Token p99 was
higher (165–171 µs versus 88–90 µs); resolving all annotations cost about 1.79 ms.
This supports inline stabilization at the tested scale, not uniform tail gains.

Reference: [core and buffers](../architecture/core-and-buffers.md).

## Reversible edits

Undo uses ordinary mutations so it shares forward editing's edit-log and anchor
repair path. Session-local history belongs to the shared buffer model so every
view sees one edit history. View snapshots use byte offsets: replay restores the
exact content state, so persistent anchors would add lifetime complexity without
improving stability. View identities avoid retaining closed view entities.

Track content identity separately from monotonic public revisions so undo can
return to clean content without weakening revision-based conflict detection.
History persistence and branching await a demonstrated recovery benefit.

Reference: [core and buffers](../architecture/core-and-buffers.md).

## Line diff for extensions

Comparing two texts is an editor feature, not part of version control: version
control and the diff view consume it. It is one stateless native function over
strings or buffers, with policy (when to diff, what to show) left to packages,
as `buffer.find` leaves it for search. Reload and the extension API share one
comparison so they agree on what changed.

Rejected: a `buffer.diff(text)` method beside a text-to-text function, which
gives one operation two entry points; and a native text handle or headless
baseline buffer that would spare resending a baseline, which adds state and a
buffer kind. Revisit the handle only if resending baselines is measured as a
problem.

Reference: [core and buffers](../architecture/core-and-buffers.md#line-diff).
