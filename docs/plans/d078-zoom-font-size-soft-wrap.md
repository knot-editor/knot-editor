# D078 — Zoom, font size, and soft wrap

Status: 🚧 checkpoints 1–3 complete; checkpoint 4 next. Item of the
[daily-driver plan](daily-driver.md), scheduled by the
[editor API plan](editor-api.md) before D112.

## Outcome

- ⌘= / ⌘- / ⌘0 (ctrl on Windows and Linux) zoom the whole application, as
  VS Code's window zoom does: editors, terminals, tabs, the status bar, trees,
  the palette, pickers, and inputs all grow and shrink together. The zoom
  level survives restarts.
- `font.size` sets the base text size of editors, and `font.size` scoped
  `{ view: "terminal" }` sets the terminal's, so the two may differ while
  zoom scales both.
- `editor.softWrap` wraps long lines at the viewport width. It resolves per
  language, view, and document like the indentation settings, so the bundled
  Markdown package turns it on for Markdown and Toggle Soft Wrap writes it
  scoped to the current document.
- Every movement that wrapping changes exists twice, by visual row and by
  logical line, so the keymap decides which a key performs. The Knot preset
  binds the visual-row commands.

## Design

### Zoom

- One application zoom level, an integer clamped to a small range; each step
  scales by 1.2, as VS Code's `window.zoomLevel` does. It lives in a gpui
  `Global` beside the other application state.
- Zoom In, Zoom Out, and Reset Zoom are native commands in the palette,
  bound to `secondary-=` (also `secondary-+`), `secondary--`, and
  `secondary-0`. In terminals on Windows and Linux they follow the existing
  derived rule for ctrl chords.
- Each product window sets `window.set_rem_size(BASE_REM × zoom)` when it
  renders, which scales every surface styled with gpui's rem-based helpers.
  Surfaces that paint with pixel constants multiply them by the zoom factor:
  the editor and gutter, terminal cells, `TextInput`, the picker's row height,
  tab chrome in `product/render.rs`, and the tab-drag preview. A zoom change
  refreshes every window.
- The level is one record in the application state database (`zoom`, through
  `record_store.rs`, mirroring `command_history.rs`), read at startup before
  the first window opens and written with a short debounce. There is no
  setting: the database is the only source.
- The architecture invariant that lists zoom as view state changes: zoom is
  application state.

### Font size

- `font.size` is a native `Number` setting, default 14, read per view in the
  view's settings scope. Editors and text frames resolve it under their own
  view kind; terminals resolve it under `{ view: "terminal" }`, which is the
  first setting terminals read.
- Text size is `font.size × zoom`. Line height derives from it at the current
  20/14 ratio, rounded to whole pixels, replacing the `LINE_HEIGHT` constant.
  A change re-lays out the view; a terminal whose cell metrics change resizes
  its PTY through the existing resize path.
- Other typography (families, ligatures, terminal preferences) stays D059 and
  D045.

### Soft wrap

- `editor.softWrap` is a native boolean setting, default off. The bundled
  `@knot/language-markdown` package sets it to true scoped
  `{ language: "markdown" }` in `start()`.
- Toggle Soft Wrap writes the opposite of the resolved value scoped to the
  view's document, as the indentation picker writes `editor.insertSpaces`, so
  every view of the document follows and the value disappears with the
  document. A view without a document, such as a text frame, reports the
  command unavailable.
- Wrapping is always at the viewport width; there is no wrap column setting.
- Rows are measured in monospace cells without shaping: a character takes its
  `unicode-width` width (wide CJK and emoji take 2), tabs expand to the next
  stop, and a row breaks after the last whitespace that fits, or mid-word when
  a word is wider than the row. Continuation rows are indented like the line's
  first row, as VS Code's default `wrappingIndent: same` does, unless that
  indentation leaves too little room, when they start at column 0.
- Each view owns a wrap map: per buffer line, the byte offsets where its
  continuation rows start, plus prefix sums of row counts. A commit rebuilds
  the rows of the lines it touched; a change of wrap width, font size, zoom,
  or tab size rebuilds the whole map. When wrap is off the map is the
  identity, one row per line.
- Every geometry path moves from buffer lines to rows: scroll height and
  clamping, visible rows, caret and selection painting, hit testing, IME
  bounds and character index, the completion popup position, decorations, the
  cursor line highlight, and the gutter, which shows a line's number and
  marker on its first row only. Each row is shaped and painted as its own
  slice of the line. Horizontal scrolling is off while wrapping.
- Restored view state stays byte offsets; `top` becomes the start of the top
  visible row, which clamps and snaps as today.

### Movement

