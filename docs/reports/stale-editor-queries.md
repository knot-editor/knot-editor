# Stale editor view queries panic the host

November 2026. Examines why an extension's editor row query against a
stale view panics the host instead of failing the query, and what hardening
would take. Supports [D185](../roadmap.md#bugs). Current behavior is owned
by the [extension host](../architecture/extension-host.md).

## Summary

`editor_row_at` in `src/app/extensions/host/handlers.rs` resolves a row
through `Editor::row_start` → `EditorState::row_start` →
`content_range`/`line_end` → `BufferModel::line_range` →
`TextBuffer::line_start`, which panics on an out-of-range line
(`crates/knot-core/src/buffer.rs:650`). A query that names a row the buffer
no longer has kills the calling thread instead of returning an error.

## Evidence

During the D163 checkpoint-2 experiment (reverted; per-case isolation
kept), an in-place `TestEditor::load` let the Vim package query row 13 of a
3-line buffer: the package's retained viewport row survived the text
replacement. The `topics::scrolling` test died with `line 13 out of range
(3)` through the chain above. No production occurrence is known; reaching
the panic requires a view row that outlives the text it was measured
against, which today only stale test state produces.

## Proposed hardening, deferred

Return a query error (the handlers' existing `AccessError` shape) from
out-of-range row, line, and offset queries instead of panicking, audit the
sibling `editor_*` handlers in `handlers.rs` for the same direct indexing,
and cover one stale-row query with a regression test. Deferred because no
production path is known to reach it; the reconsider trigger is a real
occurrence or the next extension-host hardening pass.
