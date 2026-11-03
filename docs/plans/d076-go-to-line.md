# D076 — Go to Line

Status: 🚧 checkpoints 1–3 ✅; awaiting the manual review gate of
checkpoint 3 before this plan is removed. Item of the
[daily-driver plan](daily-driver.md); depends on D071 (shared picker and
transient slot). Promotes a single-selection first slice of D109.

## Goal

ctrl-G in an editor opens a small prompt. Typing `120` or `120:8` moves the
caret there while typing, with the target line shown by the cursor-line
highlight (D068) and centered when it was off screen. Enter keeps the
position; Esc restores the view exactly as it was.

Go to Line is an ordinary bundled JavaScript package, `@knot/go-to-line`,
built on public primitives. It is the forcing function for three small,
generic API additions that a Vim mode (D058), occurrence highlighting (D080),
and language tooling (D067) also need. Nothing in it is a private bypass.

## Design

### Public API additions

- **Editor handle (first slice of D109).** `CommandContext.editor: Editor |
  null` is the captured editor presentation, with the same admission authority
  as `context.buffer`; it is null for other surfaces. It offers:
  - `selection(): Promise<{ anchor, caret }>`, UTF-8 byte offsets;
  - `setSelection({ anchor, caret }, { reveal?: "nearest" | "center" })`:
    `nearest` scrolls the minimum to show the caret, as ordinary movement does;
    `center` centers the caret's line only when it is outside the viewport;
  - `viewState(): Promise<EditorViewState>` and
    `restoreViewState(state)`, reusing the existing public `EditorViewState`.

  Calls on a view that has closed reject with `InvalidTarget`. Setting a
  selection breaks the typing undo group, as other caret moves do, and is not an
  edit. Multiple selections, scrolling without a caret move, the visible range,
  cursor style, and cursor observation (D080) stay in D109, which extends this
  handle rather than replacing it.
- **Buffer line lookups**, answered by the model's line index:
  `lineCount()`, `lineRange(line)` (the line's `ByteRange` without its
  terminator), and `lineAt(offset)`. Lines are 0-based, like every other
  coordinate in the API; the package presents 1-based numbers. A column is
  computed in JavaScript from a snapshot of that one line, so large files never
  cross the boundary whole.
