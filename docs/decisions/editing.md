# Editing decisions

Native editing behavior over the buffer and its syntax tree. The
[decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Language editing configuration

Comment tokens and bracket pairs describe a language as its grammar does, so
they are fields of the language registration rather than settings: they
replace with the language under the latest-registration rule, and a derived
language spreads `languages.get`, as a derived theme spreads `themes.get`.
Whether a feature is enabled stays a setting. Following Zed, one bracket list
with per-pair flags serves matching, auto-closing, and surrounding, rather than
VS Code's three parallel lists that must agree. A list of line comment tokens
lets uncommenting strip the longest, so a Rust doc comment never leaves `/ foo`.
Indentation comes from tree-sitter queries, as in Zed, Helix, and Neovim,
rather than regex patterns or bracket heuristics.

The bundled data departs from Zed where text-based matching would mislead: no
`<` pair in Rust or ECMAScript, where comparisons and arrows would mismatch, and
no `'` in Rust because of lifetimes. Quotes never auto-close in comments, so an
apostrophe stays single.

Packages keep behavior cases as JSONC beside their data, run by a generic Rust
product test through the native commands, so a package's data and the behavior
it produces are reviewed together. A package test against the in-memory host
could only restate the data, since the behavior is native. TypeScript case
modules would add type checking but need Node, `build.rs`, or V8 to load them
for Rust (D133).

References: [languages](../architecture/editor-and-languages.md#syntax-highlighting-and-languages)
and [extension guide](../extension-api.md#register-a-language).

## Indentation

Use nvim-treesitter's indent captures and the semantics of its indent module,
evaluated natively, so upstream queries adapt with light edits and painting
never waits on JavaScript. Knot departs where the module mishandles ordinary
code: string contents and block-comment lines are content, not code to indent;
a block typed before its closer is an ERROR in every grammar, often swallowing
the enclosing block's closer too, and upstream queries otherwise rely on
auto-closing to avoid it; Python dedents one level after `return`, as VS Code
does, instead of upstream's column 0 below a nested block; and Rust arguments
indent one level, as rustfmt lays them out, since upstream's query has no
alignment.

Enter commits first and indents from the reparsed tree, as Zed and Neovim do,
because a typed `}` or `else` must already be in the tree and Python's scanner
needs a real newline; Helix's indenting from the tree before the edit cannot
see the typed text. Dropping the correction when another commit lands first
means a parse that overruns its foreground budget never races typing. Typing
only ever outdents, as in Zed and VS Code, and only when the match reaches the
caret, so a brace someone placed stays when they type ` else {` after it.
Reindenting pasted text and an `editor.autoIndent` setting are D135 and D136.

References: [languages](../architecture/editor-and-languages.md#syntax-highlighting-and-languages)
and [editor views](../architecture/editor-and-languages.md#editor-views).

## Brackets

Matching, auto-closing, and wrapping are native rather than an extension,
because the closer must land in the opener's commit and undo step, and an
extension command is an asynchronous turn that ordered key delivery keeps in
sequence but cannot join to the typed edit's commit. Neovim's autopair plugins
work only because its Lua runs synchronously in-process. The data stays
programmable through the registration, and settings turn the behavior off, as
VS Code and Zed split it.

Matching pairs the bracket tokens of one syntax node rather than scanning text,
so brackets inside strings and comments never match, and no per-language query
such as Zed's `brackets.scm` is needed. String and comment context reuses the
highlight query's captures rather than per-grammar node lists, since every
bundled query uses nvim-treesitter's names.

Behavior follows VS Code's defaults, the conventions most users know. Stepping
over a closer and deleting an empty pair apply only to closers the view
inserted, as VS Code's `auto` overtype does: overtyping any closer, as Sublime
Text and Helix do, inserts nothing when someone fixing unbalanced code needs an
extra `)`.

References: [editor views](../architecture/editor-and-languages.md#editor-views)
and [languages](../architecture/editor-and-languages.md#syntax-highlighting-and-languages).

## Editor gutter

The gutter is a fixed order of native lanes, and extensions share one marker
lane rather than defining lanes, because nothing yet needs a second one and
every lane would need its own width, order, and input rules. The marker lane is
always reserved, as VS Code's glyph margin and Zed's gutter are, so text never
shifts sideways when a diagnostic appears or clears. Showing one marker per line
keeps a click unambiguous: it runs the command of what is painted. Multi-line
bars and marker tooltips wait for a consumer.

Relative line numbers exist because they cost one subtraction and a Vim mode
needs them. The cursor-line highlight has no setting because a theme can make
its color transparent.

Documents show the gutter and text frames opt in through their registration,
because a setting cannot express that split: the store has one default per key,
and a frame's view ID scope exists only once someone writes it.

References: [editor views](../architecture/editor-and-languages.md#editor-views)
and [extension guide](../extension-api.md#read-only-text-frames).

## Editing commands

Editing commands follow VS Code's behavior and default bindings, because they
are the conventions most editor users already have in their fingers; macOS
keeps Cocoa's text keys, as every native text field does. Each command is one
model batch recording the selection before and after, so undo restores where
it started. Move Lines deletes and reinserts the neighbouring line rather than
replacing the span, so anchored contributions on the moved lines survive.

Indentation style is detected once, on load, and written as a document-scoped
`editor.insertSpaces` rather than a field on the buffer, so a document's
indentation is one resolved setting that the status bar and any extension
change with an ordinary write. A mixed file falls back to the default rather
than guessing. Detecting the size, and EditorConfig, are D090.

Words split at VS Code's default separators rather than at Unicode word
boundaries alone: UAX #29 is designed for prose and keeps `some.thing`, `a:b`,
and `don't` as one word, which made double-click select both sides of a member
access. One definition serves double-click, word drags, and word movement.
Language-specific word characters, such as `-` in CSS, are D131.

References: [editor views](../architecture/editor-and-languages.md#editor-views)
and [languages](../architecture/editor-and-languages.md#syntax-highlighting-and-languages).

## Zoom, text size, and soft wrap

Zoom is one application level, as VS Code's window zoom is, rather than per view
or per window: someone zooming wants everything larger, chrome included, and one
level keeps every window consistent. It is a state-database record rather than a
setting, because settings are rebuilt from code on every launch while a level
chosen with a key must survive restarts. `font.size` is the per-scope setting,
so terminals can differ from editors while zoom scales both.

Wrapping measures monospace cells with `unicode-width` rather than shaping,
which would cost a shaping pass per line on every resize while cells cost a
scan; proportional fallback glyphs may make a row slightly off. Views rewrap
only changed lines from a per-revision log on the model, because the model's
pending change queue belongs to extension delivery and one view's consumption
would hide changes from the next. Toggle Soft Wrap writes a document-scoped
setting so every view of a document agrees and the choice ends with it.

Movement that wrapping changes exists twice, as Emacs pairs `next-line` under
`line-move-visual` with `next-logical-line`: row movement by default, as VS Code
and Zed do, and logical twins, so a keymap such as a Vim mode chooses per key.

References: [editor views](../architecture/editor-and-languages.md#editor-views),
[zoom](../architecture/workbench-and-lifecycle.md#zoom), and
[core and buffers](../architecture/core-and-buffers.md#application-buffer-ownership).

## Folding

Fold state belongs to the view, as anchored byte ranges, so two views of one
document fold independently and edits keep a fold over its text. Hidden lines
are zero-row lines in the wrap map rather than a second line-to-row mapping, so
every row consumer counts visible rows with no parallel bookkeeping. Regions
come from a per-language query, with indentation as the fallback, so every
language folds usefully at once and a package refines it later. Revealing a
hidden position expands its folds, as VS Code does, rather than refusing the
move.

Rejected: fold state in the buffer, which would couple views and need an
ownership story across splits; and a fold-aware second row space, which every
row consumer would have to translate through.

Reference: [editor views](../architecture/editor-and-languages.md#editor-views).
