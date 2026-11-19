# D151 — Smaller Vim commands

Status: 🚧 in progress; checkpoints 1 and 2 approved. Roadmap item [D151](../roadmap.md#vim-mode).

## Goal

Daily Vim use stops hitting small gaps in `@knot/vim`: counted `ctrl-w` focus
keys, a line break from `r<cr>`, Vim's `J` and `gJ`, the `_` and `|` motions,
`gi`, `ga`, the paste variants `gp`, `gP`, and `]p`, growing `i(` and `a(` in
visual mode, and sentence motions and objects.

## Design

- Everything lives in the Vim package over existing primitives, with cases
  beside the others in `extensions/@knot/vim/test/`. The one native change is
  an optional `count` on the directional focus commands.
- **Counted focus.** Every child command resolves the pane its parent started
  in, so the package cannot repeat `workbench.focus-pane-*` to walk several
  panes. `FocusScope` gains an optional `count`; `focus_toward` steps from the
  pane each step lands on and stops at the edge. `ctrl-w` `h j k l` pass the
  count through.
- **`J`** follows Vim: no space before `)`, none when the next line starts with
  `)` or the current line ends in whitespace; `gJ` joins with none.
- **`gi`** needs a last-insert position, which the package does not track yet;
  it records one when insert mode ends, per buffer, beside the other marks.
- **`]p`** re-indents the pasted lines to the current line, over the existing
  indent primitives.

## Exclusions

- `it` and `at` wait for D144's syntax-tree queries; `ctrl-w n` waits for
  D061's empty-pane handling. Both stay on the roadmap with those items.
- `ctrl-w x`, `r`, and `R` (swap and rotate panes) need new workbench layout
  operations and move to D208.

## Checkpoints

### 1. Counted `ctrl-w` focus ✅

- ✅ Add `count` to `FocusScope` and loop in `focus_pane_toward`; pass the
  count from `windowCommand` for the four focus actions; update the command
  argument docs.
- ✅ Validation: a Rust workbench test (several panes, stopping at the edge,
  `withinArea`), and a Vim test through the real editor.
- ✅ Review gate: `3<c-w>l` and `2<c-w>j` land where Vim would.
- **Implementation context:** `src/app/product/commands.rs` (`FocusScope`,
  `focus_toward`), `src/app/product/navigation.rs` (`focus_pane_toward`),
  `WINDOW_COMMANDS`, `REPEATED`, and `windowCommand` in
  `extensions/@knot/vim/src/extension.ts`, `WindowAction` in `keys.ts`;
  command docs under `docs/extension-api/`.

### 2. Small edits, keys, and paste variants ✅

- ✅ `r<cr>`, `J` and `gJ`, `_`, `|`, `gi`, `ga`, `gp`, `gP`, `]p` (with `[p`,
  `[P`, and `]P`).
- ✅ Validation: key-parser cases in `keys.test.ts`, and end-to-end cases
  through the editor in `test/cases/` (`operators`, `motions`, `insert`) and
  `ga` in `vim.rs`.
- ✅ Review gate (`try.small-edits`, approved): try commands for each key, including counts and visual mode
  where Vim defines them.
- **Implementation context:** `keys.ts` (`g`, `[`, `]` prefixes, `r` at line
  296, `join` and `put` commands), `normal.ts` (`put` and `join` handlers at
  ~208, 274, 312), `motions.ts`, `registers.ts` for put text; the
  last-insert position goes where `normal.ts` stores marks (~line 690).

### 3. Growing `i(` and `a(` in visual mode

- A repeated pair object with a non-empty selection selects the enclosing
  pair, for each bracket and quote object that nests.
- Validation: cases in `objects.test.ts` and visual-mode editor tests.
- Review gate: repeated `vi(` and `va(` grow outward as in Vim.
- **Implementation context:** `objects.ts` (`textObject`), `visual.ts`
  (`select-object` command), `keys.ts` (`select-object`).

### 4. Sentence motions and objects

- `(`, `)`, `is`, `as`, with counts and as operator targets.
- Validation: cases beside the paragraph and word ones in `motions.test.ts`
  and `objects.test.ts`, covering blank lines, abbreviations Vim treats as
  ends, and closing quotes or brackets after the period.
- Review gate: `das`, `cis`, and `)` behave as in Vim on prose and on code
  comments.
- **Implementation context:** `motions.ts`, `objects.ts`, the motion and
  object tables in `keys.ts`.

## Deferred

Tag objects (D144), `ctrl-w n` (D061), pane swap and rotate (D208).
