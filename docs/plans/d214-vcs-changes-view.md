# D214 — VCS changes view

Status: 🚧 planned. Roadmap item [D214](../roadmap.md), part of the
[version control plan](vcs.md), milestone 4.

## Outcome

A Changes tool window lists the changed files of the window's repositories in
the groups their backend defines. For Git these are Staged, Unstaged, Conflicts
(only when there are any), and Untracked. Files appear as a tree of folders by
default, or as a flat list through a header button. Double-click or Enter on a
file opens its changes in the diff view, comparing the sides that its group
stands for. Rows and group headers carry the backend's actions: for Git,
Stage, Unstage, Discard, and Rollback, on a file, a folder, or a whole group.
Rollback of the focused editor's file also works from the palette.

The commands for a file's path and location (Copy Path, Copy Relative Path,
Copy File Name, Reveal in Finder) become native, so the explorer, the changes
view, and editors share them.

## Design

### Native file commands

`file.copy-path`, `file.copy-relative-path`, `file.copy-name`, and
`file.reveal` are native Knot commands. Each takes an optional `{ uri }`
argument. Without one, it resolves its target from focus: the `resource` of the
selected row of a focused tree, else the focused editor's file. A target with no
file leaves the command unavailable. The relative path is below the window root
that contains the file, falling back to the full path. Reveal has the
platform's label (Reveal in Finder on macOS).

The explorer drops its own four commands and lists the native ones in its row
menus. Its rows already carry `resource`. The changes view lists them the same
way, so neither package depends on the other. The tab context menu lists them
too, for the tab's file.

### Contract

The backend's `status` answer grows from a list into an object:

- `files`: the HEAD-relative snapshot it returns today, which decorations keep
  using unchanged.
- `groups`: ordered `{ id, label, files, actions }`, with `files` as file URIs. A file
  can be in more than one group, such as a staged file edited again, which is in
  Staged and in Unstaged. Group order and membership are the backend's.

A single answer keeps one `git` call per refresh, one coalesced pull, and the
existing `changed` and `statusChanged` triggers, and the groups never disagree
with the decorations.

An action is `{ label, icon?, command, on, menuOnly?, showOnHover? }`. `command`
names one of the backend's own commands, and `on` lists the rows it appears on:
`group`, `folder`, `file`. The view runs it with
`{ repository, group, path }` as arguments, where `path` is the file, the
folder, or the repository root for a group header. The backend decides what an
action means and asks for confirmation itself. No callbacks cross the contract.

Two methods are added:

- `compare({ repository, group, file })` returns the sides of the file's diff in
  that group: `original` as `{ label, text }`, and `modified` as `"document"`
  (the working file, opened editable) or `{ label, text }`. It returns null
  when the file has no diff there.
- `rollback({ repository, path })` restores the path to its baseline on disk.
  `vcs.rollbackFile` calls it, since every backend has a baseline to restore.
  It asks for confirmation first, and on a document with unsaved edits the
  prompt says they will be lost. Once confirmed, the file is restored and the
  document reloads from disk, with no Reload or Keep prompt after it.

### The view

`@knot/vcs` registers the `vcs.changes` tree view, titled Changes, at
`leftTop` after the Explorer. It covers the known repositories that cover the
window's roots. With one repository, the groups are the top level. With
several, each repository is an expanded row named after its root, with its
groups under it. A window with none shows one "No repository" row.

- **Rows.** Group headers show the file count as status text. In tree mode,
  folders are rows shaped like the explorer's (folder icon, expanded by default,
  `resource`), so D178's compaction applies to both trees once the native tree
  does it. In list mode, a file's directory below the repository is its detail
  text. A file row shows the file icon and has `resource`, so it drags into the
  editor and the existing status decorations color it and badge it with its
  HEAD-relative status, as in the explorer; the view sets no status of its own.
  An untracked
  folder that Git lists as one entry (past the untracked limit) is a row with
  no children.
- **Layout.** A header button toggles `vcs.changes.layout` (`tree`, the
  default, or `list`).
- **Refresh.** Each new snapshot for a repository invalidates the view. Row IDs
  are stable (repository, group, path), so selection and expansion survive.
- **Opening.** Double-click or Enter on a file runs `vcs.showFileChanges`,
  which asks `compare` and opens the result through the `diff` contract, at
  most one per file and group. As with `vcs.showChanges`, a second request
  closes the open diff and opens a new tab. When the repository's status is
  pulled again, snapshot sides are refreshed with `setText`, and the tab closes
  when `compare` returns null. A single click only selects, because diff tabs
  are not previews.
- **Row menus.** The backend's actions for the row, then Show Diff (file rows)
  and Open File, then the native file commands.
- **Buttons, not checkboxes.** Stage and Unstage are hover buttons, as in VS
  Code. See how this looks in use; Zed-style checkboxes would need a tree row
  checkbox, and may follow.

### Git

Groups, in order:

| Group | From `porcelain=v2` | Diff sides |
|---|---|---|
| Staged | index column set | HEAD ↔ index |
| Unstaged | worktree column set | index ↔ working file |
| Conflicts | `u` entries | HEAD ↔ working file |
| Untracked | `?` entries | empty ↔ working file |

