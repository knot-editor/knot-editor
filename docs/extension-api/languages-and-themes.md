# Languages, themes, and icons

Part of the [extension guide](README.md).

## Register a language

Knot parses with tree-sitter grammars compiled into the binary;
`languages.grammars()` lists their IDs. A language registration names one of
them, claims file patterns for detection, and supplies the highlight and
optional injection queries:

```js
import { languages } from "knot";

await languages.register({
  id: "rust",
  name: "Rust",
  grammar: "rust",
  fileExtensions: ["rs"],
  queries: {
    highlights: `(line_comment) @comment
"fn" @keyword.function`,
  },
});
```

Use LSP `languageId` values for IDs (`typescriptreact`, `shellscript`) and
[nvim-treesitter's capture names](https://github.com/nvim-treesitter/nvim-treesitter/blob/main/CONTRIBUTING.md#highlights)
in queries; the theme resolves a capture by its longest known dotted prefix and
ignores names it does not know. Predicates are the ones the Rust tree-sitter
binding evaluates (`#eq?`, `#match?` with Rust regex syntax, `#any-of?`, and
their negations), plus `(#set! priority N)`. Injections use
`@injection.content` with `@injection.language` or
`(#set! injection.language "…")`, and may name a language by a registered file
extension, as Markdown fences do.

Queries compile before `register` resolves: an unknown grammar rejects
`UnknownGrammar` and a bad query `InvalidQuery`, with the query, message, row,
and column in `details`. Register in activation so restored documents highlight
from their first frame. The latest registration of an ID or file pattern
wins, so personal config replaces a package's language from `post-init`. The
bundled `@knot/language-*` packages, one per language, are complete examples.

A registration also carries the data native editing commands read: comment
tokens, bracket pairs, and the characters before which pairs auto-close.
Tokens omit the trailing space. Commenting inserts the first line token and a
space; uncommenting strips the longest line token that matches, so Rust lists
its doc-comment tokens too. A language with only a block comment wraps each
line. One `brackets` list serves matching, auto-closing, and surrounding; a
pair whose open equals its close, such as quotes, never takes part in matching:

```js
await languages.register({
  id: "rust", name: "Rust", grammar: "rust", fileExtensions: ["rs"],
  queries: { highlights: "" },
  comments: { line: ["//", "///", "//!"], block: ["/*", "*/"] },
  brackets: [
    { open: "{", close: "}" },
    { open: '"', close: '"', notIn: ["string", "comment"] },
  ],
  autoCloseBefore: ";:.,=}])>",
});
```

`autoClose` defaults to true and `newline`, whether Enter between the pair
opens an indented middle line, to true unless open equals close. Typing an
opener inserts its closer when the pair auto-closes and the next character is
whitespace, the end of the line, or in `autoCloseBefore`; a quote also needs
no word character before it. `notIn` suppresses auto-closing inside strings or
comments, which Knot finds from the `@string` and `@comment` captures of the
highlight query, so name them as nvim-treesitter does. Typing any opener over
a selection wraps it, including pairs that do not auto-close, and matching
pairs in the syntax tree highlight their partner and drive Jump to Matching
Bracket. The `editor.autoClosingBrackets` and `editor.autoSurround` settings,
scoped by language, turn closing and wrapping off.

An optional `queries.indents` drives indentation: Enter indents the new line
from it, typing a closer such as `}` at the start of a line outdents the
line, and Reindent Buffer and Reindent Selected Lines recompute the document's or
the selection's lines. It uses
[nvim-treesitter's indent captures](https://github.com/nvim-treesitter/nvim-treesitter/blob/main/CONTRIBUTING.md#indents)
(`@indent.begin`, `end`, `branch`, `dedent`, `align`, `ignore`, `auto`,
`zero`) and their properties, so its queries adapt with light edits:

```js
queries: {
  highlights: "",
  indents: `
(block) @indent.begin
"}" @indent.branch
(block "}" @indent.end)
(string_literal) @indent.ignore
(block_comment) @indent.auto`,
},
```

Lines inside an `@indent.ignore` node keep their indentation, because it is
content; lines inside an `@indent.auto` node move with its first line, so a
block comment's ` *` lines stay aligned. Knot adds `(#set! indent.after_end)`,
which limits an `@indent.dedent` to the lines after its node, as after
Python's `return`, and the node predicates `has-parent?`, `not-has-parent?`,
`kind-eq?`, and `not-kind-eq?`; rewrite `lua-match?` to `match?`, since any
other predicate fails to compile. Typing reindents a line when a closing
capture's match reaches the caret, so a pattern can capture a following
token, such as the colon after Python's `else`, as `@_end`. Inside a parse
error, an opening bracket from `brackets` that the error leaves unclosed
indents the lines after it, so a block typed before its closer indents
without help from the query. A language without an indents query keeps the
previous line's indentation.

An optional `queries.folds` lists the nodes whose lines fold, as `@fold`
captures. A captured node spanning several lines is a region from its first
line through its last; when that last line starts with a closing bracket it
stays visible, so capture a block rather than its parent to keep `} else {`
lines. The Fold, Unfold, Toggle Fold, Fold All, and Unfold All commands use
the regions; a language without a folds query folds by indentation, where a
line followed by more-indented lines starts a region.

```js
queries: {
  highlights: "",
  folds: "(block) @fold\n(struct_item) @fold",
},
```

To change one value of a registered language, spread `languages.get`, which returns the full
definition, and register the result from `post-init`:

```js
const rust = await languages.get("rust");
await languages.register({ ...rust, comments: { ...rust.comments, line: ["//"] } });
```

To send files to a language without replacing its registration, associate
patterns with its ID. The claim competes with registrations' claims, so the
latest one wins and disposing it reveals the previous one:

```js
await languages.associate("markdown", { fileExtensions: ["mdx"] });
```

`DocumentInfo.language` reports a document's language, or `plaintext`.
`documents.setLanguage(id, "python")` makes a language explicit, which is
recorded in window layouts and survives restarts and project switches; `null`
returns the document to detection. Users do the same with Set Language Mode.

## Register a theme and define colors

A theme names its appearance, sets colors by key, and styles capture names.
Colors are `#rrggbb` or `#rrggbbaa`; the native keys are declared in
`ThemeColors` in `knot.d.ts`, and a key a theme leaves out takes its default for
the theme's appearance:

```js
import { themes } from "knot";

await themes.register({
  name: "Ember",
  appearance: "dark",
  colors: { background: "#1c1917", "editor.selection": "#f9731640" },
  syntax: {
    comment: { color: "#78716c", fontStyle: "italic" },
    keyword: { color: "#f97316", fontWeight: 700 },
  },
});
```

A theme's `chrome` sets the shape and weight of the workbench, such as
`{ cardRadius: 0, cardGap: 0, cardInset: 0, cardBorder: 1 }` for square cards
that meet at borders; `ThemeChrome` in `knot.d.ts` lists every value and its
range, and a value a theme leaves out keeps its Islands default.

`syntax` keys are capture names or a dotted prefix of them, such as `markup`;
a capture takes its longest styled prefix, and an unknown name rejects
`UnknownCapture`. Themes are code, so derive one by spreading a registered
theme:

```js
const base = await themes.get("One Dark");
await themes.register({
  ...base,
  name: "My Dark",
  colors: { ...base.colors, "editor.background": "#101010" },
});
```

The `theme.mode` (`dark`, `light`, or `system`), `theme.dark`, and
`theme.light` [settings](settings.md#read-and-write-settings) select the theme. Knot
resolves it when startup completes and again whenever the selection changes,
so register themes in activation; a theme registered or disposed later shows
at the next selection change. The latest registration of a name wins.

The bundled [`@knot/themes`](../../extensions/@knot/themes/) package registers
the default themes and the `themes.select` command, which remembers its choice
and sets it again when the package starts. Customize themes in `post-init`:
it runs after the package, so its settings win over the remembered choice, and
`themes.get` can see the bundled themes to derive from them, which `pre-init`
cannot.

Define the color keys your package paints with, giving each a default per
appearance, either a color or another key to follow. Themes may then set it,
and keys stay open, so a theme can style a package that loads after it:

```js
await themes.defineColor({
  key: "diff.add",
  description: "Added lines in diffs",
  defaults: { dark: "success", light: "#2ea04340" },
});
```

A key defined by native code or a live lifecycle rejects `DuplicateColor`;
teardown removes your definitions.

Editor decorations, gutter markers, and tree badges name the color key they
paint with, native or defined, so a contribution looks right under every
theme. A decoration's style is `background`, `line`, or a straight or `wavy`
underline, and a range that wants two styles is two contributions:

```js
await buffer.contributions.replace([
  { range, decoration: { color: "diff.add", style: "line" }, gutter: "diff.add" },
  { range: problem, decoration: { color: "error", style: "wavy" } },
], { ifRevision: revision });
```

Those belong to the buffer, so every view of it paints them. Highlights that
follow one view, such as matches around its caret, go through
`editor.setDecorations(items, { ifRevision })` instead, which takes
`{ range, decoration }` items and replaces the caller's set on that editor
alone.

## Define icons, packs, and associations

Icons are named, such as `file.rust` or `action.refresh`, and trees, tabs,
actions, and status items refer to them by name. A definition is a builtin
Lucide glyph (`{ lucide: "star" }`) or SVG text (`{ svg }`), with an optional
`color`: a color key paints the icon in that one color, `"original"` keeps an
SVG's own colors, and no color uses the surface's icon color. An unknown name
paints a generic fallback icon.

```js
import { icons, themes } from "knot";

await themes.defineColor({
  key: "icon.docs", description: "The docs icon", defaults: { dark: "#61afef", light: "#2a6fb0" },
});
await icons.define("file.docs", { lucide: "book-open", color: "icon.docs" });
await icons.associate({
  files: { "README.md": "file.docs" },
  extensions: { md: "file.docs", "d.ts": "file.docs" },
  folders: { docs: "folder.docs" },
});
await icons.override("action.refresh", { svg: "<svg ...>...</svg>", color: "original" });
await icons.registerPack({ name: "Mine", icons: { "file.docs": { lucide: "file-text" } } });
```

A name resolves to the first override (mainly for personal config), then the
pack named by the `icons.pack` setting, then defaults, then the fallback.
`define` adds a default for a name your package introduces, so a pack or
override restyles it instead. A file paints the name claimed for its file name,
then its extension, then `file.<languageId>`, then `file`; a folder paints its
claimed name, then `folder`. Associations are global, not per pack, and
everything is lifecycle-owned. The Lucide glyphs are ISC-licensed.
