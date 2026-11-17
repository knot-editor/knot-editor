# D156 — Fewer host round trips per key

Status: 🚧 approved, not started. Source: [roadmap](../roadmap.md) D156.

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
  `snapshot` pair. `BufferLines` in Vim seeds its cache from the selection
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

### 1. Request counter and baseline

Deliverables: a test-support count of foreground host requests on the
extension host; a Vim test that presses `j`, `w`, and `x` with chrome and
word highlight active and asserts the current counts; the baseline counts
and per-case time recorded in this plan.

Validation: the new test; `just check`.

Review gate: the counts match the analysis above, and the counter measures
what a real key costs.

**Implementation context:** count in `ProductExtensionHost::handle_request`
(`src/app/extensions/host.rs`) behind `test-support`, exposed through
`knot::test_support` like `pending_view_request_count`. The test lives in
`crates/knot-ext-vim/tests/vim.rs`, using its `TestProduct` setup with the
chrome and word-highlight packages loaded; settle after each key before
reading the count.

### 2. Optimized host glue in debug

Deliverables: dev-profile `opt-level = 3` for `knot-host`, `knot-protocol`,
`v8`, `serde`, and `serde_json`; before and after numbers for the Vim suite,
the whole workspace test time, and an incremental rebuild after touching
`knot-host`, recorded here. A short profile of one turn notes whether the
root-side adapters still dominate.

Validation: `just test` timings; `just check`.

Review gate: the speedup is worth the rebuild cost; drop any crate whose
optimization does not pay.

**Implementation context:** `[profile.dev.package.*]` entries in the root
`Cargo.toml`, beside the existing `tree-sitter`, `gpui-pre`, and `taffy`
entries and their comment style. `just test-timings` gives per-binary
numbers.

### 3. View target on the command context

Deliverables: `CommandTarget.viewTarget`; Vim's `stateOf` uses it and no
longer calls `windows.get`; the budget test lowers by one per key.

Validation: an extension API test that a command from an editor tab gets
that tab's `ViewTarget` and one from a tool or palette without a view gets
null; Vim suite; budget test; `just check`.

Review gate: Vim behavior unchanged, including keys arriving before the
tab's activation is observed.

**Implementation context:** `CommandCapture` in
`crates/knot-protocol/src/lib.rs` gains the pane identity; the capture is
filled where command targets are captured in `src/app/commands/`; JavaScript
builds the target in `bootstrap.js` (`windowTarget({ window })` near the
command frame, interning through `presentationTarget`). Contract in
`knot.d.ts`, docs in `docs/extension-api.md`, flow in
`docs/architecture/commands.md`. Vim change in
`extensions/@knot/vim/src/extension.ts`.

### 4. Selection notification payload

Deliverables: listeners receive the selection; the acknowledgement returns
it; Vim, chrome, and word highlight use it; the budget test lowers.
Lines on the notification arrive in checkpoint 5.

Validation: extension API tests that the payload is the selection at turn
start, including a change during the listener queueing one more turn with
the newer selection; Vim, chrome, and word-highlight suites; budget test;
`just check`.

Review gate: decision text updated in
`docs/decisions/extension-host.md#editor-handles` and its index line.

**Implementation context:** `acknowledge_editor_selection` in
`src/app/extensions/host.rs` returns the selection, mirroring
`acknowledge_editor_bars`; the `"selection"` branch in `bootstrap.js`
passes a frozen event to listeners, as the `"bars"` branch does.
`EditorEvent` docs in `knot-protocol` stay payload-free on the wire. Update
`docs/architecture/extension-host.md#editor-handle-notifications`.

### 5. Selection with nearby lines

Deliverables: `selection({ lines })`, `onDidChangeSelection(listener,
{ lines })`, and `Buffer.lines(first, last)`; Vim's `BufferLines` seeds
from the selection read and loads through `buffer.lines`; chrome takes the
caret position from the payload; the budget test reaches its final numbers.

Validation: extension API tests for clamping at buffer ends, CRLF lines,
multi-byte text, and an invalid span; Vim, chrome, and word-highlight
suites; budget test; `just check`.

Review gate: the final counts per key, and the line read shape is one an
other modal package would use.

**Implementation context:** `editor_selection` and `line_range` in
`src/app/extensions/host/handlers.rs` show the editor and buffer read
patterns; the line-span logic is a `knot-model` buffer function with plain
tests. JavaScript wrappers near `selection()` in `bootstrap.js`; contract in
`knot.d.ts` and `docs/extension-api.md`. Vim: `lines.ts`, the `motion` and
`followSelection` paths in `normal.ts` and `visual.ts`; chrome:
`caretText` in `extensions/@knot/chrome/src/extension.ts`.
