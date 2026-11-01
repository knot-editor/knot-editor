# D134 — Tree-sitter indentation

Status: 🚧 checkpoint 2 approved; checkpoint 3 in progress. Item of the
[daily-driver plan](daily-driver.md); split from D066 in the
[roadmap](../roadmap.md).

## Outcome

Enter indents the new line from the language's syntax tree, as Zed, Helix,
and Neovim do: after `fn main() {`, `if x:`, or an unclosed `(`, the new line
is indented one level, and after a statement that ends a block it is not.
Enter between a `{|}` pair opens an indented middle line. Typing `}`,
`else`, or `elif x:` at the start of a line outdents it. Reindent Lines
re-computes the selected lines' indentation. Languages without an indent
query keep D074's behavior.

## Design

### Queries and captures

- Language registration gains an optional `queries.indents` beside
  `highlights` and `injections`, compiled with them in
  `SyntaxLanguage::new`. `languages.get` returns it, so a derived language
  spreads it.
- Captures follow nvim-treesitter's indent module, so its queries adapt with
  light edits: `@indent.begin`, `@indent.end`, `@indent.branch`,
  `@indent.dedent`, `@indent.align`, `@indent.ignore`, `@indent.auto`,
  `@indent.zero`, with the `indent.immediate`, `indent.open_delimiter`,
  `indent.close_delimiter`, and `indent.avoid_last_matching_next`
  properties.
- `@indent.align` follows nvim: when the open delimiter ends its line, the
  next line indents one level (hanging indent); otherwise lines align to the
  column after the delimiter, as PEP 8 Python continuation lines do.
- Predicates: the evaluator adds `has-parent?`, `not-has-parent?`,
  `kind-eq?`, and `not-kind-eq?`, which compare node kinds and cannot be
  written as patterns. `lua-match?` is rewritten to `match?` when adapting,
  as highlights queries were, and any other unknown predicate fails to
  compile, so a pattern never matches unconditionally.
- The evaluator is native and pure: given a layer's tree, its language's
  compiled query, the line texts, and a line, it returns the line's
  indentation in columns, as nvim does, or that the line keeps its
  indentation or follows an earlier line. Within one reindent each line
  sees the indentation given to the lines before it, so alignment follows
  a moved delimiter. The editor turns columns into text with the document's
  `IndentUnit` (tabs or spaces, D074): tabs as far as they fit, then
  spaces.
- Inside a parse error, each opening bracket of the language's bracket
  pairs that the error leaves unclosed before the line indents it one
  level, at most one per row, as the block it opens would. A block typed
  before its closer is an ERROR node in every grammar: `fn main() {` alone,
  or an `if x {` that swallows its function's closing brace, leaving the
  function's `{` unclosed. Upstream queries patch individual cases with
  `ERROR` patterns and otherwise rely on auto-closing; the rule covers
  every language with bracket data.
- The layer is the deepest one at the line's first non-blank byte, through
  `SyntaxState`, so a code fence in Markdown indents as its language. A layer
  whose language has no indent query falls back to keeping the indentation,
  not to the host language's query.

### Strings and comments

Multi-line strings and comments are ordinary code and must be right:

- Enter inside a multi-line string or block comment keeps the current
  line's indentation. Comment continuation (` * `, `///`) is D131.
- Reindent never changes lines inside a string, because their leading
  whitespace is content.
- Reindent moves a block comment's interior lines by the same amount as its
  first line, so ` *` lines stay aligned under `/*`. nvim's `@indent.auto`
  copies the previous line instead, which loses that alignment.

### When indentation is computed

Following Zed and Neovim, the edit commits first and indentation is computed
on the reparsed tree, which the typed `}` or `else` must already be in:

- Enter commits the line break with the indentation it keeps today (D074),
  then an auto-indent request recomputes the new line. Inserting first also
  gives Python's scanner a real newline to parse.
- When the commit's foreground parse finishes within its 1 ms budget, the
  request applies immediately. Otherwise it applies when the background
  parse is accepted, if the buffer revision and the line are unchanged since
  the request; a superseded request is dropped, leaving D074's indentation.
  This is the revalidation every asynchronous result performs.
- The adjustment joins the triggering edit's undo step and keeps the caret
  at the end of the line's indentation, so Enter stays one undo step that
  restores its selection.

### Enter between brackets

Enter with the caret between an open and close of a pair whose `newline`
flag is set (D073, on by default unless open equals close) inserts two line
breaks: the middle line is indented from the query, or one unit deeper than
the opening line without one, and the closer keeps the opening line's
indentation. This works for every language with bracket data.

### Outdent while typing

As in Zed, typing reindents the caret's line only when the typed text
completes a node that starts at the line's first non-blank byte and is
captured by `@indent.end`, `@indent.branch`, or `@indent.dedent` (`}`,
`)`, `else`, `elif`, `except`), and only when the result is shallower than
the current indentation. It never indents a line further while typing.

### Blank lines

