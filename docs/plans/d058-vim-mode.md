# D058 — Vim mode

Status: 🚧 checkpoint 1 complete; checkpoint 2 implemented, at its review gate. Item of the [editor API](editor-api.md) plan; brief and
guardrails in the [extensibility report](../reports/extensibility.md).

## Outcome

A bundled `@knot/vim` package that its author can use daily for ordinary code
editing: normal, insert, visual, and visual-line modes; operators with counts,
motions, and plain text objects; registers; marks; scrolling; window commands;
and a small Ex line. It is built only on public APIs, like any installed
package. A `vim.enabled` setting, off by default, turns it on and off at
runtime, and scoped values turn it off for a language or view.

This is a smaller first version of the index plan's D058 scope. It starts
before D112, D066, and D143, which it no longer waits for, and leaves out
everything that needs Find (D031) or bulk edits (D092).

## Scope

- **Modes:** normal, insert, visual (`v`), visual-line (`V`), and
  operator-pending.
- **Counts** on motions, operators, and most commands (`3w`, `d2w`, `2dd`,
  `5x`). Counts on insert commands (`3ihi<Esc>`) are left out because they
  replay inserted text.
- **Motions:** `h j k l`, `gj gk`, `w W b B e E ge gE`, `0 ^ $`, `gg G` with
  counts, `f F t T ; ,`, `%`, `{ }`, and `H M L`.
- **Operators:** `d c y`, `> <`, `=`, `gu gU g~`, doubled for lines (`dd`,
  `cc`, `yy`, `>>`, `==`…), plus `x X s S D C Y r ~ J`, `p P`, `o O`, `i a I
  A`, `u`, and `ctrl-r`.
- **Text objects:** `iw aw iW aW`, `i( a(` with `b`, `i{ a{` with `B`, `i[
  a[`, `i< a<`, `i" a"`, `i' a'`, ``i` a` ``, and `ip ap`.
- **Registers:**
  - unnamed, `"+`, and `"*`, all on the system clipboard
  - `"a`–`"z`, with `"A`–`"Z` appending
  - `"0` for the last yank, `"1`–`"9` for deletes, `"-` for small deletes
  - `"_`, `"%`, and `":`
  - named and numbered registers persist across restarts in extension storage
- **Marks:** `m{a-z}` in a buffer, `m{A-Z}` across open documents, `'` and
  `` ` `` jumps, `''` and ``` `` ``` to the last jump, and a jumplist on
  `ctrl-o` and `ctrl-i`.
- **Scrolling:** `ctrl-d ctrl-u ctrl-f ctrl-b ctrl-e ctrl-y`, and `zz zt zb`.
- **Windows:** `ctrl-w` followed by `h j k l`, `s`, `v`, `c`, `o`, and `w`,
  over the existing workbench commands.
- **Ex line:** `:w`, `:q`, `:q!`, `:wq`, `:x`, `:e <path>`, `:<line>`, and
  `:set` for `number`, `relativenumber`, `wrap`, `tabstop`, `shiftwidth`, and
  `expandtab`, with their `no` forms.
- **Presentation:**
  - a status item showing the mode (`-- INSERT --`, `-- VISUAL --`,
    `-- VISUAL LINE --`, blank in normal mode) and the pending keys, as
    `showcmd` does
  - a block caret in normal and visual mode, a bar in insert mode, and an
    underline while an operator is pending

## Exclusions

These stay deferred:

- **Search:** `/ ? n N * #` and `:noh` are D150, after Find (D031), so Vim
  search reuses its match layer instead of building a second engine.
- **Substitution:** `:s` and `:%s` are D150 too. They are whole-buffer edits
  and wait for D092.
- **Folding commands:** D066 adds them to this package when it lands.
- **Smaller left-outs:**
  - `.` repeat, `q` macros, and the `".` register (D113)
  - visual-block mode (D142)
  - tree-sitter text objects (D144)
  - the broad Ex set (D146)
  - Ex completion (D143)
