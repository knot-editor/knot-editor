# D211 — VCS foundation and Git change gutter

Status: 🚧 approved, not started. Source: [roadmap D211](../roadmap.md), part
of the [version control plan](vcs.md), whose architecture section this plan
refines without reopening.

## Outcome

An open file in a Git repository shows a change gutter against HEAD: bars for
added and modified lines and a wedge where lines were removed, current while
typing and after commits, checkouts, and resets made in a terminal. Alt+F5 and
Shift+Alt+F5 move the caret to the next and previous change. Several version
control backends can be active at once; Git is the first.

On the way, extensions gain `documents.track`, a simple way to follow every
open document, validated by migrating `@knot/projects` to it.

## Design

### Document tracking

```ts
await documents.track((document, tracked) => {
  tracked.onDidChange((now, before) => …); // kind, resourceUri, dirty, or language changed
  tracked.onDidClose(() => …);             // closed, or the lifecycle ended
}); // resolves with a Disposable
```

- The listener runs for every document already open, then for each newly
  opened one. Revision-only changes are not reported; text goes through the
  buffer as today.
- Built in the host's JavaScript over the existing workbench observation, so
  native code is unchanged. Its callbacks are delivered in the order of that
  observation; checkpoint 1 establishes how they order against a separate
  `workbench.observe` subscription of the same package, which `@knot/projects`
  depends on ("commands and application events take turns").
- It does not wait for `launched`; callers that care check it themselves.

### Packages and contracts

- A new group crate `knot-ext-vcs` compiles `@knot/vcs` and `@knot/git`, joined
  in `knot-app` like the other domains.
- `@knot/vcs`'s contract module defines both directions, since no callbacks
  cross a contract and `client()` binds only the latest registration:
  - **`vcs`**, served by `@knot/vcs` and called by backends: register a
    backend by ID, and report that a repository's baselines changed.
  - **`backendContract(id)`**, a memoized per-backend factory like Knot's
    `pasteContract(languageId)`, served by each backend and called by
    `@knot/vcs`: the repositories covering a root, the repository of a file,
    and a file's baseline. One contract per backend lets several run at once.
- Backends `require` `@knot/vcs`; `@knot/vcs` requires no backend.

### Ownership

- **`@knot/vcs`**: tracking documents, mapping files to repositories, the
  baseline cache, a debounced `diff` on edits, the gutter contributions, and
  the navigation commands. It is backend-agnostic.
- **`@knot/git`** (declares the `process` entitlement): finding repositories
  with `git rev-parse`, HEAD baselines with `git cat-file --filters`, and
  watching repository metadata to notify `vcs`.

### Repository discovery

Each backend is asked for the repositories covering each window root: one
containing the root and those nested below it. A document under none of
those is looked up from its own directory, as if it were a root, and the
answer is cached per directory. When several repositories, of one backend or
several, contain a file, the deepest repository root wins.

### Baseline and gutter

- Git's baseline is HEAD, as in IntelliJ; staging stays Git-specific (D216).
  A baseline is text, or none: untracked, ignored, or binary files get no
  gutter, and a file added to the index but absent from HEAD shows as
  entirely added.
- Hunks become one contribution set per buffer: a `bar` in `diff.added` for
  insertions, a `bar` in `diff.modified` for replacements, and a `deletion`
  in `diff.removed` for removals. Marks run no click command until D221.
- Edits rediff against the cached baseline; the baseline is refetched when
  the backend reports a change, the document's file changes, or it is saved.

### Navigation

`vcs.nextChange` and `vcs.previousChange` move the caret to the start of the
next or previous hunk in the focused editor, wrapping at the ends, bound to
Alt+F5 and Shift+Alt+F5 as in VS Code.

## Exclusions

- File status, decorations, and the current ref (D212, D213).
- The hunk popup, click actions, and revert (D221); the diff view (D197).
- Staging, an index baseline, or a baseline setting (D216).
- A root-wide status scan; discovery here only finds repositories.
- Native changes: every piece uses existing APIs.

## Deferred decisions

- ✅ `documents.track` replaced `@knot/projects`' document handling entirely.
  Checkpoint 1's ordering result: a tracker is a separate subscription, so its
  callbacks are in order among themselves but interleave arbitrarily with a
  separate `workbench.observe`'s for the same event. Projects tolerates this
  because each handler ends in a live `track(state)` query.
- Whether `files.watch` reports changes inside `.git`, which is hidden; if
  not, checkpoint 5 decides between a generic watch option and polling on
  window focus.
- Whether the gutter has an enable setting. The debounce is a fixed 150 ms; checkpoint 3 adds no setting.

## Checkpoints

