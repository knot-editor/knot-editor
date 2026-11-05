# D114 — Decoration styles

Status: 🚧 checkpoints 1 and 2 complete; checkpoint 3's documentation
written, awaiting review. Item of the
[editor API plan](editor-api.md); its first consumer is D080, word occurrence
highlighting ([daily-driver plan](daily-driver.md)), planned separately and
right after this item. Find (D031) and Vim search (D058) consume it later.

## Goal

An editor contribution's decoration chooses how it paints, not only its
color: a background behind its text, a background across its whole lines, a
straight underline, or today's wavy underline. Match highlights, diff lines,
and diagnostics then share one anchored contribution path, and every style
follows the theme through its color key.

## Design

- **Contract.** `EditorContribution.decoration` becomes
  `{ color: ColorKey, style: DecorationStyle } | null`, with
  `DecorationStyle = "background" | "line" | "underline" | "wavy"` and
  `style` required. The bare color-key form goes away; nothing outside the
  repository uses it. One contribution has one style; a range that wants two,
  such as a background and an underline, is two overlapping contributions,
  which the store already allows. Themes cannot change a key's style.
- **Underline and wavy** stay text-run layers in
  `EditorView::decoration_layers`, one layer per (key, style) pair in the
  existing order, so the latest layer's underline wins where they overlap
  and `error`, `warning`, and `info` stay on top. A straight underline is
  1 px, wavy keeps 1.5 px.
- **Background** paints as quads behind the text, not as a
  `TextRun.background_color`, which would cover the selection quad. It spans
  the decoration's text on each visual row, through the same row-span
  geometry as the selection, so wrapped and RTL rows work. It never reaches
  the pane edge, and an empty range or empty line paints nothing.
- **Line** paints a full-width band across the text area for every row of
  every line the range touches, like the cursor-line highlight, so diff
  hunks and changed lines read as blocks, including blank lines. A range that
  ends at column 0 of a later line leaves that line out, as Tab-indent does,
  so a range built from `lineRange(a).start` to `lineRange(b + 1).start`
  covers lines `a..=b`. An empty range paints its own line. The gutter is not
  tinted; diff gutter markers stay the gutter marker's job.
- **Paint order** behind the text: cursor-line highlight, line decorations,
  background decorations, selection, bracket match, then text and
  underlines. The selection therefore stays readable over match highlights,
  as in VS Code. Overlapping translucent backgrounds stack, painted in
  `decoration_precedence` order so severity keys land on top.
- **Ownership** is unchanged: contributions belong to a buffer and an
  extension lifecycle, so a decoration shows in every view of the buffer.

## Exclusions

- Virtual text, inlay hints, and anchored floating panels (D145).
- Foreground color, bold, italic, strikethrough, and borders: no consumer
  needs them yet, and the object leaves room for them.
- Gutter tinting for line decorations, overview-ruler or scrollbar marks.
- Per-view decorations; a view-scoped consumer would need its own source.

## Deferred decisions

- D080's own plan decides its color keys, debounce, and word rules, and
  whether its daily-driver dependency on D031 goes away now that this item
  supplies the shared highlight layer.
- Native color keys for Find's match and current match belong to D031.

## Checkpoints

### 1. Typed decoration contract and underline styles ✅

- Complete and approved: `decoration` is `{ color, style }` from the
  declarations through bootstrap validation, V8 decoding, the protocol, and
  the model to the view; `underline` and `wavy` paint as separate layers per
  key and style, and `background` and `line` are stored but not yet painted.
  The extension guide's example uses the object form.

### 2. Background and line painting ✅

- Complete and approved: `line` decorations paint full-width bands over
  every row of the lines they cover, `background` decorations paint per-row
  spans that end at the text, both after the cursor-line highlight and
  beneath the selection. `decoration_fills` in `src/app/editor/layout.rs`
  computes them, tested in `src/app/editor/tests/decorations.rs`; the
  column-0 rule lives in the contribution projection in
  `src/app/editor.rs`. Reviewed in the app on soft-wrapped Markdown, blank
  and RTL lines, and both appearances.

### 3. Documentation and completion 🚧

- ✅ Semantic UI, the extension guide, decisions (Decoration styles, and the
  gutter's note), the editor API plan, and the extensibility report are
  updated. ⏳ Documentation review, then removal of this plan.

- [Semantic UI](../architecture/semantic-ui.md) describes the styles, the
  two paint paths, and the order; the
  [extension guide](../extension-api.md#register-a-theme-and-define-colors)
  example uses the object form and shows `line` for a diff;
  [decisions](../decisions.md) records why backgrounds are quads, why style
  is required and one per contribution, and the column-0 rule for lines.
- Mark D114 ✅ in the [editor API plan](editor-api.md) with links, update the
  extensibility report's D114 row, and remove this plan.
- **Review gate:** documentation review.
