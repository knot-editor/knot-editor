# D221 — Hunk popup

Status: 🚧 planned. Roadmap item [D221](../roadmap.md), part of the
[version control plan](vcs.md), milestone 3.

## Outcome

A click on a hunk mark in the gutter opens a popup under the hunk. It shows
the baseline's lines that the hunk replaced, in the file's language. Its toolbar
moves to the previous and next hunk, reverts the hunk, and opens the file's
diff at that hunk. The same popup opens from the palette for the hunk at the
caret, and a hunk can be reverted from the palette without opening the popup.

## Design

Everything lives in `@knot/vcs` and is built on
[editor popovers](../architecture/editor-and-languages.md#editor-popovers),
with one native change: a popover's layout may hold no view.

- **Content.** The popover shows only the baseline's lines for the hunk, in a
  read-only `buffers.create` buffer in the document's language, banded
  `diff.removed.background` through a view decoration. The current lines stay
  where they are in the editor beside the popup. A pure insertion has no
  baseline lines, so its popup is just the toolbar (a popover's layout may hold
  no view). Changes inside a line are not highlighted, because `diff` reports
  none. Width is `"editor"`; the baseline editor uses the popover's default
  `maxLines`.
- **Anchor.** A hunk with current lines is anchored to them, so the popup sits
  under the changed block (or over it when there's no room below). A removal
  is anchored to the line before it, so the popup sits where the removed lines
  were; a removal at the top of the file uses line 0.
- **One popup per editor.** It has a fixed popover ID, `vcs.hunk`. The package
  remembers, per editor, which hunk the popup shows. Opening it on another hunk
  updates it in place.
- **Opening.** A click on a hunk mark runs `vcs.showHunk` instead of
  `vcs.showChanges`. `vcs.showHunk` ("Show Hunk") opens the popup for the
  clicked mark's hunk, or for the hunk at the caret from the palette. It has no
  default key. A caret on no hunk leaves it unavailable.
- **Toolbar.** Previous Hunk, Next Hunk, Revert Hunk, Compare with Last
  Commit, and Copy Hunk (not on an insertion). `vcs.copyHunk` ("Copy
  Hunk") copies the baseline's lines for the hunk at the caret. Previous
  and Next move the caret to that hunk, as `vcs.previousHunk` and
  `vcs.nextHunk` do (wrapping at the ends), and move the popup there. Compare
  with Last Commit runs `vcs.showChanges` at the hunk's line.
- **Navigation while open.** `vcs.nextHunk` and `vcs.previousHunk` (Alt+F5,
  Alt+Shift+F5, `]c` and `[c`) also move the popup when it is open on the
  focused editor, so stepping through changes keeps it showing. Without a popup
  they behave as today.
- **Revert.** `vcs.revertHunk` ("Revert Hunk") replaces the hunk's current
  lines with its baseline lines in one edit at the diffed revision, one undo
  step. It works from the toolbar on the popup's hunk, and from the palette on
  the hunk at the caret. The edit closes the popup, since popovers close on any
  outside edit. A missing final newline on either side is handled so that the
  revert restores the baseline text exactly.
- **Closing.** Knot's own rules close the popup (Escape, click outside, edits,
  the anchor deleted). `@knot/vcs` also closes it when the document's baseline
  is fetched again or goes away, rather than following a hunk that may have
  moved, and when the document closes.

### Exclusions

- Backend actions in the popup, such as staging a hunk: D216 adds the contract
  hook along with its first action.
- Showing the hunk inline, with the deleted lines pushed between the editor's
  lines as Zed does: a follow-up of D231, inline zones.
- Highlighting changes inside a line.
- A default key for `vcs.showHunk`.

### Deferred decisions

None.

## Checkpoints

### 1. Hunk model: popup anchor and revert edit ✅

Done and reviewed: `extensions/@knot/vcs/src/hunk.ts` holds the hunk lookup,
popup anchor, baseline text, and revert edit (`hunkAt`, `anchorLines`,
`baselineText`, `revertEdit`), with unit tests in `test/hunk.test.ts`. Hunk
lines carry their terminators and have no trailing empty line, unlike
`Buffer` lines; `revertEdit` takes the buffer's `lineCount`/`lineRange`.

### 2. The popup, its commands, and the click ✅

Done and reviewed in the app: `extensions/@knot/vcs/src/extension.ts` holds
the popup, `vcs.showHunk`, `vcs.revertHunk`, `vcs.copyHunk`, and navigation
that moves an open popup, and `gutter.ts` makes a click on a mark run
`vcs.showHunk`. Product tests are in `crates/knot-ext-vcs/tests/vcs/popup.rs`.
The popup state is kept per document, not per editor, so opening it in a
second editor of the same document replaces the first. A popover's layout may
now hold no view, which an insertion's popup relies on (native change in
`src/app/editor/popovers.rs`).

### 3. Documentation

Move what lasts into the current docs and remove the plan.

**Implementation context.** Update the Gutter, Navigation, and Showing changes
sections of `docs/architecture/version-control.md` (the click now opens the
popup), the `vcs.md` plan's D221 item, the roadmap, and the user guide that
lists the version control commands, if any. Record the content and anchoring
choices in `docs/decisions/bundled-packages.md` next to the other version
control decisions, and add them to the `docs/decisions.md` index.

**Review gate.** Docs match the behavior; plan removed.
