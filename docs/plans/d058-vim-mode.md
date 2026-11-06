# D058 — Vim mode

Status: 🚧 checkpoints 1–4 complete. Item of the [editor API](editor-api.md) plan; brief and
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
  - global marks across restarts and for closed files (D152)
  - smaller Vim commands outside this scope, such as `r<cr>`, sentence and
    tag motions and objects, `gJ`, and a repeated `i(` growing a visual
    selection to the enclosing pair (D151)
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
  filesystem, and `":` holds the last Ex command line once checkpoint 6 adds
  the Ex line.

### Marks and the jumplist

- **Buffer marks** `a`–`z` are `buffer.createMark` marks keyed by document.
- **Global marks** `A`–`Z` keep a document ID and a mark, and jump with
  `documents.present`. They last while their document is open (D152).
- **The jumplist** is per editor, holds up to 100 marks, and records the jumps
  Vim records (`G`, `gg`, `%`, `{ }`, `H M L`, mark jumps, and `:<line>`).
  As with Vim's default `jumpoptions`, a jump appends at the end wherever
  `ctrl-o` stepped back to, and a line is held once.

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
  today, so checkpoint 6 adds the smallest generic primitive for it. It
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

### 2. Operators, text objects, and registers ✅

Complete and approved after dogfooding: operator-pending mode for `d c y > <
= gu gU g~` over motions, text objects, and doubled lines, with an underline
caret while an operator waits; `x X s S D C Y r ~ J`, `f F t T ; ,`, `%`, and
`{ }`; and the register model with clipboard linewise detection and
persistence. `normal.ts` runs commands, and `objects.ts` and `registers.ts`
are pure. Text objects count brackets and quotes in the text, as Vim does,
while `%` uses the syntax-aware native jump and counts text only where it
finds no partner. Characterwise text ending at a line's start becomes
linewise from the indentation, so `d}` and `ci{` take whole lines. An undo
group opened through a handle restored no caret, because extension edit
batches record no view; the group now records the editor's selection when
it opens and closes ([core and buffers](../architecture/core-and-buffers.md)).
The case runner releases each case's document, which every later case
otherwise paid for, until cases outran the key hold's timeout. Left out
(D151): `r<cr>`, and Vim's `J` rule of no space before `)`.

### 3. Visual modes ✅

Complete and approved after dogfooding: `v` and `V` with every motion, `o`,
`gv`, operators on the selection, and Escape. Outside selections such as a
mouse drag enter visual mode, and a click leaves it. `visual.ts` keeps the
selection's ends as characters and shows them as an editor selection through
the character under the caret, or through whole lines; operators reuse normal
mode's code. The selection listener tells the package's own selections from
outside ones by comparing with the one it last set. Also in scope: text
objects that select or extend, `u U ~ J r` and the linewise `D X Y C S R`,
counts on `>` and `<`, `p` keeping the replaced text while `P` leaves the
registers, `v$` taking the line break, and `gv` over `buffer.createMark`.
Dogfooding found the arrows collapsing the selection, so the arrows, Home,
and End are now Vim motions outside insert mode. Visual `I` and `A` wait for
D142. Cases cover the visual motions and operators, and a product test drags
with the mouse.

### 4. Marks and the jumplist ✅

Complete and approved after dogfooding: `m{a-z}` buffer marks and `m{A-Z}`
global marks over `buffer.createMark`, with `'` and `` ` `` as motions,
operator targets, and visual motions; a global mark in another document
presents it with `documents.present`, as a plain motion only. `''`,
``` `` ```, and `m'` use the position before the latest jump, and each
editor's jumplist on `ctrl-o` and `ctrl-i` or Tab records `G`, `gg`, `%`,
and `{ }` and mark jumps; later checkpoints record theirs. `jumps.ts` is
pure. As with Vim's default `jumpoptions`, a jump appends at the end after a
step back rather than truncating, and the list holds a line once, so
`ctrl-o` never returns along one line. A mark at a line's start survived
`dd`, because a zero-width anchor at a deletion's start stays, so linewise
deletes take the line break before their lines. Global marks end with their
document and do not persist (D152).

