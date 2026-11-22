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
- ✅ `files.watch` hides `.git`, so a generic `includeHidden` option was added
  rather than polling on window focus.
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

### 3. Gutter against the test backend ✅

Complete. `@knot/vcs` follows documents with `documents.track`, finds each
file's repository (roots of every window, then the file's directory, deepest
wins), caches baselines, and paints `diff` hunks with a fixed 150 ms debounce
and no setting. A backend whose contract stops binding is dropped and its marks
cleared. Tests: `extensions/@knot/vcs/test/gutter.test.ts` (fake host) and
`crates/knot-ext-vcs/tests/gutter.rs` (real host, `TestWindow::gutter_bars`).
A removal after the last line of a text without a final newline is not covered.

### 4. `@knot/git` baselines ✅

Complete. `@knot/git` declares `process`, finds repositories and serves HEAD
baselines; see [Git baseline](../decisions/bundled-packages.md#git-baseline).
Tests: `crates/knot-ext-vcs/tests/git.rs` (real host, temporary repositories).
A missing `git` is untested, since the product harness uses the real `PATH`.
`TestWindow::append` does not notify extensions of the edit, so editing after
open is covered only by the fake-host tests.

### 5. Freshness ✅

Complete. `@knot/git` watches each repository it serves and reports changes
through `vcs`; `files.watch` gained `includeHidden` because it hid `.git`
(see [Git baseline](../decisions/bundled-packages.md#git-baseline)). Tests:
`crates/knot-ext-vcs/tests/git.rs` covers a commit, soft reset, branch switch,
staging, and a deleted repository made outside Knot; a move or save of a
document is covered by the fake-host tests in
`extensions/@knot/vcs/test/gutter.test.ts`, since the product harness has no
Save As. Real-host watch tests need `TestProduct::watch_files`.

### 6. Navigation ✅

Complete. `vcs.nextChange` and `vcs.previousChange` are bound to Alt+F5 and
Alt+Shift+F5, and `]c` and `[c` in Vim mode; see
[version control](../architecture/version-control.md#navigation).
Review gate passed; D211 durable behavior lives in that reference.
