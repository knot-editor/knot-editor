# D198 — Extension providers

Status: 🚧 checkpoint 1 awaiting review. Roadmap item [D198](../roadmap.md).

## Goal

A package can offer a named provider that other packages find and call by name,
so a feature built by one package can be reused by others as it is, not copied.
The first user is Vim: typing `:e foo` keeps the Ex line at the top, and the
list under it becomes the Go to File list for `foo`, built by the same code
with the same rows, ranking, recent files, icons, and highlights as Cmd-P.
Deleting back to `:` brings back the usual Ex completions.

## User-visible behavior

- `:e` with nothing after it lists the recent files, as Go to File does with an
  empty query. Typing after `:e ` filters the list as Go to File does.
- Up, Down, Ctrl-N, and Ctrl-P move the highlight. Enter opens the highlighted
  file. When nothing matches (`:e newfile.txt`, `:e ~/elsewhere/x`), Enter opens
  the typed path as `:e <path>` does today, so creating a file with `:e` still
  works. Tab puts the highlighted file's path into the line.
- When no package provides the files list (Go to File missing, failed, or
  restarting), Vim still works: `:e <path>` opens the path, with no list.
- A package that registers a provider under a name already in use replaces it
  for every consumer that looks it up afterwards, as a later keybinding
  replaces an earlier one. Personal config can therefore replace Go to File's
  list everywhere, Vim included.

## Design

- **A provider registry owned by the application.** `providers.register(name,
  implementation)` registers an object of async methods under a name and
  returns a `Disposable`; the registration belongs to the registering
  package's lifecycle and goes away when that package unloads or restarts.
  Names are free strings owned by the packages that agree on them; the
  application never interprets a provider's contract.
- **One or many is the consumer's choice, with no declaration.**
  `providers.get(name)` returns the most recent registration, or null.
  `providers.all(name)` returns every registration in registration order, for
  contracts several packages contribute to. A `providers.declare(name,
  "single" | "multiple")` step was considered and rejected: a second
  registration of a single provider must still be accepted for overrides to
  work, so a declaration would only document intent, while adding a question
  of which package declares a name and an ordering rule between declaring and
  registering packages.
- **Calls cross isolates through the host.** A handle from `get` or `all`
  exposes the provider's methods as async functions. Each call runs in the
  provider's isolate as one of its roots, with JSON arguments and a JSON
  result; a throw in the provider rejects the caller's call. Callbacks,
  handles, and targets do not cross as values; instead a call carries the
  caller's window context, so a provider sees the same window target a command
  run from that window would (Go to File needs it for the window's roots and
  recent files).
- **Handles are bound to one registration.** A call through a handle whose
  registration was disposed rejects with a host error; consumers look the
  provider up again when they need it, which is cheap. A package calling its
  own provider through the registry would deadlock while it runs one root at
  a time ([D182](../roadmap.md)), so a package uses its own code directly.
- **Manifest dependencies are unchanged.** They already order startup and fail
  dependents; a provider lookup happens when a feature runs, so a consumer
  that can fall back needs no dependency. A consumer that cannot work without
  a provider declares a dependency to get startup ordering and a clear failure.
