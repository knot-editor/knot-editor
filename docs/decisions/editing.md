# Editing decisions

Native editing behavior over the buffer and its syntax tree. The
[decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Language editing configuration

Comment tokens and bracket pairs describe a language as its grammar does, so
they are optional fields of the language registration rather than settings:
they replace with the language under the latest-registration rule, and a
derived language spreads `languages.get`, as a derived theme spreads
`themes.get`. Whether a feature such as auto-closing is enabled stays a
language-scoped setting. Following Zed, one bracket list serves matching,
auto-closing, and surrounding, with per-pair flags instead of VS Code's three
parallel lists; a line comment list lets uncommenting strip the longest token,
so a Rust doc comment never leaves `/ foo`. Indentation is not editing
data: it comes from tree-sitter `indents` queries, as in Zed, Helix, and
Neovim, rather than regex patterns or bracket heuristics.

The bundled data departs from Zed where text-based matching would mislead:
no `<` pair in Rust or ECMAScript, where comparisons and arrows would
mismatch, and no `'` in Rust because of lifetimes. Quotes never auto-close in
comments, so an apostrophe stays single, and Markdown quotes only surround.

Each package keeps its comment toggling cases as JSONC beside its sources,
run by a generic Rust product test through the native toggle on the
registered configuration, so a package's data and the behavior it produces
are reviewed together. A package test against the in-memory host could only
restate the data, since toggling is native. TypeScript case modules would add
type checking but need Node, `build.rs`, or V8 to load them for Rust (D133).

References: [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages)
and [extension guide](../extension-api.md#register-a-language).

## Indentation

Indent queries use nvim-treesitter's captures and the semantics of its
indent module, evaluated natively, so its queries adapt with light edits and
painting never waits on JavaScript. Knot departs where the module mishandles
ordinary code. Lines inside a string keep their indentation, since it is
content, and lines inside an `@indent.auto` node move with its first line
rather than copying the previous one, so a block comment's ` *` lines stay
aligned. Inside a parse error, an opening bracket the error leaves unclosed
indents the lines after it: a block typed before its closer is an ERROR in
every grammar, often swallowing the enclosing block's closer too, and
upstream queries otherwise rely on auto-closing. `indent.after_end` limits
a dedent to the lines after its node, so Python goes back one level after
`return`, as VS Code does, instead of upstream's column 0 below a nested
block, while a multi-line `return foo(` keeps its continuation lines. Rust
arguments indent one level, as rustfmt lays them out, since upstream's Rust
query has no alignment. Markdown has no query: its text belongs to the
injected inline layer, and a layer without a query keeps its lines, while a
code fence indents as its language.

Following Zed and Neovim, Enter commits first and indents from the reparsed
tree, which a typed `}` or `else` must already be in and which gives
Python's scanner a real newline; Helix's indenting from the tree before the
edit cannot see the typed text. The correction joins the edit's undo step
and is dropped if another commit lands first, so a parse that overruns its
foreground budget leaves the kept indentation rather than racing typing.
Typing only ever outdents, as in Zed and VS Code, and only when the
capture's match reaches the caret, so a brace someone placed stays when they
type ` else {` after it, and Python's `else` waits for its colon.

Each package keeps Enter, typing, and reindent cases as JSONC run through an
editor, like its comment cases, because indentation crosses the
editor's commit and parse path. Reindenting pasted text and an
`editor.autoIndent` setting are D135 and D136.

References: [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages),
[editor views](../architecture/workbench-and-lifecycle.md#editor-views), and
[extension guide](../extension-api.md#register-a-language).

## Brackets

Bracket matching, auto-closing, and wrapping are native, over the bracket
pairs language registrations carry. Typing must insert the closer in the same
commit and undo step as the opener, and an extension command is an
asynchronous turn, which ordered key delivery keeps in sequence but cannot join
to the typed edit's commit; painting and raw input never call JavaScript. The
data stays programmable through the registration, and settings turn the
behavior off, as VS Code and Zed split it; Neovim's autopair plugins work only
because its Lua runs synchronously in-process.

Matching pairs the bracket tokens of one syntax node rather than scanning
text, so brackets inside strings and comments never match, and it needs no
per-language query such as Zed's `brackets.scm`: a token's partner is its
first balanced sibling with the paired text. Whether the caret is inside a
string or comment reuses the highlight query's `string` and `comment`
captures rather than adding per-grammar node lists, since every bundled query
uses nvim-treesitter's names. Markdown paragraphs belong to the injected inline
layer, so Markdown Inline carries Markdown's pairs, as it already did its
comment tokens.

Behavior follows VS Code's defaults: the highlight shows only next to a
bracket, preferring the one before the caret, and Jump to Matching Bracket
lands before the partner, so repeating it alternates, or before the enclosing
closer. Stepping over a closer and deleting an empty pair apply only to
closers the view inserted, while the caret stays inside them, as VS Code's
`auto` overtype and delete do. Overtyping any closer, as Sublime Text and
Helix do, inserts nothing when someone fixing unbalanced code needs an extra
`)`. Each package keeps bracket cases as JSONC run through an editor, like
its indentation cases.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views)
and [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages).

## Editor gutter

The gutter is a fixed order of native lanes, markers then line numbers, with
the fold lane beside the text. Extensions share the marker lane rather
than defining lanes, because nothing yet needs a second one and every lane
would otherwise need its own width, order, and input rules. The marker lane
is always reserved, as VS Code's glyph margin and Zed's gutter are, so text
never shifts sideways when a diagnostic appears or clears; the number lane
sizes to the line count's digits instead, which changes only when the count
gains a digit. A line shows one marker, chosen by decoration severity and
then by the latest, so a click runs the command of what is painted.
Line decorations leave the gutter alone, so markers stay the gutter's only
signal; multi-line bars and marker tooltips wait for a consumer.

`editor.lineNumbers` takes `"relative"` beside `"on"` and `"off"`, because it
costs one subtraction and a Vim mode needs it. The cursor line's background
follows VS Code's default `renderLineHighlight`: the text area only, hidden
while text is selected, shown without focus, with no setting because a theme
can make the color transparent.

Documents show the gutter and text frames opt in through their
registration. A setting alone cannot express that split, since the settings
store has one default per key and the frame's own view ID scope only exists
once someone writes it; generated output rarely wants line numbers, while a
frame that does still follows `editor.lineNumbers` under its view ID.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views)
and [extension guide](../extension-api.md#read-only-text-frames).

## Editing commands

Editing commands follow VS Code's behavior and default bindings, because
they are the conventions most editor users already have in their fingers:
Enter keeps the indentation before the caret where a language has no indent
query (see [Indentation](#indentation)), Tab indents a selection across
lines and Shift-Tab always outdents, a selection ending at column 0 leaves
that line out, and Home goes to the first non-blank character before
column 0. macOS keeps ctrl-a at column 0, as every Cocoa text field does. A
second block toggle unwraps when the delimiters sit just outside the
selection, as in VS Code, so toggling twice is a no-op even though wrapping
leaves the inner text selected.

Each command is one model batch that records the view's selection before
and after, the same grouping extensions open across batches through the
editor handle. Move Lines deletes the neighbouring line and reinserts it on
the other side rather than replacing the span, so anchored contributions on
the moved lines survive.

Tabs or spaces are detected once, when a document enters from a resource,
from its first 10,000 lines: a style wins when it indents at least twice as
many lines as the other, so a mixed file falls back to `editor.insertSpaces`
instead of guessing. One-space indents are alignment and ` *` lines are
block-comment continuations, so neither counts. Detection writes
`editor.insertSpaces` scoped to the document, rather than keeping a field on
the buffer, so the indentation of a document is one resolved setting that the
status bar and any extension change with an ordinary write. The size always
comes from `editor.tabSize`; detecting it, and EditorConfig, are D090.

Words split at VS Code's default separators rather than at Unicode word
boundaries alone. UAX #29 is designed for prose and keeps `some.thing`,
`a:b`, and `don't` as one word, which made double-click select both sides
of a member access. Splitting Unicode segments at separators keeps
ideographs and emoji as separate units, and one definition serves
double-click, word drags, and word movement. Language-specific word
characters, such as `-` in CSS, are D131.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views)
and [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages).

## Zoom, text size, and soft wrap

Zoom is one application level, as VS Code's window zoom is, rather than per
view or per window: someone zooming wants everything larger, chrome included,
and one level keeps every window consistent. It is not a setting, because
settings are rebuilt from code on every launch while a level chosen with a
key must survive restarts, so it is a record of the application state
database, read before the first window. `font.size` is the setting, resolved
per view scope, so terminals can differ from editors while zoom scales both.

Wrapped rows are measured in monospace cells with `unicode-width` rather than
shaped. Shaping every line of a file whenever the width changes would cost a
shaping pass per line on resize, while cells cost a scan, and Knot's text is
monospace; proportional fallback glyphs may make a row slightly longer or
shorter than the viewport. Vertical movement keeps its column in the same
cells (see [views and extension UI](platform.md#views-and-extension-ui)).
Whitespace hangs past the edge instead of starting a row and continuation rows
keep the first row's indentation, as VS Code's defaults do. Views rewrap only
the lines each commit changed, from a bounded per-revision log on the model,
because the model's pending change queue belongs to extension delivery and one
view's consumption would hide changes from the next.

Toggle Soft Wrap writes a document-scoped `editor.softWrap`, as the
indentation picker writes `editor.insertSpaces`, so every view of a document
agrees and the choice ends with the document, while languages and personal
config set the default; the Markdown package wraps prose.

Movement that wrapping changes exists twice, as Emacs pairs `next-line` under
`line-move-visual` with `next-logical-line`: the plain commands move by row,
as VS Code and Zed do by default, and `-logical` twins move by buffer line, so
a keymap such as a Vim mode chooses per key. The Knot preset binds the row
commands except macOS's ctrl-A and ctrl-E, which Cocoa defines as paragraph
movement. Without an affinity for positions at a break, End on a row that does
not end its line stops before its last character, usually the space the row
broke after, and a caret at a break shows at the start of the next row.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views),
[zoom](../architecture/workbench-and-lifecycle.md#zoom), and
[core and buffers](../architecture/core-and-buffers.md#application-buffer-ownership).

## Folding

Fold state belongs to the view, as anchored byte ranges, so two views of one
document fold independently and edits keep a fold over its text. Hidden lines
are zero-row lines in the wrap map rather than a second line-to-row mapping, so
the viewport, scrolling, row queries, and painting count visible rows with no
parallel bookkeeping. Regions come from a per-language `folds` query, with
indentation for languages without one, because every language folds usefully
at once and a package refines it later; regions are only candidates, and just
collapsed folds are state and persist with the tab's view state. The caret
never rests in hidden lines, and revealing a hidden position expands its folds,
as VS Code does, rather than refusing the move.

Rejected: fold state in the buffer, which would couple views and need an
ownership story across splits; and a fold-aware second row space, which every
row consumer would have to translate through.

Reference: [editor views](../architecture/workbench-and-lifecycle.md#editor-views).
