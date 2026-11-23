# Version control

Status: 🚧 in progress; milestone 1 is done, D213 is next. Replaces D126 from the [roadmap](../roadmap.md)
with D209–D216, D220, and D221, and sequences D087, D199, and D197 into it.

## Goal

Version control in Knot, with Git as the first backend: a change gutter and
navigation between changes, file status, diffs, a changes view with commit,
and Git's own operations. The foundations come first and are built to last; Git is their first
consumer, not a reason to cut them short. Work is ordered so that each
milestone reaches a usable Git feature, without building anything that a later
milestone has to redo.

## How this plan is executed

This plan is an index. Each item below is a brief, not a design, and this file
stays high level. Before implementing an item, analyze it in its own session
with the `brainstorm-plan` skill and write its design in a separate plan,
`docs/plans/d###-<slug>.md`. Link that plan from the item here, and never add
design detail to this file. Mark progress here with ✅ and 🚧. Remove each
item's plan once it is complete, and remove this plan once every item's durable
behavior lives in the architecture references and decisions.

## Architecture

These choices were agreed before planning any item; item plans refine them but
do not reopen them without discussion. The item that implements a choice
records it, with its rejected alternatives, in `docs/decisions/`, so this
section can go when the plan does.

- **Diff stands on its own.** Comparing two texts is an editor feature like
  `vim -d`, not part of version control. Version control and Show Difference
  (D174) are consumers of it.
- **Native code paints; packages supply the data.** The change gutter is
  ordinary gutter contributions from a package, through a gutter API rich
  enough for multi-line bars, deletion markers, and click actions, which
  language-server diagnostics reuse.
- **The shared/specific split follows IntelliJ.** Shared through the contract:
  baselines for the gutter and diffs, file status, the current ref's label,
  the changes view with provider-defined groups, and committing with
  provider-supplied options. Backend-specific, outside the contract:
  staging, branches, log, push, and similar operations.
- **Native code stays VCS-agnostic.** File status reaches tabs and the explorer
  as generic resource decorations (color keys and badges per URI). The
  semantic statuses live in `@knot/vcs`'s API to backends, which maps them to
  decorations.
- **Git through its command line.** `@knot/git` runs `git`, so config, hooks,
  signing, credential helpers, filters, and sparse or split indexes behave as
  in the terminal. In-process reads (libgit2, gix) are revisited only if
  spawning proves slow.

## Milestones

Items in a milestone are listed in dependency order.

### 1. Change gutter

✅ Done: D087 entitlements, D199 typed contracts, D209 `diff`, D210 richer
gutter contributions, and D211 `@knot/vcs` and `@knot/git` with the change
gutter and change navigation. See [version control](../architecture/version-control.md),
[packages](../extension-api/packages.md), and
[application state](../extension-api/application.md#compare-texts).

### 2. File status

- ✅ **D212 — Resource decorations.** Color keys and badges per URI, painted on
  tabs and in trees. See
  [resource decorations](../architecture/semantic-ui.md#resource-decorations).
- 🚧 **D213 — VCS file status and current ref.** Status decorations from
  `@knot/vcs`, and the current ref's label in the status bar. Depends on D211,
  D212. 🚧 Plan: [d213-vcs-file-status](d213-vcs-file-status.md).

### 3. Diffs

- **D197 — Diff view.** Opening a file's changes from the
  gutter and from `@knot/vcs` is part of it.
- **D220 — Anchored floating panels.** Split from D145, which keeps virtual
  text for later consumers.
- **D221 — Hunk popup.** The hunk's baseline against the current lines, with
  revert, hunk navigation, opening the diff, and backend actions. Depends on
  D211, D220.

### 4. Changes and commit

- **D214 — VCS changes view.** Provider-defined groups of changed files with
  their actions. Depends on D213, D197.
- **D215 — VCS commit.** The commit UI and committing through the provider,
  with provider-supplied options. Depends on D214.

### 5. Git operations

- **D216 — Git operations.** Staging, branches, push and pull, stash, and
  other Git-specific workflows outside the shared contract. Depends on D215.

### After this plan

Separate roadmap items, not part of this plan: blame (D217), log and graph
views with file history (D218), and merge conflict resolution (D219).