- **Editor completion is a provider.** A package offers completion by
  registering an `editor.completion` provider with `provideCompletions`; the
  editor reads every registration, as `providers.all` does, and Knot defines
  that contract. `editor.registerCompletionProvider` and the separate
  completion registry go away. Merging, stale-result rejection, and
  per-provider failure stay completion-specific in the completion controller,
  so the [capability aggregation](../decisions/semantic-ui.md#capability-aggregation)
  decision still holds; only registration is shared. Completion behaves as
  before.
- **Go to File provides its three lists.** `workspace.files`,
  `workspace.buffers`, and `workspace.actions` each offer `rows(query)`,
  returning the picker rows Search Everywhere shows on its Files, Buffers, and
  Actions tabs for that query in the caller's window. Row IDs are the plain
  identities consumers act on: a file URI, a document ID, a command name.
  Search Everywhere keeps calling the same functions directly.
- **The input list can show picker rows.** Input completions gain the row
  fields the picker paints (description and hint, beside label, icon, and
  match ranges), the input list paints them with the picker's row element so
  both look the same, and an accepted input reports which completion was
  highlighted, so Enter can act on the row rather than only on the text.

## Exclusions

- Importing another package's code as a module (`import { … } from
  "@knot/go-to-file"`) stays a future improvement, [D199](../roadmap.md); it
  would build on these cross-isolate calls.
- No streaming results, cancellation signals, or callbacks across packages.
- No typed contract registry or shared contract-types package: a consumer
  declares the interface it expects locally.
- No new consumers beyond Vim's `:e` and editor completion, and no Ex commands contributed by other
  packages.

## Deferred decisions

- Whether `all` consumers need a way to observe registrations coming and
  going, rather than looking up on each use.
- Whether calls need a per-call timeout before D019's per-turn timeouts.

## Checkpoints

### 1. Provider registry and cross-package calls — ✅ implemented, review gate pending

Deliverables: `providers.register`, `providers.get`, and `providers.all` in the
public API and the in-memory test host, cross-isolate calls with the caller's
window context, lifecycle cleanup on unload and restart, and the documentation
of the new contract.

Validation: product tests with two installed packages and personal config
covering registration, latest-wins `get`, ordered `all`, results and thrown
errors crossing back, the caller's window context reaching the provider, a
disposed or restarted provider rejecting stale handles and disappearing from
lookups, and a consumer running while its provider package failed to start.
Fake-host tests for the same lookup rules. `just check`.

Review gate: a `post-init.js` registering `try.*` commands that register a
provider from personal config, call Go to File-independent sample providers,
and show the results in a message.

Implemented ✅: the registry, calls, cleanup, product tests (including a
personal-config override), fake-host tests, the contract test, and the
providers sections in the [extension host](../architecture/extension-host.md#providers)
and [extension guide](../extension-api.md#share-a-feature-with-other-packages).
Scope decisions made while implementing:
- A provider method receives one JSON argument and a call context
  `{ window }`, so `rows(query)` on a handle is `rows(query, context)` in the
  provider, and `provideCompletions(request, context)` keeps its shape in
  checkpoint 2.
- A package calling its own provider is rejected with `UnsupportedOperation`
  rather than left to deadlock.
- A disposed or restarted registration rejects with `ProviderNotFound`; a
  provider's throw rejects with `ProviderFailure` and its message.

**Implementation context**
- Mirror editor completion providers for the reverse call path:
  `registerCompletionProvider` and `globalThis.__knotRequestCompletions` in
  `crates/knot-host/src/js/bootstrap.js`, their registration in
  `src/app/extensions/semantics.rs`, and the request fan-out from
  `src/app/extensions/host.rs`.
- Host operations go in `src/app/extensions/host/handlers.rs` with
  `#[host_operation]`; transport types in `crates/knot-protocol/src/lib.rs`;
  the registry's plain-data rules (ordering, latest wins, ownership by
  lifecycle) in a new `crates/knot-model/src/providers.rs` with plain tests,
  wrapped by the extension host's semantics state.
- Lifecycle cleanup joins the existing teardown that drops completion
  providers and other registrations on unload and restart
  (`docs/architecture/extension-host.md`, restart section).
- Window context: reuse how command contexts carry `windowTarget` and how
  `windowTarget()` in `bootstrap.js` maps native windows to frozen target
  objects.
- Public contract: `crates/knot-host/src/js/knot.d.ts`, `knot.js` exports, and
  `scripts/extensions/testing/knot.ts` and `host.ts` for the fake host; extend
  `scripts/extensions/contract.test.ts`.
- Tests: `src/app/product/tests/extension_api.rs`, `startup.rs`, and
  `extension_restart.rs` patterns.
- Docs: a providers section in `docs/architecture/extension-host.md` and
  `docs/extension-api.md`.

### 2. Editor completion on providers

Deliverables: completion providers registered as `editor.completion`
providers and read by the editor through the provider registry; the
completion registry, `editor.registerCompletionProvider`, its unregister
operation, and the `CompletionProviderNotFound` error removed; completion
behavior unchanged.

Validation: the existing completion product and fake-host tests moved to the
new registration and passing unchanged in behavior, plus a test that a
completion provider registered after another still contributes (no latest
wins). `just check`.

