# D137 — Extension isolate memory

Status: 🚧 checkpoints 1 and 3 implemented; checkpoint 3 awaiting its review
gate, checkpoint 2 deferred. Selected in the [roadmap](../roadmap.md);
independent of the daily-driver items.

## Goal

Know what one extension package costs in memory and startup time, and make an
idle package cheap enough that splitting bundled workflows into small packages
(one per feature, such as `@knot/go-to-line`) is never a reason to merge them.

Every package keeps its own persistent isolate. Sharing an isolate between
bundled packages, with one context each, is out of scope: it would serialize
their turns, make termination, Restart Extension, and the heap limit
affect every package together, and run bundled code differently from installed
code ([extension runtime](../decisions.md#extension-runtime)). It is
reconsidered only if this item's measurements show per-isolate tuning falls
short.

## Design

- **Measure first.** A `Developer: Show Extension Memory` command lists each
  package's V8 heap statistics (used, total, and external) and the process's
  resident size, gathered at a turn boundary so no lock is held across a turn.
  A release-build measurement records the resident size of startup with and
  without one extra empty bundled package, and per-package heap after startup
  settles, before and after each optimization.
- **Startup snapshot.** `bootstrap.js` and the `knot` facade are the same
  for every package, yet each isolate compiles and evaluates them. Build a V8
  startup snapshot of an initialized extension context once, and create every
  isolate from it, registering the native callbacks
  (`host_request_callback`, `keybinding_callback`, and the others) as external
  references. Per-package state (`RuntimeLocalState`, the module graph) stays
  outside the snapshot. The bootstrap is an ES module, so the first step is
  confirming the `v8` crate can snapshot an evaluated module; if it cannot,
  turn the bootstrap into a classic script that the snapshot evaluates, or fall
  back to a code cache, which removes compile cost but not heap.
- **Young generation sized for idle packages.** ❌ Rejected by checkpoint 3's
  measurements: V8's defaults stay (see checkpoint 3).
- **Release idle memory.** After a package's startup turn settles, and after
  any turn that ran long, signal low memory to that isolate so V8 compacts and
  returns pages. Only at turn boundaries, from the worker that holds the lock.

Whether the snapshot is built at compile time in `build.rs` or once per
process at startup is decided in checkpoint 2 from measured cost: a build-time
snapshot ties the build to the exact V8 version, which the crate already
pins.

## Exclusions

- Sharing isolates or contexts between packages.
- Unloading idle packages and recreating them on demand.
- Changing the per-package heap limit (32 MiB) or quotas (D019).

## Deferred decisions

- Whether the developer command stays once tuning is done, or becomes a test
  that asserts a per-package budget. Decide after checkpoint 3 from how stable
  the numbers are.

## Checkpoints

### 1. Measurement — ✅ complete

`Developer: Show Extension Memory` opens the process's resident size and each
live lifecycle's heap as a clean untitled document
([memory diagnostics](../architecture/extension-host.md#memory-diagnostics)).
Review decision: the baseline showed about half of each idle heap reserved but
unused and a snapshot worth only ~1 ms per package, so checkpoint 3 ran before
checkpoint 2.

**Measurement procedure.** Release build with the Metal Toolchain installed
(`xcodebuild -downloadComponent MetalToolchain`). Run each measurement in its
own process, since a shared test process mixes their resident sizes:

```sh
cargo test --release --lib -- --ignored --nocapture measure_isolate_cost
cargo test --release --lib -- --ignored --nocapture measure_bundled
```

- `measure_isolate_cost` (`src/host/pool.rs`) loads one warm-up package, then
  50 packages on a single worker for each of two sources: an empty module, and
  one that allocates about 8 MiB of temporary objects at startup. It reports
  isolate creation, time through the settled startup, resident size per added
  package (the N to N+1 cost), and median heap per package. Repeat it three
  times and record the range.
- `measure_bundled_package_memory` (`src/app/extensions/host.rs`) starts the
  bundled packages under product discovery, waits until every package is
  between turns, and prints the Show Extension Memory report.

**Measurements** (Apple M1 Pro, macOS, release, V8 152.2, 32 MiB heap limit,
2026-11-02). "Before" is the runtime at checkpoint 1; "after" adds checkpoint
3's idle release.

| Measure | Before | After |
| --- | --- | --- |
| Isolate creation, median | 1.02–1.05 ms | 1.09–1.15 ms |
| Empty package through startup, median | 1.03–1.07 ms | 1.52–1.59 ms |
| Empty package: resident size per package | 1.25–1.55 MiB | 1.21 MiB |
| Empty package: heap used / total | 0.36 / 0.75 MiB | 0.31 / 0.75 MiB |
| 8 MiB startup burst through startup, median | 7.4–7.6 ms | 8.1–8.2 ms |
| 8 MiB startup burst: resident size per package | 13.7–14.0 MiB | 1.60 MiB |
| 8 MiB startup burst: heap used / total | 9.3 / 13.1–13.3 MiB | 0.43 / 1.39 MiB |
| Bundled packages: test process resident size | 61.6–62.3 MiB | 60.8–61.3 MiB |
| Bundled packages: heap used / total, sum of 8 | 4.6 / 12.6 MiB | 3.0 / 7.6 MiB |

| Bundled package, used / total | Before | After |
| --- | --- | --- |
| `@knot/language-markdown` | 0.4 / 1.2 MiB | 0.3 / 0.8 MiB |
| `@knot/language-others` | 0.6 / 1.8 MiB | 0.5 / 1.2 MiB |
| `@knot/language-python` | 0.4 / 1.2 MiB | 0.4 / 0.8 MiB |
| `@knot/language-rust` | 0.4 / 1.2 MiB | 0.4 / 0.8 MiB |
| `@knot/language-toml` | 0.4 / 1.2 MiB | 0.3 / 0.8 MiB |
| `@knot/language-typescript` | 0.5 / 1.2 MiB | 0.4 / 0.8 MiB |
| `@knot/projects` | 0.6 / 2.0 MiB | 0.5 / 1.2 MiB |
| `@knot/themes` | 1.3 / 2.8 MiB | 0.4 / 1.2 MiB |

External memory is 0.0 MiB everywhere. The checkpoint 1 baseline's 114.1 MiB
process size ran both measurements in one process; the "before" column above
repeats it in isolation.

### 2. Startup snapshot — ⏸ deferred, decided at checkpoint 3's review

Deliverables: every isolate starts from one snapshot of the initialized
extension context; the bootstrap is no longer compiled per package.

Validation: the full extension host and product test suites unchanged; tests
that native callbacks, `knot.platform`, private dispatch handles, and
per-package module resolution behave the same from the snapshot; the
measurement repeated and recorded here.

Review gate: memory and creation-time gain against build and code complexity.

**Implementation context**

- `initialize_extension_context` in `src/host/engine/pool.rs` installs
  `__knotNativeBindings` and evaluates `PRIVATE_BOOTSTRAP_SOURCE` and
  `PUBLIC_FACADE_SOURCE` (`src/host/engine/mod.rs`); the snapshot replaces
  this per-isolate work, while slots and callbacks set in
  `new_with_graph` stay per isolate.
- Module evaluation and the module registry: `src/host/engine/modules.rs`.
- One-time V8 setup: `initialize()` in `src/host/engine/mod.rs`.
- Documentation: [extension host](../architecture/extension-host.md#runtime-and-scheduler-flow).

### 3. Heap sizing and idle release — ✅ implemented, 🔍 awaiting review

✅ After a turn settles a package or config entry's evaluation, and after any
turn of 100 ms or more, the worker sends the isolate a low-memory notification
([runtime flow](../architecture/extension-host.md#runtime-and-scheduler-flow)).
It costs about 0.5 ms per package at startup, on the worker. A package that
allocates 8 MiB at startup now keeps 1.6 MiB resident instead of 14 MiB, the
bundled packages hold 7.6 MiB of heap instead of 12.6 MiB, and an empty package
is unchanged (see the measurements in checkpoint 1).

❌ Young-generation sizing is rejected. Maximum sizes of 3 MiB, 1.5 MiB, and
768 KiB, with and without a smaller initial size, left an empty package at
0.75 MiB total, since V8 clamps the young generation to its minimum. Below
3 MiB they slowed a bursty startup by about 2 ms through extra scavenges and
promotion. With idle release, a 3 MiB maximum measured the same as V8's 6 MiB
default.

✅ Tests in `src/host/engine/tests.rs`:
`startup_garbage_is_released_once_the_package_settles` and
`a_long_allocating_turn_runs_normally_and_its_garbage_is_released`, which fail
without the release. The existing `heap_exhaustion_probe_child` still reaches
`HeapLimitExceeded`; a fatal turn skips the release.

Review gate: the final numbers. On approval, record the result and rationale in
[decisions](../decisions.md#extension-runtime), decide checkpoint 2 and the
deferred developer-command question, and remove this plan.
