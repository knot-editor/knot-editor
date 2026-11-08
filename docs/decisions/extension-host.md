# Extension host decisions

The JavaScript runtime, package admission, authoring, editor handles,
observation, and extension storage. The [decisions index](../decisions.md) lists
every topic, and the [documentation guide](../README.md#decisions) sets the
entry rules.

## Extension runtime

Use JavaScript on direct V8, with Knot-owned protocols insulating editor models
from runtime machinery. Persistent isolates preserve author state; a bounded
worker pool fixes thread cost as extension count grows. Locked turns permit
worker migration without a Knot-owned unsafe mobility wrapper. Serial callback
roots simplify ownership, while yielding for host work preserves pool capacity.
Forced interruption is fatal to the affected lifecycle, giving cleanup a definite
boundary.

An application-owned pool and awaitable inbox avoid periodic wake loops and
per-extension transport threads. Authoritative teardown releases foreground
resources even when JavaScript cannot run cleanup. Local disposer inactivity
prevents queued callbacks; native acknowledgement orders accepted mutations
without waiting for a self-disposing callback to finish.

Immutable external UTF-16 snapshots serve V8 without changing the UTF-8 core.
A prototype V8 149/deno_core benchmark on Apple M1 Pro/macOS release builds
measured 10 MiB Unicode marshaling at p50 36.0 ms versus 10.6 ms for cold external
UTF-16 and 321 µs cached. A whole-document UTF-16 boundary-table adapter exceeded
its 32 MiB isolate heap cap. This supports revision/range-scoped snapshots and
caching, not a public byte-array API or a current pool-performance claim.

Immutable captured module graphs separate filesystem discovery from execution
and preserve useful source locations without granting cross-package imports.
Private native dispatch handles prevent author globals from fabricating callback
context. Typed field decoding and error variants avoid JSON conversion hooks,
diagnostic-string protocols, and silent numeric precision loss.

Idle isolates rarely collect garbage on their own, so the worker sends a
low-memory notification after a package's evaluation settles and after any turn
of 100 ms or more. On Apple M1 Pro/macOS release builds with V8 152.2, a
package that allocated about 8 MiB at startup kept 14 MiB resident and 9.3 MiB
of dead heap without it, and 1.6 MiB resident with it; the bundled packages'
heap fell from 12.6 to 7.6 MiB. It costs about 0.5 ms per package at startup,
on the worker. Tuning the young generation was rejected: V8 clamps it to a
minimum, so an empty package stayed at 0.75 MiB of heap, and limits below
3 MiB slowed a bursty startup by about 2 ms.

The fixed idle cost stays: an empty package costs about 1.2 MiB resident, of
which a bare isolate and context are about 1.05 MiB. A startup snapshot could
skip only the bootstrap's 0.15 MiB and 0.75 ms, so lowering the floor waits for
enough extensions to justify V8 flags, a snapshot, or sharing (D139).
Developer: Show Extension Memory stays to repeat those measurements.

Reference: [extension host](../architecture/extension-host.md).

## Handler declarations

Annotated async handlers keep argument, result, and error types paired at their
definition. Generated adapters and linked descriptors avoid parallel registration
and transport tables. Deterministic assembly diagnoses duplicates; descriptor
metadata checks the TypeScript error union without another membership list.
Explicit error codes survive Rust renames.

Host operations and native commands share the owned async call contract, while
foreground owners remain independent of response transport. Short foreground
requests run in the dispatch turn to preserve key-command/text-input ordering;
suspended work holds no gpui borrow. Synchronous participant selection preserves
focus precedence and inline extension composition.

V8 values that are not host-operation arguments keep hand-written decoders and
encoders in the engine's marshaling module: provider results, command outcomes
returned by extension handlers, and events and buffer changes delivered to
listeners. They cross in the other direction, where Rust invokes JavaScript, so
no handler descriptor exists to drive them, and each type has exactly one
decoder rather than a parallel registration. The declaration reader delegates
to the same decoders for ranges, edits, contributions, command arguments, and
destinations, so both directions share one conversion per type.

References: [extension host](../architecture/extension-host.md) and
[commands](../architecture/commands.md).

## Installed extension directories

Use inspectable installed directories containing built JavaScript and a JSONC
manifest; package management stays outside the editor. Dependencies establish
startup order and command availability, not module access across isolates.
A global command namespace keeps invocation independent of packaging.

Require an explicit API version before evaluation, separate from package version.
Recoverable manifest identity must survive validation failure so rejected
packages cannot conceal duplicates or appear as missing dependencies. Retained
startup outcomes expose failures while independent packages remain usable.

Reference: [extension host](../architecture/extension-host.md).

## Bundled extension packages

Compile bundled workflow packages into the binary instead of installing them
beside it or copying them into user data. Sources stay in the repository, work
identically in development and release without packaging steps, and cannot
drift from the shipped binary. The build compiles their TypeScript, so emitted
JavaScript is never committed.

Bundled packages use the reserved `@knot` scope and the same manifest, API-version,
dependency, and rollback rules as installed packages. The reserved scope makes
bundled identity structural: an installed directory can never claim it. A
rejected installed directory that names a bundled identity is reported as a
directory conflict without rejecting the identity, so a named diagnostic always
means that identity is unavailable. Installed packages may require bundled ones.
A bundled package failure is reported and fails its dependents without stopping
product startup.

Reference: [extension host](../architecture/extension-host.md#package-admission-and-startup).

## Extension authoring and tests

Write bundled packages in full TypeScript, compiled per file without
a bundler, so the runtime keeps loading ordinary captured modules and readable
stack traces need no source maps. Compiling in `build.rs` makes Node a
build requirement; that was preferred over committing generated `dist/`, a Rust
TypeScript compiler dependency, or restricting authors to type-erasable syntax.
Installed packages still take built JavaScript, so TypeScript stays authoring
tooling rather than a runtime.

Test extension policy under `node:test` against an in-memory `knot` host rather
than only through product tests, which need a shell, V8 pool, and SQLite for
every policy case. Resolving the `knot` specifier to the fake keeps extension
code free of dependency injection. Typing the fake against the declarations
makes API drift a check failure. Implementing behavior instead of mocks keeps
flow tests readable, and `not faked` failures keep its surface demand-driven.
The fake is not a second host: product tests remain the authority for anything
crossing the native boundary. Packages create state in `start()` so each test
starts fresh without module reloading.

Package behavior cases run in a windowless session with the same key router,
native commands, and ordered key delivery as a window. A window's drawing had
been most of a case's cost: on Apple M1 Pro debug builds, 364 Vim cases took
94 s in a window and 1.7 s headless. A fake host that stubs native commands was
rejected, because the cases exist to exercise them.

References: [extension guide](../extension-api.md#write-and-test-in-typescript)
and [development](../development.md).

## Editor handles

A long-lived editor handle is bound to the exact tab presentation rather than
to a command capture or the tab's editor entity, because a mode that follows
editors must never act on a replacement tab, and tab moves keep the entity
alive while changing where it is shown. The host re-resolves the presentation
on every call and drops stale handles after each observation refresh, which
also tells the owner once, so no second ownership catalog or per-tab close hook
exists. Handles travel as the same authority integer as command captures, so
the editor operations stay one set.

Selection notifications carry no payload and coalesce to one pending turn per
handle. A payload would need the old selection or the cause, which no consumer
asked for (D080 may need the cause); a queue would let a slow listener grow
unbounded work, while pulling the current selection is always correct, as with
picker highlight previews. Acknowledging when the listener turn starts, rather
than when it ends, keeps a long listener from losing the change that arrived
during it. Observing the editor entity covers every cause of a caret change
without instrumenting each selection mutation, at the cost of comparing two
offsets per notification of a watched editor.

On Apple M1 Pro/macOS release builds under the gpui test dispatcher, a caret
change reached a listener that read the selection and set a key context in
p50 305 µs, p90 342 µs, p99 415 µs, and max 639 µs over 1,000 changes, the
same order as a key-bound command awaiting `setSelection` (see
[commands and keymaps](commands-and-settings.md#commands-and-keymaps)). Ten
caret changes made in separate foreground updates cost one listener run in each
of 200 bursts. The dispatcher polls rather than running the platform loop, so a
real window adds its wakeup latency.

A mode that changes text and then lets the user type, as Vim's `ciw…<Esc>`
does, must undo as one step across extension commands and native typing, so
undo groups are opened and closed rather than scoped to a callback, which
cannot span key presses. The group lives in the buffer's history, so native
typing and paste join it without a hook, and the editor handle opens it so the
step restores that view's selection. It is lifecycle-owned like key contexts,
so a stuck or unloaded extension cannot leave history unbreakable. One group
per buffer and no nesting: nothing needs a counter, and a second open is a
typed error. Undo ends the group first rather than rejecting, which keeps Undo
usable while a mode is in a bad state.

A mode also changes how the editor looks and what it shows. The caret style is
lifecycle-owned view state like key contexts, with the latest lifecycle's
choice winning and clearing revealing the previous, because a scoped setting
would be a settings write per view on every mode change and would outlive a
stuck package. A mark is a native zero-width anchored range rather than offsets
an extension remaps, because remapping from asynchronous change events would
lag typing, while the store already repairs positions synchronously in each
commit. Its meaning is the store's: it stays with the character after it, and a
deletion of that text removes it for good, since undo does not revive consumed
ranges. Names, persistence, and bookmark lists stay in extensions, which keep
their own records and recreate marks as documents open. The clipboard can be
read like it can be written, with no entitlement until D087 gates both.

The handle's vertical unit is the screen row, with queries from row to buffer
line and start column and from position to row, rather than a native
move-to-screen command, so `H`, `M`, `L`, and the scrolls are extension code
and a Helix mode composes the same queries. The first design returned only the
line; scrolling through one wrapped line needed the column.

Reference: [extension host](../architecture/extension-host.md#editor-handle-notifications).

## Extension restart

A native Restart Extension command is the minimal escape hatch for a stuck turn
without adopting timeout or quota policy (D019). A terminating unload already
ends a running turn and the shared foreground cleanup already removes every
registration, so restart reuses them, rediscovers the one package, and loads a
fresh lifecycle under the same storage and recovery owner. Dependents keep
running because packages share only the global command namespace. Personal
config stays excluded; restarting Knot is its recovery path. A switch hung before
its native commit leaves the source intact, and restarting the package recovers.

Reference: [extension host](../architecture/extension-host.md#restarting-a-package).

## Personal configuration

Use one manifest-free config directory with optional pre-init and post-init
modules sharing JavaScript state. Prefer an existing XDG location so users have
one unambiguous source root across platforms.

Config failure is fatal to startup: partially applied personal setup would leave
unpredictable behavior. Installed-package failures remain independent. Product
windows wait for both phases, and a dedicated error window keeps diagnostics
available when config startup fails.

References: [extension host](../architecture/extension-host.md) and
[workbench lifecycle](../architecture/workbench-and-lifecycle.md).

## Application observation

Atomic registration, initial snapshot, and sequence watermark prevent gaps.
Compare metadata at committed transitions instead of creating a second catalog
or exposing text-edit payloads as application events. Queries derive current
state from its existing owners.

Existing asynchronous FIFO roots let listeners await host work or self-disposal
without blocking commits. A query may see newer state than its event; ordinary
editor observation does not require replay, acknowledgement, or recovery logs.

Reference: [extension host](../architecture/extension-host.md).

## Extension record storage

Separate SQLite databases by canonical package identity, with a distinct config
owner, so durable ownership survives reload and installation-path changes.
Application-selected owners keep extensions out of other databases and separate
extension records from application recovery metadata.

Bundled SQLite, serialized reused connections, and synchronous calls on the
existing background executor avoid a dedicated storage service. Rollback
journaling with FULL synchronization provides committed-write acknowledgement.
One JSON record is the atomic replacement unit; extensions own value schemas.
Incompatible or damaged databases fail without replacement.

Drain admitted work before calling gpui quit because its final shutdown hook's
short foreground wait cannot supply this guarantee. The drain runs independently
of its awaiting caller, preserving accepted native work without a final JS
callback.

Reference: [extension host](../architecture/extension-host.md#extension-record-storage).
