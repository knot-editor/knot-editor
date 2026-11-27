# V8 code cache for extension modules

Status: 🚧 planned. Source: step 4 of the [test speed plan](test-speed.md).
Background: [extension host](../architecture/extension-host.md).

## Outcome

Launching a product, and so every product test, stops re-parsing and
re-compiling the bundled TypeScript output. Modules compile from a V8 code
cache when one is valid and from source otherwise, with the same behavior
either way.

## Why

Every package gets its own isolate (`Isolate::new` in
`crates/knot-host/src/engine/capsule.rs`), and each module is compiled from
source (`compile_module_graph` in `engine/modules.rs`), with nothing cached.
In the `knot-ext-vcs` tests, product startup is about 49ms per test (4.5s of an
18.9s serial run), and profiling shows V8 scanning, parsing and lazy compiling.
The same floor applies to every product test (`app::product::tests`, 584
tests, and the other `knot-ext-*` crates), so the win is suite-wide. A real
launch pays it too, once per package.

## Design

- **Mechanism.** `compile_module` takes a `ScriptCompiler::Source` built with
  `CachedData` and `CompileOptions::ConsumeCodeCache`. V8 checks the cache
  against the source hash, its own version, and its flags, and rejects a
  mismatch (`CachedData::rejected()`); a rejected cache compiles from source
  and the result is the same. A cache is therefore never a correctness input,
  and a corrupt or stale one costs only time.
- **Producing a cache.** After a module has run, `create_code_cache` on its
  `UnboundModuleScript` includes the functions that executed, which a cache
  taken right after compile would leave out. So caches come from a run, not
  from a bare compile.
- **Where caches live.** Decide at the first checkpoint, after measuring:
  - *Build time.* The `knot-bundled` build helper compiles the TypeScript; a
    step after it would load each package in a V8 isolate and store the caches
    beside the sources. Fixed per build and needs no writable state, but the
    build script then links V8 (which `rusty_v8` makes costly to compile twice)
    and `BundledSource` grows a cache field.
  - *First run.* Caches are written to a directory under the application's
    cache folder, keyed by source hash and V8 version. Simple, but tests and
    CI then depend on a writable, warm directory, and a cold run pays full
    cost.
  - A third option is a one-shot `cargo` test-support step that fills a cache
    for tests only. Likely not worth it; keep it only if build time rules out
    the first.
- **Scope.** Bundled packages and the contract modules they compile. User and
  third-party packages (read from disk at runtime) can take the first-run
  route later or never; the design should not require it.
- **Not a startup snapshot.** V8 snapshots (`SnapshotCreator`) do not capture
  module instances, need external-reference tables for every host binding, and
  would couple isolates that are isolated by design. Code cache gets most of
  the parse and compile cost without that.

## Checkpoints

1. **Measure.** Profile `TestProduct::launch` (a `sample` of one test binary
   run with `--test-threads 1`, as in step 3 of the test speed plan) and split
   the 49ms into isolate creation, module parse and compile, module
   evaluation, and host handshake. Count the isolates and bytes compiled per
   launch. If compile and parse are under about a third, stop and record why in
   the test speed plan. Otherwise pick the cache location from the options
   above, and write the decision here.
2. **Consume.** Let `compile_module_graph` take optional cached data per
   specifier and use it when present. Test: a cache built from a module
   compiles it with `rejected() == false`; the same cache against changed
   source is rejected and the module still compiles and runs; garbage bytes
   are rejected without failing the launch.
3. **Produce and ship.** Implement the chosen location. Packages loaded with
   and without a cache must behave identically; run the package test suites
   both ways. Keep a way to disable the cache (an environment variable read at
   startup is enough) so a suspected cache bug can be ruled out.
4. **Measure again.** Re-run `just test-timings` and the vcs serial run
   (`cargo nextest run -p knot-ext-vcs -j1`), and record the new startup
   floor, per-crate totals and any build-time cost in the test speed plan's
   Progress table. Run `just check`.
5. **Docs.** Record the mechanism and where caches come from in
   [extension host](../architecture/extension-host.md), and delete this plan.

## Risks

- Build time and size: a build step that runs V8, and caches that grow the
  binary or the target folder.
- V8 flags or version drift between producing and consuming processes make
  every cache miss silently. Checkpoint 2's rejection test and a startup
  counter of hits and misses (visible in tests) guard against a cache that
  quietly never applies.
- Cache creation after execution can capture state-dependent lazy functions;
  verify that code run from a cache behaves as from source across the package
  suites.
