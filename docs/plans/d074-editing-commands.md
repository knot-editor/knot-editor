# D074 — Editing commands

Status: 🚧 in progress: checkpoint 4 awaits its review gate. Item of the
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

### 2. Comment toggling — ✅ approved

Each `SyntaxLanguage` carries its registration's editing configuration, and
`SyntaxState::language_at` finds the deepest layer at the selection start;
a buffer without syntax has no tokens. `markdown_inline` registers
Markdown's block comment because it is the layer inside every paragraph.
Block toggle skips surrounding whitespace and also unwraps when the
delimiters sit just outside the selection, as VS Code does, so a second
toggle reverses a wrap. Block cases live beside the line cases in each
package's `test/comments.jsonc`. Linux binds ctrl-shift-a through a
Linux-only preset table. Checkpoint 5 moves these rules into the semantic
UI and editor views references and decisions.

### 3. Line commands and Smart Home — ✅ approved

Line commands live in `src/app/editor/lines.rs` and act on the lines the
selection touches. Move lines deletes and reinserts the neighbouring line so
the moved lines keep their anchored contributions; copies and moved
neighbours take the buffer's line ending. Delete lines keeps the caret's
column on the line that takes their place; Join lines also drops trailing
whitespace and leaves the caret at the join. Smart Home binds home,
shift-home, ⌘←, and ⇧⌘←, while ctrl-a keeps column 0. Checkpoint 5 moves
these rules into the editor views reference and decisions.

### 4. Unit drag — ✅ implemented, awaiting review

- Deliverables: dragging after a double- or triple-click extends by words
  or lines.
- Validation: editor view tests that drive mouse down with `click_count`
  2 and 3 and then mouse moves, in both directions.
- **Implementation context:** `src/app/editor/input.rs`, `on_mouse_down`
  and `on_mouse_move`; the drag keeps its unit next to `drag_anchor`.
- Review gate: try word and line drags.
- Notes:
  - ✅ `drag_anchor` became a `DragAnchor` holding the unit and the clicked
    unit's range. A drag keeps the clicked word or line selected and
    extends to the far edge of the unit under the pointer, in either
    direction; character drags are the same rule with an empty unit.
    Shift-click still extends by characters.
  - ✅ Review feedback: Unicode word segmentation kept `some.thing` and
    `a:b` as one word. Words now follow VS Code's default word separators
    (`text::word_segments`): each run of separators is its own unit,
    splitting the Unicode segments, while word characters keep Unicode
    segmentation for ideographs and emoji. Double-click, word drags, and
    word movement share the definition; movement stops at word and
    separator-run edges and skips whitespace. Per-language word characters
    stay with D131.

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