### 1. `documents.track` and `@knot/projects` ✅

Complete. `documents.track` is documented in
[application](../extension-api/application.md#inspect-and-change-application-state) and
[application observation](../architecture/extension-host.md#application-observation);
`@knot/projects` follows documents with it and keeps `workbench.observe` for
windows and `launched`. A tracker is ordered within itself but not against a
separate `workbench.observe`.

### 2. Skeleton and contracts ✅

Complete. `@knot/vcs` serves `vcs` and defines `backendContract(id)`; `@knot/git`
is a skeleton that registers and finds no repositories. The contract shape is in
[version control backends](../decisions/bundled-packages.md#version-control-backends)
and [sharing between packages](../extension-api/packages.md#share-a-feature-with-other-packages).
A backend that stops is dropped when its contract no longer binds, since the
host has no unload signal (D227); checkpoint 3 handles `ContractNotFound` from
a backend by dropping it and clearing its marks.

### 3. Gutter against the test backend 🚧 awaiting review

Deliverables: in `@knot/vcs`, document tracking through `documents.track`,
discovery (roots, then the file's directory, deepest repository wins), the
baseline cache, the debounced diff on edits, and the gutter contributions.

Validation: behavior tests with the test backend for added, modified, and
removed hunks; typing updates marks; a document outside every root;
competing nested repositories from two backends; a document with no baseline
shows nothing; `RevisionConflict` retries; closing releases state.

Review gate: the complete backend-agnostic gutter.

Implemented: `@knot/vcs` follows documents with `documents.track`, finds the
repository from the roots of every window and then the file's directory, caches
baselines per document, and paints with `diff`. Behavior tests are in
`extensions/@knot/vcs/test/gutter.test.ts` (fake host) and
`crates/knot-ext-vcs/tests/gutter.rs` (real host, through
`TestWindow::gutter_bars`). Roots are the union over windows, so a document
shown in two windows is resolved once.

**Implementation context:**
- `diff` and `buffer.contributions.replace` with `ifRevision`, as in
  `docs/extension-api/application.md#compare-texts` and the gutter example in
  `docs/extension-api/languages-and-themes.md`.
- Window roots through `workspaces.get` and `workspaceChanged` events.
- Debounce and revision-retry pattern from
  `extensions/@knot/word-highlight/src/extension.ts`.

### 4. `@knot/git` baselines

Deliverables: `@knot/git` declares `process`, registers with `vcs`, finds
repositories, and serves HEAD baselines through `git cat-file --filters`;
untracked, ignored, binary, and added-only files; a missing `git` makes the
backend inactive with one notice.

Validation: behavior tests over temporary Git repositories: a committed
file edited in Knot shows its hunks, CRLF and filter conversion compare
cleanly, a nested repository and a submodule resolve to the deepest root, and
the edge cases above.

Review gate: the first usable Git gutter, refreshed when a document opens or
its file changes. Record Git's HEAD baseline in `docs/decisions/`.

**Implementation context:**
- `processes.run` with `GIT_OPTIONAL_LOCKS=0`, as in
  `docs/extension-api/application.md#run-programs`; entitlement declaration
  as in `docs/architecture/extension-host.md#extension-entitlements`.
- Process tests in `src/app/product/tests/processes.rs`; temporary
  repositories with `tempfile`, as in the workbench tests.

### 5. Freshness

Deliverables: `@knot/git` watches each repository's metadata (HEAD, the
current branch's ref, packed refs) and reports changes through `vcs`;
`@knot/vcs` refetches baselines for affected documents, and re-resolves a
document that moved or was saved.

Validation: behavior tests for a commit, checkout, and reset made outside
Knot, a branch switch, Save As into another repository, and a watcher that
stops when the repository is deleted.

Review gate: settles the `.git` watching decision.

**Implementation context:**
- `files.watch` in `docs/extension-api/application.md#read-files` and its
  native side in `docs/decisions/documents-and-persistence.md#file-watching`;
  watch tests in `src/app/product/tests/document_watch.rs`.
- `documents.track`'s `onDidChange` for moves and saves.

### 6. Navigation

Deliverables: `vcs.nextChange` and `vcs.previousChange` with wrap-around,
Alt+F5 and Shift+Alt+F5, and palette entries.

Validation: behavior tests for moving across hunks, wrapping, a deletion
hunk, and a document without changes.

Review gate: D211 complete. Move durable behavior into an architecture
reference and the extension guide, mark D211 ✅ in the
[version control plan](vcs.md), and remove this plan.

**Implementation context:**
- Command and keybinding registration as in
  `extensions/@knot/go-to-line/src/extension.ts`; editor selection through
  `workbench.editor(target)`.