Enter on a line holding only whitespace clears that whitespace, so pressing
Enter repeatedly leaves no trailing whitespace, as VS Code does.

### Reindent Lines

`editor.reindent-lines`, titled Reindent Lines, recomputes the indentation of
the selected lines (D074's `selected_lines`), or the caret's line, as one
undo step through `commit_line_edits`. It is in the palette and unbound,
since Tab follows VS Code (D074). Lines in a layer without an indent query
are left unchanged.

### Tests

Each package with a query keeps `test/indents.jsonc` beside
`test/comments.jsonc`, run by a generic product test through the real
editor view on the registered language. A case names `before` and `after`
lines with `‸` marking the caret (`|` is a Rust closure), and one of three
actions: `enter`, `type` (text typed at the caret), or `reindent` (the
lines marked by a selection, or the caret's line). Evaluator details are
covered by focused unit tests against inline queries.

## Exclusions

- Reindenting pasted text: D135.
- An `editor.autoIndent` setting to turn this off or limit it: D136.
- Queries for the `@knot/language-others` grammars: D132, with their other
  editing data.
- Comment continuation on Enter and `onEnterRules`: D131.
- Auto-closing and matching brackets: D075.
- Folds and text objects from queries: D066.

## Deferred decisions

- Per-predicate choice between native evaluation and rewriting the adapted
  queries, settled in checkpoint 1.
- How `@indent.align` mixes tabs and spaces in a tab-indented document:
  checkpoint 1 follows nvim (tabs, then spaces to the column) unless it
  misbehaves in practice.

## Checkpoints

### 1. Indent query, evaluator, and Reindent Lines (Rust) — ✅ approved

Language registration takes an optional `queries.indents`, evaluated
natively by `src/app/syntax/indent.rs` through `SyntaxState::reindent` and
`line_indent`. Reindent Lines (`editor.reindent-lines`) applies it; blank
lines among several selected lines stay as they are. `@knot/language-rust`
ships an adapted query: arguments indent one level as rustfmt lays them
out, since upstream uses no `@indent.align` for Rust; raw strings are
ignored rather than auto-indented; and block comments are
`@indent.auto`. `@indent.zero` applies to the leaf token at the line's
start, as in nvim. Packages keep cases in `test/indents.jsonc`, run by
`src/app/product/tests/language_indents.rs`; a language with an indent
query must have cases.

### 2. Enter from the query, brackets, and blank lines — ✅ approved

Enter commits the break with the kept indentation, then
`EditorView::apply_pending_indent` corrects the new line once the tree is
current at the same revision, immediately or after a background parse, and
joins the change to Enter's undo step through
`BufferModel::amend_last_edit_from_view`; any later commit drops the
request. Enter between a `newline` bracket pair opens an indented middle
line in every language with bracket data, a whitespace-only line keeps none
of its whitespace, and unclosed brackets inside a parse error indent the
lines after them (see Design). The Rust `indents.jsonc` carries `enter`
cases checked with the caret.

### 3. Outdent while typing

Deliverables:

- Typing at the start of a line's content reindents it when the typed text
  completes an `@indent.end`, `@indent.branch`, or `@indent.dedent` node,
  only to a shallower indentation, joined to the typing undo group.
- `type` cases in the Rust `indents.jsonc` (`}`, `)`, `]`, closing a `match`
  arm block) and negative cases where typing must not change indentation.

Validation: the runner's `type` cases; an editor test that IME composition
and typing in the middle of a line never reindent.

Review gate: typing behavior approved before the remaining languages adopt
queries.

**Implementation context:** the typing path is `replace_text_in_range` in
`src/app/editor/input.rs`, which reaches `replace_text_with_grouping` with
`EditGroupKind::Typing`; reuse checkpoint 2's request so the trigger runs on
the reparsed tree.

### 4. Bundled language queries

Deliverables: `indents` queries adapted from nvim-treesitter and
`indents.jsonc` cases (enter, type, reindent) for TypeScript, JavaScript,
and JSX (shared ECMAScript queries), Python (`else`/`elif`/`except`
outdent, hanging and aligned brackets, dedent after `return`), TOML, and
Markdown where nvim-treesitter has one, including a Rust code fence inside
Markdown indenting as Rust.

Validation: every package's cases; extension package tests.

Review gate: daily editing in each language approved.

**Implementation context:** `extensions/@knot/language-typescript/src/`
(ECMAScript and JSX query modules shared across its languages),
`extensions/@knot/language-python/src/`, `language-toml`, and
`language-markdown`, each with `src/languages.ts` and `test/`.

### 5. Documentation

Deliverables: the extension guide's language section documents `indents`
and the capture list; [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages)
and [editor views](../architecture/workbench-and-lifecycle.md#editor-views)
describe the evaluator, the auto-indent request after parse, and the typing
trigger; [decisions](../decisions.md#language-editing-configuration) records
the nvim capture set, inserting before computing, and the string and comment
rules; the daily-driver item is marked ✅ and this plan removed.

Review gate: documentation approved.
