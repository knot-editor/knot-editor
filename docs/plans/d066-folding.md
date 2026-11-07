# D066 — Folding

Status: 🚧 checkpoint 1 complete; checkpoints 2–5 not started. Item D066 of the [editor API](editor-api.md) plan
(phase 4), from the [roadmap](../roadmap.md). Depends on D109 and D112, which
landed.

## Outcome

Editor views fold and unfold line regions, as VS Code does. Regions come from a
per-language `folds` query, with an indentation fallback for languages without
one; extensions can also fold arbitrary ranges through the editor handle. A
fold lane in the gutter and an inline placeholder show and toggle folds, and
Vim mode gains its standard fold keys. Folds survive edits and restarts.

## Design

- **Fold state is per view.** An `Editor` holds its collapsed folds as
  anchored ranges (`AnchoredRangeStore`, as the auto-closed bracket pairs
  do), so edits above or inside keep them in place. A collapsed fold hides the
  lines after its first line through its last; the first line stays visible
  and shows a `…` placeholder after its text. Nested folds collapse
  independently.
- **Rows.** Hidden lines are zero-row lines in `WrapMap`, so `viewport`,
  `rowAt`, `rowOf`, `scrollBy`, scroll clamping, and painting count only
  visible rows with no second mapping. Line numbers still show buffer lines,
  so folded ranges skip numbers.
- **Regions.**
  - `folds` query in `LanguageQueries` beside `indents`, compiled with the
    grammar; `@fold` captures give a node's lines (first line to last line,
    dropping a trailing closing-delimiter line as the query author chooses via
    the captured range). Regions of the deepest layer at a line win for
    injections.
  - Languages with no `folds` query (and plain text) fall back to
    indentation: a line followed by more-indented lines starts a region.
  - Regions are computed per syntax generation and cached; they are only
    candidates. Only collapsed folds are state.
  - Bundled language packages (`@knot/language-*`) ship `folds` queries.
- **Commands.** Fold, Unfold, Toggle Fold act on the region starting on the
  caret's line, else the innermost enclosing region. Fold All, Unfold All
  complete the set. Registered natively beside the other editor commands and
  bound as VS Code does where the keymap has room.
- **Editor handle.** `editor.fold(range)`, `editor.unfold(range)`,
  `editor.folds()` (collapsed ranges), and `editor.foldRegions()`
  (candidates), all in lines. Manual folds are collapsed folds with no
  region behind them.
- **Movement and editing, as VS Code.**
  - Up/Down and page movement step over folds by rows. A caret that would sit
    in hidden text moves to the fold's first line; a selection that ends in a
    fold extends over the whole fold.
  - Anything that reveals a position in hidden lines (Go to Line, Find,
    bracket jump, `reveal`, an edit by extension, undo) unfolds the folds
    hiding it. Typing cannot reach hidden text.
  - Deleting or cutting a selection that includes a collapsed fold's first
    line removes the hidden lines too, since they are selected as text.
- **Gutter.** A third lane after the numbers: chevrons for foldable lines
  appear while the pointer is over the gutter; collapsed folds always show
  one. Clicking toggles; Alt-click toggles recursively. Clicking the
  placeholder unfolds. The lane honors a setting `editor.folding`
  (default on) resolved per scope, plus `editor.showFoldingControls`
  (`"mouseover"` | `"always"`).
- **Persistence.** Restorable view state records collapsed folds as byte
  ranges, clamped to the text like the other offsets and dropped when empty.
- **Theme.** `editor.foldPlaceholder` color key (native default).

## Exclusions

- Public syntax-tree queries and text objects (D144).
- Fold-aware search highlighting beyond unfolding on reveal.
- Folding markers in comments (`#region`) and fold-by-level commands.
- Fold state shared between views of one document.

## Deferred decisions

- Whether `folds` queries later share captures with D144's `textobjects`.
- Whether indentation fallback becomes a per-language opt-out.

## Checkpoints

### 1. Fold state, rows, and the editor handle ✅

