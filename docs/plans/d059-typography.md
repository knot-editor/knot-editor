# D059 — Professional typography

Status: ✅ complete. Source: [roadmap D059](../roadmap.md). Covers the font part
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

### 2. Configurable line height — ✅

Completed. `font.lineHeight` sizes editor rows and terminal cells as the
font's ascent plus descent times the factor, rounded up to whole physical
pixels; see [settings](../settings.md),
[editor views](../architecture/editor-and-languages.md#editor-views), and the
terminal geometry paragraph of
[workbench and lifecycle](../architecture/workbench-and-lifecycle.md).

### 3. Ligatures in terminals — ✅

Completed. Terminals shape identically styled punctuation runs together; see
the terminal geometry paragraph of
[workbench and lifecycle](../architecture/workbench-and-lifecycle.md) and the
[typography decision](../decisions/editing.md#typography).
