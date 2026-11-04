# D078 — Zoom, font size, and soft wrap

Status: 🚧 checkpoint 1 implemented, awaiting review. Item of the
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

## Checkpoint 1 — Font size and scalable metrics

Status: ✅ implemented; 👀 awaiting review.

Deliverables:
- ✅ `font.size` setting; editors and text frames resolve it in their scope,
  terminals under `{ view: "terminal" }`. Values clamp to 6–72.
- ✅ Editor metrics (font size, line height, gutter widths) computed per view
  from the resolved size instead of the `FONT_SIZE` and `LINE_HEIGHT`
  constants; terminal cell metrics from its resolved size. The gutter's
  fixed lane widths and marker size scale with the text, so zoom
  (checkpoint 2) scales them through the same metrics.
- ✅ Changing the setting re-lays out open editors and terminals. An editor
  keeps the same line at the top of its viewport across the change.

Validation: settings resolution tests for both scopes; editor view tests that
a larger size changes line height, visible lines, hit testing, and the
gutter; a terminal test that a size change resizes the grid.

Review gate: metrics are one per-view value threaded through geometry, with
no remaining `LINE_HEIGHT` constant in the editor.

**Implementation context:** constants live in `src/app/typography.rs`
(`TEXT_SIZE`) and `src/app/editor.rs` (`LINE_HEIGHT`, used across
`editor.rs`, `editor/paint.rs`, `layout.rs`, `input.rs`, `completion.rs`,
`editing.rs`). Define the setting beside `EDITOR_TAB_SIZE` in
`src/app/settings.rs` with an accessor like `editor_tab_size`, and resolve it
through `EditorView::settings_scope` as `tab_size` does. Terminal metrics are
in `src/app/terminal/layout.rs` and `terminal/view.rs`. Update tests in
`src/app/editor/tests.rs` that use `LINE_HEIGHT` to read the view's metrics.

## Checkpoint 2 — Application zoom

Deliverables:
- The zoom `Global`, Zoom In / Zoom Out / Reset Zoom commands, and their
  preset bindings on all three platforms.
- Per-window rem size and zoom-scaled pixel constants on every custom-painted
  surface; a zoom change refreshes all windows.
- The `zoom` record in the state database, restored before the first window
  and flushed at quit.

Validation: command tests for stepping and clamping; gpui tests that a window
renders with the scaled rem size and an editor and terminal with scaled text;
a record round-trip test like `command_history`'s; preset tests for the
bindings on macOS, Windows, and Linux.

Review gate: zoom in the real app scales the whole window consistently,
including the palette, picker, and status bar, and survives a restart.

**Implementation context:** mirror `src/app/command_history.rs` over
`src/app/record_store.rs` for persistence and register the global as
`ApplicationToolLayout` is (see `AGENTS.md`, *Registering an application
subsystem*); flush from `extensions::host::quit_after_storage`. Declare the
commands with the `#[command]` pattern in `src/app/product/commands.rs` and
bind them in `src/app/commands/keymaps/presets.rs`. Set rem size in
`impl Render for ProductShell` (`src/app/product/render.rs`). Pixel constants
to scale: `src/app/picker.rs` (`LINE_HEIGHT`), `src/app/text_input.rs`
(`INPUT_LINE_HEIGHT`, `FONT_SIZE`), `src/app/product/render.rs`,
`src/app/product/tab_drag.rs`, and the metrics from checkpoint 1.

## Checkpoint 3 — Soft wrap rendering

Deliverables:
- `editor.softWrap` setting, Toggle Soft Wrap writing a document-scoped
  value, and the Markdown package's scoped default.
- The per-view wrap map with incremental rebuilds, and every geometry path
  listed above on rows; horizontal scroll off while wrapping.
- Existing movement keeps logical-line behavior in this checkpoint.

Validation: wrap map unit tests (whitespace breaks, long words, wide
characters, tabs, continuation indent, incremental rebuild matching a full
rebuild); editor view tests for caret and selection rows, hit testing, IME
bounds, gutter numbering on first rows, scroll clamping, and view state
restore mid-line; a toggle test that every view of the document follows; a
`node:test` case for the Markdown package's scoped write.

Review gate: wrapping Markdown and a long code file in the real app paints,
selects, clicks, and types correctly, and resizing rewraps.

**Implementation context:** add `src/app/editor/wrap.rs` and the
`unicode-width` dependency. Rows rebuild from `EditorView::refresh`
(`src/app/editor.rs`, which already refreshes `line_count`) for the commit's
changed lines, and fully when `paint.rs` measures a new width (where
`viewport_h` is set). Replace row arithmetic in `visible_lines`,
`fully_visible_lines`, `clamp_scroll`, `ensure_cursor_visible`, `view_state`,
`restore_view_state`, `set_selection`; painting in `editor/paint.rs`;
hit testing in `editor/input.rs` (including `bounds_for_range` and
`character_index_for_point`); `editor/completion.rs`; and
`editor/gutter.rs`. Shape per row through `shape_line` in
`editor/layout.rs`. Declare Toggle Soft Wrap in
`src/app/editor/native_commands.rs`; the document-scoped write mirrors the
indentation detection in `src/app/indentation.rs`. The Markdown default goes
in `extensions/@knot/language-markdown/src/main.ts`.

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
