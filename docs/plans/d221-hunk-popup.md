# D221 — Hunk popup

Status: 🚧 planned. Roadmap item [D221](../roadmap.md), part of the
[version control plan](vcs.md), milestone 3.

## Outcome

A click on a change mark in the gutter opens a popup under the hunk. It shows
the baseline's lines that the hunk replaced, in the file's language. Its toolbar
moves to the previous and next hunk, reverts the hunk, and opens the file's
diff at that hunk. The same popup opens from the palette for the hunk at the
caret, and a hunk can be reverted from the palette without opening the popup.

## Design

Everything lives in `@knot/vcs` and is built on
[editor popovers](../architecture/editor-and-languages.md#editor-popovers),
with no native changes.

- **Content.** The popover shows only the baseline's lines for the hunk, in a
  read-only `buffers.create` buffer in the document's language, banded
  `diff.removed.background` through a view decoration. The current lines stay
  where they are in the editor beside the popup. A pure insertion has no
  baseline lines, so its popup is just the toolbar. Changes inside a line are
  not highlighted, because `diff` reports none. Width is `"editor"`; the
  baseline editor uses the popover's default `maxLines`.
- **Anchor.** A hunk with current lines is anchored to them, so the popup sits
  under the changed block (or over it when there's no room below). A removal
  is anchored to the line before it, so the popup sits where the removed lines
  were; a removal at the top of the file uses line 0.
- **One popup per editor.** It has a fixed popover ID, `vcs.hunk`. The package
  remembers, per editor, which hunk the popup shows. Opening it on another hunk
  updates it in place.
- **Opening.** A click on a change mark runs `vcs.showChange` instead of
  `vcs.showChanges`. `vcs.showChange` ("Show Change") opens the popup for the
  clicked mark's hunk, or for the hunk at the caret from the palette. It has no
  default key. A caret on no hunk leaves it unavailable.
- **Toolbar.** Previous Change, Next Change, Revert Change, and Compare with
  Last Commit. Previous and Next move the caret to that hunk, as
  `vcs.previousChange` and `vcs.nextChange` do (wrapping at the ends), and move
  the popup there. Compare with Last Commit runs `vcs.showChanges` at the
  hunk's line.
- **Navigation while open.** `vcs.nextChange` and `vcs.previousChange` (Alt+F5,
  Alt+Shift+F5, `]c` and `[c`) also move the popup when it is open on the
  focused editor, so stepping through changes keeps it showing. Without a popup
  they behave as today.
- **Revert.** `vcs.revertChange` ("Revert Change") replaces the hunk's current
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
- A default key for `vcs.showChange`.

### Deferred decisions

None.

## Checkpoints

### 1. Hunk model: popup anchor and revert edit ✅

Pure functions in `@knot/vcs` for what a hunk means, tested without a host.

- The hunk containing (or, for a removal, at) a line, used by click, caret,
  and revert.
- The popover anchor of a hunk, per the rules above.
- The baseline text of a hunk, and the edit that reverts it: the byte range of
  the current lines and its replacement text, including the end-of-file cases
  (removal at the end, missing final newline on either side).

**Implementation context.** Add the functions to a new
`extensions/@knot/vcs/src/hunk.ts`, next to `hunks.ts` (gutter contributions)
and `navigation.ts` (`targetLine`), whose style it mirrors. Tests go in
`extensions/@knot/vcs/test/hunk.test.ts`, written like `navigation.test.ts`.
Byte ranges come from `Buffer.lineRange`/`lineCount`; keep the pure part on
line indices and baseline text so the tests need no buffer, as `targetLine`
does.

**Validation.** Unit tests for insertions, modifications, removals (top,
middle, end of file), and files with and without a final newline.

**Review gate.** The hunk rules and revert edits read right.

### 2. The popup, its commands, and the click

The user-visible feature: `vcs.showChange`, `vcs.revertChange`, the toolbar,
navigation moving the popup, and the closing rules.

- Gutter marks run `vcs.showChange` with the mark's range as arguments, as
  they now pass it to `vcs.showChanges`.
- The popup state per editor (hunk, baseline buffer) is dropped on
  `onDidClosePopover`, on baseline refetch, and on document close, and its
  buffer is disposed.
- Icons for the toolbar buttons, defined with `icons.define`.

**Implementation context.** Commands are registered in `start()` in
`extensions/@knot/vcs/src/extension.ts`, next to `showChanges` and `go`; reuse
`go`'s caret move and `showChanges`' hunk lookup, and extend `go` to move an
open popup. `SHOW_CHANGES` in `gutter.ts` becomes the new click command. The
popover API is `Editor.openPopover`/`closePopover`/`onDidClosePopover` in
`crates/knot-host/src/js/knot.d.ts`; the toolbar is `CompositeButton[]`, whose
commands get the anchoring editor as `editor`. Icons follow
`extensions/@knot/diff/src/extension.ts` (`icons.define("diff.previous", …)`).
The baseline refetch hook is `syncDiff` in `update()`, and document close is
`release`. Product tests go in `crates/knot-ext-vcs/tests/vcs/` (a new
`popup.rs`, registered in `main.rs`), over the fixed-baseline backend that
`gutter.rs` uses; `test/gutter.test.ts` covers the in-memory paths it can't
drive. Write a personal-config `post-init.js` with `Try:` commands for the
manual check.

**Validation.** Product tests: a click on a mark opens the popup with the
baseline lines; Next and Previous move the caret and the popup, wrapping;
Revert restores the baseline text in one undo step and closes the popup;
`vcs.showChange` and `vcs.revertChange` from the caret; a baseline refetch
closes the popup; an insertion's popup has only the toolbar. `just check`.

**Review gate.** Manual check in the app through the `Try:` commands.

### 3. Documentation

Move what lasts into the current docs and remove the plan.

**Implementation context.** Update the Gutter, Navigation, and Showing changes
sections of `docs/architecture/version-control.md` (the click now opens the
popup), the `vcs.md` plan's D221 item, the roadmap, and the user guide that
lists the version control commands, if any. Record the content and anchoring
choices in `docs/decisions/bundled-packages.md` next to the other version
control decisions, and add them to the `docs/decisions.md` index.

**Review gate.** Docs match the behavior; plan removed.
