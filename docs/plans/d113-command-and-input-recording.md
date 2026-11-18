# D113: Command and input recording

Status: 🚧 checkpoint 1 done; checkpoint 2 implemented, awaiting its review gate. Roadmap item [D113](../roadmap.md). Builds on
[command dispatch](../architecture/commands.md#dispatch),
[text bindings](../architecture/commands.md#text-bindings),
[ordered delivery](../architecture/commands.md#ordered-delivery), and the
[bundled Vim package](../decisions/bundled-packages.md#bundled-vim-mode).

## Goal

A package can record the input an editor receives (the commands its keys and
text bindings run, the text typed into it, and accepted completions) as plain
data, and replay it through ordinary command dispatch. `@knot/vim` uses it to
ship `.`, `q`/`@`/`@@` macros, the `".` register, and counts on insert
commands such as `3ihi<Esc>`.

## Design

- **Native, generic recorder.** The window already sees every keyboard input
  that reaches an editor: a completed binding enters `run_key_command`, a text
  binding dispatches from `take_typed_text`, and other committed text reaches
  `Editor::type_text`. The window appends to each active recording of the
  focused editor at those points. A Vim-only recorder over `buffer.onDidChange`
  was rejected: it cannot see normal-mode keys or native insert-mode commands
  such as arrows and Backspace, so it cannot build macros.
- **Only key-originated input is recorded.** Palette, menu, scripted, and
  nested (child) invocations are not, so replay never records or doubles its
  own steps. Mouse input and composition preedit are not recorded; an IME's
  commit is, as text.
- **A recording is data.** Each entry is `{ command, arguments }`. Typed text
  becomes the new native command `editor.type { text }`, which types exactly
  as the keyboard does (auto-closing, overtyping, indent triggers). An accepted
  completion becomes `editor.type { text, replace }`, where `replace` is the
  number of characters before the caret the completion replaced, so replay
  inserts the same text instead of re-running the popup.
- **Editor-scoped API, several recordings at once.** `EditorHandle.record()`
  returns a `Recording` whose `stop()` resolves with the entries. A recording
  keeps only input delivered to its own editor. It ends when the tab closes,
  when disposed, and at lifecycle teardown. Recordings are independent, since
  a Vim macro and an insert session overlap. The host tracks them per
  lifecycle as it tracks open undo groups.
- **Replay is the package's own loop over `commands.invoke`.** Children
  inherit the captured editor, so replay always targets the editor that
  started it. Vim feeds its own `vim.type` and `vim.key` entries straight into
  its parser instead of re-entering itself, and stops a macro as soon as a step
  moves the window's active tab away from that editor, as Vim stops a macro at
  a failing step. A macro therefore never edits the wrong buffer.
- **Vim's `.`** keeps the last parsed change command plus the recording of its
  insert session (started on entering insert mode, stopped by
  `vim.leave-insert`, which drops its own trailing entry). `.` reruns the
  command and replays the insert entries inside one undo group; a count
  replaces the original count. Counts on insert commands replay the session
  `count - 1` more times before leaving insert mode. The `".` register is the
  concatenated text of the last session's `editor.type` and newline entries.
- **Vim macros** live in the lettered registers: `q{a-z}` records, `q{A-Z}`
  appends, `q` stops, `@{reg}` replays with an optional count, `@@` repeats the
  last one. A recorded macro replaces the register's text and persists with the
  registers (the stored format's version changes, with no migration). The mode
  status shows `recording @a` while recording.

## Exclusions

- Macros that move between tabs or windows (`:wn`, `:cn`, `ctrl-w w`) stop at
  the switch. Following focus needs replay steps to run as roots against the
  window's current focus, which waits for [D182](../roadmap.md).
- Putting or editing a macro as register text (`"qp`, then `"qyy`), and `@a`
  executing a text register as keys.
- A non-Vim keyboard-macro package.
- `@:` and `:normal`, which belong to [D150](../roadmap.md).

## Deferred decisions

- Whether replay must suppress completion popups that typed text would open;
  checkpoint 1 measures whether an open popup changes later replayed keys
  (Enter, Tab) and suppresses it during `editor.type` replay only if it does.

## Checkpoints

### 1. Recording primitive and `editor.type` ✅

Complete and approved. Behavior lives in
[Commands](../architecture/commands.md#recording) and the
[extension guide](../extension-api.md). Key commands record only when they are
extension commands or native editor commands, so window-level ones such as
opening the palette are left out. Auto-closed brackets are covered by the
model's `type_text` tests, not the product tests, whose buffers have no
language.

### 2. Vim `.`, `".`, and counts on insert commands ✅ (review gate pending)

Deliverables:
- `.` and `{count}.` for operators, `x`, `p`, `r`, `J`, `~`, indentation, and
  every insert-entering command (`i a I A o O c s S C`, visual `c`), each one
  undo step.
- Counts on insert commands: `3ihi<Esc>`, `2o`, `3A`.
- The `".` register, readable by `".p` and `ctrl-r .` where puts already
  support registers.

Validation:
- New `extensions/@knot/vim/test/cases/repeat.jsonc` cases through the real
  editor: repeated change words, repeated inserts with Backspace, newlines, and
  auto-closed brackets, counts on `.`, undo of a repeat, `.` after a visual
  change, and counted inserts.
- Parser unit tests in `extensions/@knot/vim/test/keys.test.ts` for `.` and
  counted insert commands.
- `just check`.

Review gate: daily Vim use of `.` and counted inserts through `Try:` commands
on a scratch buffer.

Scope decisions: a visual operator repeats as the same operator over as much
text from the caret (lines, or characters on one line); a characterwise
selection over line breaks and visual `p` and `r` are not repeated. A search
operator repeats with `n`. The `".` register is stored with the others. Insert
mode's `ctrl-r .` is not built, since insert mode has no register paste yet.

Implementation context:
- Parser: `extensions/@knot/vim/src/keys.ts` (add the `.` command, keep counts
  on insert commands); execution in `src/normal.ts` (`run`, `setMode`,
  `leaveInsert`) and `src/visual.ts`.
- State: the last change and its insert recording on `Shared` in
  `src/normal.ts`; the recording starts in `setMode(..., "insert")` and stops
  in `leaveInsert`, through the followed editor's handle.
- `".`: `src/registers.ts`, read-only like Vim's.
- Case runner: `crates/knot-ext-vim/tests/vim.rs`.

### 3. Vim macros

Deliverables:
- `q{a-z}`, `q{A-Z}`, `q`, `@{reg}`, `{count}@{reg}`, `@@`; the `recording @a`
  status; macros persisted with the registers; replay stopping when focus
  leaves the editor or a step is unavailable.
- [Bundled Vim decision](../decisions/bundled-packages.md#bundled-vim-mode)
  and [index](../decisions.md) updated with recording and replay; D113 closed
  in the roadmap and D150's dependency note adjusted.

Validation:
- `extensions/@knot/vim/test/cases/macros.jsonc`: macros mixing normal and
  insert keys, searches, counts, appending, `@@`, a macro that invokes `.`, a
  macro that stops at a tab switch, persistence across `vim.reset`-free
  restarts of the package where the runner supports it.
- `extensions/@knot/vim/test/registers.test.ts` for the stored format.
- `just check`.

Review gate: recording and replaying real macros through `Try:` commands.

Implementation context:
- Keys: `q` and `@` in `src/keys.ts`; recording start and stop in
  `src/extension.ts` (`Vim.keys`, status items) using the followed editor's
  handle; replay loop beside `run` in `src/normal.ts`, feeding Vim's own
  entries to `feed` and the rest to `commands.invoke`, checking the window's
  active target after each step.
- Storage: `Registers.toJson`/`fromJson` in `src/registers.ts`.
