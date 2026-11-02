# D068 — Editor gutter and line numbers

Status: 🚧 checkpoint 2 awaiting review. Item of the
[daily-driver plan](daily-driver.md); depends on D064 (themes, complete).

## Outcome

Document editors show a gutter with line numbers. The current line's number
stands out, and that line gets VS Code's background highlight. Clicking a
number selects its line, and dragging selects whole lines. Extension markers
keep a stable lane of their own beside the numbers: the most severe marker on
a line wins, and markers with a command look and act clickable. The
`editor.lineNumbers` setting switches numbers between `"on"`, `"off"`, and
`"relative"`, per language or view. Contributed text frames start without a
gutter unless their registration asks for one.

## Design

### Lanes

The gutter is an ordered row of native lanes, from left to right:

1. **Markers.** Extension `EditorContribution.gutter` markers.
2. **Line numbers.**
3. **Folds.** Reserved for D066; not built here.

A small `GutterLayout` value computes each lane's x range from the view's
state: whether the gutter shows, the line-number mode, and the line count.
Painting, hit testing, and `text_bounds()` all read from that one value. It is
plain data rather than a lane trait or registry. D066 adds its lane as one
more field.

Widths:

- **Markers.** A fixed width, always reserved while the gutter shows, so text
  never shifts sideways when a diagnostic appears or clears. VS Code and Zed
  reserve this space the same way.
- **Line numbers.** Wide enough for the digits of the line count, at least 2,
  plus padding. Relative mode keeps the same width, so moving the caret never
  changes the layout. The lane is absent when `editor.lineNumbers` is
  `"off"`.

The gutter stays fixed during horizontal scrolling, as the marker column
already does.

### Line numbers

- Numbers are right-aligned, in the editor font, and 1-based.
- `"relative"` shows each line's distance from the caret's line, and the
  caret's line shows its absolute number, as Vim's
  `number relativenumber` and VS Code's `"relative"` do.
- The caret's line paints its number in an active color, including while a
  selection exists.
- Each view resolves `editor.lineNumbers` from its own settings scope
  (language and view kind) while laying out, like `editor.tabSize`, so a
  change only needs a repaint.

### Current-line highlight

VS Code's default (`editor.renderLineHighlight: "line"`) is the rule here. The
caret's line paints a full-width background behind the text in the text area,
but not in the gutter. The highlight shows only while the selection is empty,
and it shows whether or not the view is focused. Knot adds no setting to
choose the mode. A theme that wants no highlight sets the key to transparent.

### Markers

The marker shape stays as it is: a dot on the first line of the range, in the
contribution's color key. When several markers fall on one line, the marker
painted on top follows the same order decorations use: `error` above
`warning` above `info` above other keys. Clicking a marker that has a command
still runs it, and the pointer becomes a hand over such markers. Elsewhere,
the gutter shows an arrow pointer rather than the text I-beam.

### Interaction

Each lane handles its own pointer behavior:

| Lane | Click | Drag | Hover |
| --- | --- | --- | --- |
| Markers | runs the top marker's command, if it has one | none | hand over a marker with a command; arrow otherwise |
| Line numbers | selects the line, including its line break; shift-click extends the selection by lines | extends by whole lines, reusing the triple-click `SelectionUnit::Line` path | arrow |

### Color keys

These are new native keys in the `theme.rs` table, each with a dark and light
default:

- `editor.lineNumber`: line numbers.
- `editor.lineNumber.active`: the caret line's number.
- `editor.gutter.background`: the gutter; defaults to `editor.background`.
- `editor.lineHighlight`: the current-line background.

The bundled themes in `@knot/themes` get values ported from their upstream
themes, which have the matching VS Code keys: `editorLineNumber.foreground`,
`editorLineNumber.activeForeground`, `editorGutter.background`, and
`editor.lineHighlightBackground`.

### Per-surface enablement

Two controls decide what each surface shows:

- **The `editor.lineNumbers` setting.** A string: `"on"`, `"off"`, or
  `"relative"`, native default `"on"`. Any other text means `"on"`, following
  `theme.mode`'s rule. Being an ordinary setting, it can be scoped to a
  language or a view.
- **A `gutter?: boolean` option on text-frame view registrations.** It sits
  beside `language` on `ViewContribution`, applies to text frames only, and
  defaults to `false`. Without it, a text frame has no gutter at all, neither
  markers nor numbers. With it, the frame gets the full gutter, and its line
  numbers still follow `editor.lineNumbers` resolved under its view ID.

The setting alone could not express this split. The settings store has one
default per key, so it cannot default numbers to on in documents and off in
generated frames.

## Exclusions

- Folding and its lane: D066.
- Bar markers that span several lines (such as Git diff hunks), other marker
  shapes, and hover tooltips on markers: D114's extension-owned presentation.
