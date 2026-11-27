# D215 — VCS commit

Status: 🚧 in progress (checkpoints 1 to 7 done; 8 remains). Source: [roadmap D215](../roadmap.md#version-control), milestone 4 of
the [version control plan](vcs.md).

## Outcome

Committing no longer needs a terminal. The Changes tool window gains a commit
panel under its tree, as in IntelliJ, VS Code, and Zed: a message editor, the
backend's options as checkboxes (for Git: Amend, Sign-off), and a Commit
button. Cmd+Alt+C focuses the message and Cmd+Enter in it commits. Earlier messages
come back through a history picker. What Version Control runs on the user's
behalf is logged in a Version Control output tool window, and Cmd+click on an
http(s) URL in any editor, the output included, opens it in the browser.

## Design

### Commit through the provider

The backend contract gains three methods, all data, no callbacks:

- `commitOptions(repository) → [{ id, label, tooltip? }]`: toggles the panel
  shows as checkboxes, all off by default.
- `commitMessage({ repository, options }) → string | null`: the message the
  options suggest. Git answers HEAD's message when `amend` is on, null
  otherwise. When an option toggles and the answer changes, the panel swaps
  the draft for the suggestion and keeps the user's draft to put back when the
  suggestion goes away, as IntelliJ does with Amend.
- `commit({ repository, message, options }) → { summary } | null`: commits,
  asking the user for anything it needs itself; null means the user cancelled.
  A failure rejects, and `@knot/vcs` raises the error notice.

Git commits the index with `git commit -F -` (message on stdin), adding
`--amend` and `--signoff` per option. With nothing staged, it confirms "Stage
all changes and commit?" and runs `git commit -a`; untracked files are never
included. An empty message is refused before calling the backend, except when
amending.

### The panel

The panel commits one repository: the one of the changes view's selected row,
or the only one. Its header names that repository when the window covers
several. Each repository has one draft, a `buffers.create` buffer with
`persist` keyed by the repository, cleared after a successful commit. Commit is
disabled while the draft is empty and nothing is being amended.

History is a per-repository `history` list, newest first, capped at 50,
recorded on a successful commit. `vcs.commitMessageHistory` (Ctrl+M in the
message editor, as IntelliJ) opens a picker over it and replaces the draft
with the chosen message, as one undoable edit.

### UI primitives as extension API

- **Layout tool windows.** A view registered with `content: { kind: "layout" }`
  shows a [view layout](../../src/app/view_layout.rs), the builder composites
  and popovers share, with `view.update({ layout, toolbar }, { window })`
  replacing it per window. Keyed members keep their views across updates as in
  composites. The Changes tree becomes an `embedded` tree inside the
  `vcs.changes` layout tool window.
- **Controls member.** A layout member `{ type: "controls", items }` with
  `checkbox` (`label`, `checked`, `command`, `arguments`, `tooltip`) and
  `button` (`label`, `primary`, `disabled`, `command`, `arguments`) items,
  laid out in a wrapping row. Like the toolbar, a click runs the command with
  the tab or tool window as target, and the package sends the items again to
  change their state.
- **Editor placeholder.** An editor member takes `placeholder`, shown muted
  while its buffer is empty ("Commit message (⌘⏎ to commit)").

### Output

`@knot/vcs` owns a Version Control output tool window (`vcs.output`,
`bottomRight`, stripe shown), a text frame with the gutter off. Backends report
through the `vcs` contract: `log({ backend, repository, command, output,
failed })` appends a block with the command line, its stdout and stderr, and
the result. Only commands the user started are logged (commit, and the changes
view's Stage, Unstage, Discard, and Rollback, and later D216's push and pull),
never background reads. The log keeps the last 256 KiB and shows its end after
each append.

It must be discoverable:

- the stripe button, like any tool window;
- an output button in the Changes toolbar;
- `vcs.showOutput` (Show Version Control Output) in the palette;
- a notice after each commit, "Committed <short ID>: <subject>" or the failure,
  with a Show Output action.

### Links in editors

URL detection leaves the terminal for a pure function in `knot-model` over a
line of text, which the terminal and editors share. In any editor, holding Cmd
underlines the http(s) URL under the pointer and Cmd+click opens it with the
platform browser; without Cmd, clicks place the caret as before. Detection runs
on the hovered line only, so nothing is precomputed.

## Exclusions

- Per-file commit checkboxes and partial commits (needs D177 multi-select);
  Git commits what is staged.
- Committing several repositories at once.
- Commit templates (`commit.template`), message linting, and author override.
- Push after commit and every other Git operation: D216.
- Links to files or other schemes (D042), and link detection beyond the
  hovered line.

## Deferred decisions

- Whether amending pre-fills on toggle or also shows HEAD's files in the tree.
- Whether D216's push output should also raise a notice with a "Create Pull
  Request" action parsed from the remote message, as Zed does.

## Checkpoints

### 1. Links in editors ✅

Shared URL detection lives in `knot-model` (`links`), and editors and text
frames open http(s) URLs with Cmd+click; see
[editor and languages](../architecture/editor-and-languages.md).

### 2. Layout tool windows, controls, placeholders ✅

`kind: "layout"` tool windows, the `controls` member, and editor placeholders
are documented in the [ui guide](../extension-api/ui.md#layout-tool-windows)
and [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#layout-tool-windows).
The controls' look uses plain gpui-component checkbox and button styling and
may deserve polish when the commit panel lands.

### 3. Version Control output ✅

`vcs.log`, the `vcs.output` tool window, `vcs.showOutput`, and the Changes
toolbar button are described in [version control](../architecture/version-control.md#version-control-changes-view).
Text frames take `follow: true` to show their end after `setText`; see the
[ui guide](../extension-api/ui.md#read-only-text-frames).

### 4. Commit panel ✅

Deliverables: the three backend methods and Git's implementation, the Changes
view as a layout tool window with the panel, per-repository persisted drafts,
the Amend swap, `vcs.commit` and `vcs.focusCommitMessage` with Cmd+Alt+C and
Cmd+Enter, the commit notice, and logging through checkpoint 3.

Validation: `@knot/vcs` tests for target repository choice, draft and
suggestion swapping, disabled Commit, and failure notices over a fake backend;
`crates/knot-ext-vcs/tests/commit.rs` over temporary repositories for a commit
of the index, amend with its suggested message, sign-off, the stage-all
confirmation, a failing `commit-msg` hook, and the draft cleared and the status
refreshed after commit. `just check`.

Review gate: commit, amend, and sign off in a scratch repository; a failing hook
shows in the output. The first review found Cmd+K focusing nothing (it begins the split chords, so the binding is now Cmd+Alt+C) and the panel
ugly; checkpoints 5 to 7 address both, and the gate is repeated after 7.

Scope decisions: the panel follows the file last chosen in the tree rather than
the selected row (a selection is readable only inside a tree command), has no
header (the editor placeholder names the repository), and the product tests at
`crates/knot-ext-vcs/tests/vcs/commit.rs` drive the controls' commands, since a
synthetic control click is not dispatched to the package under the test driver.
See [version control](../architecture/version-control.md#commit-panel).

**Implementation context:** backend contract in
`extensions/@knot/vcs/src/contract.ts`; the view in `changes.ts` and `rows.ts`
becomes the embedded tree of the layout; new `commit.ts` owns the panel. Git
side in a new `extensions/@knot/git/src/commit.ts`, registered in `main.ts` like
`compare` and `rollback`. Persisted buffers are
[extension buffers](../extension-api/application.md#own-a-buffer). Update
[version control](../architecture/version-control.md) and the backends and
changes view decisions in [bundled packages](../decisions/bundled-packages.md).

### 5. Panel polish and member focus ✅

`ViewRegistration.focus(window, { member })`, the editor member's `gutter: false`,
and the compact controls row are described in the [ui guide](../extension-api/ui.md#layout-tool-windows).
Cmd+K begins the split chords, so the panel's focus binding is Cmd+Alt+C.

### 6. Resizable commit editor ✅

A pixel-sized member of a column with `resize: "<name>"` shows a handle on its
top edge, described in the [ui guide](../extension-api/ui.md#layout-tool-windows).
Scope decisions: the dragged height (40 to 480 pixels) is remembered under that
name for the whole application rather than per window, as one record in the
state database, so every repository's commit editor shares it.


### 7. Message ruler ✅

`editor.rulers` and a layout editor member's `rulers` draw vertical guides, described in
[editor and languages](../architecture/editor-and-languages.md) and the
[ui guide](../extension-api/ui.md#layout-tool-windows). The commit message
shows one at 72. Checkpoint 4's gate was repeated and passed.

### 8. Message history

Deliverables: per-repository history recorded on commit, `vcs.commitMessageHistory`
with Ctrl+M in the message editor, replacing the draft as one undo step.

Validation: `@knot/vcs` tests for recording, cap, order, and replacement;
a product test that the picker lists earlier messages. `just check`.

Review gate: commit twice, then recall the first message.

**Implementation context:** the `history` named list and pickers are in
[ui](../extension-api/ui.md#ask-the-user-transiently); keybinding scoped to the
message editor's view through `keymaps.bind`. When done, move the durable parts
into the docs above, mark D215 ✅ in [vcs](vcs.md), and delete this plan.
