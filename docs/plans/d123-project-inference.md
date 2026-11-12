# D123 — Project inference extension

Status: 🚧 approved, not started. Item D123 of the
[daily-driver plan](daily-driver.md), promoted from the [roadmap](../roadmap.md).
Depends on ✅ D122 (extension file read, list, and stat API).

## Goal

When a user opens a document that lives inside a version-control repository
but outside the current window's roots, Knot notices and offers to work on it
as a project, the way Emacs's `project.el` infers a project from the file's
location. Opening a stray file from a checkout should not leave the user with
an unrooted window and no explorer, file index, or project session for it.

## Chosen design

- **Home: `@knot/projects`.** Inference folds into the existing bundled
  projects package rather than becoming a new `@knot/project-inference`
  package. `@knot/projects` already owns the registry, window bindings,
  live tracking, session switch/restore, and package storage, and its
  tracking already remembers any root set a window shows. A separate package
  could only call `windows.open` / `workspaces.replaceRoots`, would race with
  projects' tracking observer, and could not reuse the switch path. Inference
  is project policy, so it lives with the rest of project policy.

- **Marker: `.git` only, file or directory.** The inferred root is the
  nearest ancestor directory of the document that contains a `.git` entry.
  `files.stat` reports the target kind for symlinks and does not filter
  hidden entries, so a `.git` directory (normal clone), a `.git` file
  (worktree or submodule), both count. The walk stops at the first match;
  nested repositories infer their inner root. `.hg`, other VCS markers, and
  non-VCS project markers (`Cargo.toml`, `package.json`, …) are excluded
  (see Exclusions).

- **Trigger: opening, not restoring or focusing.** Inference is evaluated
  after launch, when a document is opened (`documentOpened`) or its resource
  changes (`documentChanged` with a changed `resourceUri`, i.e. Save As into
  a repository). Documents restored at startup and tab-focus changes never
  trigger an offer, so launch cannot produce a prompt storm. Evaluation runs
  off the package's existing serial observation handling in
  `onApplicationEvent`, gated on `state.launched` like `track`.

- **Guards — no offer when:**
  - the document is untitled, a destination without a stored file, or its URI
    is not a `file:` URI (inference walks stored resources via D122);
  - the document already lies under one of its window's roots;
  - no `.git` ancestor exists;
  - the inferred root is the filesystem root;
  - the inferred root is the user's home directory (a dotfiles repository at
    `$HOME` must not claim every loose file under it);
  - the inferred root is already a root of any open window (that project is
    already open; tracking binds windows by root set, so there is nothing to
    offer);
  - the root was previously dismissed (persisted, below), or an offer for the
    same root is already shown or was already shown this session in that
    window.

- **The offer** is one native confirmation (`workbench.showConfirmation`,
  targeted at the window presenting the document), naming the inferred root
  folder, e.g. "Open /path/to/repo as a project?". A confirmation that fails
  with `OperationInProgress` (another transient interaction owns the window)
  counts as *not shown*, not as a dismissal: the root stays eligible and a
  later opened document may offer it again. Dismissal by the user counts and
  is remembered.

- **Accepting:**
  - *Window without roots:* `workspaces.replaceRoots([inferred], { target })`
    in place. Existing `@knot/projects` tracking then remembers/binds the
    root set as an unnamed (or matching suspended) project with no extra
    code. The document stays open; roots never close documents.
  - *Window with roots:* a **new window** rooted at the inferred root
    (`windows.open({ roots: [inferred] })`), with the document presented
    there (`documents.open(uri, { target: newWindow })`). The originating
    window is untouched. Adding the folder as an extra root of the current
    window was rejected: projects follow root edits, so it would silently
    rewrite the current project's identity. Switching the current window via
    `switchTo` was rejected: it closes the source project's windows, which is
    far heavier than opening one file warrants.

- **Dismissal memory** lives in `@knot/projects` package storage under its
  own record key (e.g. `inference:dismissed`), a list of normalized root
  URIs dismissed by the user. It persists across restarts; there is no
  settings entry and no reset command (see Exclusions). Separately, an
  in-memory per-session set caps offers to at most one shown offer per root
  per window, so opening ten files from one checkout shows at most one
  prompt per window. Accepting a root makes later offers moot through the
  already-a-root guard.