- Extension-defined lanes. Extensions share the one marker lane.
- A setting to choose the current-line highlight mode. Themes control its
  color.
- Numbering wrapped lines. D078 defines visual lines, and the gutter then
  numbers only the first visual line of each buffer line.

## Deferred decisions

- Whether markers need ordering beyond severity (such as by priority or
  source) once a second marker source appears.
- Whether `"relative"` needs a view-local toggle command for D058, or
  whether the setting is enough.

## Checkpoints

### 1. Gutter layout, line numbers, and current line ✅

Complete and approved. `GutterLayout` (`src/app/editor/gutter.rs`) is
recomputed at the start of each paint, and `text_bounds()` reads the last
painted lanes. `EditorRenderingOptions.show_gutter` is the surface switch that
checkpoint 3 sets for frames. Editor tests cover lane widths, labels in each
mode, scoped settings, and the line highlight. The native color key test and
the `@knot/themes` test that every theme sets exactly the native keys cover
the new keys.

### 2. Lane interaction and marker precedence 🚧 review

Progress: ✅ deliverables landed. Each line keeps one marker, the most severe
and then the latest, carrying its own command, so a click runs what is
painted; text-region actions no longer carry a gutter flag. Pointer styles
come from hitboxes inserted in prepaint, which now also lays out the gutter:
the text area shows an I-beam, command markers a hand, and the rest of the
gutter the default arrow. Shift-click on a number extends from the
selection's anchor to the clicked line's far edge. No example extension
paints gutter markers, so the manual check needs a snippet.

Deliverables:

- Hit testing by lane.
- Line-number click, shift-click, and line drag.
- The hand pointer over actionable markers, and an arrow elsewhere in the
  gutter.
- Severity precedence for markers on the same line.

Validation:

- Editor tests:
  - clicking a number selects that line, including its line break, or the
    end of text on the last line;
  - shift-click extends by lines;
  - a drag through the number lane extends by whole lines, upward and
    downward;
  - clicking the marker lane with no command marker leaves the selection
    unchanged;
  - with overlapping `info` and `error` markers, `error` is painted and its
    command runs.
- `project_contributions` unit tests for precedence.

Review gate: try selecting by line numbers, and clicking markers from the
example extension, in the running app.

Implementation context:

- `src/app/editor/input.rs`:
  - `on_mouse_down` currently treats any x left of `text_bounds()` as the
    gutter. Branch on the `GutterLayout` lane instead.
  - The number lane sets `DragAnchor { unit: SelectionUnit::Line, .. }` from
    `unit_range` and lets `on_mouse_move` extend it as triple-click drags do.
    `hit_test` already maps y to a line.
- Cursor styles: set them in paint with
  `window.set_cursor_style(style, &hitbox)` over a gutter hitbox, as
  `src/app/terminal/view.rs` does for links.
- `src/app/editor.rs::project_contributions`: sort the markers on each line
  using the model's decoration paint order, which already ranks `error`,
  `warning`, and `info`. Keep the topmost marker per line both for painting
  and for gutter actions.

### 3. Text-frame enablement and documentation

Deliverables:

- The `gutter` option on text-frame registrations, from `knot.d.ts` through
  to `TextView`. Frames default to no gutter.
- Architecture, decisions, and extension-guide updates.
- D068 marked ✅ in the daily-driver plan, and this plan removed once its
  content is preserved.

Validation:

- Host decode tests for the new argument.
- A product test, beside the existing text-frame tests, that a frame
  registered without `gutter` lays out text at x = 0, and one registered with
  `gutter: true` reserves the marker and number lanes.

Review gate: confirm that an example text frame looks right without a gutter,
and that the docs describe the final behavior.

Implementation context:

- Contract and transport:
  - `src/host/js/knot.d.ts`: add `gutter?: boolean` to `ViewContribution`,
    beside `language`.
  - `src/host/js/bootstrap.js`: add the new argument to the `registerView`
    argument packing.
  - `src/app/extensions/host/handlers.rs`: decode it in `TreeArguments`,
    after `language` at index 9.
  - `docs/schemas/knot.schema.json`, if it lists view fields.
- Storage and application:
  - `src/app/extensions/semantics.rs`: store it on the registration beside
    `language` (field around line 87, applied around line 167).
  - `src/app/text_view.rs`: pass it to `EditorView::for_contributed_view`,
    which sets whether the gutter shows.
- Documentation:
  - `docs/architecture/workbench-and-lifecycle.md#editor-views`: lanes,
    interaction, and enablement.
  - `docs/architecture/semantic-ui.md`: marker precedence and color keys.
  - `docs/decisions.md`: why the marker lane is always reserved, and why
    frames have a registration option rather than a setting default.
  - `docs/extension-api.md`: the frame option and `editor.lineNumbers`.
