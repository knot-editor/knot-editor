# Version control

Part of the [architecture](../architecture.md). [Decisions](../decisions.md)
records rationale ([backends](../decisions/bundled-packages.md#version-control-backends),
[Git baseline](../decisions/bundled-packages.md#git-baseline),
[status](../decisions/bundled-packages.md#version-control-status)). The contracts
are described in [packages](../extension-api/packages.md#contracts-knot-defines).

Version control is two bundled packages in the `knot-ext-vcs` crate. `@knot/vcs`
is backend-agnostic and owns the change gutter, navigation, file status, the
current ref, and the changes view; a backend such as `@knot/git` only answers questions about
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
A backend supplies a baseline, which is text or none, and optionally the
staged text between it and the working file; a file with no baseline
(untracked, ignored, or binary) has no gutter. Both are cached per document and
fetched again when the backend reports a change, when the document's file
changes, and when the document goes from dirty to clean.

Edits rediff the buffer against the cached baseline after a fixed 150 ms
debounce, using the extension API's `diff`. Each hunk becomes one contribution:
a `diff.added` bar for an insertion, a `diff.modified` bar for a replacement,
and a `diff.removed` wedge for a removal. The contributions are replaced at the
revision that was diffed, and a diff an edit raced is repeated. A click on a
mark runs `vcs.showHunk`.

A hunk is staged when the staged text, diffed against the buffer the same way,
has no hunk overlapping the hunk's new lines (or touching a removal's boundary),
that is the buffer equals the staged text there; a partly staged hunk counts
as unstaged. Staged hunks paint with the dimmer `diff.added.staged`,
`diff.modified.staged`, and `diff.removed.staged`, which [`@knot/diff`](#the-diff-package) defines.

## Hunk popup

`vcs.showHunk` (Show Hunk) opens an [editor zone](editor-and-languages.md#editor-zones)
for the hunk of the clicked mark, or of the caret's line from the palette; it
has no default key and is unavailable when the caret is on no hunk. Run on the
hunk that is already open, it closes the zone. The zone has the ID `vcs.hunk`
and shows the hunk's baseline lines in a read-only buffer without a gutter, in
the document's language, banded `diff.removed.background`; the current lines
stay in the editor. It sits above the hunk's first current line (a removal above
the line that followed it, or below the last line for a removal at the end of
the file), with a gutter strip in the hunk's mark color. An insertion has no
baseline lines, so its zone is only the toolbar and has no strip. Changes inside
a line are not highlighted. A deletion's wedge is clickable within half a row
of the boundary it points at.

The toolbar has Previous Hunk, Next Hunk, the backend's buttons (below), Revert Hunk, Compare with Last Commit
(`vcs.showChanges` at the hunk's line), and Copy Hunk, which an insertion omits.
`vcs.revertHunk` replaces the hunk's current lines with its baseline lines in
one edit at the diffed revision, one undo step, restoring a missing final
newline exactly. `vcs.copyHunk` copies the baseline lines. Both work from the
palette on the hunk at the caret.

A backend adds buttons through `hunkActions(repository)`: each names one of its
commands, an icon, and the hunk states (`staged`, `unstaged`) it applies to, and
the popup shows those that match its hunk, after Next Hunk. A staged hunk's popup
leads with a "Staged" (green) or "Unstaged" (muted) badge, a toolbar label that runs nothing. The command runs in
the editor with `{ repository, file, line }`, the hunk's first current line (the
boundary line for a removal).

A document has one popup. Opening it on another hunk updates it in place, and
[navigation](#navigation) moves it while it is open. Escape closes it, in the
zone or from the text through `vcs.closeHunk` (bound while the zone's key
context is held), and `@knot/vcs` closes it on an
edit of the document, when the baseline is fetched again or goes away, and when
the document closes, rather than following a hunk that may have moved. When the
baseline is fetched again, as after staging, the popup stays on the same hunk if
the new diff still has it, and closes otherwise. A popup replaces its zone in
place when the new hunk has baseline lines as the old one did (or neither has),
and closes and opens it otherwise.

## File status

`@knot/vcs` keeps the set of *known repositories*: every backend's repositories
covering every window's root, plus those found for followed documents outside
any root. The set is rebuilt when repositories are found again. For each, it
pulls the backend's `status`, whose `files` are a snapshot of the files that
differ from the baseline (its `groups` feed the [changes view](#changes-view)); a file not listed is unchanged and ignored files are never listed. A
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

## Changes view

The `vcs.changes` layout tool window, titled Changes, sits at `leftTop` after the
Explorer. It holds the embedded `vcs.changes.tree` over the [commit panel](#commit-panel),
and its toolbar toggles the layout and opens the output. The tree covers the known repositories that cover the window's roots. One
repository shows its groups at the top level; several show each repository as an
expanded row named after its root. A window with none shows a "No repository"
row. Each new status snapshot invalidates the view, and row IDs (repository,
group, path) are stable, so selection and expansion survive.

The backend's `status` answer is `{ files, groups }`. `groups` are ordered
`{ id, label, files, actions }`, with `files` as file URIs; a file can be in
several groups, and order and membership are the backend's. The view lists only
non-empty groups, with the file count as status text. In tree layout, files sit
in folder rows shaped like the explorer's, expanded by default; in list layout, a
file's directory below the repository is its detail text. The header button
toggles `vcs.changes.layout` (`tree` or `list`). File rows carry `resource`, so
they drag into the editor and take the [status decorations](#file-status); the
view sets no status of its own. An untracked folder that Git lists as one entry is
a row with no children.

An action is `{ label, icon?, command, on, menuOnly?, showOnHover? }`: `command`
is one of the backend's own commands and `on` lists the rows it appears on
(`group`, `folder`, `file`). The view runs it with `{ repository, group, path }`,
where `path` is the file, the folder, or the repository root for a header. The
backend decides what it means and asks for any confirmation itself. A row's menu
lists the backend's actions, then Show Diff (files) and Open File, then the
[native file commands](commands.md#file-commands).

A click on a file runs `vcs.showFileChanges` as a preview diff, and double-click or Enter as
a regular one; the command asks the
backend's `compare({ repository, group, file })` for the sides of that file's diff
in that group, an `original` snapshot and a `modified` side that is either
`"document"` (the working file, editable) or a snapshot, and opens them through
the `diff` contract; `null` means the file has no diff there. The working file is
loaded without a tab of its own. Each file and group
keeps at most one diff, and a second request shows that tab, keeping it for
double-click or Enter. When the repository's status is pulled again, snapshot sides are replaced
and the tab closes once `compare` returns null; when another group of the snapshot
lists the file, such as Staged after staging, the same kind of tab (preview or kept)
opens on that group instead.

`vcs.rollbackFile` (Rollback File) works from the palette on the focused editor's
file. It confirms, saying that unsaved edits will be lost when the document is
dirty, calls the backend's `rollback({ repository, path })`, which restores the
path to its baseline on disk, and reloads the document from disk with no Reload or
Keep prompt after it.

## Commit panel

`@knot/vcs` lays the Changes tool window out per window as the tree, a message
editor, and a row of controls: the backend's options as checkboxes, then Commit.
The panel commits one repository, the one of the file last chosen in the window's
tree, or else the first covering the window; with several repositories the
editor's placeholder names it. Each repository has one draft, an extension buffer
persisted under a key of the repository and cleared after a commit. Commit is
disabled while the draft is empty and Amend is off.

A backend serves three methods: `commitOptions(repository)` (the toggles, all off
at first), `commitMessage({ repository, options })` (the message the options
suggest, or null), and `commit({ repository, message, options })`, which commits,
asks the user for what it needs itself, and answers `{ summary }`, or null when
the user cancelled. A failure rejects. When an option toggles and the suggestion
changes, the panel replaces the draft with it and puts the user's draft back when
the suggestion goes away. `vcs.commit` refuses an empty message, except while
`amend` is on, before calling the backend. Afterwards it raises a "Committed
<summary>" notice, or the failure as an error notice, each with a Show Output
action, and asks for the repository's status again.

`vcs.focusCommitMessage` (Cmd+Alt+C) shows the tool window and focuses the message editor, and Cmd+Enter in an
editor runs `vcs.commit.fromMessage`, which commits only when the editor is in
the Changes tool window.

A successful commit records its message in the repository's `history` list (the
host keeps the newest 50). `vcs.commitMessageHistory` (Ctrl+M in the panel's
message editor, through `vcs.commitMessageHistory.fromMessage`) opens a picker
over it, the subject as the label, and replaces the draft with the chosen message
as one undoable edit; with no history it raises a notice.

## Current ref

The `vcs.ref` status item shows a branch icon, the backend's `ref` label, and
its tooltip, with no click action. Each window shows the ref of the repository
of its focused editor's file, falling back to that of its first root; a window
with neither, or a backend with no ref, shows nothing. It is recomputed when the
focused editor, a window's roots, the windows, or a repository's baselines
change.

## Navigation

`vcs.nextHunk` and `vcs.previousHunk` (Alt+F5 and Alt+Shift+F5 in editors,
`]c` and `[c` in Vim mode) diff the focused editor against its cached baseline
and move the caret to the first line of the nearest hunk, wrapping at the ends.
A removal's line is the one that followed the removed lines. While the
[hunk popup](#hunk-popup) is open on the focused editor, it moves to the new
hunk. A document without a baseline or changes leaves the caret where it is.

## Showing changes

`vcs.showChanges`, titled Compare with Last Commit and bound to Cmd+D
(Ctrl+D elsewhere) in editors, opens the focused file's diff through the `diff` contract of
[`@knot/diff`](#the-diff-package), which `@knot/vcs` requires: the baseline as
a snapshot labeled with the backend's ref (or "Baseline" without one), against
the document, which stays editable. It starts the caret at the hunk popup's line, or the
caret's line from the palette. A file without a baseline leaves
the command unavailable, and a file equal to its baseline raises a "No changes"
notice instead of a tab. The diff opens as the pane's preview tab. Each document keeps at most one such diff: a second
request closes the open one and opens a new tab. The open diff's original side
is replaced when the baseline is fetched again, and the tab closes when the file
has no baseline any more or the document closes. The diff also gets the baseline's `staged` text, which
the diff package uses to draw staged hunks dimmer; a refetch replaces it too, with the
baseline when nothing is staged.

## The diff package

`@knot/diff` opens comparisons as composite tabs and serves the `diff` contract
(`open`, `setText`, `keep`, `activate`, `close`) to packages that show one; `open` takes `preview` for the pane's preview tab. A tab is a row of two
editors, `original` and `modified`, with a connector strip between them. A side
is an open document, which the tab edits, or a read-only snapshot held in an
extension buffer; `setText` on a snapshot shows a new buffer, as a read-only
buffer takes no edits. After each edit of either buffer, debounced by 150 ms,
the package compares them natively and, from the hunks and the revisions they
describe, paints line bands and gutter wedges on each editor as view
decorations, sends the strip's links, and links the editors' scrolling. A
comparison that an edit raced is dropped, and the edit's own notification
compares again.

`open` takes an optional `staged` text, and `setText` replaces it with the side
`"staged"`. A hunk is staged when the modified side matches that text where it
changed, by the [gutter's rule](#gutter); the bands, wedges, strip links, and
inline blocks of a staged hunk use the dimmer `diff.added.staged`,
`diff.modified.staged`, `diff.removed.staged`, and the `.background.staged` forms
of the three band colors, all defined by the package.

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
- **Groups** come from the same `porcelain=v2` output, one `git` call per
  refresh: Staged (index column set), Unstaged (worktree column set), Conflicts
  (`u` entries, listed only when there are any), and Untracked (`?` entries). A
  staged file edited again is in Staged and in Unstaged. `compare` reads sides with
  `git cat-file --filters`: HEAD and the index for Staged, the index and the
  working file for Unstaged, HEAD and the working file for Conflicts, and empty
  and the working file for Untracked. A staged rename's original is the old path
  in HEAD, a deleted file's modified side is empty, and a binary file or a folder
  has no comparison.
- **Actions** are commands `@knot/git` registers outside the palette, each one
  `git` call over literal pathspecs: Stage (`git add`) on Unstaged, Conflicts, and
  Untracked; Unstage (`git restore --staged`) on Staged; Discard (`git restore`,
  or deleting untracked files) on Unstaged and Untracked, after a confirmation;
  and Rollback (`git restore --source=HEAD --staged --worktree`) on Staged and
  Unstaged, menu only and after a confirmation. Each acts only on the files its
  group lists at or below the row's path, so a folder or header never touches
  another group's files. A file that is added but not in HEAD is unstaged and left
  on disk as untracked. The index and working tree watches report the change, so
  an action needs no refresh of its own.
- **Stage and Unstage Hunk** (`git.stageHunk`, `git.unstageHunk`) are the
  backend's `hunkActions`, also on the palette for the hunk at the caret. They
  read HEAD, the index entry, and the buffer's text, find the hunk of HEAD
  against the buffer at the line, and edit the index entry: staging replaces the
  index's lines that differ from the buffer there with the buffer's, unstaging
  replaces the index's lines that differ from HEAD there with HEAD's. The text
  is written with `git hash-object -w --stdin --path` (clean filters apply) and
  `git update-index --cacheinfo`, keeping the entry's mode, so unsaved edits stage as
  they show and the working file is untouched.
- **Commit** runs `git commit -F -` with the message on stdin, adding `--amend`
  and `--signoff` for the options of the same name; Amend suggests HEAD's
  message. With nothing staged and tracked files changed, it confirms "Stage all
  changes and commit?" in the caller's window and adds `-a`; untracked files are
  never included. It is logged through `gitLogged`, so a failing hook shows in the
  output.
- **Output** is the Version Control tool window (`vcs.output`, bottom right, text
  frame that follows its end), which `@knot/vcs` owns. A backend reports each
  command the user started through `vcs.log` (command line, stdout and stderr,
  failure), never its background reads, and the log keeps the last 256 KiB. It
  opens from its stripe button, the Changes toolbar, or `vcs.showOutput`. Git logs
  its actions through `gitLogged`.
- **Ref** is the branch name, or the short commit ID when HEAD is detached; an
  unborn branch keeps its name.

## Tests

`extensions/@knot/vcs/test` runs `@knot/vcs` over an in-memory host, including
moves and saves, which the product harness cannot drive. `crates/knot-ext-vcs/tests`
runs the packages in the launched product: `gutter.rs` over a fixed-baseline
backend, and `git.rs`, `diff.rs`, `status.rs`, `decorations.rs`, `ref.rs`, and
`changes.rs` over temporary repositories. A test that reads `git status` must set
`GIT_OPTIONAL_LOCKS=0`, or it takes the index lock and races the extension's
writes. A missing `git` is untested.
