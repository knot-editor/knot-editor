# D110 — Key contexts and ordered key delivery

Status: 🚧 checkpoint 1 awaiting review. Item of the
[editor API and Vim mode](editor-api.md) plan; background in the
[extensibility report](../reports/extensibility.md#keystroke-ordering-and-latency).

## Outcome

An extension can put an editor view into a named key context, bind keys and
typed text within that context, and rely on every key after an
extension-handled key reaching it in order. That is enough for a JavaScript
modal editor: insert mode stays native typing with IME and runs no
JavaScript per keystroke, while normal-mode keys reach the extension in
order however fast they are typed.

## Design

- **The grammar stays in JavaScript.** Counts, operators, and pending state
  belong to the extension. gpui abandons a pending key sequence after one
  second (`Window` in gpui 0.2.2), so Vim's `d`, pause, `w` cannot be a
  native sequence, and counts cannot be enumerated as bindings. Native
  scoped sequences stay as they are for ordinary chords.
- **View key contexts.** An extension sets and clears named contexts on an
  editor view through the editor handle. Names are generic strings; a
  context is owned by the lifecycle that set it, so teardown clears it.
  Contexts are presentation state: not recorded in layouts, gone when the
  view closes.
- **Context-scoped bindings.** `keymaps.bind` takes a `context` option beside
  `view`. A binding's scope is a set of facts, and the binding matching the
  most facts wins, then the latest write, as settings scopes resolve; the
  preset stays beneath every extension binding. Knot resolves contexts in
  `dispatch_keybinding` against the view's live state rather than gpui's
  key context, because gpui matches against the last rendered frame and a
  context set by the previous key may not be painted yet. The gpui binding
  for a context-scoped key is therefore broad, and an unmatched key
  propagates to text input as an unbound one does today.
- **Text fallback.** A binding may route typed text, rather than one key,
  to a command within a view kind and context: committed text that the
  editor would insert goes to the command as `{ text }` instead. This is
  VS Code's `type` override scoped to a context, as an Evil state map in
  Emacs has its own default binding, and how Zed's Vim reads the character
  after `f` or `r`. Text arrives after layout and IME, so `$` and `{` are
  right on any keyboard layout. Insert mode declares no fallback and types
  natively. Composition preedit in a fallback context is not shown.
- **Ordered delivery.** While an extension command invoked by a key on a
  view is unfinished, later key events and text input for that view are
  queued and replayed in order through gpui's `dispatch_keystroke` and the
  view's input path once it finishes. Native commands already keep this
  order by running in the dispatch turn; this extends it to every
  key-invoked extension command, with no opt-in. The queue is released
  early when focus leaves the view (a command awaiting a picker or input)
  and after a one-second safety timeout, so a stuck extension never holds
  the keyboard; Restart Extension remains the recovery.
- **Asynchronous, not synchronous.** Ordered delivery gives each key the
  state its predecessor committed, which is what synchronous handling
  would give, without blocking the foreground on a busy isolate or adding
  a synchronous host API. The invariant that raw input never calls
  JavaScript synchronously stands. Latency is checkpoint 1's question.

## Exclusions

- How a freshly opened editor enters a context without a key press: D109's
  long-lived handles supply it. Until then contexts are set from a
  command's `context.editor`.
- Contexts on terminals, trees, and contributed views (D147).
- Several contexts per binding, `when`-style expressions, and negation
  (D148).
- Recording and replaying input (D113).

## Deferred decisions

- The exact timeout, settled from checkpoint 1 and daily use.
- Whether Show Keybindings needs a context column beyond listing the scope;
  decide when checkpoint 2 touches the listing.

## Checkpoints

### 1. Latency spike

Measure key-to-commit latency for a trivial extension command: a key bound
to a command whose handler calls `context.editor.setSelection`, from
dispatch to the committed selection, p50 and p99 over many keys, on a
release build. Also measure the same path with a fixture that types text
right after the key to confirm today's race.

- Deliverables: an ignored measurement test, numbers recorded in this plan.
  ✅ `measure_extension_key_latency` in
  `src/app/product/tests/keybindings.rs`.
- ✅ Results, Apple M1 Pro/macOS 27 release build, gpui test dispatcher,
  1,000 keys per run, three runs:
  - Key bound to a command that awaits `editor.setSelection`, from
    `simulate_keystrokes` to the committed caret: p50 169–176 µs, p90
    187–200 µs, p99 245–277 µs, max under 0.5 ms.
  - A no-op extension command, dispatch to completion: p50 21 µs, p99
    62–64 µs, so most of the key path is the foreground request and the
    turn's resumption after it, not dispatch.
  - Simulating an unbound key: p50 5 µs, so the harness adds little.
  - Text typed right after the key landed before the command's commit in
    199–200 of 200 trials: the race is real and ordered delivery
    (checkpoint 3) is required.
  - Limitation: the test dispatcher polls rather than running the
    platform's run loop, so a real window adds its own wakeup latency;
    the margin to a 16 ms frame leaves room for it.
- Review gate: if p50 is well under a frame (around 1–2 ms) and p99 under
  a frame, continue. Otherwise investigate the pool hop first, for example
  waking the worker directly, before reconsidering synchronous handling.

**Implementation context:** mirror the ignored measurements in
`src/host/pool.rs` and `src/app/extensions/host.rs`
(`#[ignore = "measurement: run in release with --ignored --nocapture"]`).
The path is `ProductView::dispatch_keybinding` (`src/app/product.rs`) →
`CommandDispatcher::dispatch` (`src/app/commands/dispatch.rs`, which
returns a `CommandExecution` with a completion receiver) →
`extensions::host::start_global_command` → the `editorSetSelection` handler
in `src/app/extensions/host/handlers.rs`. Product tests already bind keys
and drive editors in `src/app/product/tests/keybindings.rs`.

### 2. View key contexts and context-scoped bindings

`Editor.setKeyContext(name, active)` on the editor handle, lifecycle-owned
contexts on `EditorView`, and a `context` option on `keymaps.bind` and
`unbind`, resolved by fact count. Teardown clears a lifecycle's contexts
and bindings.

- Tests: registry precedence (context beats view beats global, latest write
  among equals, preset beneath), teardown revealing earlier writes; product
  tests that a key resolves by the live context immediately after it is set,
  before a repaint, and that an unmatched key in a context still types.
- Review gate: the API shape in `knot.d.ts` and the precedence rule.

**Implementation context:** `BindingRegistry` in
`src/app/commands/keymaps.rs` keys slots by `(owner, key, view)`; extend the
slot key and `winning_slot`/`refresh_key` with the context, and
`gpui_bindings` with a broad `product > editor` predicate for context-scoped
keys. `KeybindingMutation` in `src/host/protocol.rs` and its decoder carry
the option. `dispatch_keybinding` and `target_view_kind` in
`src/app/product.rs` pass the focused editor's contexts to `resolve`.
The handle operation follows `editor_set_selection` in
`src/app/extensions/host/handlers.rs` (`contextual_editor`), with the JS
side beside the existing `Editor` in `src/host/js/knot.d.ts`. Lifecycle
cleanup hooks in the shared foreground teardown that already removes
keybinding owners (`remove_owner`).

### 3. Ordered key delivery

A per-view hold that starts in `dispatch_keybinding` when the resolved
command belongs to an extension, queues later key-downs and the editor's
committed text input, and replays them in order on completion, focus loss,
or timeout.

- Tests: keys and typed text after a slow extension command apply after its
  commit and in order; a command that changes context makes the next queued
  key resolve in the new context; a focus change and the timeout release the
  queue; native commands are unaffected.
- Review gate: behavior when the queued key itself starts another extension
  command (the hold chains), and the timeout value.

**Implementation context:** the hold lives with the product window, which
already sees every keymap action and owns the command target capture; it
awaits the `CommandExecution` completion from
`CommandDispatcher::dispatch`. Whether a command is an extension's comes
from `CommandCatalog::resolve` (`src/app/commands/catalog.rs`). Replay
uses `Window::dispatch_keystroke`; text input queues in the
`EntityInputHandler` impl in `src/app/editor/input.rs`
(`replace_text_in_range`), which consults the hold before editing.

### 4. Text fallback

`keymaps.bindText(command, { view, context })` and `unbindText`, a slot in
the same registry that wins by the same rule, routing committed text input
in a matching view to the command with `{ text }` as its arguments, under
ordered delivery.

- Tests: typed and IME-committed text reach the command in order and are not
  inserted; keys bound in the same context still win over the fallback; no
  fallback means native typing; teardown restores typing.
- Review gate: the argument shape and preedit handling.

**Implementation context:** routing happens in `replace_text_in_range` in
`src/app/editor/input.rs`, which asks the keymap registry for a text
binding for its view kind and contexts and dispatches through the product
command path rather than editing. Registry storage beside key slots in
`src/app/commands/keymaps.rs`; Show Keybindings lists it as `text`.

### 5. Small modal example and documentation

A deliberately tiny `@example/modal`, to be deleted when D058 lands: one
command enters a `modal` context on the editor, where `h`/`j`/`k`/`l` move
the caret, other text is swallowed by a text fallback, and Escape leaves.
Package tests under `node:test` against the fake host.

Update [commands](../architecture/commands.md) (contexts, ordered delivery,
text fallback), the [extension guide](../extension-api.md), `knot.d.ts`,
and [decisions](../decisions.md) with the validated choices and checkpoint
1's numbers.

- Review gate: final review of docs and the example's size.

**Implementation context:** mirror `examples/extensions/@example/git-status`
(`knot.jsonc`, `src/`, `test/`, `tsconfig.json`); the fake host in
`scripts/extensions/` needs `setKeyContext` and `bindText` only as far as the
tests use them.
