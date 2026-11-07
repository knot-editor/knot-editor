# D112 — Screen movement and layout queries

Status: 🚧 approved, not started. Item of the
[editor API plan](editor-api.md); builds on D078 (visual rows, done) and D109
(the editor handle, done). Vim's `H M L`, `zz`, and the half-page and page
scrolls already ship on buffer lines (the Vim package) and are
approximate under soft wrap.

## Goal

An extension can place the caret and the scroll position by screen row, so
Vim's screen motions and scrolls are exact on wrapped text. Without wrapping,
rows and lines coincide and nothing behaves differently.

## Design

- **Rows are the handle's unit.** `visibleLines()` is replaced by
  `viewport()`, returning the first wholly visible row and the number of
  wholly visible rows. `scrollBy(n)` counts rows. The extension guide and
  `knot.d.ts` describe a row as one screen line of a possibly wrapped line.
- **Row to position queries.** `rowAt(row)` returns the buffer line and byte
  column where a document row starts, clamped to the last row. `rowOf(offset)`
  returns the row a byte offset shows on, with a caret at a break on the next
  row as the view draws it. Extensions compute `H M L` targets from these, so
  operator targets such as `dH` need no caret move.
- **An always-centering reveal.** `"always-center"` joins `setSelection`'s
  reveals and centers the caret's row even when it is on screen. `"center"`
  keeps centering only an off-screen caret, which Go to Line and Vim's
  jumps rely on.
- **Vim moves to rows.** `H M L`, `zz`, the visual-mode scrolls, and
  `ctrl-d ctrl-u ctrl-f ctrl-b ctrl-e ctrl-y` compute rows from `viewport()`
  and the new queries.

## Exclusions

- A native move-to-screen command; the queries cover it.
- Fold-aware rows, which D066 owns.
- Virtual text and anchored panels (D145).

## Checkpoints

### 1. Row queries and the centering reveal

Deliverables:

- `viewport`, `rowAt`, `rowOf`, row-unit `scrollBy`, and `"always-center"`
  across the host operation, `bootstrap.js`, `knot.d.ts`, and the extension
  guide; `visibleLines` and its handler go away.
- Behavior notes in
  [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#editor-views);
  rationale in [decisions](../decisions.md#editor-handles) only if review
  validates a non-obvious choice.

Validation:

- Editor tests in `src/app/editor/tests/wrap.rs` for the queries and scroll
  under wrapping, with a long line at the top, middle, and bottom of the
  viewport, and at the end of the text.
- Handle tests in `src/app/product/tests/editor_api.rs` for each operation,
  including invalid arguments and a closed editor.

**Implementation context:**

- `src/app/editor.rs`: `fully_visible_lines` becomes the `viewport` read over
  `scroll_top` and `viewport_rows`; `scroll_by_lines` becomes a row scroll,
  dropping its wrapped-line rounding; `set_selection` gains the `Reveal`
  variant beside `Center`, `Top`, and `Bottom`.
- `src/app/editor/wrap.rs`: `position(row)` and `row_of(line, col)` already
  map rows to lines and back; `rowAt` and `rowOf` wrap them with the
  column-to-byte conversion used by `cellsBefore`.
- `src/app/extensions/host/handlers.rs`: mirror `editor_visible_lines`,
  `editor_scroll_by`, and the `cellsBefore` handlers (`LineMeasureArguments`);
  add the variant to the reveal parser near line 2082.
- `src/host/js/bootstrap.js` (editor handle, near `visibleLines`) validates
  arguments; `src/host/js/knot.d.ts` declares the types.

Review gate: the user reviews the API names and row semantics before Vim
moves onto them.

### 2. Vim on rows

Deliverables:

- `extensions/@knot/vim/src/normal.ts`: the `window-*` motions, `reposition`,
  and the scroll commands use rows; `zz` uses `"always-center"` in normal
  mode.
- Cases for wrapped text covering `H M L`, counts, `zz zt zb`, and the
  scrolls, beside the existing no-wrap cases.
- The D058 plan's scrolling note drops its soft-wrap caveat.

Validation:

- `KNOT_CASES` Vim cases through the windowless harness (see
  [development](../development.md)); existing no-wrap cases pass unchanged.

**Implementation context:**

- `extensions/@knot/vim/src/normal.ts` (`window-top/middle/bottom` near
  line 305, `reposition` near line 400, the scroll block near line 359) and
  the `keys.ts` doc comment on `zt/zz/zb`.
- Cases live in `extensions/@knot/vim/test`; product-level Vim tests in
  `src/app/product/tests/vim.rs`.
- A case's `view` sizes the window in lines; wrapped cases also need a
  narrow wrap width, set through `editor.softWrap` in the case.

Review gate: the user dogfoods Vim with soft wrap on, through a `try.*`
command in the personal `post-init.js`, then the plan is removed and
durable behavior moves to the architecture references.
