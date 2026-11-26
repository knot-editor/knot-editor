# Knot Code Architecture

Current boundaries, ownership, dependency direction, and cross-cutting
invariants. [Subsystem references](#subsystem-references) explain relevant runtime
flows; implementation details and API contracts live in code.
[Decisions](decisions.md) records rationale, while [roadmap](roadmap.md) and
[active plans](plans/) track unimplemented work.

## System map

Knot is a Cargo workspace. The root `knot` package is the gpui application;
leaf crates under `crates/` cannot see gpui. Each crate depends on the ones
nested under it:

```text
knot-app            the binary
├── knot-ext-*      bundled packages, one crate per domain
└── knot            gpui application and native views
    ├── knot-model  editor state and policy
    │   ├── knot-core
    │   └── knot-protocol
    ├── knot-host   extension runtimes
    │   └── knot-protocol
    ├── knot-terminal
    └── knot-process
```

`knot-protocol` uses `knot-handler-macros`, and the bundled-package crates
build with `knot-bundled`.

- `knot-core` owns UI-, platform-, and scripting-independent text and edit primitives.
- The root `knot` package (`src/app`) owns the gpui application, foreground
  editor state as gpui entities and globals, native views (using
  gpui-component for presentational leaf widgets), command dispatch, startup,
  extension storage, and language registrations and document language
  detection over the model's registry.
- `knot-protocol` owns the Knot-owned serde transport types and the typed
  operation declarations, and depends on neither V8 nor gpui.
- `knot-model` owns editor state and policy that need no gpui, V8, or host:
  buffers, editing, history, and syntax with the compiled-in grammars;
  documents, resource URIs and providers, file watching, and the workspace
  file index; the language, theme, and settings registries; the command
  catalog, menus, and keymaps; the workbench layout over tab identities,
  launches, and protected-closure decisions; extension-API semantics such as
  providers, completion sessions, checked ranges, and atomic setting writes;
  and the state database. Its policy functions take plain data and the models
  they change and return results or effects as data; application globals and
  entities wrap its state. The native I/O it owns (providers, watching, the
  state database) runs off the foreground.
- `knot-bundled` owns the `BundledSource` type and the build-script helper
  that compiles bundled TypeScript packages. The `knot-ext-*` group crates
  (languages, vcs, vim, workbench) compile and expose the packages of one
  domain, and the `knot` library never depends on them.
- `knot-host` owns V8, bounded scheduling, and immutable module-graph
  validation. Its public control surface and protocol expose no V8 or gpui
  objects.
- `knot-terminal` owns the terminal engine over the statically linked
  libghostty-vt, the PTY and its threads, and frame reading for painting,
  without depending on gpui; terminal sessions and views in `src/app/terminal`
  build on it.
- `knot-handler-macros` generates adapters for annotated native host handlers
  and commands, without depending on V8, gpui, or editor models. Application
  owners expose semantic operations; declaration adapters own transport.
- `knot-process` owns child processes with piped, text-only stdio, without
  gpui: spawning in a process group, dedicated threads, and group kill.

The `knot` binary lives in `knot-app`, which joins the group crates' packages,
creates the platform application, and enters the product through `knot::run`.
Only the binary links gpui's platform crates, so test binaries never load a
native platform.

## Ownership

gpui's foreground thread exclusively owns editor state; native I/O runs off it.
The application owns documents, resource providers, the command catalog and
keymap registry, the in-memory settings store, terminal sessions, and one
extension pool. Native views resolve settings for their own scope while laying
out, so a write needs no per-view push.

A document owns persistence identity and a shared `BufferModel`. The model owns
text, its own linear history, anchored geometry, semantic contributions, and
its language and syntax state. Extensions contribute language definitions and
themes; native code owns grammars, parsing, and the defaults of the color keys
it paints with, and the application theme maps color keys and highlight names
to styles.

Each window owns a workbench: a center of document and terminal tabs
surrounded by tool-window slots, each area a split tree of tabbed panes, along
with Knot-side focus and key routing. Tabs hold identities, and the window's
workbench entity owns the view behind each one. The workbench owns layout
transitions; windows coordinate tab transfers, application-wide protected
closure, and session replacement. The application keeps the registry of open windows, and
a native window renders one window and forwards raw input to it. Views own
presentation and interaction; terminal sessions own PTYs, processes, and
emulator state independently of their sole attached view.

The application owns one SQLite state database of UI-state records (tool
layout, open windows, zoom) and a separate recovery database that protects
dirty document text independently of extension health.

Extension runtimes retain JavaScript state in persistent isolates. Host workers
enter one turn at a time and yield while awaiting native work. The application
owns foreground protocol routing and resource cleanup; the host owns scheduling
and runtime disposal. Lifecycle teardown and host release share foreground
cleanup so registrations and contributions cannot outlive their host.
Extension-spawned child processes are owned per lifecycle the same way:
teardown kills their process groups, so no child outlives its package.

Queries and observation derive from the existing document and window owners
rather than keeping a second catalog. Each window owns its native semantic
surfaces; extensions supply asynchronous content, never rendering callbacks.

## Invariants

- The gpui application is a thin shell over `knot-model`. A command, handler,
  or product operation resolves the entities and captured targets, calls a
  model function, runs the effects it returns (clipboard writes, background
  parse jobs, prompts, saves, closes), and notifies. What an operation means
  (validation, protocol and model conversion, scope, and revalidation checks)
  is model code with plain tests. Awaiting, revalidating after an await, and
  scheduling stay in the application; views, painting, and platform input stay
  in views.
- Text and history are shared buffer state. Cursor, selection, folding,
  scroll, and wrapping are per-editor state that runs without a window, and
  the view that renders an editor owns focus, painting, and pointer and
  platform input. A view need not have a text buffer.
- `BufferModel` owns editable text, never resource identity or persistence.
- Stable positions and edit events form one contract. `AnchoredRangeStore` owns
  geometry; feature and presentation policy belong above `core`.
- V8 objects stay inside `host`; extension requests carry Knot-owned semantic
  data and opaque identities.
- A command keeps the tab and view it was invoked on as its target across
  awaits, and operations on that target reject once it is gone; access
  through an explicit buffer handle does not depend on the view staying open.
- Asynchronous results revalidate captured targets, revisions or generations,
  and lifecycle before changing foreground state.
- Generic resource state uses normalized URIs. Platform paths and native I/O
  errors stay in providers or explicit diagnostic code.
- Native painting and raw input never synchronously call JavaScript.
- Syntax trees follow every commit; parsing that doesn't fit the foreground
  budget continues in the background against a text snapshot.

## Subsystem references

- [Core and buffers](architecture/core-and-buffers.md): text storage, stable
  positions, anchored ranges, model commits, syntax state, and reversible edits.
- [Documents and persistence](architecture/documents-and-persistence.md):
  identity, dirty state, resource providers, workspaces, Open, and Save.
- [Workbench and lifecycle](architecture/workbench-and-lifecycle.md): startup,
  windows, panes, tabs, protected closure, relocation, and terminal ownership.
- [Editor and languages](architecture/editor-and-languages.md): editor state
  and views, native editing behavior, editor bars, syntax highlighting,
  language registration, and language-driven editing.
- [Commands](architecture/commands.md): captured targets, focus routing,
  composition, cancellation, and keymaps.
- [Extension host](architecture/extension-host.md): runtime isolation, transport,
  scheduling, lifecycle, observation, and extension storage.
- [Version control](architecture/version-control.md): repositories, baselines,
  the change gutter, navigation, and Git.
- [Semantic UI](architecture/semantic-ui.md): contributions, highlight layers,
  themes, icons, trees, completion aggregation, pickers, and generated text
  surfaces.
