# D058 — Vim mode

Status: 📝 proposed. Item of the [editor API](editor-api.md) plan; brief and
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
handle, as `@example/modal`'s `follow.ts` does:

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
- **`j` and `k`** delegate to native `editor.move-down-logical` and
  `move-up-logical` through `commands.invoke`, and `gj`/`gk` to the row
  commands. Native code keeps the sticky column, shaping, and soft wrap. When
  an operator uses them, the range is computed by buffer lines, since the
  operation is linewise.
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

- The unnamed register, `"+`, and `"*` read and write the clipboard.
- Each yank or delete remembers its text and whether it was linewise. When the
  clipboard still holds that exact text, `p` pastes linewise; otherwise it
  pastes characterwise.
- Deletes follow Vim's rules:
  - a linewise or multi-line delete shifts `"1`–`"9`
  - smaller deletes go to `"-`
  - yanks go to `"0`
- Named and numbered registers are one storage record, written when they
  change.
- `"%` reads the document's URI and `":` holds the last Ex command line.

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

## Checkpoints

### 1. Package, modes, and basic movement 📝

Deliverables:

- `extensions/@knot/vim` with `vim.enabled`, editor following, the two
  contexts, and the state machine with counts and pending-key display.
- Normal ↔ insert through `i a I A o O` and `escape`, with an undo group per
  insert session and caret clamping. `u` and `ctrl-r` invoke native undo and
  redo.
- Motions `h j k l gj gk w W b B e E ge gE 0 ^ $ gg G`.
- Caret styles and the mode status item.
- `@example/modal`, its product test, and its documentation are removed.
  Nothing else lives under `examples/extensions`, so the example build
  wiring in `build.rs` and `scripts/extensions/tasks.mjs`, the `just`
  install target, and the repository-map and extension-guide references go
  too.

Validation:

- `node:test` unit tests for the motion and word modules and the
  state-machine parser.
- Fake-host flow tests for following, mode switching, enabling and disabling,
  and scoped disabling.
- A Rust product test that drives real keys and typed text through an editor
  view: counts, insert entry and exit as one undo step, the caret clamp, and
  `j` across soft-wrapped rows.
- The bundled-package load test lists `@knot/vim`.

Review gate: dogfood through `Try:` commands in `post-init.js` that enable
Vim globally and disable it for Markdown.

Implementation context:

- Mirror `extensions/@knot/status-bar` and `go-to-line` for the manifest,
  `tsconfig.json`, `src/main.ts` → `start()`, and `test/`. `build.rs` embeds
  every directory under `extensions/@knot` automatically.
- Port `examples/extensions/@example/modal/src/{follow,mode,motion}.ts` as
  the seed. Port `src/app/product/tests/modal_example.rs` to
  `src/app/product/tests/vim.rs`, loading the bundled package as
  `go_to_line.rs` and `status_bar_package.rs` do. Add the package to
  `bundled_packages_load_with_product_discovery` in
  `src/app/extensions/host.rs`.
- APIs:
  - `settings.define`, `settings.get`, and `settings.observe`
  - `workbench.editor`, `workbench.observe`, and `registerStatusItem`
  - `keymaps.bind` and `bindText` with `{ view: "editor", context }`
  - `commands.invoke`, plus `beginUndoGroup`, `setKeyContext`, and
    `setCursorStyle` on the editor handle
- The fake host is `scripts/extensions/testing/`. It already covers editors,
  settings, and status items (`editor.test.ts`, `settings.test.ts`,
  `status.test.ts`). Extend it only where a `not faked` failure demands it.
- Check that native `escape` bindings in the editor context (completion
  dismissal) still win when they apply.

### 2. Operators, text objects, and registers 📝

Deliverables:

- Operator-pending mode for `d c y > < = gu gU g~` over every motion and
  over the text objects, plus their doubled linewise forms.
- `x X s S D C Y r ~ J`, `f F t T ; ,`, `%`, and `{ }`.
- The register model, its clipboard and linewise rules, persistence, and `p`
  and `P` with counts.

Validation:

- Unit tests for every text object and the delete-register shifting rules.
- Flow tests for register persistence across a package restart in the fake
  host, and for clipboard linewise detection.
- A product test for `ciw…<Esc>` as one undo step, `dd` then `p`, and `>`
  through the native indent.

Review gate: a dogfooding session on Rust and TypeScript files.

Implementation context:

- Pure modules for text objects and registers beside `motion.ts`.
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

- Flow tests for anchor and inclusive-caret mapping and for entering visual
  mode from an outside selection.
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

- Flow tests for marks across edits and documents, the jumplist, Ex parsing,
  and `:set` writes.
- A product test for `:w`, `:q!`, `:e`, and `zz`.
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