- **Home directory fact.** Extensions have no home-directory fact today
  (`platform` is only the OS string), so D123 adds one small native fact
  beside it: a `homeUri` export (`string | null`, a normalized `file:` URI,
  null when no home directory is known). The guard compares the inferred
  root against it exactly, after the same URI normalization the rest of the
  package uses. No heuristic path-depth rule.

## Boundaries

- Inference is bundled-package code on the public API only, plus the one
  `homeUri` host fact. No native project inference, no private host bypass,
  consistent with the [bundled explorer](../decisions/bundled-packages.md#bundled-explorer)
  and [bundled project registry](../decisions/bundled-packages.md#bundled-project-registry)
  decisions.
- Inference never mutates the registry directly. Accepting goes through
  public root/window operations; `@knot/projects` tracking
  (`rememberUnbound`, `restoreUnbound`, session records) remains the single
  owner of remembering and binding, exactly as for folders opened by hand.
- The ancestor walk uses only `files.stat` on `<dir>/.git` candidates and
  string parent computation over normalized URIs (mirroring `parentOf` in
  `extensions/@knot/explorer/src/rows.ts`); it does not list directories, so
  cost is one stat per ancestor level and independent of tree size. The
  workspace file index and `files.search` are not involved.
- Failures (stat errors, a window closing mid-offer, storage write failure)
  are silent: no offer, no report prompt. Inference is a convenience and must
  never interrupt editing with its own diagnostics. The next eligible
  document may retry.

## Exclusions

- No `.hg`/`.svn`/other VCS markers and no language/tool project markers.
  `.git` first, per the brief; more markers wait for a real consumer.
- No marker configuration setting, no enable/disable setting.
- No offer for documents restored at startup, on tab focus, or from the
  Projects tree; no "Open as Project" command in v1.
- No dismissal-reset command or settings UI; manual Open Folder and project
  switching remain the recovery path.
- No change to `@knot/projects` switching, sessions, pruning, or the
  registry schema. The dismissal record is a separate storage key, not a
  registry field.
- No multi-root offer (never proposes adding several inferred roots, and
  never proposes adding a root to a rooted window).

## Decisions still deferred

- Whether a marker setting or additional markers (`.hg`) are worth it —
  revisit after daily use of the `.git`-only version.
- Whether startup-restored loose documents should ever be evaluated (e.g.
  once, after launch settles) — deferred until the opening-only trigger is
  proven insufficient in daily use.
- Exact offer wording and confirm/cancel labels beyond naming the folder —
  settled at implementation against existing `showConfirmation` call sites
  (e.g. project removal in `extensions/@knot/projects/src/commands.ts`).

## Checkpoints

### 1. `homeUri` host fact ✅

Expose the user's home directory to extensions as a normalized `file:` URI
(or null), beside the existing `platform` export.

Deliverables:

- `homeUri` in the public facade: `crates/knot-host/src/js/knot.d.ts`,
  `bootstrap.js`, and `knot.js`, fed from a new `homeUri` native binding
  installed next to `platform` in `initialize_extension_context`
  (`crates/knot-host/src/engine/pool.rs`). Source it with
  `directories::BaseDirs` (already a workspace dependency, used the same way
  in `src/app/product.rs` for terminal working directories); `knot-host`
  gains that dependency. Normalize to the URI form document/resource URIs
  use, so extension string comparison is valid.
- The fake knot host used by extension tests mirrors it:
  `scripts/extensions/testing/host.ts` / `knot.ts` (today they hardcode
  `platform: "macos"`), plus the surface assertion in
  `src/app/extensions/host/engine_tests.rs` that enumerates the facade keys.
- Document the fact in `docs/extension-api.md` next to `platform`.

**Implementation context:** mirror how `platform` travels
native binding → `bootstrap.js` const → `knot.js` re-export → `knot.d.ts`;
there is no annotated-handler/declaration step for these constants.
`host::declarations` validation is unaffected.

Validation: existing host/engine tests plus a new assertion that `homeUri`
is a `file:` URI or null in the real host and settable in the fake host;
`just check`.

Review gate: fact shape (`homeUri: string | null`, top-level like
`platform`) confirmed before package work builds on it.

### 2. Inference core in `@knot/projects` (no UI yet)

Pure decision logic and dismissal storage, covered by package tests, with no
prompt wired up.

Deliverables:

- New module `extensions/@knot/projects/src/inference.ts`: given a document
  URI, the presenting window's roots, all windows' roots, the home URI, and
  the dismissed set, resolve the inferred root or null. Contains the
  ancestor `.git` walk over `files.stat`, parent computation, and every
  guard from Chosen design. Kept free of confirmation/window mutation so it
  is unit-testable against the fake host's `files`.
- Dismissal record: load/write helpers alongside the registry/session
  storage helpers in `src/state.ts` (records via `storage.read/write`, last
  good state kept on failure, mirroring `reload`), stored under a dedicated
  key and loaded in `start()` (`src/extension.ts`).
- Package tests `extensions/@knot/projects/test/inference.test.ts` on the
  existing fake-host support (`test/support.ts`): nearest-ancestor wins,
  `.git` file counts, no marker → null, inside-a-root → null, filesystem
  root and home guards, already-open root → null, dismissed root → null,
  non-`file:` URI → null.

**Implementation context:** mirror `parentOf`/`within` string handling in
`extensions/@knot/explorer/src/rows.ts` and root comparison via
`windowRoots`/`sameRoots` in `src/layout.ts` / `src/registry.ts`. State
additions go on `ProjectsState` in `src/state.ts`, initialized in
`createState`.

Validation: package test runner for `@knot/projects`
(`scripts/extensions`), existing projects tests unchanged and green.

Review gate: guard set and walk semantics reviewed against the brief's three
open questions (markers, no-root behavior, repetition) before prompts land.

### 3. Offer wiring and acceptance

Connect the core to observation, the confirmation, and the two acceptance
actions; prove the flow end to end.

Deliverables:

- In `src/tracking.ts` / `src/extension.ts`: after `launched`,
  `documentOpened` and URI-changing `documentChanged` events evaluate
  inference for the document's presenting window (found via
  `windows.list()` presentations). Offers are serialized through the
  package's existing serial observation handling; at most one confirmation
  is outstanding, and the per-window/per-root session cap is recorded when
  an offer is *shown*.
- `workbench.showConfirmation` targeted at that window, naming the inferred
  folder. Confirmed → checkpoint-3 acceptance by window state (rootless:
  `workspaces.replaceRoots`; rooted: `windows.open` + `documents.open` in
  the new window). Dismissed → persist dismissal, then continue. Errors
  (including `OperationInProgress`, closed target) leave the root eligible
  and are swallowed.
- Package tests: offer shown once for ten documents from one checkout;
  dismissal persists and suppresses after a simulated restart (reload of
  storage records); rootless acceptance replaces roots; rooted acceptance
  opens a new window and leaves the source window's roots unchanged; busy
  transient slot defers without recording dismissal.
- One integration test in the real editor
  (`crates/knot-ext-workbench/tests/workbench/projects.rs` or a sibling
  module) proving a bundled-package offer/accept flow against real roots on
  disk, matching how the other workbench packages are covered.
- Update the `@knot/projects` package header comment in `src/extension.ts`
  to describe inference, and the extension guide's project section if it
  enumerates package behavior.

**Implementation context:** observation and launch gating mirror
`onApplicationEvent`/`afterLaunch` in `src/tracking.ts`; confirmation usage
mirrors `src/commands.ts` (project removal). Acceptance must not call
`switchTo` (see Chosen design). Any manual-verification build gets `try.*`
commands via a personal-config `post-init.js`, per repository rules, rather
than test-only product commands.

Validation: full `@knot/projects` package tests, the workbench integration
test, and `just check` (with `cargo fmt` run once at the end). Manual gate:
open a file from a checkout in a rootless window (offer → window adopts the
root), dismiss it and open another file from the same checkout (no second
offer), and open an outside file in a rooted window (offer → new window,
original untouched).

Review gate: end-to-end behavior accepted; then record the validated choices
in `docs/decisions/bundled-packages.md` (new project-inference entry plus
index line in `docs/decisions.md`), fold durable behavior into
`docs/architecture/` only if boundaries changed, and remove this plan.

## Cross-check

- Source item: [daily-driver D123](daily-driver.md); dependencies D122 ✅.
- No new crate, no new package, no registry schema change; the only native
  change is the additive `homeUri` facade fact (checkpoint 1).
- Nothing here revisits D084 (project selector), D089 (project-local
  settings), or D124 (workspace trust): inference opens ordinary roots and
  runs no project code.
