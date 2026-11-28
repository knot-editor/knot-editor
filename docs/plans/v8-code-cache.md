# V8 code cache for extension modules

Status: 🚧 planned. Source: step 4a of the [test speed plan](test-speed.md).
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
In the `knot-ext-vcs` tests, `wait_for_product_startup` is about 49ms per
test (4.5s of an 18.9s serial run) and the whole product launch floor is
about 0.15s per test (step 4 of the test speed plan); profiling shows V8
scanning, parsing and lazy compiling in that startup. The same floor
applies to every product test (`app::product::tests`, 584 tests, and the
other `knot-ext-*` crates), so the win is suite-wide. A real launch pays
it too, once per package.

Every lifecycle also compiles the host's own modules: `ModuleRegistry::new`
(in `engine/mod.rs`) seeds each registry with the private bootstrap, the
public `knot` facade, and the contract module (compiled from
`crates/knot-host/src/js/`). Those are the same bytes in every isolate and
may dominate a small package's own modules, so checkpoint 1 measures them
separately and the scope below follows that measurement.

## Design

- **Mechanism.** When cached data is present for a specifier,
  `compile_module_graph` builds a `v8::script_compiler::Source` with
  `Source::new_with_cached_data` and calls `compile_module2` with
  `CompileOptions::ConsumeCodeCache` instead of `compile_module`. V8 checks
  the cache against the source hash, its own version, and its flags, and
  rejects a mismatch; after compiling, `source.get_cached_data()` reports
  `rejected()`. A rejected cache compiles from source and the result is the
  same. A cache is therefore never a correctness input, and a corrupt or
  stale one costs only time.
- **Carrying caches.** Caches travel with the sources, not as a
  `compile_module_graph` parameter: `ModuleRegistry` gains a map from
  specifier to cached bytes beside its `sources` map, so the recursive
  compile, the contract pre-compile (`compile_package_contract`), and
  fixture evaluation all consume the same way. `validate_module_graph_entry`
  compiles without evaluating, so it consumes a cache when one is supplied
  but never produces one.
- **Producing a cache.** After a module has run,
  `get_unbound_module_script().create_code_cache()` includes the functions
  that executed, which a cache taken right after compile would leave out.
  How much that matters for modules is unmeasured; checkpoint 1 compares a
  compile-only cache against a post-run one before the location decision
  relies on it.
- **Where caches live.** Decide at the first checkpoint, after measuring:
  - *Build time.* Caches ship beside the sources and `BundledSource` grows
    a cache field (`&'static [&'static [u8]]` via `include_bytes!`). The
    producer cannot be a bare V8 isolate in a `knot-bundled` build script:
    a package entry imports the `knot` facade and private bootstrap, which
    expect the native bindings only a runtime capsule registers, and the
    build helper today knows only `dist/…` paths, not the synthetic
    `file://` specifiers the app layer assigns. Group crates also do not
    depend on `v8`, so a build script would pay the `rusty_v8` build once
    per group crate. A build-time producer therefore means a one-shot
    harness that loads each package through the real `knot-host` capsule
    (as the test-support step below does) and writes the caches where the
    build helper picks them up, with the producer using the same specifier
    and origin the consumer will use.
  - *First run.* After a lifecycle's entry evaluation settles, the capsule
    iterates its registry's compiled modules, calls `create_code_cache` on
    each, and writes the bytes to a file keyed by a hash of the source
    (V8 rejects a version or flag mismatch itself, so the key needs no V8
    version; `script_compiler::cached_data_version_tag()` is available if
    a key ever wants one). Knot has no application cache folder yet, so
    this route introduces one (`dirs::cache_dir()`, beside the config
    directory `user_config_root` selects). Contract modules shared by
    several packages are then produced once per consuming capsule until
    one writes them; identical bytes make that harmless. Simple, but
    tests and CI then depend on a writable, warm directory, and a cold
    run pays full cost.
  - A third option is a one-shot `cargo` test-support step that fills a
    cache for tests only. Likely not worth it; keep it only if build time
    rules out the first.
- **Scope.** Whatever checkpoint 1 shows dominating, from: the host
  modules every registry seeds, bundled packages, and the contract
  modules they compile. User and third-party packages (read from disk at
  runtime) can take the first-run route later or never; the design should
  not require it.
- **Disabling.** `KNOT_DISABLE_CODE_CACHE`, read when a module registry
  is created, skips both consuming and producing, so a suspected cache
  bug can be ruled out and the package suites can run both ways.
- **Hit and miss counter.** The engine counts caches consumed with
  `rejected() == false` (hits), caches rejected, and modules compiled
  without a cache, exposed for tests, so a cache that quietly never
  applies fails a test instead of passing silently.
- **Not a startup snapshot.** V8 snapshots (`SnapshotCreator`) do not capture
  module instances, need external-reference tables for every host binding, and
  would couple isolates that are isolated by design. Code cache gets most of
  the parse and compile cost without that.

## Checkpoints

1. **Measure.** Profile `TestProduct::launch` (a `sample` of one test binary
   run with `--test-threads 1`, as in step 3 of the test speed plan) and split
   the startup into isolate creation, module parse and compile, module
   evaluation, and host handshake, counting the isolates and the bytes
   compiled per launch **per specifier**, split into host modules,
   bundled package modules, and contract modules. The denominator for the
   stop gate is the 49ms `wait_for_product_startup` figure above: if parse
   and compile are under about a third of it, stop and record why in the
   test speed plan. Also compare a compile-only cache with a post-run
   cache on the largest module (hit rate and compile time saved).
   Otherwise pick the cache location from the options above, and write
   the decision, with the per-specifier numbers, here.
2. **Consume.** Give `ModuleRegistry` the specifier-to-cache map, use it in
   `compile_module_graph` as above, add the hit/miss counter, and honor
   `KNOT_DISABLE_CODE_CACHE`. Test: a cache built from a module compiles
   it with `rejected() == false` and the counter records a hit; the same
   cache against changed source is rejected and the module still compiles
   and runs; garbage bytes are rejected without failing the launch.
3. **Produce and ship.** Implement the chosen location, including the
   production hook and keying it needs. Packages loaded with and without
   a cache must behave identically; run the package test suites both ways
   (with `KNOT_DISABLE_CODE_CACHE=1` and without).
4. **Measure again.** Re-run `just test-timings` and the vcs serial run
   (`cargo nextest run -p knot-ext-vcs -j1`), and record the new startup
   floor, per-crate totals and any build-time cost in the test speed
   plan's Progress table. Run `just check`.
5. **Docs.** Record the mechanism and where caches come from in
   [extension host](../architecture/extension-host.md), and delete this plan.

## Risks

- Build time and size: a producer step that runs V8, and caches that grow
  the binary or the target folder. A cache baked into the binary is also
  specific to the architecture and V8 flags of the machine that built it;
  consumers on anything else reject it and pay full cost.
- V8 flags or version drift between producing and consuming processes make
  every cache miss silently. Checkpoint 2's rejection test and hit/miss
  counter guard against a cache that quietly never applies.
- Cache creation after execution can capture state-dependent lazy functions;
  verify that code run from a cache behaves as from source across the package
  suites.
