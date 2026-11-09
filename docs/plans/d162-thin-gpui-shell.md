# D162 — Plain models and a thin gpui shell

Status: 🚧 checkpoints 1–3 complete; checkpoint 4 complete; checkpoint 5 next. Roadmap item [D162](../roadmap.md).
Builds on the [crate boundaries](../decisions/platform.md#crate-boundaries)
and the [gpui snapshots](../decisions/platform.md#gpui-snapshots-and-gpui-component)
decisions. Re-read the port findings under Background first and revise these
checkpoints where they show other churn.

## Goal

The buffer model, editor state, application stores, extension-API semantics,
and product operations are plain `knot-model` code with plain tests. The gpui
application keeps views, platform input, and a thin shell that resolves
entities, calls the model, runs the effects it returns, and notifies. A
future gpui port, or a migration to another framework, rewrites views and
ports that shell mechanically, without revisiting behavior. Users see no
behavior change.

## Background

- After D161, `knot-model` holds the code that was already gpui-free, the
  key, color, and string vocabulary, and the workbench layout. What remains
  in the gpui crate mixes runtime use with behavior.
- `BufferModel` (`src/app/model.rs`) commit methods already take no context;
  it needs gpui only for `notify` and the background syntax parse
  (`set_syntax`, `continue_syntax_parse`, `finish_syntax_parse`, lines
  753-786). `BufferRegistry` (line 1368) maps transport handles to
  `WeakEntity<BufferModel>`.
- `Editor` (`src/app/editor.rs`) is separate from `EditorView` but uses
  `Context<Self>` for `notify`, observing its model, release, the clipboard
  (`editor/editing.rs`), and reading settings and theme globals.
  `src/app/editor/` has 116 gpui tests against 21 plain ones.
- The shell is about 19k lines: `product.rs`, `product/`,
  `extensions/host.rs`, and `extensions/host/handlers.rs` (268 functions,
  mixing protocol conversion, validation, and target revalidation with
  entity access).

- The port to `gpui-pre` 0.3.11 (see the
  [decision](../decisions/platform.md#gpui-snapshots-and-gpui-component))
  showed where gpui churn lands, which this plan should keep inside the
  shell:
  - Async contexts became infallible: `AsyncApp::update` and entity access
    return values, while `WeakEntity` and window updates still return
    `Result`. About 40 sites changed.
  - Focus takes the app: `FocusHandle::focus(window, cx)`,
    `Window::focus(handle, cx)`, `Window::blur(cx)`.
  - Keystroke notation became OS-aware (`Keystroke::parse` rejects
    `secondary` without an OS; `KeyBinding::load` takes the host OS).
  - `ShapedLine::paint` gained alignment arguments, and scroll-wheel events
    became `ScrollEvent`.
  - gpui's test scheduler fails a test when another thread wakes a
    foreground task unless that is allowed (`allow_thread_wakeups` in
    `src/app/mod.rs`); 225 tests failed this way. Storage tests run on
    gpui's `ThreadedDispatcher`, since a Mac platform exists only on the
    main thread.
  - Untouched: editor layout, the text system, the key interceptor, and
    tab drag beyond the window-closed callback.

## Design

- **The shell keeps the runtime.** No Knot abstraction over spawn, notify,
  observe, or globals: with one framework and one implementation it would
  copy gpui's shape and not ease a migration. Globals remain the application
  service registry, each wrapping a plain store.
- **Behavior lives in `knot-model`.** A handler, command, or product
  operation in the shell has the shape *resolve the entities and captured
  targets, call a `knot-model` function, run the returned effects, notify*.
  What an operation means — validation, protocol and model conversion, what
  an edit, close, or move does — is model code.
- **D161's rule applies unchanged.** Model functions take plain data and
  `&mut` models and return results or effects as data; no runtime closures,
  subscriptions, or spawning. Plain tests are the second consumer: an API a
  plain test cannot call without imitating gpui is redesigned.
- **Async stays in the shell.** Awaiting host results, revalidating captured
  targets, revisions, and lifecycle after an await, and scheduling background
  work remain shell responsibilities; the model supplies the revalidation
  checks and the work to run.
- **Effects as data.** For example, the buffer model hands out a syntax parse
  job from an immutable snapshot and accepts or rebases its result; the
  editor returns clipboard writes and receives clipboard reads; operations
  return the documents, tabs, or views they changed.

## Exclusions

- Rewriting or moving views, painting, layout, IME, tab drag, or terminal
  emulation and PTYs.
- Making product, session, or Vim-case tests run outside a `TestAppContext`:
  they test the integration deliberately.
- Changing the extension API, protocol, or persistence formats.

## Deferred decisions

- How far product operations go into the model. Each checkpoint stops when
  the remaining shell code is lookup, call, effects, and notify; it does not
  chase every `cx` use.
- Whether a terminal-session part moves, decided only if a terminal change
  needs it.

## Checkpoints

### 1. Plain buffer model — ✅ complete

`BufferModel` is plain `knot-model` code (`crates/knot-model/src/buffer.rs`)
whose syntax parse continues as a job it hands out and takes back; the
`Buffer` entity (`src/app/model.rs`) derefs to it, runs the jobs, and
notifies. See [core and buffers](../architecture/core-and-buffers.md#application-buffer-ownership).
Test-only model counters sit behind `knot-model`'s `test-support` feature.

### 2. Plain editor state — ✅ complete

`EditorState` (`crates/knot-model/src/editor.rs`) runs the native commands
over a buffer model given per call, follows the `EditorSettings` its owner
resolves, and queues clipboard and document-setting writes as effects;
completion sessions live in `crates/knot-model/src/completion.rs`. The
`Editor` entity (`src/app/editor.rs`) derefs to it, resolves settings, runs
the effects, and notifies. Command names are constants in
`knot_model::editor::commands`, which `#[command]` takes by path. See
[editor views](../architecture/workbench-and-lifecycle.md#editor-views).
Single-editor command tests are plain; the shell keeps the painting,
hit-testing, layout, settings-integration, and background-parse cases, which
reach state through `test-support` helpers.

### 3. Application stores — ✅ complete

The document collection, settings store, language registry, theme registry,
command-history ranking, and state database are plain `knot-model` code
(`documents.rs`, `settings.rs`, `languages.rs`, `themes.rs`,
`command_history.rs`, `state_database.rs`) with plain tests. Their globals in
`src/app` wrap them and add the effects: `DocumentCollection` derefs to the
plain collection and runs language detection, model stamping, recovery, and
observation; `record_store.rs` supplies the executor for debounced writes;
language reattachment over entities stays in the shell. Extension handlers
wrap moved types that need `HostArguments` in local argument types.

Validation limitation: the vim fixture in `terminal/session_tests.rs` fails in
roughly one full run in thirty (vim never draws). It predates this work, and
the terminal is out of scope. Flaky product tests that raced the extension
package's worker thread now wait for it to settle.

### 4. Extension-API semantics — ✅ complete

The extension-handler decisions that still lived in the shell are plain
`knot-model` code with plain tests: line and offset lookups and the buffer
subscription registry (`buffer.rs`), checked selection and fold ranges
(`editor.rs`, `editor/folding.rs`), atomic batch setting writes (`settings.rs`),
and application observation (`observation.rs`). The remaining handlers resolve
entities, call an owner, await, revalidate, and notify, and stay in
`src/app/extensions/host/handlers.rs`; the open, close, and workspace flows
they call move with checkpoint 5.

### 5. Product operations

- Deliverables: the decisions in protected closure, open and save,
  relocation, reopen, navigation, and product commands as model functions
  over the workbench and stores; the product session keeps windows, views,
  prompts, and async flow.
- Validation: plain tests for the moved decisions; the closure, sessions,
  operations, navigation, reopen, and tab-drag product tests unchanged.
- **Implementation context:** `src/app/product/{closure,open,save,relocate,reopen,navigation,operations,commands}.rs`
  and `src/app/product.rs`. Prompts and native dialogs stay in the shell;
  the model returns what to ask and accepts the answer.
- Review gate: a personal-config `post-init.js` with `try.*` commands over
  closing dirty documents, reopening, and moving tabs between windows;
  everything behaves as before.

### 6. Documentation

- Deliverables: `docs/architecture.md` ownership and invariants state the
  thin-shell rule; the subsystem references follow the moved owners;
  `AGENTS.md`'s routing table points at the new canonical sources; the D161
  decision in `docs/decisions/platform.md` extended with the thin-shell
  choice and the rejected runtime abstraction; this plan removed and D162
  removed from the roadmap.
- Validation: the `review-docs` checks for the touched files.
- Review gate: the docs describe the shipped boundaries only.
