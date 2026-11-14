# D123 — Project inference extension

Status: 🚧 checkpoints 1–2 complete; checkpoint 3 remains. The design was
revised after the checkpoint-2 review: projects the registry already knows
now open through the switch path, and dismissal lasts one session. Item D123 of the
[roadmap](../roadmap.md).
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
  - the root was dismissed earlier this session (below).

- **The offer** is one native confirmation (`workbench.showConfirmation`,
  targeted at the window presenting the document). For a new root it names
  the folder ("Open /path/to/repo as a project?"); for a root the registry
  already has a project for (`projectWithRoots`) it names the project
  ("Switch to knot-editor?"). A confirmation that fails with
  `OperationInProgress` (another transient interaction owns the window)
  counts as *not shown*, not as a dismissal: the root stays eligible and a
  later opened document may offer it again. Listeners are serial roots, so
  the next event is evaluated only after the offer resolves and sees its
  outcome through the guards; no separate "offer pending" state is needed.

- **Accepting** depends on whether the registry already knows the root.
  Because `@knot/projects` remembers every root set a window shows, the
  known case is the common one in daily use.
  - *New root, window without roots:* `workspaces.replaceRoots([inferred],
    { target })` in place. Tracking then remembers and binds the root set as
    an unnamed project with no extra code. The document stays open; roots
    never close documents.
  - *New root, window with roots:* a **new window** rooted at the inferred
    root (`windows.open({ roots: [inferred] })`), with the document presented
    there (`documents.open(uri, { target: newWindow })`). The originating
    window is untouched.
  - *Known (suspended) project:* go through `switchTo`, so the project's
    session (windows, layout, retained text) is restored, and present the
    document in the first restored window. Two `switchTo` options cover the
    origin:
    - *window without roots:* the origin is **adopted**: it is a switch
      source and its tabs join the first restored window as session entries
      (carried by document identity, the way switching already carries
      documents), so the stray window disappears and the user ends with only
      the project's windows. Nothing is suspended, because the origin has no
      project.
    - *window with roots:* the origin is **kept**: no switch sources, so the
      session opens in new windows and the source project's windows are
      neither suspended nor closed.
  - Rejected: going through `replaceRoots` / `windows.open` for a known
    project and letting tracking's `restoreUnbound` restore it. The origin
    holds the document, so it is not reusable and the session opens in yet
    another window, leaving two or three windows for one acceptance (and in
    the rooted case a race between `documents.open` and the switch
    releasing the new window). Adding the folder as an extra root of the
    current window was rejected: projects follow root edits, so it would
    silently rewrite the current project's identity. A plain `switchTo` from
    a rooted window was rejected: it closes the source project's windows,
    which is far heavier than opening one file warrants.

- **Dismissal lasts one session.** Cancel adds the root to an in-memory,
  application-wide set (`state.dismissed`), so no window offers it again
  until restart. Nothing is persisted: Cancel often means "not now", and a
  permanent dismissal from one click with no reset path was judged more
  annoying than one prompt per repository per session. Accepting makes later
  offers moot through the already-a-root guard, so no per-window "already
  shown" cap is needed either.

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
- Inference never mutates the registry directly. Accepting a new root goes
  through public root/window operations, and accepting a known project goes
  through `switchTo`; `@knot/projects` tracking (`rememberUnbound`,
  `restoreUnbound`, session records) remains the single owner of
  remembering and binding, exactly as for folders opened by hand.
- The ancestor walk uses only `files.stat` on `<dir>/.git` candidates and
  string parent computation over normalized URIs (mirroring `parentOf` in
  `extensions/@knot/explorer/src/rows.ts`); it does not list directories, so
  cost is one stat per ancestor level and independent of tree size. The
  workspace file index and `files.search` are not involved.
- Failures (stat errors, a window closing mid-offer, a failed acceptance)
  are silent: no offer, no report prompt. Inference is a convenience and must
  never interrupt editing with its own diagnostics. The next eligible
  document may retry.

## Exclusions

- No `.hg`/`.svn`/other VCS markers and no language/tool project markers.
  `.git` first, per the brief; more markers wait for a real consumer.
- No marker configuration setting, no enable/disable setting.
- No offer for documents restored at startup, on tab focus, or from the
  Projects tree; no "Open as Project" command in v1.
- No persisted dismissal, so no reset command or settings UI.
- No change to `@knot/projects` sessions, pruning, or the registry schema.
  Switching changes only by the two origin options above (adopt a rootless
  origin, keep a rooted one).
