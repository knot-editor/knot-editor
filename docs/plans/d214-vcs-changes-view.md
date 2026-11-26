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

### 3. The Changes view ✅

Done: `vcs.changes` lists each repository's non-empty groups in tree or list
layout (`vcs.changes.layout`, toggled from the header), refreshes on each
snapshot, and opens a file's diff through `vcs.showFileChanges` (double-click,
Enter, or Show Diff), at most one per file and group, followed on refresh. Rows
render the backend's actions, Show Diff, Open File, and the native file
commands; the Git action commands are registered in checkpoint 4. The review
kept the hover buttons. Documentation is in checkpoint 5.

### 4. Actions and rollback ✅

Done: `git.stage`, `git.unstage`, `git.discard`, and `git.rollback` act on the
exact files a group lists below the row's path, so a folder or group header
never touches another group's files. Discard (restore, or delete for untracked
files) and Rollback ask for confirmation. Git serves `rollback`, and
`vcs.rollbackFile` confirms, calls it, and reloads the document. Product tests
that read `git status` must set `GIT_OPTIONAL_LOCKS=0`, or they race the
extension's index writes. Documentation is in checkpoint 5.

### 5. Documentation

Deliverables: the changes view, the contract, and Git's groups and actions in
[version control](../architecture/version-control.md); the native file
commands in [commands](../architecture/commands.md); decisions in
[bundled packages](../decisions/bundled-packages.md) (groups in the status
answer, backend actions as command references, file and folder pathspecs,
native file commands rather than shared explorer ones) and its index line; D216
in the roadmap and the version control plan without file staging; this plan
removed.
