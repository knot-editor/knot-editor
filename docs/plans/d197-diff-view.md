# D197 — Diff view

Status: 🚧 planned. Source: [roadmap D197](../roadmap.md), milestone 3 of the
[version control plan](vcs.md).

## Outcome

Knot can compare two texts in a center tab. A diff tab shows an original and a
modified side, side by side or inline, with changed lines banded in the
`diff.*` colors. The tab stays current while the modified document is edited
in it, and commands move between hunks. Extensions open diffs through an API,
and version control uses it to show a file's changes from a command and from
the change gutter. D174, D214, D218, and D219 build on this view.

## Design

These choices were agreed while brainstorming. The checkpoint that implements
each one records it in `docs/decisions/`.

- **Native tab kind.** A new center-only surface, `TabSurfaceId::Diff`, which
  `TabKind::allows` admits only in the center. It presents a native `DiffView`
  composed of ordinary `EditorView`s: two side by side, or one in inline mode.
  It is not a contributed text frame, because it holds live documents and has
  its own scrolling and painting.
- **Native code keeps hunks current.** The view diffs its sides itself with
  `knot_model::diff::line_hunks`, the comparison behind the extension API's
  `diff`, after a short debounce and off the foreground. It keeps the result
  only if both sides' revisions still match, so no package round trip is
  needed per edit. Rejected: a package that diffs and pushes hunks to the
  view. It adds latency and state for no policy gain.
- **Sides.** Each side is either a *document*, the live buffer of an
  application document, or a *snapshot*: a private read-only `BufferModel`
  with a label and a language for highlighting, outside document identity, as
  text frames are. A document side is editable and shares the document's
  buffer and history. The diff tab counts as a view of that document for
  protected closure, so closing the document's last view prompts as usual.
- **Side-by-side alignment follows IntelliJ.** There are no filler rows. A
  band between the two editors draws a connector from each hunk to its
  counterpart, and scrolling one side scrolls the other through the hunk line
  mapping: lines outside hunks map one to one, and lines inside a hunk map
  proportionally. Rejected: VS Code's filler rows, which need virtual rows in
  both editors just to keep sides level.
- **Inline mode uses virtual block rows.** Inline mode shows the modified
  document in one editor, with each hunk's original lines painted read-only
  above it as rows in the wrap map that belong to no buffer line. Folds
  already make some lines zero rows, and this is their counterpart. The
  document text stays editable.
- **Not restored.** Diff tabs are left out of hot exit, window layouts, and
  Reopen Closed Tab. Native code cannot recreate a snapshot without the
  package that supplied it.
- **The extension API** opens a diff from two sides and returns a
  lifecycle-owned handle. The handle can replace a snapshot's text, for
  example with a new baseline, and close the tab. The tab closes when its
  owner unloads.

## Exclusions

- Changes inside a line (word or character highlighting). The comparison
  reports none, and this plan adds none.
- Three-way merge (D219), reverting hunks from the diff (D221 reverts from its
  popup), and staging (D216).
- Directory or multi-file diffs. D214 opens one file's diff at a time.
- Ignore-whitespace and other comparison options in the view's UI.

## Deferred decisions

- The exact API names and option shapes, settled in checkpoint 4 against the
  existing `documents` and `workbench` namespaces.
- Key bindings for hunk navigation and the layout toggle, settled in
  checkpoint 6 next to `vcs.nextChange`'s Alt+F5.
- Whether a diff tab can be a preview tab, settled in checkpoint 7, when the
  version control entry points show how often a diff is opened in passing.

## Checkpoints

Each checkpoint is one reviewable commit. Product tests open diffs through an
internal product operation until checkpoint 4 adds the extension API.

### 1. Diff tab kind with two snapshots ✅

