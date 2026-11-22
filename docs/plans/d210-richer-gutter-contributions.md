# D210 — Richer gutter contributions

Status: 🚧 in progress; checkpoints 1 and 2 are done. Source: [roadmap D210](../roadmap.md), part
of the [version control plan](vcs.md).

## Outcome

A package can mark lines in the gutter with a dot, a bar that spans a range's
lines, or a deletion wedge between lines, each in a theme color key, and a
click on any of them runs the contribution's command against the clicked
editor with the contribution's current range. The change gutter (D211) and
language-server diagnostics (D067) build on it, and the hunk popup (D221)
opens from the click. Native code stays VCS-agnostic: it paints marks and
defines generic `diff.*` color keys, never version control semantics.

## Design

- **Two fixed native lanes for marks.** The gutter becomes markers, line
  numbers, a new *bar lane*, then folds. The bar lane is narrow and always
  reserved, like the marker lane, so text never shifts when a change appears;
  it sits right of the numbers, where VS Code, IntelliJ, and Zed put change
  bars, so a diagnostic dot and a change bar on one line both show. This
  revises the [editor gutter decision](../decisions/editing.md#editor-gutter)
  ("one marker lane… multi-line bars wait for a consumer"). Rejected: bars in
  the shared marker lane, where a change would hide a diagnostic on the same
  line.
- **Contribution shape.** `gutter: { color, style } | null`, mirroring
  `decoration`, with `style`:
  - `dot`: the range's first line, in the marker lane (today's marker).
  - `bar`: every line of the range, all its wrapped rows, in the bar lane. A
    range ending at column 0 of a later line leaves that line out, as `line`
    decorations do.
  - `deletion`: a small wedge on the boundary above the range's start line,
    in the bar lane; at the end of a text with no line after the boundary it
    paints below the last line. A zero-width range is the usual form.

  The bare color key form goes; nothing uses it.
- **One mark per line per lane.** Each lane keeps the most severe mark of a
  line and the latest among equals, as markers do today. A `bar` claims every
  line it spans; a `deletion` claims its start line.
- **Click actions.** `command: string | { name, arguments } | null`, as key
  bindings accept, with `arguments` an object. A click on a dot, bar, or
  deletion wedge, or on an inline contribution's text, runs the command
  targeting the clicked view, so `context.editor` and `context.buffer` are
  that editor and its buffer, and the host adds `range`, the contribution's
  current anchored range, to the arguments. Clicking never selects. Marks with
  a command show the pointing hand.
- **Generic diff color keys.** Native `diff.added`, `diff.modified`, and
  `diff.removed`, defined in the native color table with defaults per
  appearance, because comparing texts is an editor feature (D209) shared by
  the change gutter and the diff view (D197). `@knot/vcs` paints with them
  rather than defining its own.

## Exclusions

- Hover tooltips on marks: roadmap D225, after the version control plan.
- The popup a click opens (D220, D221) and next or previous change (D211).
- More lanes, extension-defined lanes, or mark widths and shapes beyond the
  three styles.
- Overview-ruler or scrollbar marks.

## Deferred decisions

- Whether a collapsed fold's line shows the marks of its hidden lines. Hidden
  lines paint nothing in this plan; D211 revisits it if the change gutter
  needs it.
- Whether view-owned decoration sets (`editor.setDecorations`) gain gutter
  marks; no consumer needs them.

## Checkpoints

### 1. Marks in the model and the extension API ✅

Done and approved. Contributions carry `gutter: { color, style }` and
`command: { name, arguments }` (`knot.d.ts`); the editor projects dots into the
marker lane and bars and deletions into a bar lane
(`project_contributions` in `crates/knot-model/src/editor.rs`, read through
`gutter_markers()` and `gutter_bars()`). A deletion carries `below` when its
range starts at the end of a text whose last line has content, so checkpoint 2
paints it under that line.

### 2. The bar lane and its painting ✅

Done and approved. The gutter is markers, numbers, an 8px bar lane, then folds
(`GutterLayout` in `src/app/editor/gutter.rs`); `bar_marks` picks the visible
bar-lane marks and their rows, and `paint.rs` draws bars and wedges and inserts
their hitboxes. A click on a bar-lane mark emits `EditorContributionAction` and
selects nothing; nothing dispatches it yet. Native `diff.added`, `diff.modified`,
and `diff.removed` default to `success`, `info`, and `error`, and each bundled
theme sets them.

### 3. Click actions run commands

Deliverables: a click on any mark or inline contribution dispatches its
command through the product's command path, targeting the clicked editor, with
`range` added to its arguments; the extension test harness does the same.

Validation: product tests that a JavaScript command registered by a test
extension receives `context.editor`, `context.buffer`, and the current range
after edits moved it, from a dot, a bar, a deletion wedge, and inline text;
a text frame click runs with a null editor; clicks select nothing; an
unregistered command reports as other unknown commands do.

Review gate: the command context and arguments a click produces.

**Implementation context.**
- Input: `src/app/editor/input.rs` resolves the clicked mark per lane
  (`GutterLane::Markers` and the new `Bars`) and emits
  `EditorContributionAction` (`src/app/editor.rs:77`), which today only a test
  subscribes to; carry the command's name and arguments in it.
- Dispatch: subscribe where editor views are created
  (`src/app/workbench.rs`, `src/app/text_view.rs`) and route to
  `Product::dispatch_command` (`src/app/product.rs`) with a
  `ProductCommandTarget` for that view's tab, built as
  `capture_command_target` does, so the existing editor context reaches the
  handler. Merge `range` into the arguments as a `ByteRange` value.
- Tests: `src/app/product/tests/` with a test extension, following the
  existing extension-command product tests.

### 4. Documentation and decision

Deliverables: the guide, architecture, and decision text for the new gutter.

Validation: every relative link and anchor the changed docs touch resolves;
the guide's examples match `knot.d.ts`.

Review gate: the documentation text; then record the decision.

**Implementation context.**
- Guide: the contribution example and gutter text in
  `docs/extension-api/languages-and-themes.md` (its `defineColor` example uses
  `diff.add`, which should become a package-owned key, and its contribution
  example should show a `bar`), and the click command context in
  `docs/extension-api/commands-and-keys.md`.
- Architecture: the lane order and click behavior in
  `docs/architecture/editor-and-languages.md#editor-views`, and the marker
  selection paragraph in `docs/architecture/semantic-ui.md`.
- Decision: rewrite `docs/decisions/editing.md#editor-gutter` for two mark
  lanes with the rejected shared lane, and its index line in
  `docs/decisions.md`; record the native `diff.*` keys there too. Update the
  D210 entry in [vcs.md](vcs.md) to done with a link.