Review gate: a `try.*` command that registers an `editor.completion` provider
from personal config; trigger completion in an editor and see its items merged
with any other provider's.

**Implementation context**
- Registry and request path: `CompletionProviderRegistry` in
  `crates/knot-model/src/completion.rs`, its use in
  `src/app/extensions/semantics.rs` (`completion_snapshot`) and
  `src/app/extensions/host.rs` (`start_completion`); snapshot the
  `editor.completion` registrations from the checkpoint 1 registry instead.
- JS side: `registerCompletionProvider` and `__knotRequestCompletions` in
  `crates/knot-host/src/js/bootstrap.js`, which becomes a call to the
  provider's `provideCompletions` through the generic provider call path;
  `editor.registerCompletionProvider` and `CompletionProvider` in `knot.d.ts`
  and `knot.js`; the handlers at `register_completion_provider` in
  `src/app/extensions/host/handlers.rs`.
- Tests: completion cases in `src/app/product/tests/extension_api.rs` and
  `startup.rs`, and `scripts/extensions/testing/completion.test.ts`.
- Docs: the completion section of `docs/architecture/semantic-ui.md`
  (registration now through providers) and `docs/extension-api.md`.

### 3. Go to File provides its lists

Deliverables: `workspace.files`, `workspace.buffers`, and `workspace.actions`
registered by `@knot/go-to-file`, returning the same rows Search Everywhere
shows, with plain row IDs.

Validation: package tests through the real editor asserting that each
provider's rows match Search Everywhere's tab for the same query and window,
including recent files first and multi-root labels. `just check`.

Review gate: `try.*` commands that call each provider for a query and show the
rows, beside running Cmd-P with the same query.

**Implementation context**
- `extensions/@knot/go-to-file/src/searchEverywhere.ts` (`bufferRows`,
  `actionRows`, the tab row building) and `src/fileRows.ts`; split the
  per-tab row building into functions that take the window context, used by
  both the picker and the providers. Register in `src/extension.ts` `start()`.
- Tests next to `extensions/@knot/go-to-file/test/extension.test.ts` and
  `rows.test.ts`.

### 4. Picker rows in the input list

Deliverables: input completions with description and hint, painted by the
picker's row element, and the accepted input reporting the highlighted
completion.

Validation: product transient tests for painting data, highlight movement,
Tab, and Enter reporting the highlighted completion or none; fake-host
support. `just check`.

Review gate: a `try.*` command showing an input whose completions carry
descriptions and hints and reporting what Enter accepted.

**Implementation context**
- `src/app/transient.rs` (input view, its `uniform_list` of completions,
  `accept_completion`), `src/app/picker.rs` (the picker's row painting at
  `picker-rows`) to extract a shared row element.
- `InputCompletion` and `InputResult` in `crates/knot-protocol/src/lib.rs`,
  `show_input` in `src/app/extensions/host/handlers.rs`, and the types in
  `knot.d.ts`.
- Tests in `src/app/product/tests/transient.rs` and
  `scripts/extensions/testing/transient.test.ts`.

### 5. Vim's `:e` uses the files provider

Deliverables: `:e` and `:e <query>` list `workspace.files` rows under the Ex
line; Enter opens the highlighted file or the typed path; Tab fills the
highlighted file's path; no list without a provider. The bare `:e` running Go
to File goes away.

Validation: Vim tests through the real editor for listing, filtering, Enter on
a row, Enter with no match opening the typed path, Tab, falling back with Go
to File absent, and `:set` completions unchanged. `just check`.

Review gate: run `:e`, filter, open, and create a file in the real app; a
`try.*` command restarts Go to File to check the fallback.

**Implementation context**
- `extensions/@knot/vim/src/command-line.ts` (`commandLine`, `edit`,
  `baseDirectory`) and `src/ex.ts` (`parseEx`, `completeEx`, `resolvePath`);
  the files contract interface declared locally in Vim.
- Tests in `extensions/@knot/vim/test/ex.test.ts` and
  `test/cases/ex.jsonc`.
- Close the item: record the decision in `docs/decisions/extension-host.md`
  and its index, update `docs/architecture.md` if ownership changed, and
  remove this plan.
