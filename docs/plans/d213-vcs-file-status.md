# D213 — VCS file status and current ref

Status: 🚧 in progress; checkpoint 1 done. Source: [roadmap D213](../roadmap.md), part
of the [version control plan](vcs.md), whose architecture section this plan
refines without reopening. Builds on D211 ([version control](../architecture/version-control.md))
and D212 ([resource decorations](../architecture/semantic-ui.md#resource-decorations)).

## Outcome

The explorer, every other tree with resource rows, and document tabs show
which files changed against the version control baseline. Trees show a colored
label and a letter badge, tabs show only the color, and hovering either names
the status. Folders that contain changes take the color of their most
important change. The status bar shows the current ref (a branch, or a short
commit ID when HEAD is detached) of the repository of the focused editor's
file, or of the window's first root. Both follow changes made in Knot and in
a terminal.

## Design

### Contract

`backendContract(id)` gains two methods and `vcs` gains one notification:

```ts
// backend.<id>, served by a backend
status: s.fn(s.string() /* repository */, s.array(s.object({
  file: s.string(),   // URI
  status: s.enum(["added", "modified", "deleted", "renamed", "untracked", "conflicted"]),
}))),
ref: s.fn(s.string() /* repository */, s.nullable(s.object({
  label: s.string(), tooltip: s.optional(s.string()),
}))),

// vcs, served by @knot/vcs
statusChanged: s.fn(s.object({ backend: s.string(), repository: s.string() }), s.void()),
```

- **`status` is a snapshot of the changed files.** A file it does not list is
  unchanged. Ignored files are never listed, since the native ignore policy
  already dims them.
- **Statuses are measured against the baseline**, which is HEAD for Git, so they
  always agree with the gutter. A staged file that was edited again is
  `modified`. The index/worktree split belongs to the provider groups of D214
  and to staging in D216. `renamed` decorates the new path. Git reports it only
  for staged renames, and an unstaged rename shows as `deleted` plus
  `untracked`.
- **Status reflects files on disk only**, as in VS Code. Unsaved edits are
  already visible in the gutter.
- **`ref`** is the label of what is checked out, or null when the backend has
  none to show.

### Freshness

The gutter never needed a working-tree watch: its baseline is HEAD, and an
edit on disk reaches it through the document reload. Status does need one, so
`@knot/git` extends its existing watch for each repository it has served
instead of adding a second mechanism:

- **Metadata watch (unchanged):** HEAD, the index, `packed-refs`, and refs.
  These report `changed`. `@knot/vcs` then re-fetches the baselines *and* pulls
  status and the ref again, because a commit, checkout, or `git add` changes
  all three.
- **Working-tree watch (new):** one recursive `files.watch` on the repository
  root, started and stopped with the metadata watches. It doesn't see `.git`,
  since it doesn't opt into hidden entries. A batch whose URIs are all ignored
  according to `files.isIgnored` is skipped, checked until the first one that
  isn't ignored, so a build's output doesn't trigger refreshes. Any other batch
  reports `statusChanged`, which pulls only status and leaves the baselines
  alone.
- **Pulls are coalesced per repository** in `@knot/vcs`: one in flight and at
  most one more queued, the same pattern the gutter uses for each document. The
  ~100 ms batching of `files.watch` is the only debounce.

### Which repositories are decorated

Today `@knot/vcs` looks up repositories lazily, one document at a time. File
status in the explorer needs them up front: `@knot/vcs` keeps the set of
*known repositories*, made of every backend's repositories covering every
window's root, plus the repositories found for followed documents outside any
root. The set is rebuilt at the same moments discovery is invalidated today
(registration, drop, window opened, roots changed, `changed`). A repository
that leaves the set has its decorations removed. When repositories nest, the
deepest one wins for each file, as in the gutter. An outer repository's
entries that fall inside a nested repository, which Git reports as an
untracked folder, are dropped.

### Presentation

One `DecorationSource` owned by `@knot/vcs`, updated with only the URIs that
changed between snapshots.

| Status | Color key | Badge | Tooltip |
|---|---|---|---|
| added | `vcs.added` | A | Added |
| modified | `vcs.modified` | M | Modified |
| deleted | `vcs.deleted` | D | Deleted |
| renamed | `vcs.renamed` | R | Renamed |
| untracked | `vcs.untracked` | U | Untracked |
| conflicted | `vcs.conflicted` | C | Conflicted |

- `@knot/vcs` defines the keys with `themes.defineColor`. Their defaults follow
  `diff.added`, `diff.modified`, and `diff.removed` where that fits, and pick
  their own colors otherwise. Checkpoint 2 reviews the colors in light and dark
  themes.
- **Folder colors are set by `@knot/vcs`, not by `propagate`.** Within a
  single source, propagated colors that meet on one folder are resolved in
  hash order (`resource_decorations.rs`, `rebuild`), so a folder holding both
  a modified file and an untracked one would take an arbitrary color. Instead,
  `@knot/vcs` sets a color-only decoration on each ancestor folder up to and
  including the repository root, by the priority conflicted > deleted >
  modified > renamed > added > untracked.

### Current ref

`@knot/vcs` registers one status item, `vcs.ref`, with a branch icon, the
backend's label, and the backend's tooltip. It has no click action, since the
branch picker belongs to D216. The value is set per window:

- The repository is the one of the focused editor's file, falling back to the
  repository of the window's first root. A window with neither hides the item
  by setting empty text.
- It updates on `activeTargetChanged`, `workspaceChanged`, `windowOpened`, and
  every `changed` for that repository. A `statusChanged` doesn't affect it.

## Exclusions

- Ignored-file decorations and styles other than color, such as strikethrough
  for deleted files.
- Status for unsaved edits.
- A click action on the ref item, the branch picker, and anything Git-specific
  (D216).
- Changes view groups and the index/worktree split (D214).
- Fixing the native tie-break between propagated colors from one source.
  Nothing needs it once `@knot/vcs` sets folder colors itself.
- Settings to turn status, badges, or folder colors off.

## Deferred decisions

- Whether Git lists untracked files one by one (`--untracked-files=all`) or
  collapses an untracked folder into one entry. One by one is the plan, so
  every untracked file gets its badge. Checkpoint 3 measures it on a
  repository with a large unignored untracked folder.
- How the edge cases of Git's two-column status collapse into one
  HEAD-relative status. One example is a file added to the index and then
  removed from the working tree. Checkpoint 1 settles them with tests.
- The exact default colors (checkpoint 2) and the ref tooltip's content
  (checkpoint 4).

## Checkpoints

Each checkpoint ends with something to see or measure. Status appears in
checkpoint 2 and becomes live for edits on disk in checkpoint 3, which adds
a notification and its handler without changing how status is mapped or
painted.

### 1. Git status in the contract ✅

Done: `status` is in the contract and in Git (`extensions/@knot/git/src/status.ts`),
covered by `crates/knot-ext-vcs/tests/status.rs`. Settled edge cases: added to
the index then removed from disk equals HEAD and is not listed; removed from
the index and recreated is `modified`; a staged rename then removed from disk
lists the old path as `deleted`; intent-to-add is `added`. A nested repository
appears as one untracked folder entry, which checkpoint 2 drops.

### 2. Status decorations

Deliverables: the known-repository set in `@knot/vcs`; a status pull for each
known repository when the set changes and on `changed`; the `vcs.*` color
keys; one decoration source with the status table above, folder colors by
priority, and removal when a repository leaves the set or its backend is
dropped. Status is fresh after a commit, checkout, or `git add`, but not yet
after an edit on disk.

Validation: in-memory tests with fake backends for the mapping, folder colors
and their priority, incremental updates (a file returning to unchanged loses
its decoration and its folders recompute), nested repositories (the deepest
one wins and the outer one's untracked-folder entry is dropped), roots
changing, and a backend dropping. Product tests over Git: the explorer shows
the color, badge, and tooltip for a modified and an untracked file and colors
their folders; a document tab takes the color; a commit made with `git` clears
them. A look at the running app in light and dark themes.

Review gate: the visual result in the explorer and on tabs, and the default
colors.

**Implementation context:**
- `extensions/@knot/vcs/src/extension.ts`: extend `changed` in `vcs.serve`;
  hook the known-repository rebuild into `resolveAll`, `drop`, and the
  `workbench.observe` handler. Keep the status logic in a new `src/status.ts`
  (pure mapping, folder priority, snapshot diffing) beside `hunks.ts`.
- `createDiscovery` in `src/repositories.ts` already caches `repositories(root)`
  per backend and root; reuse it for the covering set, and `contains` for
  nesting.
- Decorations: `decorations.createSource()` and `themes.defineColor` from
  `knot` (contract in `crates/knot-host/src/js/knot.d.ts`; color definition as
  in `extensions/@knot/language-python/src/extension.ts`).
- In-memory tests: a new `extensions/@knot/vcs/test/status.test.ts` modeled on
  `gutter.test.ts` (`backend`, `rest`), reading results with
  `host.decorationsOf` from `scripts/extensions/testing/host.ts`.
- Product tests: `TestTree::decoration` in `src/app/product/driver/window.rs`
  reads a row's color, badge, and tooltip; add a tab decoration accessor there
  if none exists.

### 3. Working-tree freshness

Deliverables: `statusChanged` in `vcs`; Git's working-tree watch with the
ignored-batch filter; status pulls coalesced per repository in `@knot/vcs`.

Validation: in-memory tests that a burst of `statusChanged` and `changed`
notifications makes at most two pulls per repository, and that
`statusChanged` leaves baselines alone. Product tests over Git: an edit on disk
and a new file update the explorer without `changed` being reported, and a
write under an ignored folder triggers no pull. Measure the status call and the
watch on a large repository, including one with a large unignored untracked
folder.

Review gate: freshness in the running app, the cost measurements, and the
untracked-listing decision.

**Implementation context:**
- Watching: extend `start` in `extensions/@knot/git/src/watch.ts` with a
  recursive watch on the repository root (no `includeHidden`), filtered
  through `files.isIgnored`, reporting `statusChanged` from the same listener
  shape and stopped by the same `stop`.
- Contract: `vcs` in `extensions/@knot/vcs/src/contract.ts` and its restatement
  in `extensions/@knot/git/src/vcs.d.ts`.
- Coalescing: mirror `Follow`'s `running`/`pending` in `extension.ts` with a
  per-repository record.
- Tests: `host.notifyWatchers` and `host.watchedUris` in
  `scripts/extensions/testing/host.ts` drive in-memory watches; product tests
  go in `crates/knot-ext-vcs/tests/status.rs`.

### 4. Current ref and documentation

Deliverables: `ref` in the contract and in Git; the `vcs.ref` status item with
per-window values; the architecture reference, the extension guide, a decision
entry, and the version control plan updated.

Validation: product tests of Git's `ref` on a branch, on an unborn branch, and
with a detached HEAD. In-memory tests with `host.statusItems` covering the
focused editor's repository winning over the first root, the fallback, a
window without a repository hiding the item, and updates on focus change, root
change, and `changed`. A product test over Git showing the branch, then the
short commit after `git checkout --detach`. Check the touched documents
against the code.

Review gate: D213 complete; D214 can build on it.

**Implementation context:**
- Git: `git symbolic-ref --short -q HEAD`, falling back to
  `git rev-parse --short HEAD` when detached, in a new
  `extensions/@knot/git/src/ref.ts`.
- `workbench.registerStatusItem` and `StatusItem.update(options, { window })`
  (`knot.d.ts`); `@knot/chrome`'s `extensions/@knot/chrome/src/extension.ts`
  shows a package-owned status item. Follow `activeTargetChanged` through the
  existing `workbench.observe` call, and map an editor target to its document
  through `follows`.
- Product test access to a status item: add a `TestWindow` accessor near the
  existing driver methods if `src/app/product/tests/status_bar.rs`'s
  `ApplicationStatusBar` reads are not reachable from `knot::test_support`.
- Docs: "File status" and "Current ref" sections, and the working-tree watch
  under "Git", in `docs/architecture/version-control.md`; the contract's new
  methods in `docs/extension-api/packages.md#contracts-knot-defines`; the
  `vcs.*` color keys wherever color keys are listed; a "Version control status"
  entry in `docs/decisions/bundled-packages.md` (status against the baseline
  rather than the index, disk-only, folder colors set by the package rather
  than propagated, statuses as a shared snapshot) with its line in
  `docs/decisions.md`; mark D213 ✅ in `docs/plans/vcs.md`.
