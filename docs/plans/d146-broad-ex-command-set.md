# D146: Broad Ex command set

Status: 🚧 in progress; checkpoint 1 implemented, awaiting its review gate. Roadmap item [D146](../roadmap.md).
Builds on the [bundled Vim mode](../decisions/bundled-packages.md#bundled-vim-mode)
in [`extensions/@knot/vim/`](../../extensions/@knot/vim/).

## Goal

Daily Vim use in Knot no longer reaches for the mouse or the palette for
files, buffers, and line edits. The Ex line gains:

- `:e!` (revert to the file), `:w <file>`, `:w! <file>`, `:sav[!] <file>`;
- `:wa`, `:qa`, `:qa!`, `:wqa`, `:xa`, backed by a native **Save All**;
- `:bn`, `:bp`, `:bd[!]`, `:b <name>`, `:enew`, `:new`, `:vnew`,
  `:sp <file>`, `:vs <file>`, `:only`, `:close`, `:u`, `:red`, `:pwd`;
- the line-based range commands `:d`, `:y`, `:m`, `:t`/`:co`, `:>`, `:<`,
  `:j`;
- `:sort` with `!`, `i`, `n`, and `u`.

## Design

All Ex parsing and behavior stays in the Vim package over public APIs; Knot
gains only the generic primitives the package cannot build, each usable by
any extension.

- **`documents.reload(id)`** exposes the existing undoable reload behind
  external-change handling (`document_watch::reload`): the text becomes the
  file's as one line-diff edit that Undo takes back, so `:e!` is safer than
  Vim's. It resolves once the reload commits, and rejects for a document
  without a file. `:e! <file>` discards the current document's changes and
  opens the file, as Vim does.
- **`documents.save(id, { uri? })`** saves a document through the same path
  as Save and Save As, without a dialog; with `uri` it retargets the document
  to that file, as Save As does. It backs `:sav` and `:w <file>` on an
  untitled document, which in Vim also names the buffer.
- **`files.writeText(uri, text)`** creates or overwrites a file. It backs
  `:w <file>` on a document that already has a file: Vim writes a copy, and
  the document keeps its file and its unsaved state.
- An existing target fails with `E13: File exists` unless the command has
  `!`; the package checks with `files.stat`.
- **Native `file.save-all`** ("Save All") saves every dirty document with a
  file, in the palette and the File menu. Untitled documents are skipped, as
  Vim's `:wa` skips unnamed buffers; quitting still protects them.
- **Quitting.** `:qa` runs `application.quit`, which keeps unsaved text for
  the next launch, so `:qa!` behaves the same. `:wqa` and `:xa` run Save All,
  then quit.
- **Buffers are tabs**, as `ctrl-w` already treats windows as tabs: `:bn` and
  `:bp` cycle the pane's tabs, `:bd` closes the tab (`!` discards), and `:b`
  takes a name matched against open documents and completes from them. Vim
  buffer numbers are not supported.
- **`:pwd`** shows the directory `:e` resolves relative paths against: the
  current document's folder, or the window's first workspace root for an
  untitled document.
- **Range commands** take the existing range syntax (`%`, numbers, `.`, `$`,
  marks including `'<,'>`, offsets). Each is one undo step. `:d` and `:y` take
  an optional register and count and fill registers as `dd` and `yy` do. `:m`
  and `:t` take a destination address. `:>` and `:<` repeat (`:>>`) and
  invoke the native indent commands as `>>` does. `:j` joins through the same
  native join as `J`, `:j!` without spaces, as `gJ` would.
- **`:sort`** sorts the range (default every line) in one edit: `!` reverses,
  `i` ignores case, `n` sorts by the first decimal number (lines without one
  first, in order), `u` keeps the first of equal lines.

## Exclusions

Kept in the roadmap as the rest of D146:

- `:g`, `:v`, and `:normal`, which replay keys through the mode.
- `/pat/` and `?pat?` addresses and `;` between addresses.
- `:reg`, `:marks`, `:jumps` (they need a list surface, D054).
- `:map` and its variants, and `:command` (`post-init.js` covers bindings).
- `:!cmd`, `:r`, range filters, `:cd`, `:h`, `:&`, `:pu`, and `:sort` with a
  pattern.
- Vim buffer numbers in `:b` and `:bd`.

## Deferred decisions

None.

## Checkpoints

Only the checkpoints that touch real files or native menus have a manual
gate; the others are validated by automated tests alone, and their gate is a
passing `just check`, so they can run back to back.

### 1. File commands: `:e!`, `:w <file>`, `:sav` ✅ (review gate pending)

Deliverables: the `documents.reload`, `documents.save(id, { uri? })`, and
`files.writeText` host operations and their `knot.d.ts` contracts; `:e!`,
`:e! <file>`, `:w` with a path (copy, or naming an untitled document),
`:w!`, `:sav[!]`, and `E13` on existing targets.

Validation: product tests of `documents.reload` (reverts a dirty document as
one undo step, rejects an untitled one), `documents.save` (saves in place,
retargets with `uri`, a failure keeps identity), and `files.writeText`
(creates, overwrites, refreshes the file index); Vim product tests for each
command's resulting text, document identity, dirty state, and file contents.

Review gate (manual, ⏳): `Try: Ex files, set up a dirty original.txt` opens a
dirty file in `/tmp/knot-try-ex`; run `:e!`, `:w copy.txt`, `:w other.txt`
(E13), `:w! other.txt`, and `:sav saved.txt`, checking the tab title and the
files on disk (`Try: Ex files, open the files on disk`).

Done: the host operations (`documents.save` writes at the application level
without the window's save-cleanup), `E13` check, and the Ex commands, with
product tests.

**Implementation context**: `reload` in `src/app/product/document_watch.rs`
is fire-and-forget; give it a completion the host operation awaits.
`save_to_target` and `dispatch_save_to_captured_target` in
`src/app/product/commands.rs` and `src/app/product/save.rs` hold Save and
Save As; Save As's chosen-file branch is the path to reuse with a given URI.
Register the operations in `src/app/extensions/host/handlers.rs`, mirroring
`create_file` (error mapping, `refresh_file_index`) over
`knot_model::files` (`crates/knot-model/src/files.rs`); add the contracts to
`crates/knot-host/src/js/knot.d.ts` and update the frozen `knot.documents`
and `knot.files` key lists in `src/app/extensions/host/engine_tests.rs`.
Product tests mirror `src/app/product/tests/{document_watch,persistence,files}.rs`.
In the package, `parseEx` in `ex.ts` reads `force` but drops it for `edit`;
`runEx` and `edit` live in `command-line.ts`. Vim product tests with files
live in `crates/knot-ext-vim/tests/edit.rs`.

### 2. Save All and quitting

Deliverables: the native `file.save-all` command in the palette and File
menu; `:wa`, `:qa`, `:qa!`, `:wqa`, `:xa`.

Validation: product test that Save All saves every dirty file-backed
document across windows and leaves untitled ones dirty; Vim tests that `:wa`
saves and the quit commands dispatch quitting.

Review gate (manual): File › Save All and `:wa` with several dirty
documents.

**Implementation context**: declare the command beside `file.save` in
`src/app/product/commands.rs`; the document collection is
`ApplicationDocuments` (`src/app/documents.rs`). The File menu is built in
`src/app/product/` (see `tests/menu_bar.rs`). Name parsing goes in `NAMES`
and `parseEx` in `ex.ts`.

### 3. Buffer, window, and directory commands

Deliverables: `:bn`, `:bp`, `:bd[!]`, `:b <name>` with completion, `:enew`,
`:new`, `:vnew`, `:sp <file>`, `:vs <file>`, `:only`, `:close`, `:u`,
`:red`, `:pwd`.

Validation: `ex.test.ts` parser cases; Vim product tests for tab cycling,
closing, `:b` matching, splits opening the file, and `:pwd`'s message.

Review gate: automated only.

**Implementation context**: native commands `workbench.next-tab`,
`workbench.previous-tab`, `workbench.close-other-panes`,
`workbench.close-pane`, and the split commands already back `:split`;
`documents.createUntitled`, `documents.present`, and `documents.list` cover
`:enew` and `:b`. `baseDirectory` in `command-line.ts` is the directory
`:pwd` shows. `completeEx` in `ex.ts` gains document-name completion beside
`fileCompletionTarget`.

### 4. Range commands and `:sort`

Deliverables: `:d`, `:y`, `:m`, `:t`/`:co`, `:>`, `:<`, `:j[!]` over ranges,
with registers and counts where Vim takes them; `:sort` with `!`, `i`, `n`,
`u`.

Validation: `ex.test.ts` parser cases; pure sort tests; behavior cases in
`extensions/@knot/vim/test/cases/ex.jsonc`, including `'<,'>` from visual
mode, registers, undo as one step, and invalid ranges.

Review gate: automated only.

**Implementation context**: `resolveRange` and `substitute` in
`command-line.ts` show range resolution and a single native edit; registers
are in `registers.ts`; `dd`, `yy`, `>>`, and `J` in `normal.ts` are the
behaviors to match, and `grouped` makes several edits one undo step.
`parseEx` currently rejects a range on every command but `:s` (`E481`). The
sort is a pure function over line snapshots (`lines.ts`), applied as one
edit like `:s`.

### Completion

Rewrite the D146 roadmap entry to the exclusions above, record in
`docs/decisions/bundled-packages.md` the primitives Knot added for Ex, and
remove this plan.
