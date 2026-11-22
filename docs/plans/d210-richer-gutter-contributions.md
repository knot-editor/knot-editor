# D210 — Richer gutter contributions

Status: 🚧 approved, not started. Source: [roadmap D210](../roadmap.md), part
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

### 1. Marks in the model and the extension API

Deliverables: the new `gutter` and `command` shapes in the protocol, the host
binding and declarations, and the buffer's contribution store; projection of
contributions into per-lane rendered marks (dots by first line, bars and
deletions by line) with per-lane severity and latest-wins selection; dots keep
painting as today.

Validation: model tests for projection (multi-line bars, column-0 ends,
deletions at a line start and at the end of the text, competition within and
across lanes, edits moving and consuming ranges); marshal and binding
rejection of malformed `gutter` and `command` values; `knot.d.ts` type checks.

Review gate: the API declaration and the projection rules.

**Implementation context.**
- Protocol: replace `EditorContribution.gutter: Option<String>` with
  `Option<GutterMark { color, style: GutterStyle }>` and `command` with a
  `{ name, arguments }` value beside `Decoration` in
  `crates/knot-protocol/src/lib.rs`; reuse `Command`'s argument value type.
- Host: decode in `crates/knot-host/src/engine/marshal.rs` (around the
  `gutter` property, line ~730) and validate in
  `crates/knot-host/src/js/bootstrap.ts` (the contribution validator near line
  381), normalizing a string command to `{ name, arguments: {} }`; declare
  `GutterMark`, `GutterStyle`, and the command form on `EditorContribution`
  in `crates/knot-host/src/js/knot.d.ts`.
- Model: carry the new fields through `ContributionMetadata`,
  `replace_contributions`, and `ResolvedEditorContribution` in
  `crates/knot-model/src/buffer.rs`. In `project_contributions`
  (`crates/knot-model/src/editor.rs`), split `RenderedGutterMarker` into lane
  marks (a `style` and line), reusing the `line` decoration's column-0 rule
  for bars and `decoration_precedence` for selection; update
  `EditorState::gutter_marker` in `crates/knot-model/src/editor/gutter.rs` to
  look up by lane. Tests go in
  `crates/knot-model/src/editor/tests/decorations.rs`.
- Fix existing callers in `src/app/extensions/buffers.rs` and the extension
  test harness in `scripts/extensions/testing/`.

### 2. The bar lane and its painting

Deliverables: the bar lane in the gutter layout, painting of bars across
wrapped rows and of deletion wedges on line boundaries, the native `diff.*`
color keys, and hit testing that maps a bar-lane click to its mark.

Validation: gutter layout tests for the lane's position and fixed width with
numbers on and off and folding on and off; paint-input tests that bars cover
wrapped rows and skip hidden lines and that wedges land on boundaries,
including the end of the text; theme tests for the new keys' defaults; a
visual check in the running app with a test contribution set.

Review gate: the lane's width and look under the default themes.

**Implementation context.**
- Layout: add `GutterLane::Bars` and a `bars_start`/`bars_width` to
  `GutterLayout` in `src/app/editor/gutter.rs`, between `numbers_end` and
  `folds_start`, keeping `lane_at` and `folds_center` consistent; extend
  `src/app/editor/tests/gutter.rs`.
- Paint: extend `GutterPaint` and the gutter block in
  `src/app/editor/paint.rs` (the marker loop near line 661), computing bar
  row spans from `editor.wrap()` as markers use `first_row`; add bar and
  wedge hitboxes beside the marker hitboxes in prepaint.
- Keys: add `DIFF_ADDED`, `DIFF_MODIFIED`, `DIFF_REMOVED` to the native color
  table in `crates/knot-model/src/theme.rs`, beside `SUCCESS` and `ERROR`, and
  values in the bundled themes if they set the neighbouring keys.

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
