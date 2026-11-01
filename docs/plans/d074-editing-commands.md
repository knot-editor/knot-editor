# D074 — Editing commands

Status: 🚧 in progress: checkpoint 1 approved; checkpoint 2 is next. Item of the
[daily-driver plan](daily-driver.md); builds on D073's
[language editing configuration](../architecture/semantic-ui.md#syntax-highlighting-and-languages).

## Goal

The everyday line and indentation commands of a code editor work as in VS
Code: Enter keeps the current indentation; Tab, Shift-Tab, and indent and
outdent commands handle lines and selections in the document's indentation
style, detected from its text; line and block comments toggle with the
language at the cursor, including inside injections such as Markdown code
fences; lines duplicate, delete, move, and join; Home is smart; and a drag
after a double- or triple-click extends by words or lines. Each command is
one undo step that restores the selection it started from.

## Design

- **One undo step per command.** A command computes all its edits and
  commits them as one batch through a model operation that, like
  `replace_from_view`, records the view state before and after, so undo and
  redo restore the selection. This is the native grouping D111 later lets
  extensions open across batches.
- **Indent unit.** A new `editor.insertSpaces` boolean setting, default
  true and scopable by language, beside `editor.tabSize`. When a document
  enters the collection from a resource, as language detection does, Knot
  detects tabs versus spaces from the leading whitespace of its indented
  lines; a detected style overrides `editor.insertSpaces`, and the size
  always comes from `editor.tabSize`. A document with no indented lines, or
  no clear majority, uses the setting. Untitled documents use the setting.
- **Indentation commands**, following VS Code:
  - Enter inserts the line terminator and the current line's leading
    whitespace.
  - Tab with no selection, or a selection within one line, inserts the
    indent unit (spaces to the next tab stop, or a tab). With a selection
    across lines, it indents every selected line. Shift-Tab always outdents
    the selected lines, or the caret's line, by one unit.
  - A selection ending at column 0 does not include that line.
  - Backspace in leading whitespace made of spaces deletes back to the
    previous tab stop.
- **Comments.** Toggle line comment runs D073's evaluator over the selected
  lines. Toggle block comment wraps the selection in the block delimiters,
  or unwraps it when the selection already starts and ends with them; with
  no selection it inserts an empty pair with the caret inside, as VS Code
  does. Both read the editing configuration of the deepest syntax layer at
  the selection start: each compiled syntax language carries its
  registration's editing configuration, so an injected layer needs no
  separate lookup. A language with no tokens makes the command a no-op.
- **Line commands**, all on the lines the selection touches: copy lines up
  or down (the selection moves to the copy below), delete lines, move lines
  up or down (no-op at the document edge), and join lines (the next line's
  leading whitespace becomes one space, none before an empty line).
- **Smart Home** moves to the first non-whitespace character, or to column 0
  when already there; a select variant extends.
- **Unit drag.** A double- or triple-click records its unit, and dragging
  extends the selection by whole words or lines from the clicked unit.
- **Bindings** are VS Code's defaults in the Knot preset:

  | Command | macOS | Windows | Linux |
  | --- | --- | --- | --- |
  | Toggle line comment | ⌘/ | ctrl-/ | ctrl-/ |
  | Toggle block comment | ⇧⌥A | shift-alt-a | ctrl-shift-a |
  | Indent / outdent lines | ⌘] / ⌘[ | ctrl-] / ctrl-[ | ctrl-] / ctrl-[ |
  | Outdent | shift-tab | shift-tab | shift-tab |
  | Move lines up / down | ⌥↑ / ⌥↓ | alt-up / alt-down | alt-up / alt-down |
  | Copy lines up / down | ⇧⌥↑ / ⇧⌥↓ | shift-alt-up / -down | ctrl-shift-alt-up / -down |
  | Delete lines | ⇧⌘K | ctrl-shift-k | ctrl-shift-k |
  | Join lines | ⌃J | palette only | palette only |
  | Smart Home | home, ⌘← | home | home |

  macOS keeps ctrl-a at column 0, as every Cocoa text field does.

## Exclusions

- Enter between a `newline` bracket pair opening an indented middle line,
  and language-aware indentation: D134.
- Multiple cursors and column selection: D109's multi-selection editing.
- Detecting the indent size, and EditorConfig: D090.
- Comment continuation on Enter: D131.
- Auto-closing and bracket matching: D075.

## Deferred decisions

- None.

## Checkpoints

### 1. Batch commits and indentation — ✅ approved

Commands commit through `BufferModel::apply_edits_from_view` and
`EditorView::commit_edits` (with `map_offset`), one undo step restoring the
selection; later checkpoints reuse that path. `editor.insertSpaces` and
detection live in `src/app/indentation.rs`: a style wins when it indents at
least twice as many of the first 10,000 lines as the other, ignoring
one-space alignment and ` *` continuations, and only documents with a
resource detect. Shift-Tab binds `editor.outdent-lines`; Enter copies the
indentation before the caret. Checkpoint 5 moves these rules into the
editor views reference and decisions.

### 2. Comment toggling

- Deliverables: Toggle line comment and Toggle block comment, reading the
  language at the selection start, with their bindings; the block toggle's
  evaluator beside `toggle_comments`.
- Validation: block toggle unit tests in
  `src/app/languages/editing/tests.rs`; block cases added to the bundled
  packages' `test/comments.jsonc` and its runner; editor view tests for
  both commands, an injected Rust fence inside Markdown, a language without
  tokens, and undo.
- **Implementation context:**
  - `src/app/languages/editing.rs`: the block evaluator.
  - `src/app/syntax.rs`: `SyntaxLanguage` carries the editing
    configuration from `CompiledLanguage::compile`
    (`src/app/languages.rs`); `SyntaxState` returns the deepest layer's
    language at a byte offset.
  - `src/app/product/tests/language_editing.rs` reads the block cases.
- Review gate: toggle comments in Rust, TypeScript, Python, TOML, and a
  Markdown file with code fences.

### 3. Line commands and Smart Home

- Deliverables: copy lines up and down, delete lines, move lines up and
  down, join lines, and Smart Home with its select variant, with bindings.
- Validation: editor view tests for each command with and without a
  selection, at the first and last line, on a last line without a
  terminator, with CRLF text, and undo.
- **Implementation context:** `src/app/editor/editing.rs` and
  `native_commands.rs`, using checkpoint 1's batch commit; the Smart Home
  arm replaces `line-start` for the bound keys only.
- Review gate: daily use of the line commands.

### 4. Unit drag

- Deliverables: dragging after a double- or triple-click extends by words
  or lines.
- Validation: editor view tests that drive mouse down with `click_count`
  2 and 3 and then mouse moves, in both directions.
- **Implementation context:** `src/app/editor/input.rs`, `on_mouse_down`
  and `on_mouse_move`; the drag keeps its unit next to `drag_anchor`.
- Review gate: try word and line drags.

### 5. Documentation

- Deliverables: the editor views section of the workbench reference
  describes indentation detection, the commands' undo step, and
  language-at-cursor comments; a decision records the VS Code behavior and
  bindings and indentation detection. Remove this plan and mark D074 ✅ in
  the daily-driver plan.
- **Implementation context:**
  `docs/architecture/workbench-and-lifecycle.md#editor-views`,
  `docs/decisions.md`, `docs/plans/daily-driver.md`.
- Review gate: final review of D074.
