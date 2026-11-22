# D199 — Typed contracts between packages

Status: 🚧 checkpoint 1 done; checkpoint 2 next. Item: [D199](../roadmap.md#extension-platform-and-language-tooling),
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
[contract.js](../../crates/knot-host/src/js/contract.js) (its own file, not
in `bootstrap.js`, so the Node tests import the real builder); types in
[knot.d.ts](../../crates/knot-host/src/js/knot.d.ts); the validator is
[contracts.rs](../../crates/knot-model/src/contracts.rs). Agreed: a schema
value is its own frozen descriptor; `s.void()` marks a missing argument or
result and carries null; `s.optional(x)` is only an object field and means
absent; `s.nullable(x)` is `union([x, null()])`; a violation reads
`files: result[1].label: expected string, found number 2 (the provider's schema)`.
Checkpoint 2 adds `serve`/`client`, the call-context parameter of
`ContractImplementation`, and the package/contract prefix on violations.

### 2. Cross-isolate transport: manifest, import resolution, serve/client

Make a fixture consumer call a fixture provider through a contract, with
validation on the boundary.

- **Deliverables:** `contract` field on the manifest; bare-specifier
  resolution gated by `requires`; `serve`/`client` over a registry that
  stores the schema descriptor per registration; validated dispatch
  replacing the `callProvider` JSON passthrough.
- **Implementation context:** manifest and capture in
  [package.rs](../../crates/knot-model/src/extensions/package.rs:41)
  (`Manifest`, `deny_unknown_fields` — update its tests); module
  resolution in
  [modules.rs](../../crates/knot-host/src/engine/modules.rs:301)
  (`resolve_module_specifier` today allows only relative or `knot`);
  contract sources come from the dependency's captured
  `InstalledPackage.sources`, keyed under the consumer's graph without
  evaluating the dependency's `main`; registry extends
  [providers.rs](../../crates/knot-model/src/providers.rs); handlers
  replace `registerProvider`/`getProvider`/`listProviders`/`callProvider`
  in [handlers.rs](../../src/app/extensions/host/handlers.rs:2815),
  keeping the `ProviderNotFound` / own-package / window-context behavior.
  Startup ordering already follows `requires`
  ([load.rs](../../crates/knot-model/src/extensions/load.rs)).
- **Validation:** host engine tests for resolution gating, contract-graph
  purity, and disposal; product tests in
  [providers.rs tests](../../src/app/product/tests/providers.rs) rewritten
  as contract cases: happy path, invalid argument, invalid result,
  provider throw, disposed client, self-call rejection, stale-consumer
  schema mismatch.
- **Review gate:** a two-package fixture calling through a contract in a
  product test; API wording in `knot.d.ts` reviewed.

### 3. Migrate the native-interpreted providers

Move `editor.completion` and `editor.paste.<languageId>` onto Knot-defined
contracts; native aggregation and failure isolation stay as they are.

- **Implementation context:** Knot exports the two contract values from
  `"knot"`; serve-side registration feeds the same registry checkpoint 2
  built; native callers in
  [semantics.rs](../../src/app/extensions/semantics.rs) (completion) and
  the paste path in `src/app/editor/` switch from name/method strings to
  the contract descriptors, keeping per-provider failure isolation and
  revision discarding. Bundled servers:
  [language-markdown](../../extensions/@knot/language-markdown/src/extension.ts:14)
  (paste) and any completion registrants.
- **Validation:** existing completion/paste product tests
  ([extension_api.rs](../../src/app/product/tests/extension_api.rs),
  [editing.rs](../../src/app/product/tests/editing.rs)) pass against the
  new path; a provider returning malformed items fails only its own part.
- **Review gate:** no behavior change visible in completion or paste.

### 4. Migrate `workspace.*` and remove `providers.*`

Move the bundled package-to-package providers onto a published contract
and delete the old mechanism.

- **Implementation context:** `@knot/go-to-file` adds a contract module +
  manifest `contract` field and serves one `workspace` contract with
  `files`, `buffers`, and `actions`
  ([extension.ts](../../extensions/@knot/go-to-file/src/extension.ts:40));
  Vim imports it under a new `requires` entry, keeps its try/catch
  tolerance, and drops its local redeclaration
  ([command-line.ts](../../extensions/@knot/vim/src/command-line.ts:32));
  remove `providers` from
  [knot.js](../../crates/knot-host/src/js/knot.js:96),
  [knot.d.ts](../../crates/knot-host/src/js/knot.d.ts:2621), bootstrap, the
  model registry's old shape, and handlers; keep
  `docs/schemas/knot.schema.json` in sync with the manifest field.
- **Validation:** extension `check`/`test`, Vim and go-to-file behavior
  cases, full `just check`.
- **Review gate:** `providers.*` gone from the public API.

### 5. Document

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
  would run them in every consumer. The checkpoint-2 tests pin the
  restriction; the guide states the rule.
- Validation sits on the completion path while typing; checkpoint 3
  measures completion latency before and after.
- Native-interpreted contracts invert the dependency direction (Knot is
  the consumer). Keep their aggregation in native code rather than
  forcing it through `client()` semantics.
