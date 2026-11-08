# D139 — Per-package isolate floor

Status: ⏸ deferred. Resume once Knot ships enough extensions that their idle
cost shows in daily use; the target is 40 or more packages. Tracked in the
[roadmap](../roadmap.md).

## Goal

Lower the fixed memory each extension package costs while idle, so that 40 or
more small single-feature packages stay cheap. D137 measured that cost and
removed the garbage a package leaves behind after bursts of work, but did not
lower the fixed cost; see [extension runtime](../decisions/extension-host.md#extension-runtime).

## What D137 found

Apple M1 Pro, macOS, release, V8 152.2, 32 MiB heap limit, 2026-11-02, with
idle release in place:

| Empty package | Resident size | Heap used / total | Creation |
| --- | --- | --- | --- |
| Today: isolate, context, bootstrap, `knot` facade | 1.20 MiB | 0.31 / 0.75 MiB | ~1.24 ms |
| Bare isolate and empty context | 1.05 MiB | 0.17 / 0.50 MiB | ~0.49 ms |
| Bootstrap and facade's share | ~0.15 MiB | 0.14 / 0.25 MiB | ~0.75 ms |

At 40 packages the idle cost is about 48 MiB, and the bare isolate alone would
still be about 42 MiB. The bare row came from a temporary change that skipped
`initialize_extension_context`; it is the floor no change to the bootstrap can
go below.

- A startup snapshot saves at most the bootstrap's share, about 0.15 MiB and
  0.75 ms per package, and less memory in practice, because deserialized
  objects still live in each isolate's heap.
- Young-generation sizing does not lower the floor: V8 clamps the young
  generation to its minimum, so an empty package stays at 0.75 MiB total.
- About half of the bare isolate's resident size is heap; the rest is V8's own
  per-isolate structures.

## Measurement procedure

Release build with the Metal Toolchain installed
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
  between turns, and prints the Developer: Show Extension Memory report.
- For the floor, temporarily skip `initialize_extension_context` in
  `RuntimeCapsule::new_with_graph` and rerun `measure_isolate_cost`; never
  commit it.

## Approaches, cheapest first

- **V8 flags for small isolates.** Measure flags that may shrink a mostly idle
  isolate, such as the minimum semi-space size, lite mode, or turning off the
  optimizing compilers, through `v8::V8::set_flags_from_string` before
  initialization. Flags are process-wide, so a flag that slows a busy package
  costs every package.
- **Startup snapshot.** Build a V8 snapshot of an initialized extension context
  once, and create every isolate from it, registering the native callbacks
  (`host_request_callback`, `keybinding_callback`, and the others) as external
  references. Per-package state (`RuntimeLocalState`, the module graph) stays
  outside the snapshot. The bootstrap is an ES module, so first confirm the `v8`
  crate can snapshot an evaluated module; otherwise turn the bootstrap into a
  classic script, or fall back to a code cache, which removes compile cost but
  not heap. Decide between building it in `build.rs` and once per process from
  measured cost. `initialize_extension_context` in `src/host/engine/pool.rs`
  holds the per-isolate work it replaces.
- **Sharing or unloading, only if the above fall short.** Several bundled
  packages per isolate with one context each would pay the floor once, but
  would serialize their turns, make termination, Restart Extension, and the
  heap limit affect every package together, and run bundled code differently
  from installed code. Unloading idle packages and recreating them on demand
  would need a lifecycle that preserves author state or restarts it
  deliberately. Both were excluded from D137 and need their own design
  discussion.

## Checkpoints

### 1. Measure the real set

Repeat the procedure with the extensions Knot then ships. Record per-package
heap, the N to N+1 cost, and the floor.

Review gate: whether the idle cost is worth lowering, and which approach to try
first.

### 2. V8 flags

Measure candidate flags with the procedure; keep only flags that lower the
floor without slowing the measured startup or a long allocating turn.

Review gate: the gain against each flag's process-wide cost.

### 3. Startup snapshot

Create every isolate from one snapshot; the bootstrap is no longer compiled per
package. The full extension host and product suites pass unchanged, with tests
that native callbacks, `knot.platform`, private dispatch handles, and
per-package module resolution behave the same from the snapshot.

Review gate: memory and creation-time gain against build and code complexity,
and whether sharing or unloading needs a design.
