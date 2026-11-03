# D141 — Editor view controls

Status: 🚧 approved, not started. Item of the [editor API plan](editor-api.md);
extends the D109 editor handle and is a prerequisite of D058 (Vim mode).

## Goal

Give extensions the view-level controls a modal editor needs, on the existing
`Editor` handle and `workbench`: a cursor style per mode, the visible line
range, scrolling without moving the caret, reveal at the top or bottom (`zt`,
`zb`), live anchored positions for marks, and clipboard read. Each primitive
is generic enough for a Helix or Kakoune mode.

## Design

- **Cursor style.** `editor.setCursorStyle("bar" | "block" | "underline" | null)`.
  It is per-view presentation state owned by the caller's lifecycle, like key
  contexts: it is never recorded, teardown or `null` reverts to the default
  bar, and the view closing drops it. Block and underline paint over the
  character under the caret. A stuck or unloaded package therefore cannot
  leave a block cursor behind. A scoped setting was rejected because a mode
  change would be a settings write per view.
- **Visible lines and scrolling.** `editor.visibleLines()` returns
  `{ first, last }`, the 0-based fully visible lines from the last painted
  layout (it may lag one frame behind a just-issued scroll). `editor.scrollBy(lines)`
  scrolls without moving the caret or revealing it. `SetSelectionOptions.reveal`
  gains `"top"` and `"bottom"` beside `"nearest"` and `"center"`; `zz` is the
  existing `"center"`. Units are logical lines; soft wrap and visual lines are
  D078 and D112, which refine the same operations.
- **Marks.** `buffer.createMark(offset)` returns a lifecycle-owned `Mark` with
  `position(): Promise<number | null>` and `dispose()`. It is an undecorated
  anchored range in the model's `AnchoredRangeStore`, so it follows the same
  synchronous commit repair as contributions. The position is `null` once its
  text is deleted. Marks have no name, listing, or persistence: Vim's letters,
  global marks, bookmarks, and recreating marks when a document opens are
  extension policy over its own storage. Live tracking is native because an
  extension remapping offsets from asynchronous `onDidChange` events would lag
  typing. A native bookmarks feature, if wanted later, is a separate roadmap
  item built on this primitive.
- **Clipboard read.** `workbench.readClipboardText(): Promise<string | null>`,
  application-scoped like `writeClipboardText`, `null` for non-text content. No
  entitlement gate (D087 gates both directions later).

## Exclusions

Multiple or rectangular selections (D142), decoration styles (D114), visual-line
movement and soft-wrap units (D078, D112), mark naming, persistence, or
listing, a mark change notification, and clipboard change observation.

## Consumer

Each primitive lands with a use in `examples/extensions/@example/modal`
(removed when D058 lands): a block cursor in normal mode, `zt`/`zb`, `m` and
`'` marks, and `p` from the clipboard.

## Checkpoints

### 1. Cursor style ✅

`setCursorStyle` is lifecycle-owned view state on `EditorView`: the latest set
wins, clearing reveals the previous, and teardown or the view closing clears it.
Block is drawn translucent over the character and underline beneath it; both
are half a line height wide at a line's end. Behavior is in
[commands](../architecture/commands.md#key-contexts) and the
[extension guide](../extension-api.md). Painting has no automated test; it was
checked by eye in a real window. Shape resolution, the host operation, and the
fake host are covered by tests.

### 2. Visible lines, scrolling, and reveal ⏳

- Deliver `visibleLines`, `scrollBy`, and `reveal: "top" | "bottom"`, with the
  fake host modelling a configurable viewport and scroll position.
- Validation: `node:test` cases for the fake (clamping, caret untouched, reveal
  placement); editor tests beside the reveal tests (`src/app/editor/tests.rs`,
  around the `Reveal` cases) for clamping at both ends and caret untouched;
  product tests for the operations and `InvalidTarget`.
- **Implementation context:** `scroll`, `viewport_h`, `ensure_cursor_visible`,
  and `clamp_scroll` on `EditorView`; extend the `Reveal` enum decoded in
  `editor_set_selection`; same host-operation wiring as checkpoint 1.
- **Review gate:** the visible range agrees with what is painted.

### 3. Marks ⏳

- Deliver `buffer.createMark`, `Mark.position`, and disposal. The fake host
  moves marks with the edits it already applies, so its tests stay policy-level.
- Validation: `node:test` cases for the fake (edits around a mark, deletion
  gives `null`, disposal); model tests for edits before, inside, and after a mark, deletion,
  undo, and disposal; product tests for lifecycle teardown and buffer closure.
- **Implementation context:** `ContributionRegistry`/`ContributionSource` in
  `src/app/model.rs` and `AnchoredRangeStore` in `src/core/anchored_range.rs`
  (reuse `add`/`resolve`/`remove`); buffer operations in
  `src/app/extensions/buffers.rs` and `handlers.rs`; owners released with the
  lifecycle as contributions are.
- **Review gate:** stability semantics at replaced spans and undo are agreed.

### 4. Clipboard read, modal example, and docs ⏳

- Deliver `workbench.readClipboardText`, extend the modal example with the four
  consumers, and update the extension guide, [commands](../architecture/commands.md#extension-execution),
  and [decisions](../decisions.md#editor-handles); then remove this plan and mark
  D141 complete in the index plan.
- Validation: product test for read after write and `null` for nontext; the
  fake reads the clipboard state `writeClipboardText` already fills, and the
  example's `node:test` cases run against it.
- **Implementation context:** `writeClipboardText` handler and its `cx`
  clipboard calls; `examples/extensions/@example/modal/src` and `test`.
- **Review gate:** the example exercises every primitive end to end.