- **Line numbers:** relative numbers are `editor.lineNumbers: "relative"` in
  personal config, so the package does not set them.

Every exclusion is tracked in the roadmap or the editor API plan. A gap found
during implementation gets its entry when it is found.

## Design

### Following editors

The package defines `vim.enabled` and observes it. While the setting resolves
true for an editor, the package follows that editor through a long-lived
handle:

- It attaches on `activeTargetChanged` and keeps per-editor state keyed by the
  handle: mode, pending input, visual anchor, jumplist, and buffer marks.
- It re-resolves the setting when an editor attaches and when the setting
  changes, so a language-scoped `false` leaves that editor alone.
- Disabling the setting clears every context, caret style, and undo group,
  then disposes the handles.

### Key input

Two key contexts: `vim.normal` for every mode except insert, and `vim.insert`.

- **Typed text in `vim.normal`** routes through one `bindText` command into a
  JavaScript state machine. The state machine parses register, count,
  operator, motion or text object, and the mode it is in. Native key sequences
  are not used, because counts cannot be enumerated and gpui drops a pending
  sequence after one second (see
  [commands and keymaps](../decisions.md#commands-and-keymaps)). Ordered key
  delivery keeps later keys behind the running command.
- **Non-text keys** get explicit context bindings that feed the same state
  machine: `escape`, `enter`, `backspace`, the `ctrl-` scrolling and history
  keys, and `ctrl-w` sequences.
- **Insert mode** is native typing with every native binding. Only `escape`,
  and `ctrl-[` where it arrives as a key, return to normal mode.
- **Unbound chords** such as ⌘S stay native in every mode.

### Edits, motions, and the caret

- **Undo:** entering insert opens an undo group and leaving it closes the
  group. Each insert session, and each change (`ciw…<Esc>`), is therefore one
  undo step. Multi-batch operators open their own group.
- **`j` and `k`** move by buffer lines in JavaScript toward a wanted screen
  column, with tabs reaching the next stop, and `$` makes the wanted column
  the line end, as Vim's `curswant` does. Native logical movement could not
  keep it: every caret clamp resets the native goal column. `gj`/`gk` invoke
  the native row commands, so shaping and soft wrap stay native. When an
  operator uses `j`/`k`, the range is computed by buffer lines, since the
  operation is linewise.
- **`o` and `O`** break the line with native `editor.insert-newline`, so a
  language's indent query places the new line. Where the language has no
  indent query, and above the first line, `O` copies the current line's
  indentation instead, as Vim's autoindent does. Escape on an opened line
  left blank removes its indentation.
- **Undo and redo** restore the caret from when the change's undo group
  opened and closed, which the editor handle records, where Vim puts it at
  the change's start; the range is not available to extensions.
- **`%`** uses native `editor.jump-to-matching-bracket`, which is
  syntax-aware.
- **`>`, `<`, `=`, and `J`** select their range and invoke the native
  indent, outdent, reindent, and join commands, so they follow the
  language's indentation rules.
- **Every other motion and text object** is plain text math in JavaScript
  over line snapshots, in pure modules with no host calls.
- **Caret clamp:** after every normal-mode action, the caret is moved off the
  line terminator except on an empty line. Leaving insert mode moves the
  caret one character left, as Vim does.
- **Visual mode:** the package keeps its own visual anchor and sets a Knot
  selection that includes the character under the caret. `V` extends the
  selection to whole lines.
- **Outside selections:** a selection made outside Vim, such as a mouse drag
  or Select All, enters visual mode through `onDidChangeSelection`.

### Registers

- The unnamed register, `"+`, and `"*` read and write the clipboard, as
  Vim's `clipboard=unnamedplus` does, so a named yank leaves the clipboard
  alone.
- Each yank or delete remembers its text and whether it was linewise. When the
  clipboard still holds that exact text, `p` pastes linewise; otherwise it
  pastes characterwise.
- Deletes follow Vim's rules:
  - a linewise or multi-line delete shifts `"1`–`"9`
  - smaller deletes go to `"-`
  - yanks go to `"0`
- Named and numbered registers are one storage record, written when they
  change.
- `"%` reads the document's path, or its URI outside the local
  filesystem, and `":` holds the last Ex command line once checkpoint 4 adds
  the Ex line.

### Marks and the jumplist

- **Buffer marks** `a`–`z` are `buffer.createMark` marks keyed by document.
- **Global marks** `A`–`Z` keep a document ID and a mark, and jump with
  `workbench.present`. They last for the session only.
- **The jumplist** is per editor, holds up to 100 marks, and records the jumps
  Vim records (`G`, `gg`, `%`, `{ }`, `H M L`, mark jumps, and `:<line>`).

### Ex line

- `:` opens `workbench.showInput` with no completion.
- Commands go to native commands where they exist: `file.save`,
  `workbench.close-tab`, `workbench.open`, and the `editor.go-to-line`
  behavior.
- `:set` writes document-scoped settings, as the status bar's indentation
  picker does.
- `:e` resolves a relative path against the document's directory, or the
  window's first root for an untitled document.
- `:q!` closes the tab without the Save prompt. No public API does that
  today, so checkpoint 4 adds the smallest generic primitive for it. It
  stays a closure that cannot lose text silently: unsaved-document recovery
  still holds the text until the close commits.

## Testing

The package's behavior is tested end to end through the real editor, as
native editing is. `extensions/@knot/vim/test/cases.jsonc` holds the cases: a
marked starting text, the keys in Vim notation, the expected text and caret,
and the mode. `src/app/product/tests/vim.rs` runs every case in a product
window with the bundled package enabled from `post-init.js`, pressing real
keystrokes without settling between them, so ordered key delivery, native
commands, undo groups, and indent queries all take part. Behavior that cases
cannot express, such as the status bar, caret shapes, soft wrap, and
enablement, gets its own product test there. `node:test` covers only pure
modules (motions, text math, the parser); fake-host tests that stub native
commands are not used.

## Checkpoints

### 1. Package, modes, and basic movement ✅

Complete and approved after dogfooding: `vim.enabled` with language-scoped
values, editor following, normal and insert modes with counts and
pending-key display, the motions `h j k l gj gk w W b B e E ge gE 0 ^ $ gg G
+ -` with Enter, Backspace, and Space, `i a I A o O` with one undo group per
insert session, `u` and `ctrl-r`, caret styles, and the mode status item.
Editing keys do nothing in normal mode. The modal example and its wiring are
removed. The cases found that held input overtook a held Enter or Backspace;
release now replays one input per deferred step
([ordered delivery](../architecture/commands.md#ordered-delivery)). The
package lives in `extensions/@knot/vim`: `extension.ts` follows editors and
runs commands, `keys.ts` parses, and `motions.ts` and `text.ts` are pure.

### 2. Operators, text objects, and registers 🔍

Deliverables:

- ✅ Operator-pending mode for `d c y > < = gu gU g~` over every motion and
  over the text objects, plus their doubled linewise forms, with an
  underline caret while an operator waits.
- ✅ `x X s S D C Y r ~ J`, `f F t T ; ,`, `%` (with a count, that percentage
  of the lines), and `{ }`.
- ✅ The register model, its clipboard and linewise rules, persistence, and
  `p` and `P` with counts.

Validation:

- ✅ Unit tests for every text object and the delete-register shifting rules.
- ✅ Cases for every operator over motions and text objects, `ciw…<Esc>` as
  one undo step, `dd` then `p`, registers, and `>` and `=` through the native
  commands on Rust and TypeScript.
- ✅ Product tests for register persistence across a package restart and for
  clipboard linewise detection.

Review gate: a dogfooding session on Rust and TypeScript files. ⏳

Implementation notes:

- `normal.ts` runs commands; `objects.ts` and `registers.ts` are pure. Text
  objects match brackets and quotes by counting text, as Vim does, while `%`
  uses the syntax-aware native jump and counts text only where it finds no
  partner. Operators follow Vim's operator-pending forms: `cw` changes to the
  word's end, `dw` stops at the line's end, and characterwise text ending at
  a line's start becomes linewise from the indentation, so `d}` and `ci{`
  take whole lines. `cc` and `S` keep the first line's indentation.
- An undo group opened through a handle restored no caret, since extension
  edit batches record no view; the group now records the editor's selection
  when it opens and closes ([core and
  buffers](../architecture/core-and-buffers.md)).
- The case runner releases each case's document; without it every later case
  slowed down, past the key hold's timeout after about 150 cases.
- Left out: `r<cr>`, and Vim's `J` rule of no space before `)`.

Implementation context:

- Pure modules for text objects and registers beside `motions.ts`.
- `storage` from `knot.d.ts` holds the register record.
- `workbench.readClipboardText` and the clipboard write API.
- Native `editor.indent-lines`, `outdent-lines`, `reindent-selected-lines`,
  `join-lines`, and `jump-to-matching-bracket` are invoked on a selection set
  by the package.

### 3. Visual modes 📝

Deliverables:

- `v` and `V` with every motion, `o` to swap ends, `gv`, operators on the
  selection, and `escape` back to normal mode.
- Outside selections enter visual mode, and switching between `v` and `V`
  works.

Validation:

- Cases for every motion and operator in `v` and `V`, `o`, and `gv`; the
  runner learns to expect a selection, which the two-marker notation already
  writes.
- A product test for a mouse drag entering visual mode and `Vjd`.

Review gate: dogfooding.

Implementation context:

- The `onDidChangeSelection` listener in the follower distinguishes the
  package's own `setSelection` calls from outside changes by comparing with
  the selection it last set.

### 4. Marks, scrolling, windows, and the Ex line 📝

Deliverables:

- Buffer and global marks, `''`, and the jumplist.
- `ctrl-d ctrl-u ctrl-f ctrl-b ctrl-e ctrl-y`, `zz zt zb`, and `H M L`.
- `ctrl-w` commands.
- The Ex line and its commands, with the `:q!` primitive.
- Finishing work:
  - architecture and decision updates
  - the extension guide mentions the package
  - roadmap entries for any open decision below that was deferred instead
    of settled
  - the index plan marks D058 complete
  - this plan is removed

Validation:

- Cases for marks across edits, the jumplist, scrolling, and `:<line>`.
- Product tests for global marks across documents, `:w`, `:q!`, `:e`, `:set`
  writes, `zz`, and `ctrl-w`.
- A Rust test for the close primitive.

Review gate: a final dogfooding pass with every `Try:` command, then remove
the plan.

Implementation context:

- `editor.visibleLines`, `scrollBy`, and `setSelection` reveal. `zz` scrolls
  by the caret's offset from the middle of `visibleLines()`, so it always
  centers. `H M L` count buffer lines, so they are approximate under soft
  wrap until D112.
- `ctrl-w` maps to `workbench.focus-pane-*`, `split-horizontal`,
  `split-vertical`, `close-pane`, the commands closest to `only`, and
  `next-recent-tab`, resolved against the catalog at implementation.
- `workbench.showInput`, `workbench.open`, and `workbench.present`.
- The close primitive belongs to protected closure in `src/app/product/`
  (`closure.rs`); see
  [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Deferred decisions

- Whether global marks persist across restarts by URI and offset.
- The exact `:q!` primitive, either a discard option on closing a tab or a
  separate command, chosen at checkpoint 4.
- Whether `ctrl-v` stays native paste in normal mode or is reserved for
  visual-block mode (D142). Until then it stays native.