Complete: per-view anchored folds (`src/app/editor/folding.rs`), hidden lines
as zero-row lines in `WrapMap`, the `…` placeholder with the
`editor.foldPlaceholder` color, caret stepping and reveal-on-hidden, line
commands over a fold, and `editor.fold`/`unfold`/`folds` on the editor handle.
Behavior is in [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#editor-views)
and the [extension guide](../extension-api.md). Clicking the placeholder does
not unfold yet; that is checkpoint 3. The fake test host keeps folds but still
counts rows as lines.

### 2. Regions, commands, and bundled queries

Deliverables: `folds` query compiled with the grammar and exposed in language
registration (`queries.folds`), indentation fallback, region cache per syntax
generation, `editor.foldRegions()`, Fold/Unfold/Toggle/Fold All/Unfold All
commands and default keys, `folds` queries for Rust, TypeScript, Python,
TOML, and Markdown (sections).

**Implementation context:** `src/app/languages.rs` (`LanguageQueries`,
`compile`), `src/app/syntax.rs` and a new `src/app/syntax/fold.rs` mirroring
`syntax/indent.rs` (capture names, tests), `src/host/js/bootstrap.js` near
the `indents` handling, `docs/schemas/knot.schema.json`, `knot.d.ts`,
`extensions/@knot/language-*/src/queries/` and `languages.ts` with query test
cases like `test/cases/indents.jsonc`, `src/app/editor/native_commands.rs`
and `src/app/commands/` for registration.

Validation: query tests per language; fallback tests; command tests through
the real editor.

Review gate: fold real code in each bundled language; judge region
boundaries.

### 3. Gutter lane, placeholder interaction, and settings

Deliverables: fold lane with hover chevrons and always-visible collapsed
markers, click and alt-click, placeholder click, `editor.folding`,
`editor.showFoldingControls`, `editor.foldPlaceholder` color key, docs.

**Implementation context:** `src/app/editor/gutter.rs` (`GutterLane`,
`GutterLayout`, `lane_at`, add a lane), `paint.rs`, `input.rs` (pointer
handling per lane), `src/app/settings.rs` (mirror `editor_line_numbers`),
`src/app/theme.rs`/`themes` for the color key, settings schema.

Validation: gutter layout tests, pointer tests in editor tests, settings
scope resolution.

Review gate: use the lane by mouse in a real file in light and dark themes.

### 4. Vim folding keys

Deliverables: `@knot/vim` gains the standard fold keys on the public handle
and commands only:

- Create and delete: `zf{motion}`, `zF` (count lines), visual `zf`, `zd`,
  `zD`, `zE`.
- Open and close: `zo`, `zO`, `zc`, `zC`, `za`, `zA`, `zv`, `zx`/`zX` as
  reveal-caret.
- All folds: `zR`, `zM`, `zn`, `zN`, `zi` (fold enable toggle).
- Navigation: `zj`, `zk`, `[z`, `]z`.
- Counts work as in Vim; `j`/`k`/`G` and the existing row scrolls (`H M L`,
  `zz`) already count visible rows from D112; `dd`, `yy`, and `>>` on a
  closed fold act on the whole fold.

**Implementation context:** `extensions/@knot/vim/src/keys.ts` (the `z`
prefix near line 252), `normal.ts` (operators and line motions),
`extensions/@knot/vim/test/cases/*.jsonc` (add `folding.jsonc` and wrapped
cases), plus any primitive gaps closed as generic handle APIs (a fold-aware
line-range query), documented in `docs/extension-api.md`.

Validation: Vim end-to-end cases through the real editor covering each key,
counts, nested folds, and a closed fold under `dd`.

Review gate: a `Try:` post-init config enabling Vim; exercise each key on a
real source file.

### 5. Persistence and documentation

Deliverables: folds in `EditorViewState` and the workbench restoration
paths, architecture updates (workbench and lifecycle editor views, semantic
UI languages and `folds` queries), a decisions entry (fold state per view,
zero-row hidden lines, indentation fallback), and removal of this plan and
the "Folding does not exist yet" note. Mark D066 ✅ in the editor API plan
and update the roadmap's D144 text.

**Implementation context:** `src/app/editor.rs` (`view_state`,
`restore_view_state`), the tab view-state record in
`docs/architecture/workbench-and-lifecycle.md` (line 233), `docs/decisions.md`
(`#editor-handles` neighbor), `docs/plans/editor-api.md`.

Validation: view-state round-trip tests including clamping after the file
shrinks.

Review gate: close and reopen a window with folds in several tabs.
