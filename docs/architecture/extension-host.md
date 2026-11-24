# Extension host

Part of the [architecture](../architecture.md). [Decisions](../decisions.md)
records rationale; public API contracts live in [knot.d.ts](../../crates/knot-host/src/js/knot.d.ts).

## Boundaries

| Owner | Responsibility |
| --- | --- |
| `knot-protocol` | Knot-owned identities, semantic payloads, and failures; no gpui or concrete core models |
| `knot-protocol::declarations` | Typed async handler contracts, linked discovery, and response transport |
| `knot-host::scheduler` | V8-independent lifecycle state and bounded worker scheduling |
| `knot-host::engine` | V8 initialization, persistent isolates, contexts, bindings, and exact-once runtime teardown |
| `knot-host::timers` | V8-free timer IDs, armed waits, and the embedder's timer driver |
| `knot-host::lifecycle` | Extension identity, scheduler-visible state, and failure data |
| `knot-host::pool` | V8-free application control, awaitable event inbox, diagnostics, and shutdown |
| `knot-host::module_graph` | Validation of immutable captured sources without V8 or filesystem access |
| `knot-model::extensions::{package, load}`, `app::personal_config` | Background source discovery, admission, startup ordering, and outcomes |
| `app::extensions::host` | Foreground routing, resource ownership, observation, and lifecycle cleanup |

```text
extension JavaScript
        |
host engine + bounded scheduler
        |
Knot-owned typed protocol
        |
application-owned gpui foreground bridge
        |
application registries and models
```

## Package admission and startup

Installed packages provide a manifest and immutable package-local source graph.
The application validates the supported API version before capturing/evaluating
an entry, retaining recoverable package identity even on validation failure so
duplicates and rejected dependencies remain diagnosable. Discovery runs on gpui's
background executor; the foreground admits valid packages in dependency order
and awaits entry completion before loading dependents.

Bundled packages are written in TypeScript, compiled by the `knot-ext-*` group crates (through the `knot-bundled` build helper) into the
binary, and merged into the same discovery result, without reading sources from
disk. They use canonical `@knot/...`
identities, the same manifest, API-version, dependency, and rollback rules, and a
synthetic module root instead of a captured directory. The reserved scope cannot
be claimed by an installed package, making bundled identity structural rather
than a precedence rule; a rejected installed directory that shares a bundled
identity is diagnosed without blocking the bundled package. Installed packages
may require bundled ones. A bundled package failure is reported and fails its
dependents like any other package, without stopping product startup.

The application retains an ordered startup report, publishing outcomes as they
become known so one pending entry does not hide earlier failures. The
Developer: Show Extension Startup Report command presents it, and launch opens
it when a package failed. Failed or
cancelled entry rolls back its runtime and foreground registrations before the
next package starts; independent packages remain eligible.

Static imports resolve only the public `knot` module or relative captured sources
within the package root. Dynamic imports reject. Generated source URLs preserve
failure locations; V8 compilation and values stay inside the isolate. Private
bootstrap and dispatch handles cannot be invoked or replaced through author
globals to fabricate callback context or buffer proxies.

Personal config uses one manifest-free lifecycle with optional pre-init and
post-init entries sharing globals, compiled modules, and registrations. The
application selects one existing XDG config directory in preference to the
platform directory; exact discovery rules live in
[personal_config.rs](../../src/app/personal_config.rs). Capture resolves a
symlinked root and validates source containment. Pre-init cannot import post-init,
even indirectly. Post-init capture/validation errors are retained until that
phase rather than preventing valid pre-init work.