Index texts are read with `git cat-file --filters` on `:path`, like the
baseline. A deleted file's modified side is an empty snapshot, and a staged
rename's original is the old path in HEAD.

Actions, each one `git` call over the pathspec:

- **Stage** (`git add`): Unstaged, Conflicts, and Untracked, on files, folders,
  and headers (Stage All on a header).
- **Unstage** (`git restore --staged`): Staged, likewise (Unstage All).
- **Discard** (`git restore` for tracked files, delete for untracked ones):
  Unstaged and Untracked, with a confirmation prompt.
- **Rollback** (`git restore --source=HEAD --staged --worktree`): menu only, on
  Staged and Unstaged rows and headers, with a confirmation prompt. A file that
  is added but not in HEAD is unstaged and left on disk as untracked.

The index and HEAD watches already report `changed`, and working tree writes
report `statusChanged`, so an action needs no refresh of its own.

### Exclusions

- IntelliJ changelists.
- Commit and its message editor: D215, which builds them into this tool window.
- Staging hunks, branches, push and pull, stash: D216.
- Compact folders: D178, in the native tree.
- Selecting several rows: an action's pathspec covers a folder or a group
  instead.
- Single-click opening in a preview tab.
- Conflict resolution: D219.

### Deferred decisions

- Whether row checkboxes replace the Stage and Unstage buttons, after use.

## Checkpoints

### 1. Native file commands ✅

Done: `file.copy-path`, `file.copy-relative-path`, `file.copy-name`, and
`file.reveal` are native commands (resolution from `{ uri }`, a focused tree's
selected `resource`, or the focused editor). The explorer's row menus list
them, and the tab entries live in `@knot/chrome`. Documentation of them is in
checkpoint 5.

### 2. Groups and comparison in the contract ✅

Done: `status` returns `{ files, groups }`, and the backend contract has
`compare` and `rollback` (`extensions/@knot/vcs/src/contract.ts`). Git serves
its four groups with their actions declared and `compare` for each group; its
`rollback` rejects, and the commands `git.stage`, `git.unstage`, `git.discard`,
and `git.rollback` are not registered until checkpoint 4. A binary file or a
folder has no comparison. Documentation is in checkpoint 5.

### 3. The Changes view

Deliverables: the `vcs.changes` tree with groups, the tree and list layouts and
their toggle, repository rows when there are several, the "No repository" row,
refresh on each snapshot, `vcs.showFileChanges` with double-click, Enter, and
Show Diff, Open File, and the native file commands in the row menus. Backend
actions render on their rows but may do nothing yet.

Validation: `@knot/vcs` unit tests for building rows from groups in both
layouts, row IDs, and the arguments sent to actions; product tests over a
temporary repository for the view's rows, a refresh after a file changes on
disk, opening a staged and an unstaged diff, and the one-diff rule; `just
check`.

Review gate: in the app, through `try.*` commands, the layout, the row
presentation, and whether hover buttons read well enough or checkboxes are
needed.

**Implementation context.** Register the view like
`extensions/@knot/explorer/src/tree.ts` (`workbench.registerView`, a `tree`
provider, `placement`, frame `actions`), and reuse its row shape from
`entryItem` in `extensions/@knot/explorer/src/rows.ts`. The repositories per
window come from `extensions/@knot/vcs/src/repositories.ts`. Opening a diff
mirrors `showChanges` in `extensions/@knot/vcs/src/extension.ts` and the
contract in `extensions/@knot/vcs/src/diff.d.ts`. The setting is declared like
the explorer's in `extensions/@knot/explorer/src/settings.ts`. Tree item fields
(`statusText`, `detail`, `resource`, and actions' `menuOnly` and `showOnHover`) are described in
[extension trees](../architecture/semantic-ui.md#extension-trees).

### 4. Actions and rollback

Deliverables: Git's Stage, Unstage, Discard, and Rollback commands over file,
folder, and group pathspecs, with confirmation for Discard and Rollback;
`rollback` served by Git; `vcs.rollbackFile` for the focused editor's file,
confirmed, and reloading a document with unsaved edits.

Validation: product tests over temporary repositories for each action on a
file, a folder, and a group header, checked with `git status`; the view
refreshing after an action; Discard deleting an untracked file; Rollback of an
added file; `vcs.rollbackFile` on a clean and a dirty document; `just check`.

Review gate: in the app, through `try.*` commands, staging and discarding from
the view, and Rollback from the palette.

**Implementation context.** Git commands register in
`extensions/@knot/git/src/main.ts` and run through `git` in
`extensions/@knot/git/src/git.ts`. Confirmation mirrors `remove` in
`extensions/@knot/explorer/src/actions.ts` (`confirmLabel`). `vcs.rollbackFile`
registers beside `vcs.showChanges` in `extensions/@knot/vcs/src/extension.ts`
and finds the file's repository and backend as the gutter does.

### 5. Documentation

Deliverables: the changes view, the contract, and Git's groups and actions in
[version control](../architecture/version-control.md); the native file
commands in [commands](../architecture/commands.md); decisions in
[bundled packages](../decisions/bundled-packages.md) (groups in the status
answer, backend actions as command references, file and folder pathspecs,
native file commands rather than shared explorer ones) and its index line; D216
in the roadmap and the version control plan without file staging; this plan
removed.
