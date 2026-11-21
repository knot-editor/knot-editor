# Version control

Status: 🚧 approved, not started. Replaces D126 from the [roadmap](../roadmap.md)
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
- **The diff algorithm is a native primitive; policy is JavaScript.** A
  stateless native function computes hunks between two texts or a buffer and a
  text, as `buffer.find` does for search. Packages decide when to diff and what
  to show.
- **Native code paints; packages supply the data.** The change gutter is
  ordinary gutter contributions from a package, through a gutter API rich
  enough for multi-line bars, deletion markers, and click actions, which
  language-server diagnostics reuse.
- **Version control lives in JavaScript, in two layers.** `@knot/vcs` defines
  the provider contract and owns the shared surfaces; a backend package such as
  `@knot/git` implements a provider. Native code gains only generic
  primitives: process spawning, the diff function, gutter contributions, and
  resource decorations.
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
- **Typed, validated contracts between packages.** A package defines its
  exported contract in code with a small schema builder from Knot's API, so
  TypeScript types are inferred from the same definition and the schema ships
  in the package's built JavaScript with no extra build step. The host
  validates calls and results at the boundary against it. TypeScript, not an
  IDL, is the schema language.

## Milestones

Items in a milestone are listed in dependency order.

### 1. Change gutter

- **D087 — Extension entitlements.** Done: packages declare entitlements,
  the user decides before an installed package loads, and extensions spawn
  processes through the gated API.
- **D199 — Typed contracts between packages.** Code-defined schemas, inferred
  types, host-enforced validation.
- **D209 — Diff function for extensions.**
- **D210 — Richer gutter contributions.**
- **D211 — VCS foundation and Git change gutter.** `@knot/vcs` and `@knot/git`:
  repositories per root, baselines, the change gutter, and next and previous
  change. Depends on D087, D199, D209, D210.

### 2. File status

- **D212 — Resource decorations.** Color keys and badges per URI, painted on
  tabs and in the explorer.
- **D213 — VCS file status and current ref.** Status decorations from
  `@knot/vcs`, and the current ref's label in the status bar. Depends on D211,
  D212.

### 3. Diffs

- **D197 — Diff view.** Depends on D209. Opening a file's changes from the
  gutter and from `@knot/vcs` is part of it once D211 exists.
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
