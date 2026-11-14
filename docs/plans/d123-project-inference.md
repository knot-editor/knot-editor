# D123 — Project inference

Status: 🚧 checkpoints 1–2 complete; checkpoint 3 remains. Item D123 of the
[roadmap](../roadmap.md).
Depends on ✅ D122 (extension file read, list, and stat API).

## Goal

When a user opens a document that lives inside a version-control repository
but outside the current window's roots, Knot works on it as a project, the
way Emacs's `project.el` infers a project from the file's location. Opening
a stray file from a checkout (`knot foo.txt`) should not leave the user with
an unrooted window and no explorer, file index, or project session for it.

## Chosen design

- **Home: `@knot/projects`.** Inference folds into the existing bundled
  projects package rather than becoming a new package. `@knot/projects`
  already owns the registry, window bindings, live tracking, session
  switch/restore, and package storage. A separate package could only call
  `windows.open` / `workspaces.replaceRoots`, would race with projects'
  tracking observer, and could not reuse the switch path.

- **Marker: `.git` only, file or directory.** The inferred root is the
  nearest ancestor directory of the document that contains a `.git` entry
  (directory for a clone, file for a worktree or submodule). The walk stops
  at the first match, so nested repositories infer their inner root. No
  other VCS or language/tool markers (see Exclusions).

- **No prompt: adopt automatically, or on command.** Inference never asks.
  - *Automatic*, in a window **without roots whose only tab is the
    document**: the window adopts the inferred project. This is the
    `knot foo.txt` case: launch placement puts a file no root contains in a
    fresh rootless window created just for it, so rooting that window cannot
    surprise. A rootless window holding other loose files is left alone.
  - *Command* "Projects: Open Containing Repository as Project"
    (`projects.openContainingRepository`) on the active document, for every
    other case, notably File → Open of a file from another checkout in a
    rooted window. Nothing happens automatically there: the file opens in
    the current window as today.
  - Rejected: a confirmation after opening. The question appears over a file
    already shown, Cancel undoes nothing, and accepting moves the file to
    another window; plus it needed dismissal memory. Rejected: asking
    *before* the open, which needs a host hook putting an extension callback
    on every open and launch path. Rejected: a non-modal status-bar offer;
    status items are application-wide, not per window.

- **Trigger for the automatic path: opening, not restoring or focusing.**
  After launch, `documentOpened` and URI-changing `documentChanged` (Save As
  into a repository) evaluate inference for the document's presenting
  window. Documents restored at startup and tab-focus changes never adopt.
  Files passed to `knot` at a cold start open after `launched`, so they do.
  Evaluation runs in `onApplicationEvent` before tracking, gated on
  `state.launched`.

- **Guards — no inferred project when:**
  - the document is untitled, a destination without a stored file, or not a
    `file:` URI;
  - no `.git` ancestor exists;
  - the inferred root is the filesystem root or the user's home directory (a
    dotfiles repository at `$HOME` must not claim every loose file under it),
    using the `homeUri` host fact;
  - automatic path only: the document lies under one of its window's roots,
    the window has other tabs, or the inferred root is already a root of any
    open window (nothing to adopt; the file simply stays where it opened).

- **Adopting** depends on whether the registry already knows the root
  (`projectWithRoots`). Because `@knot/projects` remembers every root set a
  window shows, the known case is the common one in daily use.
  - *New root, window without roots:* `workspaces.replaceRoots([inferred],
    { target })` in place. Tracking remembers and binds it as an unnamed
    project; the document stays open.
  - *New root, window with roots (command):* a new window rooted at the
    inferred root (`windows.open`), with the document presented there
    (`documents.open(uri, { target })`). The originating window is
    untouched.
  - *Known suspended project:* `switchTo`, so the project's session
    (windows, layout, retained text) is restored, with the document
    presented in the first restored window. Two `switchTo` origin options:
    - *window without roots:* the origin is **adopted**: its tabs join the
      first restored window as session entries carried by document identity
      (`Entry.document`), and the origin is released, so the user ends with
      only the project's windows.
    - *window with roots:* the origin is **kept**: no switch sources, so the
      session opens in new windows and the source project's windows are
      neither suspended nor closed.
  - *Root already open (command only):* focus the window that has it and
    present the document there.
  - Rejected: adopting a known project through `replaceRoots` /
    `windows.open` and letting tracking's `restoreUnbound` restore it. The
    origin holds the document, so it is not reusable and the session opens in
    yet another window. Rejected: adding the folder as an extra root of a
    rooted window, which would silently rewrite that project's identity
    because projects follow root edits. Rejected: a plain `switchTo` from a
    rooted window, which closes the source project's windows.

- **Restoring a known project is intended.** `knot ~/src/repo/README.md`
  for a known project restores its whole session, not only the one file, as
  a project-aware editor should.

- **Home directory fact.** Extensions see `homeUri` (`string | null`, a
  normalized `file:` URI) beside `platform`; the guard compares the inferred
  root against it exactly. No heuristic path-depth rule.