- Today's `editor.move-up`, `move-down`, `move-page-up`, `move-page-down`,
  `move-line-start`, `move-smart-home`, `move-line-end`, and their `select-`
  forms move by visual row: Home and End go to the edges of the caret's row,
  and smart Home on a line's first row keeps its first-non-blank behavior.
- Each gains a logical-line twin with a `-logical` suffix, such as
  `editor.move-up-logical` and `editor.select-line-end-logical`, which behaves
  as today's command does. The preset leaves the twins unbound. Both behave
  identically while wrap is off.
- Vertical movement keeps the preferred x across rows, as it keeps it across
  lines today.
- The editor handle's visible lines, `scrollByLines`, and line reveals keep
  buffer-line units. D112 keeps movement to screen positions and an
  always-centering reveal; its visual-line movement is delivered here.

## Exclusions and deferred decisions

- A wrap column, wrapping at a ruler, or a wrapping-indent setting.
- Shaping-based wrap measurement; proportional fallback glyphs may make a row
  slightly shorter or longer than the viewport. Revisit if it shows in use.
- A separate UI font size setting, and zoom per window or per view.
- Visual-row units in the editor handle API (D112).
- Line-ending facts (D149) and any status bar item for wrap or zoom.

## Checkpoint 1 — Font size and scalable metrics ✅

Reviewed. `font.size` (default 14, clamped 6–72) is resolved per view:
editors and text frames in their own scope, terminals under
`{ view: "terminal" }`. Each editor keeps one `EditorMetrics` value
(`src/app/editor/layout.rs`) that all of its geometry uses. Line height is
the size × 20/14, rounded to whole pixels. The gutter's lanes and marker
scale with the text, and a change keeps the top line in place. Terminal
cells are measured for the resolved size, so a change resizes the PTY grid.
Tab titles and other chrome deliberately ignore `font.size`; zoom scales
them (checkpoint 2).

## Checkpoint 2 — Application zoom ✅

Reviewed. One application zoom level (`src/app/zoom.rs`), −4 to +6 in steps
of 1.2 (about 48% to 300%), persisted as the `zoom` record in the state
database and restored before the first window. Zoom In, Zoom Out, and Reset
Zoom (`workbench.zoom-*`) are in the palette and View menu and bound on all
three presets. Product windows set their rem size from the zoom; chrome sizes
written in pixels at 100% use `zoom::rems`, pixel-measured painting uses
`zoom::scaled`, and editors and terminals multiply `font.size` by the factor.
Dock sizes stay absolute window pixels, as on window resize, and hairlines keep
their pixel widths. The architecture and workbench references still describe
zoom as view state until checkpoint 4's documentation step.

## Checkpoint 3 — Soft wrap rendering ✅

Reviewed. `editor.softWrap` (default off) wraps editor views at the text
area's width; Toggle Soft Wrap writes the document's own value, and
`@knot/language-markdown` sets it for Markdown. Each view keeps a wrap map
(`src/app/editor/wrap.rs`) measured in monospace cells, and rewraps only the
lines the model's per-revision `LineChange` log (64 revisions) reports, or
every line when it fell further behind. Whitespace hangs past the edge
instead of starting a row, tabs advance from their row's start, and
continuation rows keep the first row's indentation when that leaves half the
row. All geometry counts rows; a caret at a break shows on the next row, a
click past a non-final row's end lands before its last character, and the
cursor line highlight covers every row of its line. Nothing scrolls sideways
while wrapping, and width, font, zoom, and tab-size changes keep the top
row's text on top. Movement stays logical (Up and Down land on the target
line's first row) until checkpoint 4, whose documentation step also covers
the line change log in `docs/architecture/core-and-buffers.md`. Toggle Soft
Wrap has no default key.

## Checkpoint 4 — Visual and logical movement, and docs

Deliverables:
- Visual-row behavior for the existing movement and selection commands and
  the `-logical` twins, unbound in the preset.
- Documentation: the zoom invariant in `docs/architecture.md`; editor views
  in `docs/architecture/workbench-and-lifecycle.md`; the settings in the
  extension guide; rationale (global zoom, cell measurement, document-scoped
  toggle, paired commands) in `docs/decisions.md`; D112's brief in
  `docs/plans/editor-api.md` reduced to what remains; D078 marked ✅ in
  `docs/plans/daily-driver.md` and this plan removed.

Validation: editor view tests for each command pair on wrapped and unwrapped
text, preferred x across rows, and paging by rows; keymap tests that the
preset binds the visual commands.

Review gate: both behaviors are reachable from the keymap and the defaults
feel right in daily editing.

**Implementation context:** command declarations in
`src/app/editor/native_commands.rs`, movement in `src/app/editor/editing.rs`
(paging uses `viewport_h / LINE_HEIGHT`), bindings in
`src/app/commands/keymaps/presets.rs` (`up`, `home`, `pageup`, `cmd-left`).
