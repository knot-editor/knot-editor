# Core and buffers decisions

Text storage, stable positions, anchored ranges, and history. The
[decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Text buffer and stable positions

Use a hand-built stable-ID piece table for direct control over edit events and
stable positions. A single-threaded UTF-8 core matches foreground ownership;
presentation handles graphemes and the extension boundary adapts strings.

On Apple M1 Pro/macOS release builds with a 1,001,395-line, 29 MiB Rust fixture,
edits measured p50 678 µs with the line index and 75 µs without it. Linear suffix
shifts accounted for about 88% of indexed latency; position issuance and
resolution were sub-microsecond. This supports deferring index replacement until
real workloads justify it (D092); absolute tails included system noise.

The line index is eager because views read lines from the model rather than
keeping a private copy, which had made every keystroke cost O(document) per
view. It also records UTF-16 line starts, because platform text input speaks
document-wide UTF-16 offsets and every IME callback had scanned from the start
of the document. The core stays UTF-8: the rejected whole-document UTF-16 table
was a JavaScript adapter that exceeded the isolate heap (see
[extension runtime](extension-host.md#extension-runtime)), not one integer per
line in Rust.

Contiguous typing extends the trailing add piece and the consumed edit log is
discarded, so long sessions do not degrade every buffer operation. Inserted
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