### 5. Scrolling 📝

Deliverables:

- `ctrl-d ctrl-u ctrl-f ctrl-b ctrl-e ctrl-y` with counts.
- `zz zt zb`, and `z<cr> z. z-`, which also move to the first non-blank.
- `H M L` with counts, as motions and operator targets, recorded in the
  jumplist.

Validation:

- Cases for the caret after each scroll and `H M L` where the case's view
  height makes them deterministic; the runner learns to set the visible
  height or skip line expectations that depend on it.
- Product tests for `zz` centering whatever the caret's distance from the
  middle, `zt` and `zb`, and `ctrl-d` keeping the caret's screen offset.

Review gate: dogfooding.

Implementation context:

- `editor.visibleLines`, `scrollBy`, and `setSelection` reveal. `zz` scrolls
  by the caret's offset from the middle of `visibleLines()`, so it always
  centers. `H M L` count buffer lines, so they are approximate under soft
  wrap until D112.
- `ctrl-d` and `ctrl-u` scroll half the visible lines and move the caret by
  the same amount, as Vim's default `scroll` does.

### 6. Ex line 📝

Deliverables:

- `:` opens an input, and `:w`, `:q`, `:q!`, `:wq`, `:x`, `:e <path>`,
  `:<line>`, and `:set` for `number`, `relativenumber`, `wrap`, `tabstop`,
  `shiftwidth`, and `expandtab` with their `no` forms.
- The `":` register holds the last command line.
- The `:q!` primitive: closing a tab without the Save prompt, with the exact
  primitive settled here.
- `:<line>` is recorded in the jumplist.

Validation:

- Unit tests for parsing command lines and `:set` arguments.
- Cases for `:<line>` and `":p`.
- Product tests for `:w`, `:q` on a dirty document prompting, `:q!`, `:e`
  with a relative path and from an untitled document, and `:set` writes.
- A Rust test for the close primitive, including that recovery still holds
  the text until the close commits.

Review gate: dogfooding.

Implementation context:

- `workbench.showInput`, `workbench.open`, `file.save`, and
  `workbench.close-tab`.
- `:set` writes document-scoped settings, as the status bar's indentation
  picker does; `number` and `relativenumber` write `editor.lineNumbers`.
- The close primitive belongs to protected closure in `src/app/product/`
  (`closure.rs`); see
  [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

### 7. Window commands 📝

Deliverables:

- `ctrl-w` followed by `h j k l`, `s`, `v`, `c`, `o`, and `w`, also with
  ctrl held on the second key (`ctrl-w ctrl-h`), and the pending `^W` shown
  in the status bar.

Validation:

- A product test for each `ctrl-w` command over a split window, and that a
  `ctrl-w` left pending does not swallow the next key's meaning.

Review gate: dogfooding.

Implementation context:

- `ctrl-w` maps to `workbench.focus-pane-*`, `split-horizontal`,
  `split-vertical`, `close-pane`, the commands closest to `only`, and
  `next-recent-tab`, resolved against the catalog at implementation. A
  command with no counterpart gets a roadmap entry rather than native work
  here.

### 8. Documentation and plan removal 📝

Deliverables:

- Architecture and decision updates for the package and the primitives it
  added or changed.
- The extension guide mentions the package.
- Roadmap entries for any deferred decision below that was deferred instead
  of settled, and a check that every exclusion and left-out has one.
- The index plan marks D058 complete, and this plan is removed.

Validation:

- `just check`, and the docs' links resolve.

Review gate: a final dogfooding pass with every `Try:` command, then remove
the plan.

## Deferred decisions

- The exact `:q!` primitive, either a discard option on closing a tab or a
  separate command, chosen at checkpoint 6.
- Whether `ctrl-v` stays native paste in normal mode or is reserved for
  visual-block mode. D142 settles it; until then it stays native.
