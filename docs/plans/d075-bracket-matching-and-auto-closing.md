# D075 — Bracket matching and auto-closing

Status: 🚧 approved, not started. Item of the
[daily-driver plan](daily-driver.md); consumes the bracket data of D073
([language editing configuration](../decisions.md#language-editing-configuration)).

## Outcome

Editing code with brackets behaves as it does in VS Code:

- With the caret next to a bracket, it and its partner are highlighted.
  Jump to Matching Bracket (⌘⇧\ on macOS, ctrl-shift-\ elsewhere) moves to the
  partner, or to the enclosing pair's closer when the caret is not next to a
  bracket.
- Typing an opener inserts its closer where VS Code's language-defined rules
  allow. Typing that closer steps over it, and Backspace inside the empty pair
  deletes both characters.
- Typing an opener with a selection wraps the selection and keeps the inner
  text selected.
- `editor.autoClosingBrackets` and `editor.autoSurround`, language-scoped
  booleans defaulting to true, turn auto-closing and wrapping off.

## Design

The behavior is native and the data stays in language registrations. Typing
inserts the closer in the same commit and undo step as the opener, so it
cannot wait on an asynchronous extension turn; painting and raw input never
call JavaScript; and the extension API has no caret, typed-text observation,
undo grouping, or syntax tree (D109, D110, D111, D066). Packages and personal
config change pairs, `notIn`, and `autoCloseBefore` through the existing
registration, and the settings turn the behavior off per language.

### Matching

- Matching uses the syntax tree and the registered pairs, with no new query.
  At an offset, take the leaf token ending at it, else the one starting at
  it, whose text is a registered `open` or `close` of the layer's language at
  that offset (pairs whose open equals close never match). Its partner is
  the first child (for a closer) or last child (for an opener) of the same
  parent node holding the paired text. A bracket inside a string or comment
  is part of a string or comment leaf, so it never matches.
- A token whose parent lacks the partner, as inside a parse error, has no
  match. A document without a language matches nothing.
- The highlight shows only when the caret is next to a bracket; the bracket
  before the caret wins when both sides hold one. The enclosing pair is never
  highlighted.
- Jump to Matching Bracket with the caret next to a bracket moves the caret
  to the partner, on the side VS Code places it (verify against VS Code while
  implementing); otherwise it moves to the
  closer of the innermost enclosing registered pair. It clears the selection.
- The highlight is a background behind both brackets in a new native color
  key, `editor.bracketMatch`, which the bundled themes set.

### Auto-closing

Following VS Code's `languageDefined` behavior and its `auto` overtype and
delete defaults:

- Typing (not composing) a character with no selection inserts a pair's
  closer after the caret when all of these hold:
  - the pair has `autoClose`, and the text before the caret followed by the
    typed character ends with `open`, so multi-character openers work;
  - the character after the caret is whitespace, the end of the line, or in
    `autoCloseBefore`;
  - the caret is not inside a syntax family the pair lists in `notIn`;
  - for a pair whose open equals its close, the character before the caret
    is not a word character, so `don't` stays one quote;
  - `editor.autoClosingBrackets` is true for the document's language.
- The closer joins the typing undo step, so one undo removes the pair.
- The view tracks the closers it inserted, as anchored positions, while the
  caret stays between the opener and the closer. Typing a tracked closer
  directly after the caret moves over it without editing. Backspace with the
  caret between a tracked closer and its adjacent opener deletes both in one
  edit. Leaving the pair, a selection, or an edit elsewhere drops the
  tracking; untracked closers are typed and deleted as ordinary text.
- The syntax family at the caret comes from the highlight captures covering
  it: a capture named `string` or `comment`, or a dotted child such as
  `string.special`, whose node strictly contains the caret, in the layer at
  the caret. This needs no per-grammar data because every bundled query uses
  nvim-treesitter's capture names.

### Wrapping

- Typing a registered opener while text is selected wraps the selection in
  the pair, including pairs with `autoClose: false`, and keeps the inner text
  selected, as one undo step. `editor.autoSurround` turns it off.

## Exclusions

- Bracket data for the languages in `@knot/language-others` (D132).
- Bracket pair colorization, guides, and highlighting the enclosing pair.
- Matching or auto-closing in documents without a language.
- Auto-closing after IME or dead-key composition: composition commits are not
  typing.
- Multiple selections, which do not exist yet.
- Syntax-scoped bracket data, such as inside JSX (D130).

## Deferred decisions

- None identified. Whether jump also selects to the bracket (VS Code's Select
  to Bracket) is left to daily use.

## Checkpoints

### 1. Matching, highlight, and Jump to Matching Bracket

Deliverables:

- A syntax-tree lookup returning the matching bracket ranges at an offset and
  the innermost enclosing pair, per the rules above.
- The `editor.bracketMatch` color key with native defaults, painted behind
  both brackets, recomputed when the caret moves or the tree changes, and
  values in every bundled theme.
- Jump to Matching Bracket as a native editor command with its Knot preset
  binding on each platform.

Validation: syntax unit tests (adjacency on either side, before-caret
preference, brackets in strings and comments, quotes excluded, injected
layers, parse errors, no language); editor view tests for the highlight
ranges and both Jump cases; keymap table tests for the binding.

Implementation context:

- Lookup beside `closes_line` and `line_indent` in `src/app/syntax.rs`
  (`layer_at`, `language_at`), exposed through `BufferModel` in
  `src/app/model.rs` like `editing_at`; tests in `src/app/syntax/tests.rs`.
  Brackets come from `layer.language.editing.brackets` as indentation reads
  them (`src/app/syntax/indent.rs`).
- Color key in the native table in `src/app/theme.rs` next to
  `EDITOR_SELECTION`; theme values in
  `extensions/@knot/themes/src/themes/*.ts`.
- Paint as a background like the selection in `src/app/editor/paint.rs`
  (`EditorPaintState`, `PaintColors`); cache the match on position change
  (`finish_position_change`) and when parsing finishes, so painting does no
  tree walk per frame.
- Command in `src/app/editor/native_commands.rs` (pattern of
  `editor.reindent-lines`), dispatched from `execute_editing_command` in
  `src/app/editor/editing.rs`; binding in the Knot preset under
  `src/app/commands/keymaps/` using `secondary-shift-\`.

Review gate: the highlight and Jump behave as in VS Code on the bundled
languages.

### 2. Auto-closing, overtype, paired delete, wrapping, and settings

Deliverables:

- Auto-closing on typed openers, tracked closers with overtype and paired
  Backspace, and wrapping a selection, per the rules above.
- The syntax family at an offset from highlight captures, consumed by
  `notIn`.
- `editor.autoClosingBrackets` and `editor.autoSurround` as native
  definitions, documented in `knot.d.ts`.

Validation: unit tests for the pure decision (which pair, closer insertion,
`autoCloseBefore`, word-before-quote, multi-character openers); syntax tests
for the family at an offset, including string boundaries and injections;
editor view tests for typing a pair, one undo, overtype only of tracked
closers, tracking dropped when the caret leaves, paired Backspace, wrapping
with the inner selection kept, `notIn` suppression, both settings scoped to a
language, and composition not auto-closing.

Implementation context:

- Pure decision functions on `EditingConfiguration` in
  `src/app/languages/editing.rs`, beside `toggle_comments`, taking the text
  around the caret and the syntax family so they need no buffer; tests in
  `src/app/languages/tests.rs` (or its `editing` test module).
- Family lookup in `src/app/syntax.rs` using the layer's highlight query and
  capture names (`paint_highlights` shows the cursor setup); exposed on
  `BufferModel`.
- Typing hook in `replace_text_in_range` / `replace_text_with_grouping`
  (`src/app/editor/input.rs`, `src/app/editor.rs`), keeping the `Typing`
  group and D134's `outdent_typed_line` after the edit; Backspace beside
  `tab_stop_deletion_range` in `src/app/editor/indentation.rs` and the
  deletion path in `src/app/editor/editing.rs`.
- Tracked closers as view-owned anchored positions, like the view's
  `position_range` (`add_view_position`, `resolve_view_position`,
  `remove_view_position` in `src/app/model.rs`), removed when the view drops.
- Settings constants and definitions in `src/app/settings.rs`, read per
  language like `EDITOR_INSERT_SPACES`; declarations in the `Settings`
  interface of `src/host/js/knot.d.ts`.
- Editor tests in `src/app/editor/tests/` (pattern of `indentation.rs`, which
  builds an `EditingDefinition` with `BracketPair`s).

Review gate: typing, overtype, Backspace, and wrapping feel like VS Code in
Rust, TypeScript, Python, and Markdown.

### 3. Package bracket cases and documentation

Deliverables:

- `test/brackets.jsonc` in each package with bracket data (Rust, TypeScript
  and JavaScript, Python, TOML, Markdown), run through an editor view by a
  generic product test: typing sequences with expected text and caret,
  Backspace, wrapping, and matching and Jump positions. Cases cover each
  package's departures, such as Rust's missing `'`, quotes in strings and
  comments, and Markdown quotes that only surround.
- Documentation: editor views and syntax sections of the architecture
  references, a decisions entry for native behavior over registered data,
  tree-based matching, and VS Code's tracked overtype; the extension guide's
  language registration section for what pairs, `notIn`, and
  `autoCloseBefore` now drive; the daily-driver item marked ✅ and this plan
  removed.

Validation: the product test over every package's cases; `cargo test` and
the extension package tests.

Implementation context:

- Runner beside `src/app/product/tests/language_indents.rs`, sharing its
  case format (`‸` caret markers, `languages`, `before`/`text`/`after`) and
  `start_bundled_packages` from `language_editing`; case files beside each
  package's `test/indents.jsonc`.
- Docs: `docs/architecture/workbench-and-lifecycle.md#editor-views`,
  `docs/architecture/semantic-ui.md#syntax-highlighting-and-languages`,
  `docs/decisions.md` (editing commands or language editing configuration),
  `docs/extension-api.md#register-a-language`.

Review gate: the cases read as the intended behavior for each language, and
the docs describe the shipped behavior.
