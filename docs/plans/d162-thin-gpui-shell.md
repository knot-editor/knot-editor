# D162 — Plain models and a thin gpui shell

Status: 🚧 approved, not started. Roadmap item [D162](../roadmap.md).
Order: D161 (complete; see the [crate boundaries decision](../decisions/platform.md#crate-boundaries)) → [D160](d160-gpui-component.md) → **D162.**
Do not start before D160 is complete: this plan shapes the shell against the
gpui API Knot keeps. Re-read D160's port report first and revise these
checkpoints where it shows other churn.

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

### 1. Plain buffer model

- Deliverables: `BufferModel` and its folds in `knot-model` with no gpui; the
  syntax driver as a parse job the shell runs off the foreground; a shell
  entity wrapper that notifies observers after commits and parse results;
  `BufferRegistry` keyed to the wrapper.
- Validation: `src/app/model/tests.rs` as plain tests; syntax tests that run
  a parse job, rebase, and reject a stale result without an executor; the
  documents, recovery, observation, and editing product tests unchanged.
- **Implementation context:** split `src/app/model.rs` at the
  `cx`-taking methods (lines 753-786) and the `BufferRegistry` (1368-1450).
  The background parse mirrors today's `cx.background_spawn(parse.run())`
  and the revalidation in `finish_syntax_parse`. Callers holding
  `Entity<BufferModel>` (`documents.rs`, `languages.rs`,
  `extensions/buffers.rs`, `editor.rs`, `workbench`) keep an entity of the
  wrapper. The architecture invariant "syntax trees follow every commit
  synchronously" must hold.
- Review gate: the wrapper is small, and the model API reads naturally from
  plain tests.

### 2. Plain editor state

- Deliverables: `Editor` in `knot-model`, with settings, theme values, and
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
