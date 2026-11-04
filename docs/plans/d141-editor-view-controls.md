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

### 2. Visible lines, scrolling, and reveal ✅

`visibleLines` counts only wholly visible lines and is never empty; `scrollBy`
takes whole lines and leaves the caret alone; `reveal: "top" | "bottom"` always
aligns the caret's line with that edge of the viewport, as far as the text
allows. `"center"` keeps its Go to Line meaning of recentering only when the
line is off screen, so a Vim `zz` needs an always-centering option when D058
reaches it. Behavior is in the [extension guide](../extension-api.md) and
[workbench and lifecycle](../architecture/workbench-and-lifecycle.md#editor-views).
The product test asserts only the first visible line and the caret, since
painting re-measures the viewport. Checked by hand on a large file: the reported
lines matched the reveals.

### 3. Marks ✅

`buffer.createMark(offset)` is a zero-width anchored range owned by the calling
lifecycle (`src/app/model.rs`, bridged in `src/app/extensions/buffers.rs`).
Semantics found in the anchored-range store rather than chosen: a mark stays with
the character after it, so text inserted at it moves it; a deletion of the text
it sat in consumes it and undo does not revive it (`position()` stays `null`);
an empty buffer cannot hold one (`InvalidRange`), as for contributions. Behavior
is in the [extension guide](../extension-api.md) and
[core and buffers](../architecture/core-and-buffers.md). Tests cover the model
(edits around, inside, undo and redo, lifecycle removal, offset validation), the
extension path, and the fake host; buffer closure and lifecycle teardown through
the product host are covered only by the model test.
### 4. Clipboard read, modal example, and docs ✅ (review gate pending)

Delivered: `workbench.readClipboardText()`; the modal example's block caret
(`mode.ts`), `zt`/`zb`, `m`/`'` (one mark, jumping back after edits move it), and
`p`; and the extension guide and [decisions](../decisions.md#editor-handles)
text. `null` is tested only for an empty clipboard, not an image or file.
The example's key sequences (`z t`) are checked for registration by the product
test but not typed by it.

- **Review gate:** the example exercises every primitive end to end in a real
  window. After approval, preserve nothing further (the durable behavior is
  already in the docs above), remove this plan in a separate commit, and mark D141
  complete in the [index plan](editor-api.md).
