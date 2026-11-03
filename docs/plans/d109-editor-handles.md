# D109 — Editor handles and selection observation

Status: 🚧 in progress; checkpoint 1 ✅; checkpoint 2 ✅ awaiting its review gate. Item of the [editor API](editor-api.md)
plan; absorbs D080's observation API. Builds on D110's `setKeyContext`.
Consumers: D077 (status bar), D080 (occurrence highlighting), D058 (Vim).

## Goal

An extension can follow an editor beyond one command. It takes the tab a
window presents, obtains an editor handle for it, sets key contexts on it, and
is told when its selection changes. The status bar can describe the focused
editor, and a mode can enter its key context in editors it did not get from a
command.

## Design

- **Obtaining.** `workbench.editor(target: ViewTarget): Promise<Editor>`
  returns a handle for a document tab. Targets already come from observation
  (`activeTargetChanged`, snapshot panes), so no discovery API is added. A
  target that is not a live document tab rejects `InvalidTarget`. The
  returned `Editor` is the same interface as a command's; the handle
  returned inside a command stays valid only for that invocation tree.
- **Authority.** A long-lived handle is bound to the exact tab presentation,
  not to a command capture. Each call resolves that presentation, so it
  rejects `InvalidTarget` once the tab closes, moves to another pane or
  window, or is replaced. It never retargets a replacement tab.
- **Lifetime.** The handle is `Disposable` and lifecycle-owned. Disposal, tab
  closure, and lifecycle teardown release its listeners. `onDidClose(listener)`
  tells a mode to drop its per-editor state when the tab goes away.
- **Key contexts.** `setKeyContext` works unchanged on a long-lived handle;
  the existing per-owner tracking in `key_context_editors` already clears it
  on teardown.
- **Selection observation.** `editor.onDidChangeSelection(listener)` is
  payload-free and coalesced. The view marks its selection changed; each
  listener has at most one pending notification, which fires and then reads
  `selection()`. A listener that is still running when more changes arrive
  gets one further notification, not a queue, as picker highlight previews are
  pulled. The selection changes for any reason, caret motion caused by edits
  included. Painting and input never wait on JavaScript.
- **One selection per view.** Several and rectangular selections are D142.

## Exclusions

- A window-level selection event or a global editor registry.
- Handles for terminals and tool tabs; contributed-view selection stays as is.
- Edit, view-control, and undo-group operations on the handle (D111, D141).
- A payload with the old selection or the cause of the change.

## Deferred decisions

- Whether a notification should say that a change came from an edit rather
  than a caret move; add it when a consumer (D080) needs it.
- Whether a mode needs a native default key context. A consumer such as Vim
  attaches handles to every editor from `presentationsChanged`, so a new or
  replaced tab has no context until its handle is obtained and the context is
  set, and keys typed in that window insert text. D058 decides whether that
  gap needs a native default context or is acceptable.
- Whether handles need a cap on live listeners; wait for real workloads
  (D019).

## Checkpoints

### 1. Long-lived handle from a ViewTarget ✅

Deliver `workbench.editor(target)` with the exact-tab authority, disposal,
`onDidClose`, and the docs. Key contexts work on it.

- Tests: model/product tests that a handle outlives its command; rejects after
  close, tab move, and replacement; `setKeyContext` on a handle obtained
  outside a command and cleared on teardown; `onDidClose` fires once;
  `node:test` against the fake host.
- **Implementation context.** Declare `workbench.editor` and `onDidClose` in
  `src/host/js/knot.d.ts` and `bootstrap.js` beside `editorFor` (currently
  keyed by command authority). Add a handler beside the `editor*` handlers in
  `src/app/extensions/host/handlers.rs`, annotated with `host_operation`, and
  a resolver beside `contextual_editor`/`context_target` in
  `src/app/extensions/host.rs` that maps a `ViewTarget` to an
  `Entity<EditorView>` the way `close(target)` validates one (reuse
  `validate_tab`/`validate_view`). Keep handle ownership next to
  `key_context_editors` so lifecycle teardown releases both. Extend the fake
  `knot` host in `scripts/extensions/`. Update
  [commands](../architecture/commands.md#extension-execution) and the
  extension guide.
- **Completion note.** ✅ `workbench.editor(target)` returns an
  `EditorHandle` (`Editor` plus `onDidClose` and `dispose`). Decisions made
  while implementing: the host keeps one `EditorTab` per handle (the tab's
  exact identity, resolved on every call) and carries handle authority to the
  existing `editor*` operations as `EDITOR_HANDLE_AUTHORITY + id`; the host
  drops stale handles after each application observation refresh and queues
  one payload-free `closed` event through a generic `EditorEvent`
  notification that checkpoint 2 extends with a selection event; disposal and
  teardown release without notifying; a key context set through a handle
  stays until cleared or teardown. Docs: [commands](../architecture/commands.md#extension-execution),
  [extension guide](../extension-api.md). Tests: `editor_api.rs` product tests
  (close, move, replacement, teardown) and `editor.test.ts`.
- **Review gate.** The authority rule and the `InvalidTarget` cases are
  agreed before observation builds on them. ⏳ pending.

### 2. Coalesced selection observation ✅

Deliver `onDidChangeSelection` with one pending notification per listener.

- Tests: a burst of caret moves and edits produces at most one pending
  notification per listener and the listener reads the final selection; a
  busy listener falls behind without a queue; unsubscribing, tab closure, and
  teardown stop delivery; painting is not blocked (no synchronous JavaScript).
  Fake-host coverage.
- **Implementation context.** The signal belongs in `src/app/editor.rs`, where
  `set_selection` and the edit paths change the selection (notify only when
  the anchor or caret differ). Deliver it through the pulled-event pattern of
  `nextPickerEvent` (`handlers.rs`) or the buffer subscription pair
  `subscribeBufferChanges`/`unsubscribeBufferChanges`, whichever fits the
  one-pending-per-listener rule with less new machinery; mirror the
  `bufferChangeListeners` registry in `bootstrap.js`. Record the flow in
  [extension host](../architecture/extension-host.md) and the rationale in
  [decisions](../decisions.md).
- **Completion note.** ✅ `EditorHandle.onDidChangeSelection`. Implemented
  by observing the editor entity from the host (covers every cause without
  touching each selection mutation) and comparing offsets; one pending flag
  per handle, cleared when the listener turn starts, fans out to every
  listener in JavaScript. This is per handle rather than per listener: the
  observable behavior is the same and it needs no per-listener host state.
  Delivery reuses the `EditorEvent` root from checkpoint 1. Docs:
  [extension host](../architecture/extension-host.md#editor-handle-notifications),
  [decisions](../decisions.md#editor-handles). Tests: product test for
  unchanged selections, a busy listener with a burst, and typing; fake-host
  test. Closure and teardown stop delivery by dropping the handle and its
  observation, covered only through handle removal.
- **Review gate.** Latency and coalescing behavior agreed on a small example
  consumer (an occurrence-style listener) before D077 depends on it. ⏳ pending;
  the example is checkpoint 3's deliverable, so measure latency there.

### 3. Example and durable docs

A small example under `examples/extensions/` that follows the active editor,
subscribes to its selection, and sets a key context, plus the final
architecture, decisions, and extension-guide updates. Mark D109 ✅ in the
index and remove this plan once its behavior lives in the references.
