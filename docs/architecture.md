# Knot Code Architecture

Current boundaries, ownership, dependency direction, and cross-cutting
invariants. [Subsystem references](#subsystem-references) explain relevant runtime
flows; implementation details and API contracts live in code.
[Decisions](decisions.md) records rationale, while [roadmap](roadmap.md) and
[active plans](plans/) track unimplemented work.

## System map

Knot is a Cargo workspace. The root `knot` package is the gpui application;
leaf crates under `crates/` cannot see gpui:

```text
gpui application and native views
             |            ^
             v            | foreground requests / results
       core models     typed host boundary
                              |
                     extension runtimes
```

- `knot-core` owns UI-, platform-, and scripting-independent text and edit primitives.
- The root `knot` package (`src/app`) owns the gpui shell, foreground editor
  state as gpui entities and globals, native views, command dispatch, startup,
  extension storage, and the language registry. Native views use
  gpui-component (`src/app/components.rs`) for presentational leaf widgets,
  such as tooltips and popup menus; Knot keeps its own workbench, editor,
  terminal, pickers, text input, and key routing, and paints the components
  from its theme.
- `knot-protocol` owns the Knot-owned serde transport types and the typed
  operation declarations, and depends on neither V8 nor gpui.
- `knot-model` owns editor state and policy that need no gpui, V8, or host:
  the buffer model, syntax and the compiled-in grammars, edit history, indentation and language
  editing configuration, key notation, the command catalog, keymap registries
  and matching, theme values, resource URIs and providers, package
  discovery, and the workbench layout over tab identities. Its functions take
  plain data and the models they change and return results as data;
  application globals and entities wrap its state.
- `knot-bundled` owns the `BundledSource` type and the build-script helper
  that compiles bundled TypeScript packages. The `knot-ext-*` group crates
  (languages, vim, workbench) compile and expose the packages of one domain,
  and the `knot` library never depends on them.
- `knot-host` owns V8, bounded scheduling, and immutable module-graph
  validation. Its public control surface and protocol expose no V8 or gpui
  objects.
- `knot-handler-macros` generates adapters for annotated native host handlers
  and commands, without depending on V8, gpui, or editor models. Application
  owners expose semantic operations; declaration adapters own transport.

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
text, session-local history, anchored geometry, semantic contributions, and
its language and syntax state. Extensions contribute language definitions and
themes; native code owns grammars, parsing, and the defaults of the color keys
it paints with, and the application theme maps color keys and highlight names
to styles.
Each product session owns a workbench whose split panes contain typed
document, terminal, or native tool tabs, along with Knot-side focus and key
routing; tabs hold identities, and the session's workbench entity owns the
view behind each one; the application keeps the registry of open sessions, and a window
renders one session and forwards raw input to it.
The workbench retains native tool definitions independently of visible tab
attachments, including dock placement and hide/reopen state. The application
owns one SQLite state database of UI-state records: a versioned tool-only
layout snapshot, which fresh windows inherit while already-open windows stay
independent; one record of every product window's roots and layout that
startup restores before extensions see `launched`; the palette's recently
used commands; and the zoom level. It also owns a separate recovery database that protects dirty
document text independently of extension health. Views own presentation and interaction; terminal sessions own
PTYs, processes, and emulator state independently of their sole attached view.
The workbench owns layout transitions; product sessions coordinate tab
transfers, application-wide protected closure, and session replacement.

Extension runtimes retain JavaScript state in persistent isolates. Host workers
enter one turn at a time and yield while awaiting native work. The application
owns foreground protocol routing and resource cleanup; the host owns scheduling
and runtime disposal. Lifecycle teardown and host release share foreground
cleanup so registrations and contributions cannot outlive their host.

Application queries derive metadata from existing document and session owners.
Observation retains a comparison baseline and lifecycle-scoped subscriptions,
not a second ownership catalog. Each session owns its native semantic surfaces;
extensions supply asynchronous content rather than rendering callbacks.

## Invariants

- Text and history are shared buffer state. Cursor, selection, folding,
  scroll, and wrapping are per-editor state that runs without a window, and
  the view that renders an editor owns focus, painting, and pointer and
  platform input; zoom is one application level that every view scales by. A
  view need not have a text buffer.
- `BufferModel` owns editable text, never resource identity or persistence.
- Stable positions and edit events form one contract. `AnchoredRangeStore` owns
  geometry; feature and presentation policy belong above `core`.
- V8 objects stay inside `host`; extension requests carry Knot-owned semantic
  data and opaque identities.
- Commands retain admitted presentation authority across awaits. Explicit buffer
  access is independent of presentation lifetime.
- Asynchronous results revalidate captured targets, revisions or generations,
  and lifecycle before changing foreground state.
- Generic resource state uses normalized URIs. Platform paths and native I/O
  errors stay in providers or explicit diagnostic code.
- Native painting and raw input never synchronously call JavaScript.
- Syntax trees follow every commit synchronously; parsing that exceeds a small
  foreground budget continues off the foreground from an immutable text
  snapshot, and its result is rebased or rejected against the current state.

## Subsystem references

- [Core and buffers](architecture/core-and-buffers.md): text storage, stable
  positions, anchored ranges, model commits, syntax state, and reversible edits.
- [Documents and persistence](architecture/documents-and-persistence.md):
  identity, dirty state, resource providers, workspaces, Open, and Save.
- [Workbench and lifecycle](architecture/workbench-and-lifecycle.md): startup,
  sessions, windows, panes, tabs, protected closure, relocation, and terminal
  ownership.
- [Commands](architecture/commands.md): captured targets, focus routing,
  composition, cancellation, and keymaps.
- [Extension host](architecture/extension-host.md): runtime isolation, transport,
  scheduling, lifecycle, observation, and extension storage.
- [Semantic UI](architecture/semantic-ui.md): contributions, highlight layers,
  languages, trees, completion aggregation, and generated text surfaces.
