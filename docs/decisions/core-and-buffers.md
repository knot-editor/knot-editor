# Core and buffers decisions

Text storage, stable positions, anchored ranges, and history. The [decisions
index](../decisions.md) lists every topic, and the [documentation
guide](../README.md#decisions) sets the entry rules.

## Text buffer and stable positions

A hand-built stable-ID piece table gives direct control over edit events and
stable positions; a single-threaded UTF-8 core matches foreground ownership.
On a 1M-line, 29 MiB file, edits cost p50 678 µs with the line index, mostly
linear suffix shifts, which is acceptable until real workloads justify a tree
(D092).

Views read lines from the model rather than a private copy, which made every
keystroke O(document) per view, so the line index is eager and also records
UTF-16 line starts for platform text input. Typing extends the trailing add
piece and the consumed edit log is discarded, so long sessions do not degrade.
Inserted newlines follow the first line terminator, avoiding a whole-document
scan for a rare mixed-ending case.

Reference: [core and buffers](../architecture/core-and-buffers.md).

## Anchored ranges and contributions

Keep stable geometry in core and feature policy in the application, so
independent features share one edit stream. With 10,000 annotations on a
million-line buffer, token stabilization cost p50 about 12 µs per edit against
70 µs for offset remapping, with higher p99, which supports inline
stabilization at that scale.

Reference: [core and buffers](../architecture/core-and-buffers.md).

## Reversible edits

Undo uses ordinary mutations so it shares forward editing's anchor repair.
History belongs to the shared model so every view sees one history. Content
identity is tracked apart from monotonic revisions, so undo can return to clean
without weakening conflict detection. Persistent and branching history wait for
a demonstrated need.

Reference: [core and buffers](../architecture/core-and-buffers.md).
