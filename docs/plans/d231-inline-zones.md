# D231 — Inline zones

Source: [roadmap D231](../roadmap.md). Reads with
[editor popovers](../architecture/editor-and-languages.md#editor-popovers),
[block rows](../architecture/editor-and-languages.md#editor-views), and the
[hunk popup](../architecture/version-control.md#hunk-popup).

## Outcome

An extension can open a zone in an editor: a toolbar over a layout of views
(the content of a popover) shown in rows reserved between the editor's lines,
above a chosen line or below the last one. The views take focus, clicks, and
typing. A zone scrolls, folds, and follows edits like block rows. The hunk
popup becomes a zone: the deleted lines sit between the editor's lines, above
the current ones, with the popup's toolbar.

## Design

- **Whole reserved rows.** A zone is an entry in the wrap map that reserves N
  rows instead of carrying text lines. The editor measures the content's
  natural height and rounds it up to whole line heights, so the wrap map,
  scrolling, folding, and edit-following stay integer-row. Up to one line of
  slack under the content is accepted. Rejected: pixel-height rows, which
  would touch every row-to-y conversion for no consumer's benefit.
- **Placement.** A zone sits above a line, as block rows do, or at
  `line == lineCount`, below the last line, so a removal at the end of the
  file shows where its lines were. Only zones take the end position; block
  rows keep clamping to the last line. A line's rows are its zones, then its
  block rows, then its text. Zone rows belong to no line: like block rows,
  `position` reports them as the first row of the line below, and the caret
  steps over them.
- **Same content as popovers.** A zone reuses `PopoverContent` and its
  `Sizing::Natural` layout; its views are children of the editor in the view
  tree. Only placement differs: the editor paints the content in the reserved
  rows' frame instead of floating it.
- **Width and horizontal scroll.** A zone spans the text area's width, not the
  gutter, and stays pinned when the text scrolls horizontally.
- **Height feedback.** Content height can change after layout, and depends on
  the width. The editor re-measures while rendering, including after a width
  change, updates the zone's row count, and re-wraps. The height is capped by
  the members' own limits (an editor member's `maxLines`/`maxRows`) and
  clamped to the viewport, so members scroll themselves past that. A row-count
  change keeps the top row's text at the top, as `set_blocks` does, so a zone
  above the viewport never moves the text in view.
- **Wheel routing.** A wheel over a zone scrolls the member under the pointer
  when that member can scroll, and the outer editor otherwise.
- **Own API.** `editor.openZone({ id, line, ... })` with the popover's ID,
  update-in-place, `focus`, and a `onDidCloseZone` event. A zone holds no
  caret. Keyboard access is `focus` on open; no "focus zone" command yet.
- **Lifecycle.** An edit does not close a zone; it follows its line, and a
  zone whose line is deleted moves to the start of the edit, as block rows
  do. Close reasons: Escape in a focused zone, an explicit close, owner
  teardown, and editor close. A click elsewhere does not close it. Zones are
  presentation state, never recorded.
- **State split.** `WrapMap` holds only each zone's opaque key, line, and row
  count. An `EditorZones` model beside `EditorPopovers` owns the owner, ID,
  spec, and lifecycle, and drives the wrap map through `EditorState`.
- **Existing block rows stay.** Text-only rows, which the diff package's
  inline layout uses, keep their own API.

## Exclusions

Pixel-exact heights, resizing by drag, a "focus zone" command, gutter-mark
tooltips (D225), and persisting zones across restarts.

## Deferred

Whether a zone and a popover on one line should avoid each other; start with
painting both independently.

## Checkpoints

### 1. Zones in the wrap map ✅

Zones are a sorted list in `WrapMap`, apart from block entries. `EditorZones`
(`crates/knot-model/src/editor/zones.rs`) owns owner, ID, and keys, and
`EditorState` drives the wrap map through `open_zone`, `set_zone_rows`,
`close_zone`, `dismiss_zone`, and `take_closed_zones`. A new zone has no rows
until the view measures it (checkpoint 2). Only Escape reports a closure to
the owner. Approved; verified by model tests only, since nothing opens a zone
yet.

### 2. Measuring and painting zones

Deliverable: the editor view measures a zone's `PopoverContent` at the text
area's width, sets its row count, re-measures on content or width change, and
paints it in the reserved frame, pinned horizontally; scroll, clipping at the
viewport, and folded lines hide it.

Validation: app tests with gpui test contexts beside
`src/app/editor/tests/popovers.rs` and `tests/blocks.rs`: sizing, re-measure
after a content and a width change, viewport clamp, frame position under
vertical and horizontal scroll, fold.

Review gate: code review of the measure/paint loop (no oscillation, one
re-wrap per change).

**Implementation context:** `src/app/editor/popovers.rs` (`PopoverContent`,
`popover_frames`), `src/app/editor/paint.rs` and `layout.rs` (row painting,
block rows), `src/app/view_layout.rs` (`Sizing::Natural`, `natural_height`),
and `src/app/view_tree.rs` for children.

### 3. Input and focus in zones

Deliverable: focus, clicks, typing, and Escape route to a zone's views; a
wheel over a zone goes to a scrollable member under the pointer, else the
outer editor; focus returns to the text when a focused zone closes.

Validation: app tests: focus and typing in a zone member, click routing
inside and outside the frame, Escape closing a focused zone and returning
focus, wheel routing with and without a scrollable member.

Review gate: code review of hit testing and focus handling against popovers'.

**Implementation context:** `src/app/editor/input.rs` (hit testing: a point in
a zone's frame goes to its views), focus handling via `PopoverFocus` in
`src/app/editor/popovers.rs`.

### 4. Extension API

Deliverable: `editor.openZone`, `closeZone`, and `onDidCloseZone` through the
protocol, host handlers, bootstrap, and `knot.d.ts`; per-lifecycle and
per-view teardown; documentation in `docs/extension-api/ui.md`.

Validation: product tests beside `src/app/product/tests/editor_popovers.rs`
(open, end-of-file line, update in place, keyed members kept, close reasons,
teardown).

Review gate: API shape reviewed against the popover API, and a manual `try.*`
command that opens a zone with a text editor and a tree member, checking
sizing, typing, scrolling, folding, and Escape in the real app.

**Implementation context:** `crates/knot-protocol` operation declarations,
`src/app/extensions/host/handlers.rs` and `composites.rs` (the popover
handlers to mirror), `crates/knot-host/src/js/bootstrap.ts` and `knot.d.ts`.

### 5. The hunk popup as a zone

Deliverable: `vcs.showHunk` opens a zone above the hunk's first current line
(a removal above the line that followed it, or below the last line for a
removal at the end of the file), with the deleted lines banded removed and the
same toolbar. An insertion shows only the toolbar. Previous and Next Hunk move
it; the package closes it when the baseline changes or the document closes,
and on an edit, as the popup does today.

Validation: `@knot/vcs` package tests for open (including a removal at the end
of the file), navigation, and close; update the existing hunk popup tests.

Review gate: manual `try.*` check in the real app. Then move durable behavior
into the architecture references, record the choices in
`docs/decisions/semantic-ui.md` and the hunk decision, drop the "left to D231"
note, delete this plan, and remove D231 from the roadmap.

**Implementation context:** `packages` `@knot/vcs` hunk popup code (find with
`vcs.hunk`), `docs/architecture/version-control.md#hunk-popup`,
`docs/decisions/bundled-packages.md` "Hunk popup".
