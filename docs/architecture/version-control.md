# Version control

Part of the [architecture](../architecture.md). [Decisions](../decisions.md)
records rationale ([backends](../decisions/bundled-packages.md#version-control-backends),
[Git baseline](../decisions/bundled-packages.md#git-baseline),
[status](../decisions/bundled-packages.md#version-control-status)). The contracts
are described in [packages](../extension-api/packages.md#contracts-knot-defines).

Version control is two bundled packages in the `knot-ext-vcs` crate. `@knot/vcs`
is backend-agnostic and owns the change gutter, navigation, file status, and the
current ref; a backend such as `@knot/git` only answers questions about
repositories. Several backends can be
active at once.

## Repositories

Each backend is asked for the repositories covering every window's roots: the
one containing a root and those nested below it. A document under none of them
is looked up from its own directory, as if it were a root, and the answer is
cached per directory. When several repositories contain a file, whichever
backend serves them, the deepest root wins. Repositories are found again when a
backend registers or is dropped, when a window opens or its roots change, and
when a backend reports a change.

A backend that stops is dropped when `@knot/vcs` next fails to bind its
contract, and its marks are cleared.

## Gutter

`@knot/vcs` follows every open document with a file through `documents.track`.
A backend supplies a baseline, which is text or none; a file with none (untracked,
ignored, or binary) has no gutter. The baseline is cached per document and
fetched again when the backend reports a change, when the document's file
changes, and when the document goes from dirty to clean.

Edits rediff the buffer against the cached baseline after a fixed 150 ms
debounce, using the extension API's `diff`. Each hunk becomes one contribution:
a `diff.added` bar for an insertion, a `diff.modified` bar for a replacement,
and a `diff.removed` wedge for a removal. The contributions are replaced at the
revision that was diffed, and a diff an edit raced is repeated. A click on a
mark runs `vcs.showChanges`.

## File status

`@knot/vcs` keeps the set of *known repositories*: every backend's repositories
covering every window's root, plus those found for followed documents outside
any root. The set is rebuilt when repositories are found again. For each, it
pulls the backend's `status`, a snapshot of the files that differ from the
baseline; a file not listed is unchanged and ignored files are never listed. A
repository that leaves the set loses its decorations. Where repositories nest, a
file belongs to the deepest one, so an outer repository's entries inside a
nested one are dropped.

One decoration source paints the snapshots, updated with only the URIs that
changed. A file shows its status as a color, a letter badge, and a tooltip
(`vcs.added` A, `vcs.modified` M, `vcs.deleted` D, `vcs.renamed` R,
`vcs.untracked` U, `vcs.conflicted` C); tabs show the color and tooltip only.
`@knot/vcs` defines the `vcs.*` color keys, three of them following `diff.*`.
Each folder from a change up to the repository root takes the color of its most
important change, in the order conflicted, deleted, modified, renamed, added,
untracked. `@knot/vcs` sets these folder colors itself, because colors that
propagate from one source onto the same folder have no defined order.

Status reflects files on disk only; unsaved edits show in the gutter. Pulls are
coalesced per repository: one in flight and at most one more queued. A backend
reports `changed` when baselines and status may differ, and `statusChanged`
when only the working tree did, which pulls status alone.

## Current ref

The `vcs.ref` status item shows a branch icon, the backend's `ref` label, and
its tooltip, with no click action. Each window shows the ref of the repository
of its focused editor's file, falling back to that of its first root; a window
with neither, or a backend with no ref, shows nothing. It is recomputed when the
focused editor, a window's roots, the windows, or a repository's baselines
change.

## Navigation

`vcs.nextChange` and `vcs.previousChange` (Alt+F5 and Alt+Shift+F5 in editors,
`]c` and `[c` in Vim mode) diff the focused editor against its cached baseline
and move the caret to the first line of the nearest hunk, wrapping at the ends.
A removal's line is the one that followed the removed lines. A document without
a baseline or changes leaves the caret where it is.

## Showing changes

`vcs.showChanges`, titled Compare with Last Commit and bound to Cmd+D
(Ctrl+D elsewhere) in editors, opens the focused file's diff through the `diff` contract of
[`@knot/diff`](#the-diff-package), which `@knot/vcs` requires: the baseline as
a snapshot labeled with the backend's ref (or "Baseline" without one), against
the document, which stays editable. It starts the caret at the clicked mark's
line, or the caret's line from the palette. A file without a baseline leaves
the command unavailable, and a file equal to its baseline raises a "No changes"
notice instead of a tab. Each document keeps at most one such diff: a second
request closes the open one and opens a new tab. The open diff's original side
is replaced when the baseline is fetched again, and the tab closes when the file
has no baseline any more or the document closes. The tab is a regular tab, not a
preview.

## The diff package

`@knot/diff` opens comparisons as composite tabs and serves the `diff` contract
(`open`, `setText`, `close`) to packages that show one. A tab is a row of two
editors, `original` and `modified`, with a connector strip between them. A side
is an open document, which the tab edits, or a read-only snapshot held in an
extension buffer; `setText` on a snapshot shows a new buffer, as a read-only
buffer takes no edits. After each edit of either buffer, debounced by 150 ms,
the package compares them natively and, from the hunks and the revisions they
describe, paints line bands and gutter wedges on each editor as view
decorations, sends the strip's links, and links the editors' scrolling. A
comparison that an edit raced is dropped, and the edit's own notification
compares again.

The `diff.layout` setting picks how a tab opens, and `diff.toggleLayout` (the
toolbar's last button) switches an open one. Inline keeps only the `modified`
editor and drops the strip and the scroll link; the package shows each hunk's
original lines above the modified lines they precede as block rows sourced from
the original's buffer, banded `diff.removed.background`. Switching keeps the
modified editor, its caret, and its focus, and fails with `DocumentUnsaved`
when the original is a document with unsaved changes.

`diff.nextHunk` and `diff.previousHunk` (Alt+F5 and Alt+Shift+F5 in a diff's
editors, `]c` and `[c` in Vim mode, and the tab's toolbar) move the caret of the
focused side (the modified one inline) to the first line of the nearest hunk, wrapping at the ends. Both
editors hold the `diff` key context while the tab is open, and the package
releases its snapshots when the tab closes.

## Git

`@knot/git` declares the `process` entitlement, implicitly granted to a bundled
package, and runs `git`; without it on the `PATH` it raises one notice and does
not register.

- **Repositories** come from `git rev-parse`. A root URI is derived from
  `--show-prefix` and the directory's own URI, never from Git's absolute path,
  so a symlinked directory still matches document and window URIs.
- **Baseline** is HEAD, read with `git cat-file --filters` so it is the
  checked-out text. A file in the index but not in HEAD has an empty baseline
  and shows as added. Reads set `GIT_OPTIONAL_LOCKS=0`.
- **Freshness** comes from `files.watch` with `includeHidden`, since `.git` is
  hidden. For each repository it has served, `@knot/git` watches the git
  directory (HEAD, the index, `packed-refs`) non-recursively, the common
  directory non-recursively when it differs, and the common directory's `refs`
  recursively, and reports `changed` to `vcs`. The object store is not watched.
  A watch ends when the git directory is gone, and the next lookup of a file in
  that repository watches again. The repository's working tree is watched
  recursively too, and a batch with any file the ignore policy keeps reports
  `statusChanged`; a batch of only ignored files, such as a build's output, is
  skipped.
- **Status** is `git status --porcelain=v2` against HEAD: a staged file edited
  again is `modified`, and a staged rename is `renamed` at the new path. Git's
  two columns collapse into one HEAD-relative status, so a file added to the
  index and then removed from disk is not listed. Untracked files are listed one
  by one until 2,000 are seen; past that the listing is cut short and each
  untracked folder is one entry.
- **Ref** is the branch name, or the short commit ID when HEAD is detached; an
  unborn branch keeps its name.

## Tests

`extensions/@knot/vcs/test` runs `@knot/vcs` over an in-memory host, including
moves and saves, which the product harness cannot drive. `crates/knot-ext-vcs/tests`
runs the packages in the launched product: `gutter.rs` over a fixed-baseline
backend, and `git.rs`, `diff.rs`, `status.rs`, `decorations.rs`, and `ref.rs` over
temporary repositories. A missing `git` is untested.
