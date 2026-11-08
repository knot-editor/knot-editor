# D162 — Plain models and a thin gpui shell

Status: 🚧 checkpoint 2 implemented and awaiting review. Roadmap item [D162](../roadmap.md).
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

### 2. Plain editor state — 🔍 awaiting review

- ✅ `EditorState` (`crates/knot-model/src/editor.rs` and `editor/`) holds the
  editor's state and runs the native commands over a `&mut BufferModel` it is
  given per call. It follows the `EditorSettings` its owner sets and queues
  clipboard writes and document setting changes as `EditorEffect`s; paste
  takes the clipboard's text. Completion sessions moved to
  `crates/knot-model/src/completion.rs`, and only the popup render stays in
  `src/app/completion.rs`.
- ✅ The `Editor` entity (`src/app/editor.rs`) derefs to the state. Before each
  operation it resolves the settings, and afterwards it runs the effects,
  notifies the buffer when its text changed, and notifies itself. It also
  follows buffer changes, releases its anchors, and keeps the in-flight
  completion requests. `EditorView` is unchanged apart from reading the state
  through accessors.
- ✅ The single-editor command tests run as plain tests: brackets, comments,
  folding, indentation, lines, save hygiene, decorations, and the editing,
  movement, undo-across-editors, composition, completion, reveal, and view
  state cases. The shell keeps the painting, hit-testing, wrap-layout,
  gutter, settings-integration, and background-parse cases. A few
  `test-support` helpers (`place_for_test`, `scroll_to_for_test`,
  `set_viewport_rows_for_test`) let those reach state they used to set
  directly.
- ✅ The editor command names are constants in
  `knot_model::editor::commands`; `#[command]` takes a constant's path in
  place of a literal name and `id`, so the declarations and the dispatch
  share them.
- Original deliverables: `Editor` in `knot-model`, with settings, theme values, and
  the clipboard as explicit inputs and outputs; `editor/wrap.rs`,
  `editor/completion.rs` state, and `completion.rs` aggregation move with it
  where they need no gpui; `EditorView` keeps rendering and platform input
  and owns the editor through a wrapper entity.
- Validation: the `src/app/editor/tests.rs` cases that do not paint or
  hit-test as plain tests; the rest, Vim cases, and editor API product tests
  unchanged.
- **Implementation context:** the `Context<Self>` uses in `editor.rs`
  (`notify`, `observe` of the model, `on_release`, `try_global`),
  `editor/{editing,completion,folding,indentation,brackets,lines,comments,gutter}.rs`.
  The resolved settings read in layout (`settings::ApplicationSettings`,
  `SettingsScope`) become an input value the view resolves per layout, as
  the architecture already describes. `zoom::rems` in `completion.rs` stays
  at the view edge.
- Review gate: an editor command can be exercised in a plain test in a few
  lines.

### 3. Application stores

- Deliverables: the document collection, settings store, language
  registry, theme registry, and command history as plain stores in
  `knot-model`, each wrapped by its existing global.
- Validation: their unit tests as plain tests; persistence, settings,
  languages, themes, and startup product tests unchanged.
- **Implementation context:** `src/app/documents.rs`, `settings.rs`,
  `languages.rs`, `themes.rs`, `command_history.rs`, and the globals they
  register; `record_store.rs` and `tool_layout.rs` keep SQLite I/O in the
  shell or move with a plain interface, whichever is smaller.
- Review gate: each global is a thin wrapper over its store.

### 4. Extension-API semantics

- Deliverables: the conversion, validation, and operation logic of the
  extension handlers as `knot-model` functions over the plain models; the
  handlers keep entity resolution, captured-target revalidation, effects,
  and notification.
- Validation: plain tests for the moved semantics, including the failures an
  extension receives; the extension API, observation, editor API, and
  package case tests unchanged.
- **Implementation context:** `src/app/extensions/host/handlers.rs`,
  `extensions/buffers.rs`, `extensions/semantics.rs`, and the dispatch in
  `extensions/host.rs`. Annotated handlers stay in the shell, so
  `knot-handler-macros` and `host::declarations` validation are unchanged.
  Work through handler groups (buffers and editors, workbench and views,
  documents and resources, semantic UI), each a commit.
- Review gate: a sample of handlers reads as lookup, call, effects, notify.

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
