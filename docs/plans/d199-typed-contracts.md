# D199 — Typed contracts between packages

Status: 🚧 checkpoints 1–5 done, 6 awaiting review. Item: [D199](../roadmap.md#extension-platform-and-language-tooling),
part of the [version control plan](vcs.md) (milestone 1, before D211).

## Outcome

A package reuses another package through a contract defined once in code:
the provider publishes a schema-built contract, a consumer imports the
provider as a module and calls typed functions that execute in the
provider's isolate, and the host validates every argument and result
against the provider's registered schema. The untyped `providers.*`
mechanism is removed; its current users (native completion and paste,
and the bundled `workspace.*` providers) move onto contracts.

## Agreed design

- **Contract module.** A package declares `contract: "<relative .js path>"`
  in its manifest (`knot.jsonc`), next to `main`. That module builds and
  exports contract values with `defineContract(name, methods)` and the schema
  builder `s`, both from the narrow `"knot/contract"` specifier. It is pure
  schema code: it may import `"knot/contract"` and its own relative modules,
  nothing else, so purity is structural, not a convention. The host rejects a
  contract graph that reaches `"knot"`, the package's `main`, or any other
  package.
- **Identity.** A contract is keyed by the owning package plus its short
  name, e.g. `("@knot/go-to-file", "workspace")`. The author writes only the
  name; the host qualifies it with the package the contract module resolved
  from, not with whoever calls `serve`, so names cannot collide across
  packages or be claimed by another. Two contracts with one name in a package
  fail startup validation. One contract module may export several contracts.
- **Static import, gated by `requires`.** A consumer writes
  `import { vcs } from "@knot/vcs"`. The bare specifier is the dependency's
  canonical package ID; the host resolves it to that package's contract
  module and evaluates it in the consumer's isolate, so no provider code
  runs there. Importing a package not listed in the consumer manifest's
  `requires` fails module resolution at startup validation, like an
  escaping relative import today. Personal config has no manifest and may
  import any loaded package's contract. Dynamic imports stay rejected.
- **Binding through the contract value.** The contract object carries
  `serve(implementation)` (provider side; returns the usual `Disposable`)
  and `client()` (consumer side; async, resolves the live client whose
  methods are the contract's functions). Any package may `serve` a contract;
  `client()` resolves the latest live registration, so personal config can
  override a bundled provider and disposal falls back to the previous one.
  A consumer that must tolerate a missing or restarting provider catches the
  rejection, as Vim does for its file list. A client call rejects once the
  registration is disposed or its lifecycle ends — look the client up
  again — mirroring today's `ProviderNotFound`. Calling a contract you
  serve yourself rejects, as with providers today; use the code directly.
  Method context (the caller's command window) crosses out of band as
  today, never as a schema value.
- **Schema builder.** Small and JSON-shaped: string, int (safe integers
  only, as elsewhere on the boundary), number, boolean, null, literal /
  string enum, array, tuple, record, strict object with optional fields,
  union, nullable, and a `json` escape hatch for genuinely free-form values
  that follows the `validateJsonValue` rules. `s.fn(argument, result)` is the
  one method kind: async, at most one argument, one result (`void`
  allowed). It is our own builder, not a library: the descriptor must be
  plain data the Rust validator reads, and contract modules stay small. No callbacks, handles, targets, buffers, or
  events cross in this item; subscriptions are deferred (see Exclusions).
  TypeScript types are inferred from the builder value, so the contract
  module is the single source for types, runtime schema, and docs.
- **Host-side validation against both schemas.** `serve` registers the
  provider's serialized descriptor and `client()` sends the consumer's; the
  host compiles each once. A call validates the argument against the
  consumer's then the provider's schema before dispatch, and the result
  against the provider's then the consumer's before answering. When the two
  descriptors are structurally equal, compared once at `client()` time to
  pick the mode and never to fail the lookup, each value is checked once.
  Nothing validates in JavaScript. A consumer built against a different
  revision fails per call with a contract error naming the package, method,
  and offending path, rather than failing to load. There is no contract
  version field and no hash check. Implementation bugs on either side surface
  the same way instead of delivering malformed JSON.
- **Native-interpreted contracts are Knot-defined.** `completion` and
  `paste(languageId)` are exported from `"knot"` with owner `knot`; any
  package may `serve` them, and native code keeps its current aggregation
  (every completion registration, latest paste registration) by calling the
  registrations itself. They do not go through `client()` and need no
  `requires` entry. `paste` is a memoized factory, one contract per language
  with one shared schema, so a paste allocates nothing: native looks up
  registrations by qualified name and validates only the provider's result
  against a descriptor compiled when it was served. Knot is the consumer
  and trusts its own descriptor, so completion validates each result item
  once.
- **Bundled `workspace.*` contracts.** `@knot/go-to-file` publishes its
  `rows(query)` lists as contracts; consumers (Vim's `:…` completion)
  declare it in `requires` and import it. This is the in-repo proof that
  D211's `@knot/vcs` / `@knot/git` split will follow.

## Exclusions and deferred decisions

- No event/subscription method kind, no streaming, no callbacks or live
  handles crossing a contract. VCS status watching designs that later if
  D213 needs it.
- No contract versioning, negotiation, or compatibility ranges.
- No change to entitlement, storage, or command mechanisms; contracts do
  not widen what a loaded package may do.
- No optional dependencies: a package that imports a contract must list it
  in `requires`, and a tolerant consumer such as Vim catches the `client()`
  rejection. Optional dependencies are D226.
- `@knot/vcs` itself is D211; this item proves the mechanism on fixture
  packages and the migrated bundled providers only.

## Checkpoints

### 1. Schema builder, descriptor, and host validator ✅

Done. The builder is the built-in `"knot/contract"` module,
[contract.ts](../../crates/knot-host/src/js/contract.ts) (its own file, not
in `bootstrap.ts`, so the Node tests import the real builder); types in
[knot.d.ts](../../crates/knot-host/src/js/knot.d.ts); the validator is
[contracts.rs](../../crates/knot-model/src/contracts.rs). Agreed: a schema
value is its own frozen descriptor; `s.void()` marks a missing argument or
result and carries null; `s.optional(x)` is only an object field and means
absent; `s.nullable(x)` is `union([x, null()])`; a violation reads
`files: result[1].label: expected string, found number 2 (the provider's schema)`.
Checkpoint 4 adds `serve`/`client`, the call-context parameter of
`ContractImplementation`, and the package/contract prefix on violations.

### 2. Host JavaScript in TypeScript, with unit tests ✅

Done. The host's `bootstrap`, `knot`, and `contract` modules are strict
TypeScript in [src/js](../../crates/knot-host/src/js), compiled by
`knot-host`'s build script and tied to `knot.d.ts` with `satisfies` and a
two-way export-name check (see [development](../development.md)); values from
the Rust side are typed `Native` in `native.d.ts`, the only place `any` is
allowed. The host project is standalone, so package settings never rebuild
it. Unit tests live in [js-tests](../../crates/knot-host/js-tests), with the
native bindings stubbed. The export-name check does not cover members of a
group object. Types exposed one drift: presentations returned by
`documents.open` and its siblings omitted the declared `tool` and
`directory`, which are now `null`.

### 3. Run the declared API against the real host ✅

Done. [coverage.js](../../scripts/extensions/examples/coverage.js) calls the
API groups with their declared option shapes, run by
`declared_api_options_are_accepted_by_the_real_host` in
[startup.rs](../../src/app/product/tests/startup.rs); `exerciseEditor` covers
the buffer and editor in a real tab. The host rejected no declared option.
Not covered because they block on the user or restructure the test windows:
pickers, input and confirmation prompts, `windows.open`/`focus`, the
`workspaces` folder methods, `workbench.replaceWindows`, `recovery`
retain/restore/release, `terminals.open`, and `files.reveal`. Later
checkpoints extend the example as they add API.

### 4. Cross-isolate transport: manifest, import resolution, serve/client ✅

Done. A package declares `contract` in its manifest
([package.rs](../../crates/knot-model/src/extensions/package.rs),
`knot.schema.json`); a bare specifier naming a required package resolves to
that package's contract module, keyed under its own root in the consumer's
graph, without running its `main`
([modules.rs](../../crates/knot-host/src/engine/modules.rs)). A contract graph
imports only `"knot/contract"` and its own package's modules; the own contract
graph is compiled first, so it is checked even when `main` does not import it.
Each package's `"knot/contract"` is its own wrapper around the builder carrying
the package ID, so ownership comes from module resolution, and a name defined
twice in one package throws. `serve`/`client` go through the provider registry
([providers.rs](../../crates/knot-model/src/providers.rs)), which keeps each
registration's schema and each consumer's binding; a call validates against
both schemas. New codes `ContractNotFound` and `ContractViolation` (read as
`@example/provider greeter: greet: argument: expected string, found number 1 (the provider's schema)`);
a throwing method stays `ProviderFailure` and a self-call rejects at call time
with `UnsupportedOperation`. Product cases are in
[contracts.rs](../../src/app/product/tests/contracts.rs); `providers.*` and
its tests stay until checkpoints 5 and 6. Personal config importing contracts
moved to checkpoint 6.

### 5. Migrate the native-interpreted providers ✅

Done. `"knot"` exports `completionContract` and `pasteContract(languageId)`
(`completion` was taken by the `completion.match` group), contracts named
`completion` and `paste.<languageId>` owned by `knot`; see
[extension host](../architecture/extension-host.md#providers). Native calls
check only the provider's result against the descriptor compiled at serve
time, so a malformed result fails only that provider's part, and extra
completion item fields are now rejected rather than ignored. Validating 500
items costs about 0.3 ms including the parse. The test fake serves the two
contracts as providers `knot:completion` and `knot:paste.<languageId>`.

### 6. Migrate `workspace.*` and remove `providers.*`

Implemented, awaiting review. `@knot/go-to-file` publishes a `workspace`
contract (`files`, `buffers`, `actions`) from
[contract.ts](../../extensions/@knot/go-to-file/src/contract.ts); Vim requires
it and imports it from `"@knot/go-to-file"`, keeping its tolerance of a
missing or restarting provider. `providers` is gone from the facade,
[knot.d.ts](../../crates/knot-host/src/js/knot.d.ts), bootstrap, the host
operations, `knot-protocol`, and the registry, whose registrations are all
served contracts. Personal config may import any discovered package's contract
from post-init: the host adds the contract modules to the loaded config graph
once packages load, and the check before that resolves a package import to an
empty stand-in. Decisions made on the way:

- A bundled package compiles alone (its own `rootDir`, fingerprint, and
  build), so Vim cannot import go-to-file's TypeScript for types; it carries a
  small ambient declaration of the one method it calls
  ([go-to-file.d.ts](../../extensions/@knot/vim/src/go-to-file.d.ts)), and the
  provider's schema still checks every call.
- The test fake serves contracts under their names (Knot's under `knot:`)
  and `knot/testing` exposes `contractClient(key)` to call them; it validates
  no schema. `register.mjs` maps `@knot/go-to-file` to its contract module.
- Vim's `requires` is hard, so Vim does not load without Go to File until
  D226. Vim's tests load both, and the missing-list case serves nothing from
  a Go to File that only publishes the contract.
- **Review gate:** `providers.*` is gone from the public API; confirm `:e `
  file completion in Vim and Search Everywhere's tabs in the running app.

### 7. Document

- **Deliverables:** a contract example under
  `scripts/extensions/examples/`; update
  [extension-host.md](../architecture/extension-host.md#providers) and the
  [extension guide](../extension-api/README.md), including the purity rule;
  check the decision entry in `docs/decisions/extension-host.md` still
  matches what was built.
- **Review gate:** D211 can now be planned against the contract API as
  documented.

## Risks

- Contract-graph purity is enforced by import restriction, not by
  sandboxing the evaluated module; a contract module with side effects
  would run them in every consumer. The checkpoint-4 tests pin the
  restriction; the guide states the rule.
- Validation sits on the completion path while typing; checkpoint 5
  measures completion latency before and after.
- Native-interpreted contracts invert the dependency direction (Knot is
  the consumer). Keep their aggregation in native code rather than
  forcing it through `client()` semantics.
