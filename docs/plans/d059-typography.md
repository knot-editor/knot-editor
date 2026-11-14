# D059 — Professional typography

Status: 🚧 planned. Source: [roadmap D059](../roadmap.md). Covers the font part
of D045; D045 keeps the non-font terminal preferences.

## Outcome

Editors, text frames, text inputs, and terminals draw text in a font the user
chooses through settings. Font family, fallbacks, weight, line height,
ligatures, and OpenType features can differ per surface, such as
`{ view: "terminal" }`, or per language, since the settings store already
resolves those scopes. Ligatures form in editors, and in terminals for runs of
punctuation, such as `->`, `=>`, `!==`.

## Design

### Settings

New native keys, resolved per scope exactly like `font.size`:

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `font.family` | string | `"Menlo"` | Primary family. |
| `font.fallbacks` | strings | `["Geeza Pro", "PingFang SC", "Apple Color Emoji"]` | Families tried for glyphs the primary lacks. |
| `font.weight` | number | `400` | Base weight, clamped to 100–900. Bold syntax styles and bold terminal cells stay at 700. |
| `font.lineHeight` | number | `1.2` | Multiple of the font's own height (ascent + descent), clamped to a sane range such as 0.8–3. Rounded to whole physical pixels. |
| `font.ligatures` | boolean | `true` | When false, disables `calt` and `liga`. |
| `font.features` | strings | `[]` | OpenType features applied after `font.ligatures`: `"ss01"` (on), `"-calt"` (off), `"cv05=2"` (value). Malformed entries are ignored. |

- Line height uses one meaning on every surface. The editor moves from
  font size × 20/14 to the font's own height × `font.lineHeight`, which keeps
  Menlo 14 at about 20px. Terminal rows get somewhat taller by default; a
  tight grid is `1.0` scoped to `{ view: "terminal" }`.
- Variable-font axes are out: gpui's `Font` has no axis support.

### Model and application split

- `knot-model` owns the definitions and a plain `FontSettings` value resolved
  from the store for a scope: family, fallbacks, weight, line-height factor,
  and the final feature list (ligature toggle merged with parsed
  `font.features`). Parsing, clamping, and merging have plain tests.
- `src/app/typography.rs` turns `FontSettings` into a gpui `Font` (and its
  bold/italic variants) and computes the line height from resolved font
  metrics. It replaces the hardcoded `text_font()`; `TEXT_SIZE` stays only
  as the reference size for chrome scaling and text inputs.
- Surfaces and their scopes:
  - editors and gutters: the editor's existing settings scope;
  - terminals: `{ view: "terminal" }`;
  - text inputs (palette, pickers): the unscoped family, fallbacks, weight,
    and features, keeping their fixed size and line height;
  - chrome drawn by gpui-component keeps the system UI font.
- Views already observe the settings store and repaint; each view builds its
  `Font` once per layout and clones it into runs.

### Terminal ligatures

The terminal shapes and clips each cell separately, so no ligature can form.
Paint instead groups consecutive cells on one line that are all ASCII
punctuation, single-width, not hidden, and identically styled (font
flags, colors, underline, strikethrough, hover) into one shaped run, clipped
to the run's cells. All other cells stay per cell. Runs are short, so the
difference between the font's advance and the rounded cell width stays well
under a pixel and the grid stays exact. Letter ligatures, such as Fira Code's
`www` or `0x`, do not form in terminals. With ligatures disabled for the
terminal scope, every cell is shaped alone as today.

## Exclusions

- Variable-font axes, proportional fonts in editors (wrap and indentation
  still measure monospace cells), and per-surface fonts for gpui-component
  chrome.
- Splitting a ligature visually for a caret placed inside it: the caret goes
  wherever gpui places that index within the shaped cluster.
- Notifications for a family that is not installed: the platform falls back.
- Non-font terminal preferences, which stay in D045.

## Deferred decisions

- Whether text inputs ever get their own size and line height settings.
- Whether a decision entry is warranted: likely only for the line-height
  meaning (font height multiple over font size multiple) and the
  punctuation-only terminal ligature runs, recorded once validated.

## Checkpoints

### 1. Font settings drive every text surface — ✅

Completed. The `font.*` family, fallbacks, weight, ligature, and feature
settings resolve into `FontSettings` per scope and drive editors, gutters,
terminals, and text inputs. Defaults follow each platform's code font. See
[settings](../settings.md), [editor views](../architecture/editor-and-languages.md#editor-views),
and the terminal geometry paragraph of
[workbench and lifecycle](../architecture/workbench-and-lifecycle.md). The test
platform's mock text system has fixed advances, so measured widths per family
were validated by hand.

### 2. Configurable line height — 🔍 awaiting review

Deliverables:

- ✅ `font.lineHeight` defined; editor rows and terminal cells use the font's
  own height × factor, rounded up to whole physical pixels, with the baseline
  centered in the row.
- ✅ A line-height change keeps the editor's top line in place and resizes the
  terminal grid.
- ✅ Docs updated for the new editor line-height rule.

Validation:

- ✅ Model tests for default and clamping.
- ✅ App tests: editor metrics for a given factor; terminal `from_metrics`
  computes cell height and baseline for factors above and below 1; a scoped
  terminal value leaves editors unchanged.
- ✅ `just check`.

Review gate (pending): `Try:` commands that set line height globally and for terminals
only.

Implementation context:

- Editor: `EditorMetrics::new` in `src/app/editor/layout.rs` computes
  `line_height` from size alone and `update_metrics` has no window; move the
  computation to where a `Window` is available (as `update_wrap` already
  does) to resolve ascent and descent, or pass them in. `EditorView::new` in
  `src/app/editor/view.rs` (~123) seeds the default metrics.
- Terminal: `TerminalLayout::from_metrics` in `src/app/terminal/layout.rs`
  already computes cell height and baseline from ascent and descent; apply
  the factor there and keep its unit tests.
- Docs: the "line height follows it at 20/14" sentence in
  `docs/architecture/editor-and-languages.md`.

### 3. Ligatures in terminals — ⬜

Deliverables:

- Terminal paint groups eligible punctuation cells into shaped runs as
  described above, when ligatures are enabled for the terminal scope.
- Terminal architecture notes the punctuation-run rule.
- Decision entry for the validated choices, if still warranted.

Validation:

- Unit tests for the pure grouping function: punctuation runs merge; a style,
  color, hover, or wide-cell boundary splits them; letters and spaces stay
  single; disabled ligatures yield single cells.
- `just check`.

Review gate: in a terminal with a ligature font, `echo '-> => != === <= |>'`
renders ligatures aligned to the grid, colored prompts and selections still
look right, and disabling ligatures for the terminal scope reverts to single
glyphs.

Implementation context:

- The cell loop in `src/app/terminal/view.rs` (~1050) shapes one
  `TextRun` per cell inside a cell-sized content mask. Extract a pure
  function over a line's cells returning column ranges to shape together;
  paint each range as one `shape_line` clipped to `cell_width × len`.
- Cell style equality covers the fields that feed the `TextRun`: bold,
  italic, foreground, underline flags, strikethrough, and hovered-link
  membership.
