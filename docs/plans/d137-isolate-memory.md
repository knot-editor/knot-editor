# D137 — Extension isolate memory

Status: 🚧 approved, not started. Selected in the [roadmap](../roadmap.md);
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
- **Young generation sized for idle packages.** `CreateParams` sets only the
  maximum heap today. Set a small initial and maximum young generation, chosen
  from the measurements, since extensions are mostly idle and allocate in short
  turns.
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

### 1. Measurement

Deliverables: the developer command and a recorded baseline in this plan:
resident size with N and N+1 packages, per-package heap after startup, and
isolate creation time, on an Apple M-series release build.

Validation: a host test that heap statistics are reported for every live
capsule and exclude disposed ones; the measurement procedure written here so
later checkpoints repeat it exactly.

Review gate: the baseline decides whether checkpoints 2 and 3 are both worth
doing and in which order.

**Implementation context**

- Isolate creation: `RuntimeCapsule::new_with_graph` in
  `src/host/engine/capsule.rs`; a heap-statistics read needs a turn-boundary
  lock like other capsule operations.
- Pool control surface: `src/host/pool.rs` and `src/host/engine/pool.rs`
  expose operations without V8 types, so statistics cross as plain numbers.
- Command: beside `extensions.show-startup-report` in
  `src/app/product/commands.rs`, presenting through the native picker or a
  message like the startup report.

### 2. Startup snapshot

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

### 3. Heap sizing and idle release

Deliverables: tuned young-generation limits and low-memory release after
startup and long turns.

Validation: host tests that an isolate still reaches its heap-limit failure
correctly, and that a package allocating in a long turn runs normally; the
measurement repeated and recorded.

Review gate: the final numbers. Record the result and rationale in
[decisions](../decisions.md#extension-runtime) and remove this plan.

**Implementation context**

- `CreateParams` in `RuntimeCapsule::new_with_graph` and `IsolateConfig` in
  `src/host/engine/mod.rs`.
- Turn completion in `src/host/engine/execution.rs` and the worker loop in
  `src/host/engine/pool.rs`, where the lock is held.
