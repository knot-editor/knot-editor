# D231 — Inline zones

Source: [roadmap D231](../roadmap.md). Reads with
[editor popovers](../architecture/editor-and-languages.md#editor-popovers),
[block rows](../architecture/editor-and-languages.md), and the
[hunk popup](../architecture/version-control.md#hunk-popup).

## Outcome

An extension can open a zone in an editor: a toolbar over a layout of views
(the content of a popover) shown in rows reserved between the editor's lines,
above a chosen line. The views take focus, clicks, and typing. A zone scrolls,
folds, and follows edits like block rows. The hunk popup becomes a zone: the
deleted lines sit between the editor's lines, above the current ones, with the
popup's toolbar.

## Design

- **Whole reserved rows.** A zone is a block entry in the wrap map that
  reserves N rows instead of carrying text lines. The editor measures the
  content's natural height and rounds it up to whole line heights, so the wrap
  map, scrolling, folding, and edit-following stay integer-row. Up to one line
  of slack under the content is accepted. Rejected: pixel-height rows, which
  would touch every row-to-y conversion for no consumer's benefit.
- **Same content as popovers.** A zone reuses `PopoverContent` and its
  `Sizing::Natural` layout; its views are children of the editor in the view
  tree. Only placement differs: the editor paints the content in the reserved
  rows' frame instead of floating it.
- **Height feedback.** Content height can change after layout. The editor
  re-measures while rendering, updates the zone's row count, and re-wraps. The
  height is capped by the layout's `maxLines` and `maxRows` limits and clamped
  to the viewport, so members scroll themselves past that.
- **Own API.** `editor.openZone({ id, line, ... })` with the popover's ID,
  update-in-place, `focus`, and a `onDidCloseZone` event. A zone sits above a
  line, as block rows do, and holds no caret.
- **Lifecycle.** An edit does not close a zone; it follows its line. Escape in
  a focused zone, an explicit close, owner teardown, and editor close end it.
  A click elsewhere does not. Zones are presentation state, never recorded.
- **Existing block rows stay.** Text-only rows, which the diff package's
  inline layout uses, keep their own API.

## Exclusions

Pixel-exact heights, resizing by drag, gutter-mark tooltips (D225), and
persisting zones across restarts.

## Deferred

Whether a zone and a popover on one line should avoid each other; start with
painting both independently.

## Checkpoints

### 1. Zones in the wrap map

Deliverable: a zone entry in `WrapMap` reserving N rows above a line, with its
row count settable, following edits, hiding with a folded line, holding no
caret, and surfacing its frame rows for painting. `EditorState` exposes
set/update/remove by owner and ID.

Validation: model tests beside the block tests (rows, scrolling, fold, edit
following, row-count change, caret stepping over).

Review gate: row semantics match block rows; no change to existing block-row
behavior.

**Implementation context:** `crates/knot-model/src/editor/wrap.rs`
(`Block`, `BlockEntry`, `block_rows`, `block_row`, `follow_blocks`,
`normalized`) and `crates/knot-model/src/editor.rs` (`set_blocks`); mirror
`crates/knot-model/src/editor/tests/blocks.rs`. Zones are a second entry kind
in the same per-line sequence so a line's rows are zones, then block rows,
then text.

### 2. Hosting views in a zone

Deliverable: the editor view measures a zone's `PopoverContent`, sets its row
count, paints it in the reserved frame, and routes focus, clicks, typing, and
Escape to it; scroll, clipping at the viewport, and folded lines hide it.
Focus returns to the text when a focused zone closes.

Validation: app tests with gpui test contexts beside
`src/app/editor/tests/popovers.rs` and `tests/blocks.rs`: sizing and re-measure,
focus and typing in a zone member, click routing, fold and scroll behavior.

Review gate: manual `try.*` command that opens a zone with a text editor and
a tree member.

**Implementation context:** `src/app/editor/popovers.rs` (`PopoverContent`,
`popover_frames`, focus handling via `PopoverFocus`), `src/app/editor/paint.rs`
and `layout.rs` (row painting, block rows), `src/app/editor/input.rs` (hit
testing: a point in a zone's frame goes to its views), and
`src/app/view_tree.rs` for children. Zones keep their state beside
`EditorPopovers` in `crates/knot-model/src/editor/popovers.rs`' style.

### 3. Extension API

Deliverable: `editor.openZone`, `closeZone`, and `onDidCloseZone` through the
protocol, host handlers, bootstrap, and `knot.d.ts`; per-lifecycle and
per-view teardown; documentation in `docs/extension-api/ui.md`.

Validation: product tests beside `src/app/product/tests/editor_popovers.rs`
(open, update in place, keyed members kept, close reasons, teardown).

Review gate: API shape reviewed against the popover API.

**Implementation context:** `crates/knot-protocol` operation declarations,
`src/app/extensions/host/handlers.rs` and `composites.rs` (the popover
handlers to mirror), `crates/knot-host/src/js/bootstrap.ts` and `knot.d.ts`.

### 4. The hunk popup as a zone

Deliverable: `vcs.showHunk` opens a zone above the hunk's first current line
(a removal above the line that followed it), with the deleted lines banded
removed and the same toolbar. An insertion shows only the toolbar. Previous and
Next Hunk move it; the package closes it when the baseline changes or the
document closes, and on an edit, as the popup does today.

Validation: `@knot/vcs` package tests for open, navigation, and close; update
the existing hunk popup tests.

Review gate: manual `try.*` check in the real app. Then move durable behavior
into the architecture references, record the choices in
`docs/decisions/semantic-ui.md` and the hunk decision, drop the "left to D231"
note, delete this plan, and remove D231 from the roadmap.

**Implementation context:** `packages` `@knot/vcs` hunk popup code (find with
`vcs.hunk`), `docs/architecture/version-control.md#hunk-popup`,
`docs/decisions/bundled-packages.md` "Hunk popup".