## Boundaries

- Inference is bundled-package code on the public API only, plus the one
  `homeUri` host fact. No native project inference, no private host bypass,
  consistent with the [bundled explorer](../decisions/bundled-packages.md#bundled-explorer)
  and [bundled project registry](../decisions/bundled-packages.md#bundled-project-registry)
  decisions.
- Inference never mutates the registry directly. A new root goes through
  public root/window operations and a known project through `switchTo`;
  tracking (`rememberUnbound`, `restoreUnbound`, session records) remains the
  single owner of remembering and binding.
- The walk uses only `files.stat` on `<dir>/.git` candidates and string
  parent computation over normalized URIs; one stat per ancestor level,
  independent of tree size. The file index and `files.search` are not
  involved.
- Automatic adoption fails silently (stat errors, a window closing midway, a
  failed switch): the file stays where it opened. The command, being
  user-invoked, tells the user when no repository contains the document and
  reports failures like the other Projects commands.

## Exclusions

- No `.hg`/`.svn`/other VCS markers and no language/tool project markers.
- No marker setting and no enable/disable setting.
- No confirmation, no dismissal memory.
- No automatic adoption for restored documents, on tab focus, in rooted
  windows, or in rootless windows with other tabs.
- No change to sessions, pruning, or the registry schema. Switching changes
  only by the two origin options (adopt a rootless origin, keep a rooted
  one).
- Never adds a root to a rooted window and never infers several roots.

## Decisions still deferred

- Whether a marker setting or additional markers (`.hg`) are worth it —
  revisit after daily use of the `.git`-only version.
- Whether the command in a rooted window should close the document's tab in
  the originating window when it is clean, instead of leaving a duplicate —
  deferred until daily use shows the duplicate is a nuisance.
- Whether File → Open of a file whose repository is already a root of
  another window should move it there — today it stays in the current
  window, matching how File → Open works without inference.

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

Completion note: the walk and guards stand. The design then moved from a
confirmation to automatic adoption, so the dismissal record, the
`dismissed` state, and the `dismissed` inference input are removed in
checkpoint 3.

### 3. Automatic adoption and the command

Wire the core to observation and a command, with both adoption paths, and
prove them end to end.

Deliverables:

- Remove the dismissal parts of checkpoint 2: `INFERENCE_DISMISSED_KEY`,
  `dismissedFromJson`, `loadDismissed`, `rememberDismissed` and their call in
  `start()`, `state.dismissed`, the `dismissed` field of `InferenceInput`,
  and their tests. Split the guards so the automatic-only ones (inside a
  window root, other tabs, root already open) are applied by the automatic
  path and not by the command.
- `switchTo` gains the two origin options (adopt a rootless origin, keep a
  rooted one) and presents a given document in the first restored window.
  The existing Projects commands call it without options, unchanged.
- Automatic path in `src/tracking.ts`: after `launched`, `documentOpened` and
  URI-changing `documentChanged` find the presenting window via
  `windows.list()` and adopt per Chosen design, before `track` runs.
- `projects.openContainingRepository` in `src/commands.ts`, titled "Projects:
  Open Containing Repository as Project", acting on the active document of
  the command's window.
- Package tests: automatic, new root → roots replaced in place, unnamed
  project bound; automatic, known project → session restored, document in
  its first window, origin released, dirty text preserved; no adoption in a
  rootless window with another tab, in a rooted window, for an untitled
  document, at home or filesystem root, or when the root is already open;
  a stat failure leaves the file in place. Command: rooted window, new
  root → new window, source roots unchanged; rooted window, known project →
  session in new windows, source project's windows and record untouched;
  root already open → that window focused with the document; no repository
  → message.
- One integration test in the real editor
  (`crates/knot-ext-workbench/tests/workbench/projects.rs` or a sibling
  module) against real roots on disk, including a file passed to a cold
  start through the launch path, which must end in a window rooted at its
  repository.
- Update the `@knot/projects` package header comment in `src/extension.ts`,
  and the extension guide's project section if it enumerates package
  behavior.

**Implementation context:** observation and launch gating mirror
`onApplicationEvent`/`afterLaunch` in `src/tracking.ts`; the command mirrors
the existing ones in `src/commands.ts`. Session entries already carry live
documents by `document` identity (`Entry` in `src/session.ts`), which the
adopt option reuses rather than capturing and reopening by URI. Manual
verification uses `try.*` commands in a personal-config `post-init.js`.

Validation: full `@knot/projects` package tests, the workbench integration
test, and `just check` (with `cargo fmt` run once at the end). Manual gate:
1. `knot <file in a never-seen checkout>` → the window is rooted at the
   repository.
2. Close it, then `knot <another file from that checkout>` → the project's
   session restores with the file in it and no stray window.
3. In a rooted window, File → Open a file from another checkout → it opens
   in place; "Open Containing Repository as Project" → new window, original
   untouched.

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