- No multi-root offer (never proposes adding several inferred roots, and
  never proposes adding a root to a rooted window).

## Decisions still deferred

- Whether a marker setting or additional markers (`.hg`) are worth it —
  revisit after daily use of the `.git`-only version.
- Whether startup-restored loose documents should ever be evaluated (e.g.
  once, after launch settles) — deferred until the opening-only trigger is
  proven insufficient in daily use.
- Whether the rooted-window acceptances should close the document's tab in
  the originating window when it is clean, instead of leaving a duplicate —
  deferred until daily use shows the duplicate is a nuisance.
- Exact offer wording and confirm/cancel labels beyond naming the folder or
  project —
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

### 2. Inference core in `@knot/projects` (no UI yet) ✅

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

Completion note: the review kept the walk and guards and replaced the
persisted dismissal record with a session-only set; removing the storage
helpers is the first deliverable of checkpoint 3.

### 3. Offer wiring and acceptance

Connect the core to observation, the confirmation, and the acceptance
actions; prove the flow end to end.

Deliverables:

- Drop the persisted dismissal from checkpoint 2: remove
  `INFERENCE_DISMISSED_KEY`, `dismissedFromJson`, `loadDismissed`,
  `rememberDismissed`, their call in `start()`, and their tests. Keep
  `state.dismissed` as the session set.
- In `src/tracking.ts` / `src/extension.ts`: after `launched`,
  `documentOpened` and URI-changing `documentChanged` events evaluate
  inference for the document's presenting window (found via
  `windows.list()` presentations).
- `workbench.showConfirmation` targeted at that window, naming the folder or
  the known project. Confirmed → acceptance per Chosen design. Dismissed →
  add the root to `state.dismissed`. Errors (including
  `OperationInProgress`, closed target, failed acceptance) leave the root
  eligible and are swallowed.
- `switchTo` gains the two origin options (adopt a rootless origin, its tabs
  joining the first restored window; keep a rooted origin, no sources), and
  presents a given document in the first restored window. The Projects
  commands keep calling it without options, unchanged.
- Package tests: offer shown once for ten documents from one checkout;
  dismissal suppresses for the rest of the session in every window and not
  after a simulated restart; busy transient slot defers without dismissing;
  new root, rootless → roots replaced in place and an unnamed project bound;
  new root, rooted → new window, source roots unchanged; known project,
  rootless → session restored, the document in its first window, the stray
  window gone, dirty text preserved; known project, rooted → session
  restored in new windows, the source project's windows and session record
  untouched.
- One integration test in the real editor
  (`crates/knot-ext-workbench/tests/workbench/projects.rs` or a sibling
  module) proving a bundled-package offer/accept flow against real roots on
  disk, including a file passed to a cold start through the launch path (it
  lands in a rootless window and must be offered).
- Update the `@knot/projects` package header comment in `src/extension.ts`
  to describe inference, and the extension guide's project section if it
  enumerates package behavior.

**Implementation context:** observation and launch gating mirror
`onApplicationEvent`/`afterLaunch` in `src/tracking.ts`; confirmation usage
mirrors `src/commands.ts` (project removal). Session entries already carry
live documents by `document` identity (`Entry` in `src/session.ts`), which
the adopt option reuses rather than capturing and reopening by URI. Any
manual-verification build gets `try.*` commands via a personal-config
`post-init.js`, per repository rules, rather than test-only product
commands.

Validation: full `@knot/projects` package tests, the workbench integration
test, and `just check` (with `cargo fmt` run once at the end). Manual gate:
1. `knot <file in a never-seen checkout>` → offer → the window adopts the
   root.
2. Close it, then `knot <another file from that checkout>` → "Switch to …"
   offer → the project's session restores with the file in it and no stray
   window.
3. Dismiss an offer, open another file from the same checkout → no offer;
   restart → offered again.
4. In a rooted window, Open File from another checkout → offer → new
   window, original untouched.

Review gate: end-to-end behavior accepted; then record the validated choices
in `docs/decisions/bundled-packages.md` (new project-inference entry plus
index line in `docs/decisions.md`), fold durable behavior into
`docs/architecture/` only if boundaries changed, and remove this plan.

## Cross-check

- Source item: [D123](../roadmap.md); dependencies D122 ✅.
- No new crate, no new package, no registry schema change, no new storage
  record; the only native change is the additive `homeUri` facade fact
  (checkpoint 1).
- Nothing here revisits D084 (project selector), D089 (project-local
  settings), or D124 (workspace trust): inference opens ordinary roots and
  runs no project code.
