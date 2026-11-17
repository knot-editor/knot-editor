# D156 — Fewer host round trips per key

Status: ✅ all checkpoints done. Source: [roadmap](../roadmap.md) D156.

## Goal

A key bound to an extension command costs as few extension-worker turns as
the work needs, and each turn costs less in debug builds. Today a Vim `j`
costs about 11 foreground requests in `@knot/vim` (window lookup, selection,
`lineAt`, two `lineRange` + `snapshot` pairs, `cellsBefore`,
`columnAtCells`, `folds`, `setSelection`, and the selection listener's
re-read), plus about 5 in the chrome status bar and 5 in word highlight.
After this plan, a motion reads what it needs in one request, selection
listeners receive the selection instead of asking for it, the window lookup
is gone, the host glue is optimized in debug builds, and a test holds the
per-key request counts so they cannot creep back.

## Design

- **View target on the command context.** `CommandTarget` gains
  `viewTarget: ViewTarget | null`, the captured tab presentation, beside
  `windowTarget`. `@knot/vim` keys its per-editor state by it instead of
  asking `windows.get` for the window's current tab.
- **Selection with nearby lines.** `Editor.selection()` takes an optional
  `{ lines: n }`. With it, the result also carries the caret's 0-based line
  and the text and UTF-8 byte start of each line from `n` above to `n` below
  the caret, clamped to the buffer. `Buffer.lines(first, last)` returns the
  same shape for any line span in one request, replacing the `lineRange` +
  `snapshot` pair where text is needed. `BufferLines` in Vim seeds its cache from the selection
  read and loads other lines through `buffer.lines`. Display-cell queries
  (`cellsBefore`, `columnAtCells`) and `folds` stay native requests: their
  measure is native and must not be duplicated in JavaScript.
- **Selection notification payload.** `onDidChangeSelection` listeners
  receive the selection as of when the listener turn starts, read by the
  acknowledgement request the turn already makes, so the payload costs no
  extra request. `onDidChangeSelection(listener, { lines: n })` asks for the
  same nearby lines as `selection({ lines: n })`; the handle serves the
  widest window any of its listeners asked for. This revisits the [editor
  handles](../decisions/extension-host.md#editor-handles) decision: the
  rejected payload carried the old selection or the cause, while the current
  selection is what every listener pulled anyway. Every selection listener
  moves to the payload: `@knot/vim` (no re-read), `@knot/chrome` (position
  from the payload's lines, no reads), and `@knot/word-highlight` (no
  selection read).
- **Optimized host glue in debug.** `knot-host` already holds the V8
  marshalling as a crate of its own, so the dev profile builds it, and the
  crates the transport runs through (`knot-protocol`, `v8`, `serde`,
  `serde_json`), at `opt-level = 3`, as `tree-sitter`, `gpui-pre`, and
  `taffy` already are. The checkpoint records per-case time and the
  incremental rebuild cost of editing `knot-host`.
- **Request budget test.** A test-support counter of foreground host
  requests lets a test press a key and read how many requests it caused,
  across every extension. A test pins the counts for representative keys
  (`j`, `w`, `x`) with Vim, chrome, and word highlight running.

## Exclusions

- No generic request batching or pipelining API.
- No JavaScript reimplementation of display cells, tab expansion, or folds.
- No value payload for `settings.observe` or other listeners: none is on the
  per-key path.
- Moving the root-side handler adapters out of the `knot` crate, unless the
  checkpoint 2 profile shows they dominate a turn; then it becomes a roadmap
  item rather than part of this plan.

## Deferred decisions

- The exact budget numbers, set from the measured counts after each change.
- Whether Vim caches `folds` between keys; only if the budget test shows it
  matters.

## Checkpoints

### 1. Request counter and baseline ✅

The counter already existed as `work_counts::request`, read through
`TestEditor::work_per_key`. The Vim test
`a_key_costs_a_bounded_number_of_requests_across_extensions` runs Vim, chrome,
and word highlight and pins the baseline of foreground requests per key
(settled after each): `j` 21, `w` 17, `x` 16. It runs in about 0.13 s in
debug.

### 2. Optimized host glue in debug ✅

Dev-profile `opt-level = 3` for `knot-host` and `knot-protocol`; adding `v8`,
`serde`, and `serde_json` gave the same speedup for a 35 s longer clean
rebuild, so they are left out. Warm debug timings: Vim `tests/vim.rs` 1.37 s
to 1.27 s, `tests/edit.rs` 0.42 s to 0.33 s, the budget case 0.13 s to
0.12 s, workspace nextest unchanged within noise (about 12.5 s), incremental
`knot-host` rebuild 6.0 s to 4.9 s. No turn profile was taken; the small gain
suggests the host glue is not what a turn spends its time on.

### 3. View target on the command context ✅

`context.target.viewTarget` is the captured tab for any tab kind (null only
when no tab was focused); Vim keys its state by it. The extension API test
covers an editor tab; the null case is untested. The budget is `j` 20,
`w` 16, `x` 15. See [commands](../architecture/commands.md).

### 4. Selection notification payload ✅

Selection listeners receive the selection as of their turn start, returned by
the acknowledgement request; Vim, chrome, and word highlight use it (word
highlight reads only before its first notification). The budget is `j` 18,
`w` 14, `x` 15. See [extension host](../architecture/extension-host.md#editor-handle-notifications)
and the [decision](../decisions/extension-host.md#editor-handles).

### 5. Selection with nearby lines ✅

`selection({ lines })` and `onDidChangeSelection(listener, { lines })` carry
the caret's line and its neighbors (the handle serves the widest window any
listener asked for), and `Buffer.lines(first, last)` reads a span in one
request. Vim seeds `BufferLines` from the selection and loads through
`lines`; chrome takes the position from the payload. `lineRange` stays for
callers that need only a byte range. The final budget is `j` 10, `w` 8, `x`
13; `j` still reads `folds` twice and `w` reads `cellsBefore`, and caching
`folds` stays deferred. See the [extension API](../extension-api.md) and
[editor handle notifications](../architecture/extension-host.md#editor-handle-notifications).
