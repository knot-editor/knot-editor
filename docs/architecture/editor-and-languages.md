# Editor and languages

Part of the [architecture](../architecture.md). [Decisions](../decisions.md)
records rationale; deferred work lives in the [roadmap](../roadmap.md).

## Editor views

An editor is two entities in `src/app/editor/` over plain state in
`knot-model` (`crates/knot-model/src/editor.rs`). `EditorState` holds one
presentation's state: cursor and selection, scroll and the viewport in rows,
wrapping, folds, IME preedit, key contexts, the caret shape, its own
decorations ([semantic UI](semantic-ui.md#semantic-extension-ui)), and its
completion controller, and it runs the native editing and movement commands
without a window or runtime. Each operation takes the buffer model it reads
or edits; the editor follows the settings its owner resolves for it and
queues clipboard writes and document setting changes as effects, and paste
receives the clipboard's text. The `Editor` entity wraps the state: before
each operation it resolves the settings for the editor's scope, then runs the
effects, notifies the buffer when its text changed, and notifies itself; it
also follows buffer changes and keeps the in-flight completion requests. The
`EditorView` renders it: it owns focus, painting, pointer gestures,
the platform input handler, shaped hit testing, and horizontal scrolling, and
each layout tells the editor its viewport height in rows and its wrap width in
cells. A document tab's presentation owns the view, which owns its editor; commands, editor
handles, and completion reach the `Editor`. Multiple editors share a model
while retaining independent presentation state. Completion ownership is
detailed in [semantic UI](semantic-ui.md#completion-ownership-and-aggregation).

A view keeps no copy of the text. It reads the lines it paints, measures, or
edits from the model, and converts positions, including the platform's
document-wide UTF-16 offsets, through the buffer's line index, so editing cost
does not grow with document size. A test guards that editing, navigation, IME,
scrolling, and painting never read the whole document.

Movement and deletion respect grapheme boundaries, and vertical movement
keeps the display cell it started from, counted with `unicode-width` and
expanded tabs, across shorter lines. Words are Unicode word segments split at VS
Code's default word separators, so each run of separators such as `.` or `::`
is its own unit; word movement stops at the edges of words and separator runs,
and double-click selects one. `Buffer.wordAt` returns the segment at an offset,
preferring a word that ends there, so extensions use the same units. Dragging after a double- or triple-click keeps
the clicked word or line selected and extends by that unit in either
direction. Text,
selection, caret, hit testing, and IME bounds share one shaping projection and
horizontal offset. The caret never sits inside a `\r\n` terminator, and
inserted newlines from typing, paste, and IME take the buffer's line ending.
Editing/navigation reveal the caret; wheel scrolling may move away from it, up
to the widest line shaped since the text last changed. Clipboard operations use
source text; expanded tabs are only a rendering projection. Tab stops follow
`editor.tabSize`, which the view resolves while laying out for its scope: its
document's effective language and the `editor` view kind, or a contributed
text frame's view ID. Views observe the settings store and repaint on any
change. Text size is `font.size` in the same scope times the zoom factor, and
line height is the font's ascent plus descent times `font.lineHeight`, rounded
up to whole physical pixels, with the baseline centered in the row; a size or
line-height change keeps the top line in place. The font (`font.family`,
`font.fallbacks`, `font.weight`, `font.lineHeight`, `font.ligatures`,
`font.features`) resolves in the same scope into
[`FontSettings`](../../crates/knot-model/src/typography.rs), which
[typography](../../src/app/typography.rs) turns into a gpui font; the gutter and
text inputs use it too, and text inputs read the unscoped values.

With `editor.softWrap` resolved true for its scope, a view lays each line out
as rows that fit the text area, measured in monospace cells rather than shaped
(`crates/knot-model/src/editor/wrap.rs`). Every geometry path counts rows: scrolling and
clamping, painting, the caret, selections, hit testing, IME bounds, the
completion popup, and the gutter, which shows a line's number and marker on
its first row. Each row is shaped as its own slice of the line, and nothing
scrolls sideways while wrapping. The view rewraps only the lines the model
reports changed since it last followed it
([core and buffers](core-and-buffers.md#application-buffer-ownership)), and
every line when the width, text size, zoom, or tab size changes, keeping the
top row's text at the top. A caret at an offset where a row breaks shows at
the start of the next row. Toggle Soft Wrap writes the opposite of the
resolved value scoped to the view's document, so every view of the document
follows and the value ends with it; a view without a document reports the
command unavailable.

Up, Down, Page Up and Down, Home, smart Home, and End move and select by row,
keeping the caret's display cell across rows; End on a row that does not end its line
stops before its last character, whose end shows on the next row. Each has a
`-logical` twin, such as `editor.move-up-logical`, that moves by buffer lines
and, for vertical movement, lands on the target line's first row. Without
wrapping the two agree. The macOS preset binds Cocoa's ctrl-A and ctrl-E to
the twins, as they reach a paragraph's edges; other keys use the row
commands. A view's own edit
refreshes its derived state synchronously before resolving the resulting caret.
Other views update through their persistent positions.

Extension commands captured in a document editor reach it through an editor
handle ([commands](commands.md#extension-execution)) that reads and moves its
selection and saves and restores its view state. Moving the selection ends
the typing undo group as other caret moves do; a centered reveal recenters
the caret's row only when it lies outside the viewport and otherwise behaves
like ordinary movement, an always-centering reveal centers it regardless, and
top and bottom reveals always align the caret's line with that edge: a top
reveal shows a line's first row and a bottom reveal its last. The handle
counts rows: it reports the first row wholly visible in the last layout and
how many are, which may lag a scroll by a frame, scrolls by rows without
moving the caret or asking the next paint to reveal it, and maps a document row
to its buffer line and start column and a line and byte column to its row. Without wrapping,
rows and lines coincide. Go to Line is the bundled `@knot/go-to-line`
package built on that handle, bound to ctrl-G in the Knot preset.

Editing commands such as Enter, indent and outdent, comment toggling, and the
line commands compute every edit first and commit them as one batch that
records the view's selection before and after, so each command is one undo
step that restores the selection it started from. Commands that act on lines
take every line the selection touches, except a last line it reaches only at
column 0. The indentation unit is a tab or `editor.tabSize` spaces, resolved
from `editor.insertSpaces` in the view's scope, which includes its document:
a document entering the collection from a resource writes the style its
leading lines show as its own document-scoped value, and any later write to
that scope replaces it. Detect Indentation from Content detects again, dropping
the document's own values when the text shows no clear style. The two Convert
Indentation commands rewrite every line's leading whitespace at the same widths
as one undo step and write the same value; they never change how deep a line is
indented. Reindent Buffer recomputes every line from the language's indent query,
at `editor.tabSize` per level, and Reindent Selected Lines only the selection.
Backspace in leading spaces deletes back to
the previous tab stop. Comment
toggling reads the editing data of the language at the selection start,
including an injected one such as a Markdown code fence's
([languages](#syntax-highlighting-and-languages)).

Enter commits the line break with the indentation before the caret, then the
view keeps a request to indent the new line from the language's indent query
([languages](#syntax-highlighting-and-languages)). The request
applies once the syntax tree is a parse of the text at the same revision,
right after the commit when the foreground parse finishes within its budget,
otherwise when a background parse does, and any later commit drops it. The
correction joins Enter's undo step through the model's amend of the last
history entry. Between a bracket pair whose `newline` flag is set, Enter opens
a middle line, indented from the query or one unit deeper, with the closer
below at the opening line's indentation; on a line holding only whitespace,
Enter clears it. Typing that completes a closing or branching capture at the
start of a line's content, such as `}` or Python's `else:`, outdents the line
the same way within the typing group, never indenting it further; composition
and paste never reindent. Reindent Selected Lines applies the query to the
selected lines as one step, and Reindent Buffer to every line.

Brackets follow VS Code's language-defined behavior, read from the bracket
pairs of the language at the caret
([languages](#syntax-highlighting-and-languages)). With the
caret next to a bracket, the view paints it and its partner in
`editor.bracketMatch`, caching the match per syntax generation and caret so
painting walks no tree; the bracket before the caret wins. Jump to Matching
Bracket moves before the partner, or before the closer of the innermost pair
around the caret. Typing an opener over a selection wraps it, as one undo
step, and keeps the inner text selected. Typing an opener without a
selection inserts its closer in the same typing commit when the pair
auto-closes, the next character is whitespace, the line end, or in
`autoCloseBefore`, the caret is not in a string or comment the pair lists in
`notIn`, and, for quotes, no word character precedes it. The view tracks
each pair it closed as an anchored range while the caret stays between
opener and closer: typing the tracked closer steps over it and Backspace
between the empty pair deletes both. Leaving the pair, selecting, or text
that no longer holds the pair drops the tracking, after which closers are
ordinary text. Composition never closes or wraps. `editor.autoClosingBrackets`
and `editor.autoSurround`, resolved for the view's scope, turn the two off.

The gutter left of the text is an ordered row of native lanes: extension
markers, line numbers, then folds. The view lays the lanes out before each paint, so
text bounds, hit testing, and painting read one layout. The marker lane has a
fixed width that stays reserved, so text never shifts when a marker appears;
the number lane fits the digits of the line count and is absent when
`editor.lineNumbers`, resolved for the view's scope, is `"off"`. Relative
numbers count from the caret's line, which shows its absolute number, and the
caret line's number paints in `editor.lineNumber.active`. As VS Code's default
does, the caret's line paints `editor.lineHighlight` across the text area
while nothing is selected, focused or not. Each lane handles its own pointer:
a number click selects its line with the line break, shift-click extends from
the selection's anchor, and dragging extends by lines; a marker click runs
the command of the marker painted there and never selects; a fold-lane click
toggles the fold or region starting on that line, Alt-click also its nested
ones, and never selects. The fold lane draws chevrons on foldable lines while
the pointer is over the gutter, or always when `editor.showFoldingControls` is
`"always"`, and always one on a collapsed fold's line; `editor.folding`, resolved
for the view's scope, turns the lane off. Documents always
have the gutter, while a contributed text frame has none unless its
registration asks for one.

A view folds line regions, which stay per view. A collapsed fold keeps its
first line visible, with an `editor.foldPlaceholder` `…` after it, and gives
its other lines no rows: the wrap map treats hidden lines as zero-row lines, so
the viewport, scrolling, row queries, and painting count only visible rows.
Each fold is an anchored byte range from the first line's start to the last
line's content end, so edits keep it over its text, and one left with fewer
than two lines is dropped. Folds may nest, and the hidden lines are their
union. The caret never rests in hidden lines: vertical and horizontal moves
step over a fold, folding moves a caret inside it to the first line, selecting
a fold's first line by pointer selects the whole fold, line commands take a
collapsed fold's lines with its first line, and anything that puts the caret
in hidden lines (Go to Line, an extension's `setSelection`, undo) expands the
folds hiding it; clicking a fold's placeholder expands it. The Fold, Unfold, Toggle Fold, Fold All, and Unfold All
commands act on the [regions](#syntax-highlighting-and-languages)
the language offers; Fold collapses the innermost uncollapsed region that
starts on the caret's line or encloses it. Extensions fold exact line ranges
through the [editor handle](../extension-api.md).

Restorable view state is the anchor and caret as byte offsets, the start of the
top visible row as a byte offset, and horizontal scroll in pixels. Applying it
clamps each offset to the current text and snaps back to a character boundary,
so a changed or shorter file never yields an invalid position, and keeps the
restored scroll instead of revealing the caret; a state restored before the
view first wraps its lines keeps its row once they wrap. It also records the
collapsed folds as byte ranges, from the first line's start to the last line's
content end; restoring replaces the view's folds with them, clamped to the text
and dropped once they span fewer than two lines. The editor handle's view state
carries no folds and leaves them as they are. Zoom is application state and
wrapping follows settings, so neither is recorded.

An editor also takes a caret shape (`bar`, `block`, or `underline`) from an
extension, kept per lifecycle like
[key contexts](commands.md#key-contexts): the latest set wins, clearing
reveals the previous one, and teardown clears its own. A block or underline
covers the character under the caret, with a half-line width at the end of a
line; the shape is presentation state and never recorded.

### Editor bars

An editor can show non-modal strips above its text, such as Find. An extension
describes a bar as data: text fields with placeholders, toggles and buttons
(an icon, a tooltip, and a command; a toggle also an `active` state), and a
message line. A bar belongs to its opening lifecycle and an extension-chosen
ID per editor; opening the same ID again replaces what it shows, keeping the
text of fields that remain, and bars stack in the order they were opened. Bar
state and field focus are `EditorState` data in
`crates/knot-model/src/editor/bars.rs`; the `Editor` entity keeps a
`TextInput` per field, and `EditorView` paints the bars and moves the window's
focus between a field and the text. Bars are presentation state: layouts never
record them, and they close with their editor or their lifecycle. Clicking the
text focuses it and leaves the bars open.

A user edit of a field reaches the owner through `onDidEditBar`, coalesced
like selection notifications, and text the extension sets through
`focusBarField` is not reported. A toggle or button click dispatches its
command through the window with the bar's editor captured, deferred
until the click has finished with its views; an extension flips a toggle by
opening the bar again. [Commands](commands.md#editor-bars) covers key routing
while a field has focus. The bundled `@knot/find` package is the first user.
Its highlights are the editor's own decorations
([semantic UI](semantic-ui.md#semantic-extension-ui)) in `editor.findMatch` and
`editor.findMatchCurrent`, capped at 1,000 matches, and it finds matches with
the buffer's native `find` ([core and buffers](core-and-buffers.md#text-search)).
Because the decorations belong to the view, each split of a buffer shows only
its own bar's matches.

## Syntax highlighting and languages

Highlight layers compose in a fixed order: syntax, then contribution
underlines; selection and decoration backgrounds paint beneath the text. Semantic tokens from a future
language client (D067) slot in above syntax. Syntax highlights are derived and
never stored as anchored ranges: each view queries its visible lines, with some
overscan, once per [syntax generation](core-and-buffers.md#syntax-state), and
painting, caret, hit testing, and IME geometry share that query, so bold and
italic captures keep geometry consistent. An injected layer paints over its
host, clipped to its included ranges.

Grammars are compiled in and named by stable native grammar IDs. Language
definitions are contributed data: extensions and personal config register an
LSP language ID, the grammar it parses with, the file names, extensions, and
shebang interpreters it claims, and its highlight, injection, and indent queries, through
the same `languages` API the bundled `@knot/language-*` packages use. An
association claims patterns for an ID without defining the language, so
personal config redirects detection without replacing a package's language.

```text
languages.register --> background query compilation --> LanguageRegistry
                                                            |
Open / Save As / restore / registration change --> detection
                                                            |
documents.setLanguage / Set Language Mode --> explicit ID   |
                                                            v
                                      BufferModel language --> SyntaxState
```

- `ApplicationLanguages` owns the registry and tracks every buffer with a
  language. Registrations and associations are lifecycle-owned and share one
  order: the latest registration defines an ID, and the latest registration or
  association claiming a file name, extension, or interpreter wins it, so
  removal reveals the previous one.
- Queries compile on the background executor at registration, so a bad query
  rejects with its location and a package's registrations compile in
  parallel; only the insertion runs on the foreground.
- Detection tries exact file names, then extensions, then the shebang
  interpreter. It runs when a document enters the collection and when
  persistence moves it to a new resource, never while typing, and never for a
  document whose language is explicit.
- A buffer keeps its ID even while nothing registers it and reports
  `plaintext` meanwhile, so personal config and restoration need no ordering
  against package startup. A registration change re-detects open documents and
  reattaches syntax wherever the resolved language changed; a language that
  injects reparses too, since its injections may resolve differently.
  Injections resolve by language ID, its lowercase, or a registered extension.
- Startup awaits every package entry before restoring windows, and the bundled
  package awaits its registrations in `start()`, so restored documents paint
  highlighted from their first frame.
- A registration's editing data, its comment tokens and bracket pairs, is
  resolved with its defaults at registration and travels with its compiled
  syntax language, so native editing code reads it without calling
  JavaScript. Editing commands take the data of the deepest syntax layer at
  the selection start, so a code fence or an injected `<script>` uses its own
  language's comments with no lookup by ID. Markdown's inline language, the
  layer inside every paragraph, therefore registers Markdown's block comment
  too. A buffer without syntax has no editing data. The line and block
  comment toggles are plain functions over text and that configuration, with
  no buffer or view.
  Each bundled language package keeps line and block comment toggling cases
  in `test/cases/comments.jsonc`, which a product test runs through the native
  toggles on the registered configuration.
- Bracket matching reads the syntax tree with the registered pairs and no
  extra query. A bracket is a leaf token whose text is a pair's opener or
  closer, in the deepest layer holding it, and its partner is the first
  sibling in its direction with the paired text, skipping balanced pairs,
  so brackets inside strings and comments, which belong to larger leaves,
  never match, and a parse error leaves a bracket unmatched. Whether the
  caret is inside a string or comment, which `notIn` reads, comes from the
  `string` and `comment` highlight captures of the layer holding the text
  before it, dotted children included, so no grammar needs extra data. A
  string holds only offsets strictly inside it; a comment also holds its end
  unless its last line ends with the language's block comment closer.
  Markdown's inline language shares Markdown's bracket pairs, since it is the
  language at the caret inside paragraphs. Packages keep typing, Backspace,
  wrapping, jump, and highlight cases in `test/cases/brackets.jsonc`, run through
  an editor view; a language with bracket pairs must have cases.
- An optional `indents` query compiles with the others and drives native
  indentation. The evaluator follows nvim-treesitter's indent module over the
  tree of the deepest layer at a line's first non-blank byte, so a code fence
  indents as its language, and a layer without a query leaves its lines
  alone rather than falling back to its host's. It returns columns, or that
  a line keeps its indentation (inside an `@indent.ignore` node, such as a
  string) or moves with an earlier line (inside an `@indent.auto` node, such
  as a block comment). Within one reindent each line sees the indentation
  given to the lines above it. Inside a parse error, each opening bracket of
  the language's bracket pairs that the error leaves unclosed before the line
  indents it one level, at most once per row. Packages keep Enter, typing,
  and reindent cases in `test/cases/indents.jsonc`, which a product test runs
  through an editor view; a language with an indent query must have cases.
- An optional `folds` query lists the nodes whose lines can fold, as `@fold`
  captures, over every layer's tree. A multi-line node is a region from its
  first line through its last; a closing bracket that ends the node and
  starts the line stays visible, an indented block after a colon starts on
  the line of its header, and trailing blank lines are left out. A language
  without a query folds by indentation. The regions are candidates that
  Fold, Unfold, and Fold All choose among, cached per text revision and
  syntax generation on the buffer model; only collapsed folds are view
  state. Packages keep region cases in `test/cases/folds.jsonc`, which a
  product test runs through an editor view; a language with a folds query
  must have cases.
- The native Set Language Mode command offers the registered languages, or Auto
  Detect, in the window's transient picker. Contributed text frames take a
  fixed language at registration.
