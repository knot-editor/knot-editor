# D114 — Decoration styles

Status: 🚧 checkpoint 1 complete; checkpoint 2 implemented, awaiting its
review gate. Item of the
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

### 2. Background and line painting 🚧

- ✅ Implemented: the projection gives a `line` decoration one entry per
  line it covers (the column-0 rule applies there), and `decoration_fills`
  in `src/app/editor/layout.rs` turns them and the painted rows into bands
  and per-row spans, which `paint.rs` draws after the cursor-line highlight.
  Tests in `src/app/editor/tests/decorations.rs`. ⏳ Review gate pending.
- `background` paints per-row text spans and `line` paints full-width bands,
  in the paint order above, on wrapped, RTL, and blank lines.
- **Validation:** unit tests on a pure function that turns a view's rendered
  decorations and rows into background rectangles, in the style of the
  existing selection and bracket geometry tests: a span across a wrap
  boundary yields one rectangle per row ending at the text, a line decoration
  covers every row of its lines and includes blank lines, a range ending at
  column 0 excludes that line, an empty `line` range paints its line, an
  empty `background` paints nothing, and severity keys sort last.
- **Review gate:** a personal-config `post-init.js` registers `try.*`
  commands that, on the active editor's buffer, contribute backgrounds on
  every occurrence of the selected word, line bands over a few lines
  including a blank one with a theme-defined `try.diffAdd` key, straight and
  wavy underlines overlapping a background, and one that clears them; the
  user checks selection over backgrounds, soft-wrapped Markdown, and both
  appearances.
- **Implementation context:**
  - `src/app/editor/paint.rs`: add the two passes between the cursor-line
    quad and the selection pass (around lines 432–455), reusing the `span`
    closure for backgrounds and the `line_highlight` rect logic for bands.
  - Extract the rectangle computation beside the existing row-span helpers so
    `src/app/editor/tests/` (new `decorations.rs`, registered like
    `gutter.rs`) can test it without painting.
  - Personal config at `user_config_root` in
    `src/app/personal_config.rs`; look at the existing `post-init.js` first.

### 3. Documentation and completion

- [Semantic UI](../architecture/semantic-ui.md) describes the styles, the
  two paint paths, and the order; the
  [extension guide](../extension-api.md#register-a-theme-and-define-colors)
  example uses the object form and shows `line` for a diff;
  [decisions](../decisions.md) records why backgrounds are quads, why style
  is required and one per contribution, and the column-0 rule for lines.
- Mark D114 ✅ in the [editor API plan](editor-api.md) with links, update the
  extensibility report's D114 row, and remove this plan.
- **Review gate:** documentation review.
