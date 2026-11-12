# D080 — Word and selection occurrence highlighting

Status: 📝 proposed. Item of the [daily-driver plan](daily-driver.md#phase-3--editing);
absorbs roadmap item [D179](../roadmap.md) (view-scoped decorations).

## Outcome

When the caret rests on a word, every whole-word, case-sensitive occurrence of
it in the buffer is highlighted, the one under the caret included. When a
single-line selection is made, the other occurrences of the selected text are
highlighted in a second color. Highlights appear after a short delay, belong to
the editor showing the caret, and never appear in other views of the same
buffer. A bundled JavaScript package, `@knot/word-highlight`, implements the
workflow on public API.

Along the way extensions gain Node-compatible timers, a native word lookup,
and per-view decorations, and `@knot/find` moves onto per-view decorations so
two find bars over one buffer stop sharing highlights.

## Reference behavior

VS Code is the base, since it is the only one of VS Code, Zed, and IntelliJ
with a text-only caret highlight:

- Caret word: VS Code's textual word highlighter, which decorates every
  match, so a word used once is still highlighted. Zed and IntelliJ highlight
  only semantic references (LSP `documentHighlight`, PSI usages, with
  read/write colors); that arrives with D067 and can feed the same package.
  Sublime has no caret highlight without plugins.
- Selection: VS Code's `editor.selectionHighlight`, Zed's
  `selection_highlight`, and Sublime's `match_selection` agree on decorating
  only the other occurrences, never the selection itself, so nothing shows
  without another match; IntelliJ has none by default. VS Code and Zed skip
  whitespace-only selections, VS Code skips multi-line ones (unless a
  setting allows them) and very long ones, and neither matches whole words.
  Confirm VS Code's case default and length limit during checkpoint 4.
- Delay: closer to Zed (100 ms for selections, 75 ms for language-server
  highlights) than VS Code (250 ms); one setting, default 75 ms, tuned at the
  review gate.

## Design

### Timers

Extensions get Node's timer globals, with Node's semantics wherever they
apply: `setTimeout`, `setInterval`, `setImmediate`, and their `clear*`
counterparts, returning Node-style `Timeout` and `Immediate` objects
(`refresh()` works; `ref()`, `unref()`, and `hasRef()` exist but have no
process to keep alive; `[Symbol.toPrimitive]` yields the numeric ID that the
`clear*` functions also accept). Node's delay coercion applies: a delay below
1 or above 2³¹−1 ms becomes 1 ms, and extra arguments pass to the callback.
`setInterval` reschedules after its callback, as Node does, so missed ticks
never stack. Each callback is its own FIFO root, drained of microtasks after
the turn, which matches Node's run-one-callback-then-drain ordering; a delay is
therefore a minimum behind the lifecycle's queued roots. Callback failures are
isolated like listener failures. Timers belong to the lifecycle: teardown,
restart, and unload cancel them.

`knot-host` owns the timer table, IDs, semantics, and expiry roots. It waits
through a `TimerDriver` trait, a required `PoolConfig` field with no default:
the driver only sleeps for a duration and then calls back. There are two
drivers. The application's uses gpui's `BackgroundExecutor` timer, which is
`Send` and needs no foreground hop. A manual driver, advanced by hand, is
test-only behind `test-support`.

Every test layer controls time, so no test waits on the wall clock:

- `knot-host` tests and application tests that build an `ExtensionPool`
  directly (`src/app/extensions/host/engine_tests.rs`, the pool tests in
  `src/app/extensions/host.rs`) use the manual driver;
- product and `knot-ext-*` integration tests use the gpui driver, whose
  `#[gpui::test]` executor has a virtual clock advanced by `advance_clock`;
- package tests under `node:test` get Node's own timers and mock them with
  `mock.timers`.

### Per-view decorations (D179)

`EditorState` keeps one decoration set per extension lifecycle, beside its
key contexts, caret shapes, and bars, anchored through the buffer's anchor
store as folds are. Projection merges these with the buffer-wide decorations,
in the same layers and severity order. Closing the view drops its sets, and
lifecycle teardown removes them as it removes key contexts.

`Editor.setDecorations(items, { ifRevision })` atomically replaces the
caller's set on that view; items are `{ range, decoration }`, and an empty
list clears it. It lives on `Editor`, so a command's captured editor can use
it as well as an `EditorHandle`.

Rejected: buffer-wide sets keyed by an optional view ID, which would make the
buffer track view identities and their closure, a second ownership catalog.
Gutter markers and click actions stay buffer-only: every plausible consumer
(breakpoints, diagnostics, version-control changes) belongs to the buffer, and
a view scope can be added to them later as a field.

### Word lookup

`buffer.wordAt(offset)` returns the native segment double-click selects
(`word_segments` in `knot-model/src/editor/text.rs`) as
`{ range, kind: "word" | "separator" | "space" }`, preferring the word before
the offset when the offset sits at a word's end, or null in an empty buffer.
Extensions then agree with double-click, word movement, and whole-word Find.
Vim keeps its own word classes, which deliberately follow `iskeyword`.

### The package

`@knot/word-highlight` follows each window's active editor as
`@knot/status-bar` does, listens to `onDidChangeSelection`, and after the
delay (restarted with `Timeout.refresh()`):

- for a bare caret on a `word` segment, sets view decorations on every
  `buffer.find(word, { wholeWord: true, caseSensitive: true })` match in
  `editor.wordHighlight`, even when the word occurs only once;
- for a non-empty single-line selection that is not only whitespace,
  highlights the other occurrences of the selected text in
  `editor.selectionHighlight`, never the selection itself;
- otherwise clears. Moving within the highlighted word keeps its highlights
  without searching again; an edit refreshes them; an editor that stops being
  active is cleared.

Matches are capped at `find`'s default limit. Settings, declared by the
package: `wordHighlight.enabled`, `selectionHighlight.enabled` (both on), and
`wordHighlight.delay` (ms). The two color keys are native, with defaults in
`theme.rs` and values in every bundled theme.

## Exclusions and deferred decisions

- Syntax-aware or semantic highlights, read/write distinction: D067.
- Scrollbar or overview marks: no surface exists.
- Highlights across files or across editors.
- View-scoped gutter markers and actions (see above).
- `process.nextTick` and other Node globals beyond timers.

## Checkpoints

### 1. Node-compatible timers

Deliverables: the globals and objects above in `bootstrap.js` and declared in
`knot.d.ts` (compatible with `@types/node` in package test builds); the
host-side timer table, expiry roots, and lifecycle cancellation; the
required `TimerDriver` trait, the test-only manual driver, and the gpui
driver in the application; the extension guide's mention.

Validation: `knot-host` tests with the manual driver for ordering, coercion,
`refresh`, intervals not stacking, clearing from inside callbacks, failure
isolation, and teardown cancellation; a product test where a package timer
fires under `advance_clock` and not before; a package test using
`mock.timers`. No test at any layer waits on real time. `just check`.

Review gate: timer semantics and the driver boundary.

**Implementation context:** listener roots enter through `RuntimeCallback`
(`crates/knot-host/src/engine/mod.rs`) and the `__knotDispatch*` table in
`crates/knot-host/src/engine/pool.rs`; mirror `__knotDispatchEditorEvent` in
`crates/knot-host/src/js/bootstrap.js`. Roots are queued by
`Scheduler::enqueue_root` (`crates/knot-host/src/scheduler.rs`); teardown is in
`crates/knot-host/src/lifecycle.rs`. `PoolConfig` is built in
`crates/knot-host/src/pool.rs` and by the application in
`src/app/extensions/host.rs` (`ExtensionPool::new`). Runtime flow docs:
[extension host](../architecture/extension-host.md#runtime-and-scheduler-flow).

### 2. Per-view decorations

Deliverables: the per-lifecycle sets on `EditorState`, anchoring, projection
merge, cleanup on view close and lifecycle teardown; `Editor.setDecorations`
through an annotated handler; `knot.d.ts`, bootstrap, and the fake host
(`scripts/extensions/testing/host.ts`); semantic UI and editor-view docs say
decorations may belong to a view.

Validation: model tests for anchoring through edits, merge order, revision
conflicts, and removal by lifecycle; product tests that two views of one
buffer paint only their own set and that teardown and tab close clear it.
`just check`.

Review gate: API shape and paint behavior.

**Implementation context:** `EditorState` and `project_contributions` in
`crates/knot-model/src/editor.rs`; the buffer-wide path is
`BufferModel::replace_contributions` (`crates/knot-model/src/buffer.rs`);
fold anchors in `crates/knot-model/src/editor/folding.rs`; lifecycle-owned
view state follows `key_contexts`/`caret_shapes`/`bars`. The handler sits
beside `setKeyContext` and `openBar` in `src/app/extensions/host/handlers.rs`;
buffer-wide handling is in `src/app/extensions/buffers.rs`.

### 3. Find on per-view decorations

Deliverables: `@knot/find` paints through its session's
`EditorHandle.setDecorations` instead of `buffer.contributions`; the D179
roadmap entry is removed.

Validation: Find's package tests; a product test in
`crates/knot-ext-workbench/tests/workbench/find.rs` with two splits of one
buffer, each with its own bar and highlights. `just check`.

Review gate: in the real app, two find bars over one buffer (personal-config
`Try:` commands).

**Implementation context:** `refresh` in `extensions/@knot/find/src/extension.ts`.

### 4. Word lookup and the highlight package

Deliverables: `buffer.wordAt`; the `editor.wordHighlight` and
`editor.selectionHighlight` keys with bundled-theme values; the
`@knot/word-highlight` package and its settings; `docs/settings.md`; the
decision entry (view-owned decorations, Node timers with a host-owned
driver); architecture updates.

Validation: model tests for `wordAt` at word starts, ends, separators, and
non-ASCII text; package tests for pure logic; product tests through the real
bundled package for caret highlights after the delay, selection highlights,
clearing, edits, and the settings. `just check`.

Review gate: feel and delay in the real app (`Try:` commands).

**Implementation context:** `word_segments` in
`crates/knot-model/src/editor/text.rs`, exposed beside `find` in the buffer
handlers; color keys in `crates/knot-model/src/theme.rs` and
`extensions/@knot/themes/src/themes/`; package layout mirrors
`extensions/@knot/find` and is registered in
`crates/knot-ext-workbench/build.rs`; window following mirrors
`extensions/@knot/status-bar/src/extension.ts`; settings mirror
`extensions/@knot/explorer/src/settings.ts`; product tests beside
`crates/knot-ext-workbench/tests/workbench/find.rs`.
