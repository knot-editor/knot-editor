# D110 — Key contexts and ordered key delivery

Status: 🚧 checkpoints 1–4 complete; checkpoint 5 next. Item of the
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
- ✅ Show Keybindings needs no context column: a context slot's row reads
  `in editor (modal)` beside the key and owner.

## Checkpoints

### 1. Latency spike ✅

Approved. `measure_extension_key_latency` in
`src/app/product/tests/keybindings.rs` measured, on an Apple M1 Pro/macOS
27 release build under the gpui test dispatcher over 1,000 keys and three
runs: a key bound to a command awaiting `editor.setSelection`, to the
committed caret, p50 169–176 µs and p99 245–277 µs; a no-op extension
command p50 21 µs, so most of the path is the foreground request and the
turn's resumption. Text typed right after the key landed before the commit
in 199–200 of 200 trials, so ordered delivery is required. The dispatcher
polls instead of running the platform loop, so a real window adds its
wakeup latency within the remaining frame margin. Checkpoint 5 moves these
numbers into decisions.

### 2. View key contexts and context-scoped bindings ✅

Approved. `Editor.setKeyContext(name, active)` sets lifecycle-owned contexts
on `EditorView`, cleared at teardown, and `keymaps.bind`/`unbind` take a
`context` that requires a `view`, so a context is always scoped to a view
kind and fact counting stays unambiguous. Extension slots beat the preset,
then the slot requiring the most facts, then the latest write; a view
holding several contexts takes the latest of their slots. gpui recognizes a
context-scoped key broadly in its view and `dispatch_keybinding` resolves
against the live contexts, so a key resolves in a context before any
repaint. Show Keybindings lists the scope as `in editor (modal)`; the
palette and menu keys ignore contexts. Contract in `src/host/js/knot.d.ts`;
tests in `src/app/commands/keymaps/tests.rs` and
`editor_key_contexts_resolve_live_and_end_with_their_lifecycle`.

### 3. Ordered key delivery ✅

Approved. A key that dispatches a non-native command holds its window's
later key-downs, intercepted before keybindings and text insertion, and the
held editor's committed text, in one queue (`src/app/product/key_hold.rs`,
`src/app/commands/key_hold.rs`). Release replays them in order on
completion, on the first key after focus left the view, or after 1 s; a
replayed key that starts another extension command holds the rest of the
queue. IME composition commits are not queued, since their preedit is
already in the text. The checkpoint 1 race fixture went from 199–200 of 200
early insertions to 0, with key-to-commit p50 about 194 µs. Tests in
`src/app/product/tests/key_hold.rs`.

### 4. Text fallback ✅

Approved. `keymaps.bindText(command, { view, context })` and `unbindText`
route committed text that the editor would insert to a command name, which
always receives exactly `{ text }`; null masks a lower text binding. Text
slots sit beside key slots and win by the same rule, and a key bound in the
context still wins because text only arrives for unbound keys. The editor's
`replace_text_in_range` resolves the binding and calls a `TextCommandRouter`
global the product shell installs: it starts the key hold synchronously,
since the editor is mid-update and cannot capture a target, and dispatches
on `window.defer`. Where a text binding applies, composition never touches
the buffer and the IME commit routes as text. Show Keybindings lists text
slots as `text` and accepting one runs nothing. Tests in
`src/app/commands/keymaps/tests.rs` and
`a_text_binding_receives_typed_and_composed_text_in_order`.

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
