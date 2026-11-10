# D120 — Remembered projects

Status: 🚧 in progress. Item of the
[daily-driver plan](daily-driver.md); depends on D070 (settings) and D084
(project selector modal).

## Goal

Every folder set a user opens is remembered as a project, as in VS Code and
Zed, without asking for a name. Reopening it, from the project picker, from
Open Folder, or from the command line, brings back its windows, tabs, layouts,
and unsaved text. Naming a project becomes optional labelling rather than the
way to opt in.

## Design

- **Inferred names.** A project's stored name is `string | null`. `null` means
  the name is derived from the roots when displayed: the first root's folder
  name, plus `+N` for further roots (today's `rootsLabel`). Renaming sets an
  explicit name; renaming to an empty string returns to the inferred one.
  Inferred names follow root edits without a write.
- **Names are not unique.** The uniqueness check and lookup by name go away.
  The D084 picker identifies projects by ID and shows the roots as the
  description, so two folders called `app` are distinguishable by path. The
  Projects tree, whose row activation resolves projects by label, is
  removed by D084, which this item depends on.
- **Automatic creation.** After launch, a window whose non-empty root set
  matches no project creates an unnamed project with an active session and
  binds to it. Windows with no roots never create one. `projects.create`
  disappears: Open Folder is how a project starts.
- **Pruning.** A `projects.rememberedLimit` setting (D070), defined by the
  package, bounds unnamed projects, defaulting to 30, by most recent
  `lastOpenedAt`. Pruning runs when a project is created. It never removes a
  named project, an active project, or a project whose suspended session holds
  retained captures; a pruned project's session record is deleted with it.
- **Open Folder opens a new window** unless the current window has no roots,
  as in Zed. Root changes inside a window then come only from Add or Remove
  Folder, so the existing rule that a bound project adopts its window's edited
  roots stays correct and never turns one remembered folder into another.
  Switching a window in place remains the picker's job, which suspends and
  restores through `switchTo`. `workspaces.openFolder` keeps its explicit
  replace semantics for extensions.
- **Opening a suspended project's folder restores it.** When an unbound
  window's roots equal a suspended project's roots, tracking switches that
  window to the project with the existing `switchTo`. The window just opened
  on those roots is replaced when it holds nothing beyond one clean, empty
  untitled document, so the folder never appears twice; otherwise the
  project opens in a new window and the unbound window binds to it once
  active. This also applies at
  `launched` to launch-request windows, after restored windows have rebound.
  Membership still requires an active session, so tracking never overwrites a
  suspended record; restoring goes through the switch, which owns captures.

## Exclusions

- Inferring a project from an opened file's VCS root (D123).
- Project-local settings (D089) and a project file in the repository: records
  stay in extension storage.
- A start surface listing recent projects (D061).

## Deferred decisions

- Whether pruning also considers age, not only count. Count alone until daily
  use says otherwise.

## Checkpoints

### 1. Open Folder opens a new window ✅

Open on a folder installs it in a window without roots and opens a new window
for it otherwise; see
[documents and persistence](../architecture/documents-and-persistence.md#workspaces-and-resource-trees)
and the Folder Open decision in
[its decisions](../decisions/documents-and-persistence.md).

### 2. Unnamed projects and inferred names ✅

A project's name is optional and displays as its first root's folder name
(plus `+N`) when absent; names are not unique, and the picker shows the roots'
paths on a second line. Tree rows gained an optional `detail` second line for
the same purpose. `projects.create` stays until checkpoint 3, because it is the
only way to make a project before creation is automatic; it accepts a blank
name and moves out with that checkpoint.

### 3. Automatic creation and pruning ✅

Tracking creates an unnamed active project for an unbound window whose
non-empty root set no project has, after launch and for windows present at
`launched`; `projects.rememberedLimit` (see [settings](../settings.md)) prunes
unnamed projects on creation, sparing named, active, unreadable, and
capture-holding ones. `projects.create` is gone.

### 4. Restoring on a hand-opened folder

- Deliverables: an unbound window matching a suspended project switches to
  it, both during tracking and at `launched` after restored windows rebind.
- Validation: `node:test` switch and tracking tests for the hand-opened
  window with matching roots and no content being replaced rather than
  duplicated, a window with other content opening the project in a
  new window, retained text coming back, and a project with an unreadable
  session record staying untouched. A product test opens a suspended
  project's folder with Open Folder and sees its tabs return.
- **Implementation context:** call `switchTo` in `switch.ts` from `track` and
  `afterLaunch` in `tracking.ts`; `isReusable` in `layout.ts` requires an empty root
  set today, so it must also accept a window whose roots equal the project's
  (the hand-opened window); `switchTo` passes the project's roots. Commands and listeners are serial roots, so the switch cannot
  interleave with tracking.
- Review gate: restoring never loses text and never fires for a window that
  is already bound.

### 5. Documentation

- Deliverables: the bundled project registry and project sessions decisions
  updated for unnamed projects, inferred names, automatic creation, pruning,
  restore on open, and the reason Open Folder opens a new window; the
  extension guide's project section and the projects package's module comment
  in `extension.ts` match the new behavior.
- Review gate: remove this plan and mark D120 ✅ in the daily-driver plan.
