# Terminal text parity with Ghostty

Status: checkpoint 1 complete; checkpoint 2 is next. No roadmap item.

## Goal

The terminal looks as sharp as Ghostty with the same font: same row height,
glyphs snapped to the emulator cell grid instead of soft, drifting runs,
and matching regular and bold glyph weight.

## Design

- **Checkpoint order is line height first, then grid.** Row height is the
  independent variable; grid painting builds on the measured cell.
- **Terminal line height defaults to 1.0.** The shared `FontSettings`
  default stays 1.2 for editors; only the `{ view: "terminal" }` scope
  resolves `font.lineHeight` to 1.0 unless the user sets it. This matches
  Ghostty's stock look without touching the editor.
- **Per-cell painting with fixed monospace advances.** Each cell's glyph
  paints at its own snapped cell origin; nothing shapes across cell
  boundaries except the existing ASCII-punctuation ligature runs, which
  stay clipped to their cells. Letter ligatures never span cells.
- **Boundary.** `knot-model` owns the scoped default; `src/app/typography`
  and `src/app/terminal/layout.rs` own measurement; `src/app/terminal/view.rs`
  owns painting. No emulator (`knot-terminal`/libghostty) change.

## Exclusions

- Palette or color changes, font fallback changes, and new `font.weight`
  semantics; checkpoint 3 matches Ghostty's rendering within the current
  semantics (base weight 100-900, bold at 700) and may revisit AA/gamma
  handling to do it.
- Editor, text input, or picker typography.
- A separate `cell-height`/`cell-width` setting; `font.lineHeight` remains
  the knob.

## Checkpoint 1 — Terminal line height 1.0 ✅

Done: stock terminals measure rows at 1.0 × (ascent + descent) while
editors stay at 1.2; an explicit `font.lineHeight` at any matching scope
wins. See `font.lineHeight` in [settings](../settings.md) and the terminal
geometry in [workbench and lifecycle](../architecture/workbench-and-lifecycle.md).
Review gate passed: `just check` green, and side-by-side `ls` output in
Ghostty and Knot measures the same row height.

## Checkpoint 2 — Per-cell grid painting

Deliverable: every non-ligature cell paints its glyph at its own snapped
cell origin with the cell's fixed advance; columns stay aligned and glyphs
sit on-pixel at 1x/2x.

- Tighten the paint path in `view.rs` so runs never carry proportional
  advances across cells; keep `shaping_runs` joining only identical-look
  ASCII punctuation on one line, clipped to its cells.
- Validation: view/layout tests for column alignment (mixed widths, wide
  glyphs, fallback glyphs, bold/italic/color boundaries), ligature
  positive/negative cases, and underline/strikethrough clipping; `just
  check`.
- Review gate: `just check`; a `try.*` command in the personal
  `post-init.js` opens a terminal on a fixed `ls`/box-drawing fixture for
  side-by-side comparison with Ghostty.

**Implementation context.** `src/app/terminal/view.rs` (`shaping_runs`
~line 888, paint loop ~line 1234, `text_origin`), `src/app/terminal/layout.rs`
(`origin`, `text_origin`, `cell_width` snapping), tests in
`src/app/terminal/view/tests.rs` (incl. `mod shaping_runs` ~line 321).
Personal config root is `user_config_root` in
`src/app/personal_config.rs`.

## Checkpoint 3 — Regular-weight parity with Ghostty

Deliverable: normal (non-bold) terminal glyphs render at the same visual
weight as Ghostty with the same font, and bold cells stand out by the same
margin.

- Investigate the cause first: weight selection (`font.weight` into the
  platform weight, `Font::bold()` mapping), synthetic emboldening anywhere
  in the text stack, and rasterization differences (antialiasing, gamma).
  Fix the actual cause; `font.weight` keeps its current meaning.
- Validation: typography/view tests locking the investigated behavior;
  `just check`.
- Review gate: `just check`; zoomed side-by-side screenshots of the same
  fixture as checkpoint 2 in Ghostty and Knot, comparing regular and bold
  stem weight.

**Implementation context.** `src/app/typography.rs` (`font`, `bold`),
`src/app/terminal/view.rs` (paint loop ~line 1225, run font selection),
the gpui text system (`font-kit`).

## Deferred

- Further subpixel-AA/gamma tuning if parity still looks off after the grid
  and weight work lands; palette and fallback-font follow-ups.