Done and approved. The diff tab kind, `DiffView`, and its placement rules are
described in [workbench and lifecycle](../architecture/workbench-and-lifecycle.md)
and [its decision](../decisions/workbench-and-lifecycle.md#diff-view). Commands
on a diff tab target the modified side's editor. Tests open a diff through
`Workbench::open_diff_in_center`, which stays test-only until checkpoint 4.
There was nothing to try in the real app, since no entry point exists yet.

### 2. Hunk bands ✅

Done and approved. A diff tab diffs its sides off the foreground when it opens
and bands each hunk on both sides in `diff.added`, `diff.modified`, and
`diff.removed`. A side with no lines in a hunk gets a deletion wedge in the
gutter, so `ViewDecoration` can carry a gutter mark. The bands come from
`knot_model::diff::hunk_decorations`. Approved on the tests alone, since no
entry point opens a diff in the real app before checkpoint 4; the look in the
default themes is first visible at the next checkpoint that opens one.

### 3. Editable document side

Deliverables:

- A side can be a document: a fresh, editable `EditorView` of its shared
  buffer.
- Rediff after each commit of either side, debounced, and kept only if both
  revisions still match.
- Closure counts a diff tab with a document side as a view of that document,
  so closing its last view prompts and hot exit protects it.

Validation: model tests for closure counting and for the debounce and
revision checks; product tests that edit in the diff tab and see the bands
follow, edit the document in its own tab, and close a dirty final view through
the diff tab.

Review gate: closure behavior and how rediffing feels while typing.

**Implementation context**

- `knot_model::workbench::closure` decides which views a close covers.
- Document views come from `src/app/editor/view.rs` and `src/app/editor.rs`;
  the vcs gutter's 150 ms debounce in `extensions/@knot/vcs/src` is the
  timing to match.

### 4. Extension API

Deliverables:

- Opening a diff from two sides with destination options, returning a
  lifecycle-owned handle with `setText` for a snapshot side and `close`. The
  tab closes when its owner unloads.
- TypeScript declarations, and the
  [extension API guide](../extension-api/application.md#compare-texts)
  extended next to `diff`.

Validation: extension tests for argument validation, `setText`, `close`, and
owner unload.

Review gate: the API shape.

**Implementation context**

- Declarations in `crates/knot-protocol/src/declarations.rs`, handlers in
  `src/app/extensions/host/handlers.rs`, and types in
  `crates/knot-host/src/js/knot.d.ts`. Lifecycle-owned teardown follows the
  existing registrations released on host release.

### 5. Synced scrolling and connectors

Deliverables:

- The hunk line mapping: lines outside hunks map one to one, lines inside a
  hunk proportionally.
- Synced vertical scrolling in both directions, without feedback loops; caret
  moves that scroll one side scroll the other.
- A connector band between the editors joining each hunk's line ranges in the
  `diff.*` colors.

Validation: model tests for the mapping (inside, between, and at the edges of
hunks, and empty sides); product tests for synced scrolling.

Review gate: how scrolling feels on long files with large hunks.

**Implementation context**

- Scroll state is per editor in `crates/knot-model/src/editor.rs`; scrolling
  and painting live in `src/app/editor/layout.rs` and
  `src/app/editor/paint.rs`. Put the mapping beside the diff view's hunks in
  `knot-model`.

### 6. Hunk navigation

Deliverables:

- Native commands in the diff view's scope for the next and previous hunk,
  wrapping at the ends, with key bindings.
- An open option that places the caret at a given line.

Validation: product tests for navigation and opening at a line.

Review gate: command names and keys.

**Implementation context**

- Follow the native editor commands in `src/app/editor/native_commands.rs`
  and the command catalog's view scopes
  ([commands](../architecture/commands.md)). Mirror `vcs.nextChange`'s
  wrapping in `extensions/@knot/vcs/src/navigation.ts`.

### 7. Version control entry points

Deliverables:

- A `vcs.showChanges` command that opens the focused file's diff, the
  baseline as a snapshot labeled with the backend's ref against the document.
  A file without a baseline shows nothing.
- Change-gutter marks carry a command that opens the file's diff at that
  hunk. D221 replaces this click with its popup.
- Open diffs follow baseline refreshes through `setText`, and close when the
  document closes or the baseline is gone.
- The preview-tab decision.

Validation: `extensions/@knot/vcs/test` cases for the command, the gutter
command, and refresh; a `crates/knot-ext-vcs/tests` case over a temporary
repository that opens a diff and commits.

Review gate: the entry points in daily use, and whether a second request
reuses the open diff tab for the same file.

**Implementation context**

- `extensions/@knot/vcs/src/hunks.ts` builds the gutter contributions; add the
  contribution `command` there. `main.ts` and `navigation.ts` show how
  commands find the focused editor's cached baseline, and `ref.ts` has the ref
  label.
- Document [version control](../architecture/version-control.md#gutter) and
  [packages](../extension-api/packages.md) as the behavior lands.

### 8. Virtual block rows

Deliverables:

- Read-only rows in the wrap map anchored above a buffer line, carrying their
  own text and highlighting and no caret, skipped by caret movement,
  selection, and hit-testing. They wrap with the editor and hide inside a
  fold. No consumer yet beyond tests.

Validation: model tests for the wrap map with block rows (row counts,
row-to-line mapping, folds, wrapping, edits that move the anchor); editor
tests for movement, selection, and hit-testing around them.

Review gate: whether block rows are general enough for later consumers.

**Implementation context**

- `crates/knot-model/src/editor/wrap.rs` owns rows. Folds as zero-row lines,
  tested in `crates/knot-model/src/editor/tests/folding.rs`, are the pattern
  to mirror. Painting and hit-testing are in `src/app/editor/paint.rs` and
  `src/app/editor/layout.rs`.

### 9. Inline layout

Deliverables:

- The diff view's inline layout: the modified side in one editor, with each
  hunk's original lines as block rows banded in `diff.removed`.
- A `diff.layout` setting (`sideBySide`, `inline`) and a command that toggles
  an open view.

Validation: product tests for toggling the layout, editing in inline mode,
and hunk navigation there.

Review gate: the inline view's look.

**Implementation context**

- The diff view from checkpoints 1–6 and the block rows of checkpoint 8. Add
  the setting to [settings](../settings.md).

### Completion

When checkpoint 9 passes review: describe the diff view in a subsystem
reference (likely [editor and languages](../architecture/editor-and-languages.md)
or a new one), record the native-tab, native-rediff, alignment, and block-row
decisions in `docs/decisions/` with index lines, mark D197 ✅ in
[vcs.md](vcs.md), remove D197 from the roadmap, and delete this plan.