Startup awaits pre-init, installed discovery, entitlement consent, and installed
loading, then post-init. A config failure
unloads every startup lifecycle; installed-package failures do not stop post-init.
Window admission and fatal diagnostics belong to
[product launch](workbench-and-lifecycle.md#product-application-launch).

### Extension entitlements

A manifest lists the capabilities a package needs beyond the editor API in
`entitlements`, starting with `process`. Bundled packages declare them the
same way and ship granted; personal config has no manifest and holds every
entitlement. Grants live per canonical package identity and entitlement as
granted or denied in one versioned record of the application state database,
independent of version, so an update declaring a new entitlement asks only
about it.

When a discovered installed package has an undecided entitlement, Knot opens
a dedicated consent window before any workbench window, modeled on the
fatal-config diagnostic window: each package with its display name, identity,
version, and undecided entitlements, with Allow and Deny per package, and
Continue. Startup waits for it; with nothing undecided no window opens.
Allow stores a grant and Deny a denial for that package's undecided
entitlements; closing the window, or continuing without deciding a package,
stores nothing, so the package does not load and Knot asks again at the next
launch. A package with any denied or undecided entitlement does not load, and
its startup report entry says why; denied packages do not open the report,
and dependents fail as with any dependency failure.

Extensions: Manage Entitlements… lists installed packages that declare
entitlements with their state, and choosing one toggles between allowed and
denied. Granting a not-loaded package activates it through the restart
machinery; revoking performs a terminating unload, which kills its processes,
and records the denial in the report. Developer: Restart Extension…
re-reads the manifest and asks about a newly undecided entitlement with the
native confirmation before activating; a denial is never overridden there.

### Extension processes

`processes.run` resolves once the program exits, with its exit code, signal,
and output; `processes.spawn` returns a handle that streams output and
controls the program. Output is UTF-8 text, decoded with replacement and
carrying split code points across chunks. Streams are pull-based: a
listener's acknowledgement releases the next chunk, so a slow listener
stalls the child on its pipe instead of growing memory, and output that
arrives before a listener attaches waits for one. A finished handle leaves
the registry once its exit is in and both streams are drained; anything else
stays until unload, and `disposeProcess` releases a handle early, killing its
group when it still runs. `command` resolves through `PATH` in the inherited
environment; `cwd` is a file URI defaulting to the home directory; `env` is
merged over inheritance with `null` unsetting; a missing program or
directory rejects with a typed code.

Each child starts in its own process group. Lifecycle teardown (unload,
restart, revoke, startup rollback, quit) kills the group, so helpers a
program started die with it, and `kill` signals the group. Cancelling a
command does not kill its processes. On macOS and Linux, startup resolves
the inherited environment once by running the user's shell as a login
interactive shell that prints its environment, with a timeout, falling back
to Knot's own environment; Windows inherits Knot's environment. The resolved
environment is the base for spawned processes only.

### Restarting a package

The native Developer: Restart Extension… command lists every bundled and installed package in
the startup report, including failed ones, in the window's transient picker. A
restart performs a terminating unload of the package's current lifecycle, so a
stuck turn stops and its queued commands cancel, and runs the shared foreground
cleanup. It then rediscovers the package (bundled sources, or its installed
directory re-read from disk), revalidates its manifest, API version, and that its
dependencies are loaded, and activates a fresh lifecycle under the same storage
and recovery owner. The outcome replaces that package's report entry and is shown
in a message. Dependents keep running: packages share only the global command
namespace, so a dependent's own registrations never depended on the restarted
lifecycle. Personal config is not restartable; restarting Knot is its recovery
path. Per-turn timeouts remain [D019](../roadmap.md).

### Memory diagnostics

Developer: Show Extension Memory opens the process's resident size and each
live lifecycle's V8 heap (used, total, external) as a new untitled document.
The report is its clean state, so closing it unedited never prompts and hot
exit does not keep it. Heap statistics are read between turns: a turn holds its
capsule's isolate data for its whole duration, so a package inside a turn is
listed as running rather than waited for, and a stuck package never blocks the
report.

## Runtime and scheduler flow

One application-owned `ProductExtensionHost` owns the pool, foreground bridges,
and awaitable event-inbox task. Each lifecycle has a persistent runtime capsule
and one FIFO task queue, scheduled the way Node does: a task runs and drains
its microtasks, and the next starts whenever the current one is parked at an
`await`. The pool has fixed worker capacity; defaults and state transitions
live in [scheduler.rs](../../crates/knot-host/src/scheduler.rs).

An invocation is a command, provider call, tree request, surface callback,
evaluation, listener dispatch, or timer callback, and every task continues
one or none. It completes when its promise settles, and several of one
package are live at once. Package and personal-config evaluation are invocations
too and not exclusive: like Node's top-level `await`, other tasks run while
it is parked, and a registration is live as soon as it resolves.

V8 initialization must precede worker creation: every thread that may lock a
shared isolate must be created after initialization. A worker locks a capsule
for one turn, enters its persistent context, runs JavaScript, drains explicit
microtasks, records failures, and leaves V8 scopes before releasing the locker.
Only thread-safe embedder state travels with the capsule. Ordinary callback
exceptions fail that request/turn; forced interruption and heap failure are fatal
to the lifecycle.

Extensions are mostly idle, so V8 rarely collects on its own garbage left by a
burst of work. The worker that finishes a turn therefore sends the isolate a
low-memory notification, a full collection that returns free pages, when the
turn settles the package or config entry's evaluation, and after any turn that
ran 100 ms or longer. The notification runs off the foreground, after the turn
and before the root's completion is delivered.

Attribution follows the continuation, as Node's `AsyncLocalStorage` does, on
V8's continuation-preserved embedder data. Entering an invocation stores its
frame there and `await` carries it; entry restores the previous frame when the
synchronous part returns, so an inline child hands its parent's frame back. The
frame names the invocation and, for a command, carries its authority (capture,
target, signal, buffer). Requests, contextual helpers, and cancellation read the
frame, so a listener or timer never borrows a command's authority. A call that
reaches a registration or subscription before its reply has installed the
handler waits in the bootstrap and resumes in a later task.

Host requests retain persistent promise resolvers and yield the worker while
awaiting native work. A validated response queues a task under the requesting
invocation, settles its resolver in a later locked turn, then drains
microtasks. Multiple requests can settle out of order, and other tasks run
meanwhile. Responses racing with a running turn are retained for the next
turn; request and turn identities reject stale or duplicate delivery. Forced
termination wakes every outstanding invocation, so it does not depend on a
later host response. A fulfilled invocation still waits for host requests it
left in flight; once it settles, its contextual requests fail authorization
and context-free ones succeed.

Buffer listeners, application listeners, semantic providers, and timer
callbacks enter as tasks of invocations of their own. Their failures are
reported and isolated, so later tasks still run. The notifications of one
subscription (a buffer's changes, an editor's selection or bar edits) take
turns in the bootstrap, so a slow listener sees them in order and the
application's acknowledgement still coalesces changes made meanwhile; different
subscriptions, listeners, and commands interleave. Dropping a native provider
receiver cancels queued work or wakes active work to release retained promises.
Diagnostics measure queue depth, lag, and worker movement; quotas, dropping,
coalescing, deadlines, and backpressure remain [D019](../roadmap.md).

Native surfaces call back into the package the same way. A picker's
`onHighlight` and `onQuery`, an input's `onChange`, and a tree edit's `validate`
run as surface-callback tasks while the command awaits the surface's result,
keyed by a session the bootstrap numbers and drops when the surface settles;
each callback restores the command's frame, so it keeps the command's
authority until then. Only `onQuery` calls overlap, and only the answer to the
latest applies; the others stay serial with coalescing. A failing callback
stops further calls and rethrows once the surface settles. Behavior of the
surfaces is in [semantic UI](semantic-ui.md#extension-transient-interactions).

Each lifecycle's timer table in the host owns timer IDs and their armed waits;
JavaScript keeps only the callbacks. A `TimerDriver`, required in the pool
configuration, does the waiting off any thread: the application's waits on
gpui's background executor, whose test clock `advance_clock` drives, and
`knot-host` tests advance a manual one. An expiry queues a task that runs only
if the arming it belongs to is still current, so clearing or refreshing a
timer whose root is already queued suppresses it. Teardown drops the table,
cancelling every wait.

The foreground command bridge owns admitted invocation trees and completion
channels; the host schedules their JavaScript. Routing, inherited presentation
authority, inline children, ancestry limits, and contextual access belong to
[commands](commands.md#extension-execution).

## Handler declarations and transport

JS-to-native capabilities are annotated async application handlers. Linked
descriptors pair typed arguments, results, and errors; assembly rejects duplicate
operations and invalid context types before startup. Foreground resource owners
expose semantic methods independently of request envelopes. Generated adapters
own decoding and response transport rather than per-operation routing tables.
Native commands share this async contract with command-specific adapters.
Implementation contracts live in
[declarations](../../crates/knot-protocol/src/declarations.rs) and
[application handlers](../../src/app/extensions/host/handlers.rs).

Owned contexts retain weak host/application access and caller authority. Short
foreground closures revalidate lifecycle, cancellation, and resource identity,
then return owned results; no foreground borrow crosses an await. Mutations
revalidate immediately before commit. Shared product operations return committed
semantic data, which handlers adapt without invoking commands or querying focus
afterward. Open/save/root behavior belongs to [documents](documents-and-persistence.md);
closure belongs to [workbench lifecycle](workbench-and-lifecycle.md).

The V8 boundary decodes recognized typed fields without conversion hooks and
rejects numeric precision loss. Typed native failures supply stable public codes;
argument and numeric errors stay author-side categories. Descriptor metadata
checks the TypeScript error union. Immutable external UTF-16 snapshot storage
retains a shared reference until V8 garbage collection or isolate disposal,
without changing the UTF-8 core.

Synchronous JavaScript keybinding calls collect an ordered batch per V8 turn.
The engine holds earlier host responses, isolate-wide, until atomic foreground
batch application settles, so an await continuation cannot overtake preceding keymap mutations.
A batch failure fails the current turn. Binding ownership and precedence belong
to [commands](commands.md#keymaps).

## Disposal, cancellation, and teardown

Disposal immediately prevents new local callback admission and retains one
native-cleanup promise, including failure. Earlier accepted mutations and
invalidations settle before acknowledgement; executing callbacks are outside that
barrier so self-disposal cannot deadlock. Queued callbacks cannot enter removed
author registrations. Native provider removal cancels pending receivers; its
surface effects belong to [semantic UI](semantic-ui.md).

Command cancellation wakes suspended execution, aborts its signal, and rejects
pending JS requests. Admission/publication races retain cancellation until the
execution is recorded. An already queued native response keeps its pending entry
until its admitted turn consumes it without settling the rejected resolver again.
This preserves invocation ownership while cancellation callbacks run. Accepted commits
are not rolled back.

Unload, startup rollback, fatal termination, heap failure, caught worker panic,
and pool shutdown converge on one runtime finalizer. Stop prevents queued work
and terminates running JavaScript. Once its locker leaves, the finalizer releases
persistent handles while locked, settles accepted native work exactly once, and
drops the isolate. A terminal lifecycle event triggers foreground cleanup.

Foreground cleanup is shared by lifecycle events, startup rollback, explicit
unload, and host release. It removes command/keymap registrations, subscriptions,
contributions, and semantic providers even when buffer models or views are still
retained. No final author callback or disposer retry is required. Cancellation
listener failure cannot prevent other listeners from receiving cancellation.

## Providers

A package shares a feature with others through a contract: a name and
methods, each with a schema, built by `defineContract` in the package's
contract module (the manifest's `contract`). The module may import only
`"knot/contract"` and its own modules, so a consumer evaluates it without
running the provider's code. The host qualifies the name with the package the
module resolved from, so contracts cannot collide across packages. A package
serves a contract with `serve`; others import the provider by package name
(listed in `requires`; personal config needs no entry and may import from
post-init) and call `client()`.

The application keeps registrations in registration order, each owned by its
lifecycle (`knot_model::providers`); lifecycle teardown drops them, and the
clients bound to them, with the other registrations. A client binds the latest
registration of a contract, so one package overrides another by serving later,
and disposing it uncovers the previous one. `serve` registers the provider's
schema and `client()` the consumer's; a call validates the argument and then
the result against both, once when the two are equal. A violation names the
package, contract, method, and path, and a consumer built against another
revision of the contract fails per call, not at load.

A call is a host request of the caller that enqueues a task in the provider's
package, like a semantic provider callback, with the method, a JSON argument,
and the caller's command window, which the provider sees as its own target. The
provider's result or thrown message settles the caller's request. A client
names one registration: once it is disposed, a call rejects `ContractNotFound`.
A call to a contract the caller serves is rejected, and a package uses its own
code directly. Other calls interleave: a provider's package may be running a
command that waits on the caller.

The application interprets no package contract, but two Knot-defined ones,
owned by `knot` and served from `completionContract` and
`pasteContract(languageId)`: `completion`, which native
[completion](semantic-ui.md#completion-ownership-and-aggregation) asks of every
registration, and `paste.<languageId>`, which native
[paste](editor-and-languages.md#editor-views) asks of the latest. Knot is their
consumer, so it validates only each provider's result, against the descriptor
compiled when it was served, and never goes through a contract client.

## Application metadata queries

Queries derive documents from the application collection and windows, panes,
tabs, roots, and focus from live windows. Diagnostic windows are excluded;
terminals are non-document surfaces. No synchronized window catalog is maintained.

Discoverable targets are distinct from command captures. They qualify identities
with the workbench and exact presentation so replacement or transfer cannot
retarget them. Queries require a live lifecycle, honor command cancellation, and
work without command context; returned targets reserve no future validity.

Each tab's `PresentationInfo.root` is its [view tree](workbench-and-lifecycle.md#view-tree)
as `ViewNodeInfo`: kind, children, and for an editor its document (null for a
snapshot), buffer, language, and read-only state. A window's `focusedView` is
the focused leaf of the focused tab. Its `statusView` is the editor view a
status display describes, at any depth, computed by the window, which already
tracks focus: the focused editor; else the last focused editor while its tab is
the selected tab of its pane; none while a terminal has focus. Tools and focus
outside tabs keep the previous editor, as in VS Code, so a consumer follows one
value and never reimplements the rule; `statusTarget` is the tab of that
editor. A change publishes `presentationsChanged` like `currentTarget`.

`windows.findView(window, target)` is the one lookup of a view in a
`WindowInfo`, for a `ViewNodeTarget` or a tab's `ViewTarget`. It reads the
information it is given, with no host call, and returns the view, its parents
nearest first, and its tab and pane, so a package never searches `pane.tabs`.

## Application observation

The foreground host owns a comparison baseline, window sequence, and
lifecycle-scoped subscriptions. Document commits, window transitions, tab focus,
native activation, and window closure signal comparison; repaint-only changes do
not publish metadata events. All documents are tracked independently of author
buffer lookup. One buffer-commit drain routes text edits to buffer subscribers;
application observation compares metadata without consuming those edits again.

The snapshot also carries the notice history, and a change to it publishes
`notificationsChanged` with the whole history.

Two events are not derived from metadata alone. `viewStateChanged` follows a
window's settled [layout change](workbench-and-lifecycle.md#window-layouts); the
host commits pending metadata first, so it never precedes the window's own
structural events. `launched` is sent once when startup has committed its
restored and requested windows; the snapshot's `launched` field reports it to
packages that start later, such as a restarted one.

Observation refreshes the committed baseline and captures registration, snapshot,
and watermark in one foreground request. The engine installs the author listener
before that root finishes; subsequent notifications run through the ordinary FIFO
callback path. Commits never await listeners, and a listener may await host work
or its own disposal. Queries can see newer state than an event. No separate replay,
recovery, or backpressure mechanism is introduced.

`documents.track` is built in the host's JavaScript on a private workbench
observation, so native code is unchanged. It buffers events until the snapshot's
documents are delivered, drops those at or before the watermark, and delivers
the rest through one queue per tracker. It is a separate subscription, so its
callbacks are not ordered against another observation's for the same event.

### Editor handle notifications

A long-lived [editor handle](commands.md#extension-execution) is notified
as a root like a setting change: `closed` when the host finds
its tab presentation gone after an observation refresh, and `selection` when
its selection changes. A handle's first `onDidChangeSelection` makes the host
observe the editor entity, so any caret change, motion or edit, reaches it, and
compare the selection with the last one it saw. Each handle has at most one
pending notification: the flag stays set until the listener turn starts and
acknowledges it, so changes in between join that turn, and changes while
listeners run queue exactly one more. The acknowledgement returns the selection
at that moment, which listeners receive, with the lines around the caret when a
listener asked for them: the runtime passes the widest window any of the
handle's listeners asked for. The foreground only compares two offsets and
enqueues a root, so painting and input never wait on JavaScript. The observation ends with the
handle.

A Vim test pins how many foreground host requests the keys `j`, `w`, and `x`
cost with Vim, the status bar, and word highlight running, so a package change
that adds round trips fails it.

## Settings

`app::settings` owns one `ApplicationSettings` global: definitions and value
entries shared by personal config, every package, and native code. Writes and
definitions commit on the foreground inside the host operation, so they resolve
after the store changed. Values have no owner and outlive lifecycles; a
definition belongs to the lifecycle that made it, and teardown removes it
together with that lifecycle's observers, leaving the values for a restarted
package to define again. Native keys are defined when the store is created.

A scope's `document` fact holds a document's UUID. Entries carrying it belong
to that document: `ApplicationDocuments::remove` drops them when the document
is released. The buffer is stamped with its document's identity when the
document is created, and a view reads that stamp into its read context; a
buffer no document owns, such as a contributed text frame's, has none and so
matches only scopes without the fact. `DocumentInfo.indentation` resolves
`editor.insertSpaces` and `editor.tabSize` for the document, and every write
or reset recompares the snapshot, so a change publishes `documentChanged`.

A definition validates the values already written for its key and drops those
of the wrong type; each drop is a warning in the startup report, which
outlives package restarts. After a write or reset, the host queues every
observer of that key through the ordinary FIFO listener path, the same as
application observation: the commit never awaits listeners, and a listener
reads the values it needs with `get`. Native consumers do not observe through
the host; editor views observe the global itself (see
[editor views](editor-and-languages.md#editor-views)).

## Extension record storage

`app::extensions::storage` owns separate SQLite databases selected by canonical
package identity or personal config, independently of lifecycle IDs and installation
paths. Built-in `@knot/...` identities are accepted only for bundled packages;
installed packages cannot use the reserved scope. Admission binds the owner
before startup requests can run; source-only diagnostic lifecycles have no
implicit durable owner. Teardown removes lifecycle bindings while retaining
application-owned connections and records.

The same admission binding stamps the owner of retained recovery captures,
which live in the application recovery store rather than an extension database
(see [retained captures](documents-and-persistence.md#retained-captures)).

Binding performs no I/O. Async handlers await background operations serialized
through a reused connection per database. Native transactions never span a JS
await. Caller lifetime is checked before database work starts; once started,
work finishes normally even if its caller disappears. Failures preserve damaged
or incompatible databases. Public value, ordering, and error contracts live in
[knot.d.ts](../../crates/knot-host/src/js/knot.d.ts); implementation and physical schema live in
[extensions/storage.rs](../../src/app/extensions/storage.rs).

Product and config-diagnostic quit stop storage admission synchronously, then
await admitted work and connection release before gpui quit. Host release also
detaches a drain. Task completion guards keep connections alive through admitted
work; the drain runs independently of the caller awaiting shutdown. No foreground
disk wait or final JavaScript callback is needed.