- **Live input.** `InputOptions.onChange?: (value) => string | null | void |
  Promise<…>`, called first with the initial value and then after each edit.
  Calls are serial and intermediate values collapse into the latest, exactly as
  `onHighlight` does, and a throw stops further calls and is rethrown once the
  input settles. A returned string is shown as a muted message line under the
  field; `null` or nothing clears it. The message line also serves future
  validation (VS Code's `validateInput`) without a separate API.

### The package

`@knot/go-to-line` registers `editor.go-to-line`, titled Go to Line….

- Opening reads the selection and view state, then shows the input with a
  message such as "Current Line: 12 of 340 (column 4)". ✅ The wording follows
  Zed's after manual review.
- Accepted forms: `N`, `N:C`, and `N,C` (compiler locations), surrounding
  whitespace ignored. ✅ A trailing separator without a column (`42:`) reads
  as `N`, so the preview holds while a column is typed. Columns count Unicode scalar values, 1-based, with a tab
  as one. A line past the end clamps to the last line, a column past the line
  end clamps to the end. Anything else restores the original view state, shows
  the usage message, and Enter does nothing beyond dismissing.
- Each valid value moves the caret (no selection) with `reveal: "center"` and
  the message names the target ("Go to line 120, character 8").
- Enter keeps the position. Esc or dismissal restores the captured
  `EditorViewState`, so selection and scroll return exactly. A command run
  without an editor reports `unavailable`.
- Relative (`+5`) and from-the-end (`-1`) forms are excluded.

### Binding

The native Knot preset binds ctrl-g to `editor.go-to-line` in the editor view
on every platform, so presets (D088) and personal bindings override it like
any default. If the package fails to start, the key reports the command as
unavailable.

## Exclusions

- The rest of D109: several or rectangular selections, scroll and visible-range
  queries, cursor style, undecorated anchored positions, clipboard read.
- Cursor and selection observation (D080).
- A navigation history or jump list.
- Go to Symbol and other `:`/`@` prefixes in one quick-open field.

## Deferred decisions

- Whether `setSelection` should take a `reveal: "top"` for Vim's `zt`; D109
  decides with D112.

## Checkpoints

### 1. Editor handle and buffer line lookups ✅

Scope decisions:

- The handle is valid within its command's invocation tree, as
  `CommandTarget` is, and resolves the captured presentation on every call.
  A handle that outlives its command is left to D109.
- `commands.unavailable()` lets a handler report the `unavailable` outcome,
  since no JavaScript path produced it.
- Line lookups and editor operations are host operations of their own
  (`lineCount`, `lineRange`, `lineAt`, `editorSelection`,
  `editorSetSelection`, `editorViewState`, `editorRestoreViewState`).

Deliverables: `context.editor` with `selection`, `setSelection`,
`viewState`, and `restoreViewState`; `Buffer.lineCount`, `lineRange`, and
`lineAt`; declarations, extension guide section, and fake-host support.

Validation: Rust product tests through a registered extension command that
read and set the caret, reveal centered and nearest, restore a view state,
reject after the view closes, and resolve line lookups on multibyte and CRLF
text; `node:test` contract tests for the fake host's implementation.

Review gate: the handle's shape is the one D109 will extend.

**Implementation context**

- Declarations: `src/host/js/knot.d.ts` (`CommandContext`, `Buffer`, a new
  `Editor` interface beside `EditorViewState`); JavaScript facade in
  `src/host/js/bootstrap.js`, mirroring `bufferFor(handle)` and the command
  context construction that sets `buffer` and `capture` (around the
  `activeCommandFrames` code).
- Host operations: new `#[host_operation]` handlers in
  `src/app/extensions/host/handlers.rs` beside `snapshot`/`applyEdits`; the
  editor operations resolve the invocation's captured target through
  `ProductCommandTarget::command_view` as `src/app/editor/native_commands.rs`
  does, and the captured invocation state lives in
  `src/app/commands/extension.rs` (`captured_buffer`).
- Editor seams: `EditorView::view_state` / `restore_view_state`,
  `select_reveal_and_focus` (currently test-only, without focus transfer for
  this use), `finish_position_change`, and `ensure_cursor_visible` in
  `src/app/editor.rs`; add the center-if-offscreen reveal there.
- Line lookups: `BufferModel::line_count`, `line_range`, `line_of_offset` in
  `src/app/model.rs`.
- Fake host: `scripts/extensions/testing/host.ts` and `knot.ts`.
- Docs: [extension guide](../extension-api.md),
  [commands](../architecture/commands.md) for the captured editor.

### 2. Live input with a message line ✅

The picker's pulled session became one transient session shared by both
interactions, so `beginInput`/`nextInputEvent` reuse its storage, collapsing,
and settlement; `nextInputEvent` carries the previous call's message.

Deliverables: `onChange` on `showInput`, the message line in the native input
view, and fake-host support.

Validation: Rust product tests that changes reach the callback in order with
collapsing, the returned message renders, a throwing callback still settles
the input and rethrows, and dismissal, unload, and cancellation settle as
today; fake-host tests for the same contract.

Review gate: the pulled-change transport matches the picker's highlight
transport rather than adding a second one.

**Implementation context**

- Transport: mirror `showPicker` with `onHighlight` in
  `src/host/js/bootstrap.js` (`beginPicker` / `nextPickerEvent` pull loop) and
  `PickerSession` / `begin_picker` / `next_picker_event` in
  `src/app/extensions/host/handlers.rs`, adding `beginInput` /
  `nextInputEvent`; the message travels back on the next pull.
- Native view: `InputView` in `src/app/transient.rs` gains a change event from
  its `TextInput` (as `Picker` subscribes to `TextInputEvent`) and a message
  line; `ProductShell::begin_input` in `src/app/product.rs` takes the watch
  sender, as `begin_owned_picker` takes `highlights`.
- Declarations: `InputOptions` in `src/host/js/knot.d.ts`; guide section on
  transient interactions; [semantic UI](../architecture/semantic-ui.md#extension-transient-interactions).

### 3. `@knot/go-to-line` and the binding ✅ (manual review pending)

The preset names the command through `GO_TO_LINE_COMMAND`, and the preset
test accepts bundled-package commands beside native ones. Enter on text newer
than the last preview applies the accepted value again rather than trusting
the preview. Without a column the caret goes to the start of the line.

Deliverables: the bundled package, ctrl-g in the Knot preset, and completion
of D076 in the daily-driver index.

Validation: `node:test` package tests for parsing, clamping, column counting
across multibyte text and tabs, preview, accept, dismiss restoring the view
state, invalid input, and a command without an editor; a Rust product test
that ctrl-g in an editor runs the bundled command end to end; the keymap
preset tests cover the new binding on all three platforms.

Review gate: daily use of the preview and the message wording.

**Implementation context**

- Package: mirror `extensions/@knot/themes` (`knot.jsonc`, `tsconfig.json`,
  `src/main.ts`, `test/*.test.ts`); add it to the bundled package list in
  `src/app/extensions/host.rs` (beside `"@knot/themes"`) and wherever
  `build.rs` enumerates bundled packages.
- Binding: `SHARED_BINDINGS` in `src/app/commands/keymaps/presets.rs`, with
  the view `Some("editor")`, and the tables in
  `src/app/commands/keymaps/tests.rs`.
- Docs on completion: the D109 entry in the [roadmap](../roadmap.md) and the
  [extensibility report](../reports/extensibility.md) note the landed slice;
  the [editor views](../architecture/workbench-and-lifecycle.md#editor-views)
  reference describes the editor handle; remove this plan once its durable
  behavior lives there.
