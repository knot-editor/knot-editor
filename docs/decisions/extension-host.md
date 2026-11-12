# Extension host decisions

The JavaScript runtime, package admission, authoring, editor handles,
observation, and extension storage. The [decisions index](../decisions.md) lists
every topic, and the [documentation guide](../README.md#decisions) sets the
entry rules.

## Extension runtime

Run JavaScript on direct V8 behind Knot-owned protocols that keep editor models
free of runtime machinery. Persistent isolates preserve author state, and a
bounded worker pool fixes thread cost as extension count grows. Locked turns let
an isolate move between workers without a Knot-owned unsafe mobility wrapper.
Serial callback roots simplify ownership, while yielding for host work preserves
pool capacity. Forced interruption is fatal to the lifecycle, giving cleanup a
definite boundary, and teardown releases foreground resources even when
JavaScript cannot run cleanup. An awaitable inbox avoids periodic wake loops and
per-extension transport threads.

Immutable external UTF-16 snapshots serve V8 without changing the UTF-8 core. A
prototype V8 149/deno_core benchmark on Apple M1 Pro/macOS release builds
measured 10 MiB Unicode marshaling at p50 36.0 ms versus 10.6 ms for cold
external UTF-16 and 321 µs cached. A whole-document UTF-16 boundary-table
adapter exceeded its 32 MiB isolate heap cap. This supports revision- and
range-scoped snapshots and caching, not a public byte-array API.

Immutable captured module graphs separate filesystem discovery from execution
and keep useful source locations without granting cross-package imports.
Private native dispatch handles stop author globals from fabricating callback
context. Typed field decoding avoids JSON conversion hooks, diagnostic-string
protocols, and silent numeric precision loss.

Idle isolates rarely collect garbage on their own, which is why the worker sends
a low-memory notification after startup and long turns. On Apple M1 Pro/macOS
release builds with V8 152.2, a package that allocated about 8 MiB at startup
kept 14 MiB resident and 9.3 MiB of dead heap without it, and 1.6 MiB resident
with it; the bundled packages' heap fell from 12.6 to 7.6 MiB, at about 0.5 ms
per package on the worker. Tuning the young generation was rejected: V8 clamps
it to a minimum, so an empty package stayed at 0.75 MiB of heap, and limits below
3 MiB slowed a bursty startup by about 2 ms.

The fixed idle cost stays: an empty package costs about 1.2 MiB resident, of
which a bare isolate and context are about 1.05 MiB. A startup snapshot could
skip only the bootstrap's 0.15 MiB and 0.75 ms, so lowering the floor waits for
enough extensions to justify V8 flags, a snapshot, or sharing (D139).

Reference: [extension host](../architecture/extension-host.md#runtime-and-scheduler-flow).

## Extension timers

Node-compatible timers live in a host-owned table per lifecycle, and the host
waits through a `TimerDriver` rather than sleeping itself. The host then owns
the IDs, armed waits, clearing, refreshing, and expiry roots, so teardown
cancels every wait and a cleared timer whose expiry is already queued cannot
run; JavaScript keeps only callbacks. A driver is required in the pool
configuration and only sleeps and calls back, which keeps the host free of
threads and lets every test layer own the clock: the application waits on
gpui's background executor, whose virtual clock `#[gpui::test]` advances, and
host and pool tests advance a manual driver.

Rejected: an optional driver with a thread-sleep default, because a production
misconfiguration would silently keep sleeping threads and tests would have to
wait on real time; and a JavaScript-side scheduler over a host sleep call,
because cancellation and lifecycle cleanup would still depend on JavaScript
running after teardown and would add no semantics the host cannot own.

Reference: [extension host](../architecture/extension-host.md#runtime-and-scheduler-flow).

## Handler declarations

Annotated async handlers keep argument, result, and error types paired at their
definition, and generated adapters avoid parallel registration and transport
tables. Explicit error codes survive Rust renames. Short foreground requests run
in the dispatch turn rather than on the executor, to preserve the order of key
commands and typed text.

Values crossing from Rust into JavaScript, such as provider results, command
outcomes, and listener events, keep one hand-written decoder per type: no
handler descriptor exists in that direction to drive generation, and the
declaration reader delegates to the same decoders, so each type has exactly one
conversion.

References: [extension host](../architecture/extension-host.md#handler-declarations-and-transport)
and [commands](../architecture/commands.md).

## Installed extension directories

Installed packages are inspectable directories of built JavaScript and a JSONC
manifest, and package management stays outside the editor. Dependencies order
startup but grant no module access across isolates, and a global command
namespace keeps invocation independent of packaging. An explicit API version,
separate from the package version, is checked before evaluation. A rejected
package keeps its identity so it cannot conceal duplicates or appear as a
missing dependency.

Reference: [extension host](../architecture/extension-host.md#package-admission-and-startup).

## Bundled extension packages

Bundled packages compile into the binary rather than installing beside it or
copying into user data, so sources stay in the repository, work identically in
development and release without packaging steps, and cannot drift from the
shipped binary. The reserved `@knot` scope makes bundled identity structural
rather than a precedence rule: no installed directory can claim it. Bundled
packages follow the installed packages' rules, so a failing one fails only its
dependents.

Reference: [extension host](../architecture/extension-host.md#package-admission-and-startup).

## Extension authoring and tests

Bundled packages are full TypeScript compiled per file without a bundler, so the
runtime loads ordinary modules and stack traces need no source maps. Compiling
in `build.rs` makes Node a build requirement; that was preferred over committing
generated `dist/`, a Rust TypeScript compiler dependency, or restricting authors
to type-erasable syntax. Installed packages still take built JavaScript, so
TypeScript stays authoring tooling rather than a runtime.

Policy is tested under `node:test` against an in-memory `knot` host rather than
only through product tests, which need a shell, V8 pool, and SQLite for every
policy case. Resolving the `knot` specifier to the fake keeps extension code free
of dependency injection, typing it against the declarations makes API drift a
check failure, and `not faked` failures keep its surface demand-driven. The fake
is not a second host: product tests remain the authority across the native
boundary.

Package behavior cases run in a window without a native window, with the same
key router, native commands, and ordered key delivery as a window. A window's drawing had
been most of a case's cost: on Apple M1 Pro debug builds, 364 Vim cases took
94 s in a window and 1.7 s headless. A fake host that stubs native commands was
rejected, because the cases exist to exercise them.

References: [extension guide](../extension-api.md#write-and-test-in-typescript)
and [development](../development.md).

## Editor handles

A long-lived editor handle binds to the exact tab presentation rather than to a
command capture or the editor entity, because a mode must never act on a
replacement tab, and tab moves keep the entity alive while changing where it is
shown. Dropping stale handles on each observation refresh avoids a second
ownership catalog or per-tab close hook. Handles travel as the same authority
integer as command captures, so the editor operations stay one set.

Selection notifications carry no payload and coalesce to one pending turn per
handle. A payload would need the old selection or the cause, which no consumer
asked for (D080 may need the cause); a queue would let a slow listener grow
unbounded work, while pulling the current selection is always correct.
Acknowledging when the listener turn starts keeps a long listener from losing a
change that arrives during it. On Apple M1 Pro/macOS release builds under the
gpui test dispatcher, a caret change reached a listener that read the selection
and set a key context in p50 305 µs, p99 415 µs over 1,000 changes, the same
order as a key-bound command awaiting `setSelection`; ten changes in separate
foreground updates cost one listener run in each of 200 bursts.

Undo groups are opened and closed rather than scoped to a callback, because a
Vim-style `ciw…<Esc>` spans extension commands and native typing, which no
callback can span. The group lives in buffer history, so native typing joins it
without a hook. Groups, caret styles, and key contexts are lifecycle-owned, so a
stuck or unloaded extension cannot leave history unbreakable or a mode's look
behind; a scoped setting for the caret was rejected because it would be a
settings write per view on every mode change. One group per buffer with no
nesting, because nothing needs a counter. Undo ends a group rather than
rejecting, so Undo stays usable while a mode is in a bad state.

Marks are native zero-width anchored ranges rather than offsets an extension
remaps, because remapping from asynchronous change events would lag typing,
while the store already repairs positions synchronously in each commit. Names,
persistence, and bookmark lists stay in extensions.

The handle's vertical unit is the screen row, with queries between rows and
buffer positions, rather than a native move-to-screen command, so `H`, `M`, `L`,
and the scrolls are extension code and a Helix mode composes the same queries.
The first design returned only the line; scrolling through one wrapped line
needed the column.

References: [commands](../architecture/commands.md#extension-execution) and
[extension host](../architecture/extension-host.md#editor-handle-notifications).

## Extension restart

Restart Extension is the minimal escape hatch for a stuck turn without adopting
timeout or quota policy (D019). It reuses terminating unload and shared cleanup
rather than a restart-specific path. Dependents keep running because packages
share only the global command namespace. Personal config is excluded;
restarting Knot is its recovery.

Reference: [extension host](../architecture/extension-host.md#restarting-a-package).

## Personal configuration

One manifest-free config directory holds optional pre-init and post-init modules
sharing state, preferring an existing XDG location so users have one unambiguous
source root across platforms. Config failure is fatal to startup, because a
partially applied personal setup would behave unpredictably, while
installed-package failures stay independent.

References: [extension host](../architecture/extension-host.md#package-admission-and-startup)
and [product launch](../architecture/workbench-and-lifecycle.md#product-application-launch).

## Application observation

Atomic registration, initial snapshot, and watermark prevent gaps. Metadata is
compared at committed transitions rather than kept in a second catalog or
exposed as text-edit payloads. Listeners run on the existing FIFO roots, so
commits never await them; ordinary editor observation needs no replay,
acknowledgement, or recovery log, accepting that a query may see newer state
than its event.

Reference: [extension host](../architecture/extension-host.md#application-observation).

## Extension record storage

One SQLite database per canonical package identity, plus one for config, so
ownership survives reloads and installation-path changes and an extension can
never reach another's records. Synchronous calls on the existing background
executor avoid a dedicated storage service, and FULL synchronization makes an
acknowledged write committed. One JSON record is the atomic unit, and extensions
own their value schemas.

Quit drains admitted work before calling gpui's quit, because its shutdown
hook's short foreground wait cannot guarantee accepted writes land.

Reference: [extension host](../architecture/extension-host.md#extension-record-storage).
