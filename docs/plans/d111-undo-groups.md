# D111 — Undo groups across edit batches

Status: 🚧 approved, not started. Item of the
[editor API plan](editor-api.md); builds on D109 (editor handles). Brief:
D111 there, and the `ciw…<Esc>` row of the
[extensibility report](../reports/extensibility.md).

## Goal

An extension makes everything one editor does between two points a single
undo step: its own edit batches and the native typing between them. Vim's
`ciw`, text typed in insert mode, and `<Esc>` undo and redo as one step. No
Vim vocabulary enters the API.

## Design

- **Open and close on the editor handle.** `Editor.beginUndoGroup()` resolves
  to a `Disposable`; disposing it ends the group. It lives on the handle, not
  on `Buffer`, because the step records the view's selection before and after
  and native typing in that view must join; a buffer-level variant has no
  consumer yet.
- **One step.** While a group is open, every commit to the buffer joins the
  step: `applyEdits`, editor commands, native typing, paste, and deletion.
  Native `break_history_group` calls (caret moves, focus, commands) do not
  end it. The step's view snapshots are the first entry's `before` and the
  last's `after`, as typing groups already merge them; an entry from another
  view drops the snapshots, as typing groups already do. A group with no
  commits leaves no step.
- **No nesting.** A second `beginUndoGroup` while one is open on the buffer
  rejects with a typed error (`OperationInProgress`). Undo and redo end the
  open group first, then replay it as the one step it is. A forward edit
  after undo clears redo as it does today.
- **Lifecycle-owned.** The owner is the opening lifecycle, as for key
  contexts. The group ends when it is disposed, when the owner's lifecycle
  tears down, when the handle's tab closes (the host already drops stale
  handles after each committed change), or when the buffer closes. A stuck
  extension therefore cannot leave a buffer unable to break its history.
- **State lives in the history.** `EditHistory` holds the open flag and owner;
  the host keeps the owner → model index it needs to end groups on teardown,
  mirroring `key_context_editors`.

## Exclusions

- Nested groups, groups opened on a buffer without a view, and groups that
  survive their lifecycle.
- Command recording and replay (D113) and persistent or branching history
  (D012).
- Any change to the existing time- and interaction-based typing groups.

## Checkpoint 1 — History and model ✅

Deliverables: an open group in `EditHistory` that `apply_replace`,
`apply_batch`, and `amend` extend, ignores `break_group`, and is ended by
undo and redo; `BufferModel` entry points to begin and end it with owner and
the already-open rejection; closing a buffer ends it.

Validation: unit tests for batches, typing, paste, and `amend` joining one
step; view snapshots across the step; redo clearing; undo and redo ending an
open group; rejection of a second group; an empty group leaving no step;
content-state and dirty tracking returning to clean after undoing the step.

Review gate: confirm the step semantics against the tests before any host
API exists.

**Implementation context.** `src/app/history.rs` (`EditHistory`,
`HistoryEntry`, `GroupState`, `break_group`, `apply_*`, `amend`, `undo`,
`redo`); `src/app/model.rs` (`history` field, `commit_batch`,
`commit_replacement`, `break_history_group` at ~1025, `replay_history`, buffer
close); tests in `src/app/model/tests.rs`. Mirror the entry-extending code in
`apply_replace` and `amend`; edits of a batch apply in reverse so earlier
offsets stay valid.

## Checkpoint 2 — Extension API and ownership ✅

Deliverables: `beginUndoGroup` on the editor handle and on the command's
`context.editor`; a host operation that begins the group for the caller's
lifecycle and an end operation behind the returned `Disposable`; teardown,
disposal, and tab-close endings; `knot.d.ts` declarations; the fake `knot`
host in `scripts/extensions` for package tests; bootstrap validation.

Validation: product tests in `src/app/product/tests/editor_api.rs` — typing
and `applyEdits` across commands as one undo step, ending on dispose,
lifecycle teardown (Restart Extension), and tab close, second-group
rejection, undo ending an open group, and a stale handle rejecting.

Review gate: the contract in `knot.d.ts` and the failure and ownership
behavior.

**Implementation context.** Mirror `editorSetKeyContext`
(`src/app/extensions/host/handlers.rs` ~2033), `set_editor_key_context` and
the owner-keyed teardown at `src/app/extensions/host.rs` (~1515, ~2516);
handle object in `src/host/js/bootstrap.js` (~340), types in
`src/host/js/knot.d.ts` (`Editor`, near `setKeyContext`); stale-handle drop
after commits in `host.rs` for the tab-close ending. Annotated handlers follow
[handler declarations](../decisions.md#handler-declarations); add the typed
error beside the existing access errors and to the TypeScript error union.

## Checkpoint 3 — Consumer and documentation ✅

Deliverables: the example `@example/modal` package gains a change command
(delete the word, enter insert mode, `<Esc>` leaves it) whose edit, typing,
and exit are one undo step; documentation; completion cleanup.

Validation: the package's `node:test` flow against the fake host; a product
test driving the example's change-then-type-then-undo path if the existing
modal tests do so.

Review gate: confirm the example proves the primitive and behavior and
rationale are recorded where the docs rules put them.

**Implementation context.** `examples/extensions/@example/modal/src/`
(`extension.ts`, `motion.ts`, `follow.ts`) and `test/extension.test.ts`.
Update [core and buffers](../architecture/core-and-buffers.md) (history
grouping), [commands](../architecture/commands.md#extension-execution)
(handle operations), the [extension guide](../extension-api.md), and add the
rationale to [decisions](../decisions.md#editor-handles). On completion mark
D111 ✅ in the editor API plan with a completion note and delete this file.
