# Extension host decisions

The JavaScript runtime, package admission, authoring, editor handles,
observation, and extension storage. The [decisions index](../decisions.md) lists
every topic, and the [documentation guide](../README.md#decisions) sets the
entry rules.

## Extension runtime

Run JavaScript on direct V8 behind Knot-owned protocols. Persistent isolates
keep author state, and a bounded worker pool fixes thread cost as extensions
grow. Forced interruption is fatal to the lifecycle, giving cleanup a definite
boundary, and teardown releases foreground resources even when JavaScript
cannot run.

Immutable external UTF-16 snapshots serve V8 without changing the UTF-8 core:
10 MiB marshaled in 36 ms copied against 10.6 ms external and 0.3 ms cached,
while a whole-document boundary table exceeded the isolate heap.

Idle isolates rarely collect garbage, so the worker sends a low-memory
notification after startup and long turns, which cut a test package from
14 MiB to 1.6 MiB resident. The fixed idle cost of about 1.2 MiB per package
stays until enough packages justify more (D139).

Reference: [extension host](../architecture/extension-host.md).

## Handler declarations

Annotated async handlers keep argument, result, and error types at their
definition; generated adapters avoid parallel registration tables, and explicit
error codes survive renames. Short foreground requests run in the dispatch turn
to preserve key and text ordering. Values crossing from Rust into JavaScript
keep one hand-written decoder per type, shared with the declaration reader.

References: [extension host](../architecture/extension-host.md) and
[commands](../architecture/commands.md).

## Installed extension directories

Installed packages are inspectable directories of built JavaScript and a JSONC
manifest. Dependencies order startup but grant no module access across
isolates. An explicit API version is required before evaluation, and a rejected
package keeps its identity so it cannot hide duplicates.

Reference: [extension host](../architecture/extension-host.md).

## Bundled extension packages

Bundled packages compile into the binary, so they work identically in
development and release and cannot drift from it. They use the reserved `@knot`
scope, which no installed directory can claim, and the same manifest and
dependency rules; a failing bundled package fails only its dependents.

Reference: [extension host](../architecture/extension-host.md#package-admission-and-startup).

## Extension authoring and tests

Bundled packages are TypeScript compiled per file in `build.rs` without a
bundler, so the runtime loads ordinary modules with readable stack traces.
Node as a build requirement was preferred over committed output or a restricted
syntax. Policy is tested under `node:test` against a typed in-memory `knot`
fake, which is not a second host: product tests remain the authority across the
native boundary. Package behavior cases run in a windowless session with the
same key router, native commands, and ordered key delivery as a window. A
window's drawing had been most of a case's cost: on Apple M1 Pro debug builds,
364 Vim cases took 94 s in a window and 1.7 s headless. A fake host that stubs
native commands was rejected, because the cases exist to exercise them.

References: [extension guide](../extension-api.md#write-and-test-in-typescript)
and [development](../development.md).

## Editor handles

A long-lived editor handle binds to the exact tab presentation, so a mode never
acts on a replacement tab; the host re-resolves it on every call and drops stale
handles on refresh, with no second ownership catalog.

Selection notifications carry no payload and coalesce to one pending turn per
handle, so a slow listener cannot grow a queue and pulling current state is
always correct. A caret change reached a listener in p50 305 µs.

Undo groups are opened and closed rather than scoped to a callback, because a
Vim-style change spans extension commands and native typing. Groups, caret
styles, and key contexts are lifecycle-owned, so a stuck extension cannot leave
them behind. Marks are native zero-width anchored ranges, because remapping
from asynchronous events would lag typing.

The handle's vertical unit is the screen row, with queries from row to buffer
line and start column and from position to row, rather than a native
move-to-screen command, so `H`, `M`, `L`, and the scrolls are extension code
and a Helix mode composes the same queries. The first design returned only the
line; scrolling through one wrapped line needed the column.

Reference: [extension host](../architecture/extension-host.md#editor-handle-notifications).

## Extension restart

Restart Extension is the minimal escape hatch for a stuck turn without timeout
or quota policy (D019). It reuses terminating unload and shared cleanup, then
loads one fresh lifecycle under the same storage. Personal config is excluded;
restarting Knot recovers it.

Reference: [extension host](../architecture/extension-host.md#restarting-a-package).

## Personal configuration

One manifest-free config directory holds optional pre-init and post-init
modules sharing state, preferring an existing XDG location. Config failure is
fatal to startup, because partial setup is unpredictable, and an error window
shows the diagnostics.

References: [extension host](../architecture/extension-host.md) and
[workbench lifecycle](../architecture/workbench-and-lifecycle.md).

## Application observation

Atomic registration, initial snapshot, and watermark prevent gaps. Metadata is
compared at committed transitions rather than kept in a second catalog, and
listeners run on existing FIFO roots without replay or acknowledgement.

Reference: [extension host](../architecture/extension-host.md).

## Extension record storage

One SQLite database per canonical package identity, plus one for config, so
ownership survives reloads and path changes. Synchronous calls on the
background executor with FULL synchronization avoid a storage service; one
JSON record is the atomic unit. Quit drains admitted work before `cx.quit()`,
because gpui's shutdown wait is too short.

Reference: [extension host](../architecture/extension-host.md#extension-record-storage).
