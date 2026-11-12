# D031 — Current-file Find

Status: 🚧 planned. Item D031 of the [daily-driver plan](daily-driver.md).

## Outcome

⌘F opens a find bar docked at the top of the focused editor, as in IntelliJ,
VS Code, and Zed. Typing highlights every match live and selects the current
one. Enter and ⇧Enter, or ⌘G and ⌘⇧G, move between matches. ⌘G and ⌘⇧G also
work from the editor with the bar open or closed. The bar shows "3 of 17",
"No results", or "1000+". Toggles and buttons switch case sensitivity and whole
word, and step or close. The editor stays editable while the bar is open:
matches are searched again after edits, and the current match follows them.
Esc in the bar closes it and clears the highlights.

## Design

Three pieces, each generic except the last:

- **Native matching: `buffer.find`.** Literal matching in `knot-model` over
  the buffer text, with `caseSensitive` and `wholeWord` options, an optional
  byte range, and a match limit. It returns the revision it searched, byte
  ranges, and whether more matches exist. It is native because extensions
  address text in UTF-8 byte offsets and should not copy the whole buffer on
  every keystroke. Whole word uses the editor's word classes. Regex stays
  deferred (D001), and D150 decides how Vim's syntax maps onto it.
- **Editor bar API.** An extension attaches a non-modal strip to the top of
  an editor view and describes it as data: text fields with a placeholder,
  toggles (icon, tooltip, state), buttons (icon, tooltip, command), and a
  message line. A bar is identified by its owner lifecycle and an
  extension-chosen ID per view. Opening the same ID again updates that bar,
  and several bars from different extensions stack in the order they were opened.
  Bars are lifecycle-owned presentation state, never recorded in layouts, and
  close with their view or their lifecycle. Field edits reach the extension
  through a coalescing subscription that never blocks input or painting.
  Clicking the editor focuses its text and leaves the bar open.
- **`@knot/find` package.** A small bundled JS package owns the workflow:
  commands, bar description, re-search on edits, navigation, and highlights.
  Highlights are the package's buffer contributions in two new theme keys,
  `editor.findMatch` and `editor.findMatchCurrent`, capped at 1,000 matches.
  D080 reuses the same contribution path with its own key. Navigation past the
  cap uses range-limited `buffer.find`, so it reaches every match.

### Key routing in a bar

While a bar field has Knot-side focus, the router resolves keys against a
native view kind `bar` whose live key context is the bar's ID. Editor-kind
bindings, such as the Emacs and movement keys of the Knot preset, therefore do
not reach the field. Keys the keymap leaves go to the field's `TextInput`,
which handles its own editing, as transient fields do. Tab and ⇧Tab are
native form keys, like Backspace, with no commands or bindings: they move
focus to the next or previous field across the editor's bars, top to bottom,
wrapping, and never indent or insert a tab. Global bindings still apply. The command context still captures the editor tab the bar belongs to,
so `editor` and `buffer` are the bar's editor. `@knot/find` binds Enter,
⇧Enter, and Esc in view `bar`, context `find`. The Knot preset binds ⌘F, ⌘G, and
⌘⇧G (with `secondary-` equivalents) for both the `editor` and `bar` view kinds,
avoiding existing bindings.

### Behavior

- ⌘F opens or focuses the bar and fills the field from a single-line
  selection, otherwise from the last query, all selected.
- The current match is the first at or after where the search started. It is
  selected and revealed, and as the selection follows edits natively, so does
  the current match.
- Esc closes the bar, clears the highlights, and leaves the current match
  selected, as the cited editors do.
- With the bar closed, ⌘G and ⌘⇧G use the last query and options from the
  caret and select the match without highlights.
- The last query and options live in the package's memory, shared by all
  editors, and are not persisted.
- Case-insensitive and substring matching by default.

## Exclusions

- Replace and regular expressions remain D001.
- No separate "use selection for find" command. ⌘E stays Open Recent, and
  ⌘F's pre-fill covers the case.
- Highlights are buffer contributions, so a second view of the buffer also
  shows them, and two find bars over one buffer share one highlight set, the
  latest search winning. View-scoped decorations are D179.
- No find history, persisted options, or search within the selection.

## Deferred decisions

- Theme defaults for the match colors and the bar's appearance are reviewed
  in the running app at checkpoint 4. The current match is selected, so
  selection paints above its background, and the review decides whether
  `editor.findMatchCurrent` needs a different style.

## Checkpoints

### 1. Native `buffer.find` ✅

Approved. `buffer.find(query, { caseSensitive, wholeWord, range, limit })`
resolves `{ revision, matches, more }`, matched natively in
`crates/knot-model/src/buffer/find.rs` and specified in `knot.d.ts`. The
limit defaults to 1,000 and is capped at 100,000. There is no reverse
search: stepping back past the highlight cap reads the matches before the
caret with a large `limit`.

### 2. Editor bar core ✅

Approved. Extensions open bars through `Editor.openBar`, `closeBar`,
`focusBarField`, and `barFields`, and hear user edits through
`EditorHandle.onDidEditBar`, coalesced like selection notifications and never
reporting text set through `focusBarField`; specified in `knot.d.ts`. Bar
state and field focus live in `crates/knot-model/src/editor/bars.rs`; fields,
focus following, and painting in `src/app/editor/bars.rs`. Theme keys
`editor.bar.*` and `input.*`. Until checkpoint 3, keys the editor view binds,
Tab included, still act on the text while a field has focus. D140 then needs
only the `showInput` option.

### 3. Bar keys ✅

Approved. While a bar field has focus, keys resolve against the native view
kind `bar` with the bar's ID as its only key context; Tab and Shift+Tab move
between fields natively. Specified in
[commands](../architecture/commands.md#editor-bars); routing in
`src/app/product/key_routing.rs`.

### 4. `@knot/find` ✅

Approved. The bundled `extensions/@knot/find` package owns the commands
`editor.find`, `editor.find-next`, `editor.find-previous`, and
`editor.find-close`, and binds Enter, ⇧Enter, and Esc in `bar`/`find`. The Knot
preset binds ⌘F, ⌘G, and ⌘⇧G, with F3 and ⇧F3 for next and previous on Windows
and Linux, where Ctrl+G is Go to Line. Highlights are buffer contributions in
`editor.findMatch` and `editor.findMatchCurrent`; the current match is a
background only, and the bundled themes' selections are translucent so it
shows through. Review found the dark themes' match colors serviceable but
subtle, particularly the current match; they may be revisited. End-to-end
tests are in `crates/knot-ext-workbench/tests/workbench/find.rs`.

### 5. Toggles and buttons ✅

Approved. Bars carry toggles (icon, tooltip, `active`, command) and buttons
(icon, tooltip, command), specified in `knot.d.ts` and painted in
`src/app/editor/bars.rs`. A click runs the command through the shell with the
bar's editor captured; an extension flips a toggle by opening the bar again.
`@knot/find` has Match Case and Whole Word toggles (⌥⌘C / ⌥⌘W in the bar;
Ctrl+Alt elsewhere) and previous, next, and close buttons.

### 6. Documentation ✅

Editor bars, the `bar` view kind, native text search, the extension guide, the
decision entry, and the roadmap (D001 narrowed, D140 updated) are documented.

**Review gate:** documentation review.
