# D182 — Interleave extension roots at awaits

Source: [D182](../roadmap.md#architecture-and-technical-debt). Background:
[scheduling report](../reports/extension-task-scheduling.md) (rechecked
against the code in November 2026; its findings hold, with the additions noted
under [Current state](#current-state)). Current behavior:
[extension host](../architecture/extension-host.md#runtime-and-scheduler-flow),
[commands](../architecture/commands.md#extension-execution),
[runtime decision](../decisions/extension-host.md#extension-runtime).

## Outcome

An extension isolate schedules work the way Node does. Every entry into a
package (command, listener, provider or tree call, timer expiry, host
response) is a task on one FIFO per isolate; each task runs and drains its
microtasks, and the next task starts whenever the current one is parked at an
`await`. A command awaiting a picker no longer freezes its package's
listeners and providers, a command that awaits its own timer or event no
longer deadlocks the package, and one package's slow command no longer delays
another's beyond the foreground key hold that keeps typing ordered.
Everything the bundled packages do today keeps working.

## Current state

Confirmed in the code:

- `scheduler.rs`: one `active: Option<ActiveRoot>` per extension; pending
  roots start only when it is empty; `enqueue_continuation` rejects other
  roots (`WrongRoot`).
- `engine/capsule.rs` `inspect_active`: a pending promise keeps the root
  active; a fulfilled one still waits while a host request it issued is
  pending.
- `engine/pool.rs` `timer_table`: an expiry is `enqueue_root`, so awaiting a
  timer inside a command deadlocks the package. No test covers it.
- `js/bootstrap.js`: `request()` and the contextual helpers read
  `activeCommandFrames.at(-1)`; `engine/marshal.rs` reads the turn-wide
  `current_root` for every host request.
- `src/app/commands/extension.rs`: `ExtensionCommandBridge` holds one
  application-wide `roots` queue and `active_root`.
- `v8` 152.2.0 exposes `set_continuation_preserved_embedder_data` /
  `get_continuation_preserved_embedder_data` on scopes.

Not in the report, also consequences of one root at a time:

- `ProviderBusy` (`handlers.rs` `call_provider`, `runs_handler_of`): a
  provider call rejects when the provider's package has a command waiting on
  the caller.
- `enqueue_child_with_target` refuses an A → B → A command chain as
  `Unavailable` even for different registrations of A.
- Package and personal-config evaluation are roots; nothing else in the
  package runs until they settle.

## Design

**Task queue.** The scheduler keeps, per isolate, one FIFO of tasks, each
tagged with the invocation (if any) it continues, plus lifecycle state
(loading, idle/queued/running, stopping, failed) and worker dispatch. It no
longer knows about active roots: a new invocation and a host response are
both just tasks. Turns stay serial per isolate. Timers keep their Node-shaped
globals (`setTimeout`, `setInterval`, `setImmediate`); there are no event-loop
phases and no `process.nextTick`.

**Attribution follows the continuation**, as Node's `AsyncLocalStorage` does,
on the same V8 primitive. Entering any invocation (command, provider call,
tree request, evaluation) stores its frame in continuation-preserved embedder
data; `await` and promise reactions carry it. Every frame carries the
invocation identity; only a command frame also carries authority (capture,
target, signal, buffer). Entry saves the current data and restores it when
the synchronous part returns, so an inline child command running inside its
parent's microtask drain hands the parent its frame back. `request()`, the
contextual helpers, command cancellation, and the native request binding
read the frame instead of `activeCommandFrames` and `current_root`.
Listeners and timers run with no frame, so they never borrow a command's
authority and their requests belong to no invocation, and a continuation that
runs inside another task's microtask drain keeps its own frame.

**Completion.** An invocation (command, provider call, tree request,
evaluation) completes when its promise settles; the bootstrap reports it
through a reaction attached to the promise, rather than the engine
re-inspecting a single active root after each turn. Host requests issued by
a finished invocation no longer keep it alive: contextual calls already
reject once the handler has an outcome (`authorize_context`), and
context-free calls succeed as in Node. Listener dispatches and timer
callbacks are tasks but not invocations: their rejections are still
reported as failures, and the buffer-change and selection acknowledgements
keep coalescing changes when dispatches interleave.

**Loading is not exclusive.** Like Node with top-level `await`, other tasks
run while a package's module evaluation is parked; a registration is live as
soon as it resolves.

**No command serialization.** The bridge drops its application-wide queue;
commands of any packages, including two of the same package, may overlap.
Typing order stays with the foreground key hold, which waits for a command's
completion (bounded by its one-second timeout). Same-lifecycle children keep
running inline; cross-lifecycle children keep awaiting foreground completion.
A command may run several children at once (`Promise.all` over
`executeCommand`); each carries its own frame, and cancellation reaches all
of them.

**What falls away.** `ProviderBusy` and `runs_handler_of`; the same-owner
refusal for A → B → A chains (a registration calling itself recursively stays
`Unavailable`); the one-child-at-a-time rule in `admit_child`; the
`WrongRoot`/`NoActiveRoot` errors.

**Kept as is.** The keybinding-batch hold stays isolate-wide: responses wait
while a batch settles; the batch request belongs to no invocation.
Cancellation, provider-receiver drops, and termination
wakeups address an invocation explicitly instead of "the active root";
stopping and failure settle every outstanding invocation.

**Packages keep today's behavior.** Where a bundled package relies on its
commands not overlapping (Vim's `vim.key` pending-key state is the known
case), it serializes that path itself, for example by chaining invocations on
a promise, and an end-to-end test pins the behavior. Module state read
before an `await` and used after it is revalidated or guarded.

**Pull loops become pushed callbacks.** The picker highlight and query,
input change, and tree-edit validation loops exist only because a separate
notification could not run while the caller awaited
([semantic UI](../decisions/semantic-ui.md)). Once tasks interleave, the
native surface invokes the package's callback as its own task while the
command awaits the surface's result, and the callback's answer (query rows,
input message and completions, validation message) travels back as that
task's result. The bootstrap captures the command's frame when the surface
opens and restores it around each callback, so callbacks keep the command's
authority until the surface settles, as they do today. The public callback
API in `knot.d.ts` stays as it is.

## Exclusions

- New behavior the change makes possible, such as Vim macros that follow
  focus ([D223](../roadmap.md#vim-mode)).
- Quotas, deadlines, and backpressure (D019).

## Deferred decisions

None blocking. Whether to expose an `AsyncLocalStorage`-like API to authors
is left until a package asks for one.

## Checkpoints

### 1. Continuation-carried attribution

Deliverables: every invocation's frame lives in continuation-preserved
embedder data, saved and restored around entry; `request()`, contextual
helpers, `__knotCancelCommand`, and the native request binding read it;
`activeCommandFrames` and `current_root` are gone. Abort on `Cancelled`
targets the issuing invocation's controller rather than the innermost frame
at rejection time (the one behavior fix). Scheduler otherwise unchanged.

Validation: existing host, command, and extension suites pass unchanged;
engine tests that a frame survives `await`, host-response continuations, and
nested inline commands (the parent's frame returns after the child's
synchronous part), that a provider continuation keeps its invocation, and
that a timer or listener callback sees no frame.

Review gate: attribution design in code; confirm no behavior change before
the scheduler moves.

**Implementation context.** `crates/knot-host/src/js/bootstrap.js`
(`requestForInvocation`, `request`, `activeCommandFrames` users around the
`invokeRegisteredCommand` frame push/pop, contextual buffer/capture/window
reads, `__knotCancelCommand`). Native: expose get/set to the bootstrap
through the existing native bindings in `crates/knot-host/src/engine/`
(`marshal.rs` request binding, around `current_root`), using the scope's
`set_continuation_preserved_embedder_data`. Clear the data at the start of
every task in `capsule.rs` turn entry points (`start_*_turn`,
`resume_response_turn`, `run_root_turn`) so a task never inherits the
previous one's. `PendingRequest.root` comes from the frame; a request with
no frame uses the running turn's root until checkpoint 2, after which it
belongs to no invocation. Tests in `crates/knot-host/src/engine/tests.rs`.

### 2. Event-loop scheduler and invocation completion

Deliverables: per-isolate task FIFO replacing `active`/`roots`; invocation
completion on promise settlement, reported by a bootstrap reaction;
listener and timer failures still reported; cancellation, termination,
provider drops, stop, and failure addressed per invocation. Package and
personal-config evaluation stay exclusive, and `ProviderBusy` and the
A → B → A refusal stay, until checkpoint 3; the bridge still serializes
commands until checkpoint 4.

Validation: scheduler state-machine tests rewritten for the task queue;
engine and product tests for awaiting a timer and an event inside a command,
a listener and a provider running while a command awaits a picker,
buffer-change and selection listeners coalescing while dispatches
interleave, a timer or listener rejection reported, and cancellation and
Restart Extension with several outstanding invocations.

Review gate: scheduling semantics and completion.

**Implementation context.** `crates/knot-host/src/scheduler.rs`
(`Extension`, `ActiveRoot`, `next_turn_on_worker`, `finish_turn`,
`enqueue_continuation`, `settle_active`, `stop`, `fail`, diagnostics and
`queue_depth`); `TurnOutcome` loses its single-root meaning.
`crates/knot-host/src/engine/mod.rs` (`RuntimeLocalState.active` becomes a
map by invocation; `ActiveCompletion` per entry), `engine/capsule.rs`
(`inspect_active`, `fail_active*`, `reject_pending_for_root`,
`reject_pending_for_command`, `cancel_provider_turn`, keybinding batch
deferral, `finish_termination_turn`), `engine/execution.rs`
(`TerminationHandle` wakes per invocation), `engine/pool.rs` (`timer_table`,
`call_provider`, `cancel_command`). Bootstrap attaches a settle reaction to
each invocation promise that reports through a native binding. Listener
acknowledgements in `bootstrap.js` (`__knotDispatchBufferChange` and the
selection dispatch) and their foreground counterparts.

### 3. Non-exclusive loading and lifted refusals

Deliverables: package and personal-config evaluation no longer exclusive, a
registration live as soon as it resolves; `ProviderBusy` and
`runs_handler_of` removed; the A → B → A refusal narrowed to a registration
already in its own ancestry.

Validation: engine and product tests for a command and a listener running
while their package's evaluation is parked, a provider called from the
provider package's own picker command, an A → B → A chain through different
registrations of A that succeeds, and a recursive same-registration call
that stays `Unavailable`.

Review gate: loading semantics and the remaining refusals.

**Implementation context.** `crates/knot-host/src/engine/pool.rs`
(`load_package`) and the evaluation paths in `engine/capsule.rs` and
`scheduler.rs` loading state; personal config in
`src/app/personal_config.rs`. App: `src/app/extensions/host/handlers.rs`
`call_provider`, `src/app/commands/extension.rs` `runs_handler_of` and
`enqueue_child_with_target`; `knot.d.ts` drops `ProviderBusy`; update
`src/app/product/tests/providers.rs`.

### 4. Overlapping commands and bundled-package audit

Deliverables: the bridge's `roots`/`active_root` removed so commands start
on enqueue; a command's children may run concurrently (`InvocationNode.child`
becomes a set, `admit_child` drops its one-child check, cancellation and
settlement walk every child); every bundled package audited for state assumed stable across an
`await` and for commands that must not overlap, with fixes (Vim key handling
serialized in the package).

Validation: product tests that two packages' commands overlap and that two
commands of one package overlap; a product test of `Promise.all` over two
`executeCommand` calls, one inline and one cross-lifecycle, including
cancelling the parent; end-to-end package tests for each fix,
including fast Vim typing across a key-hold timeout and a prompt (`:`, `/`),
`.` and macro replay; full bundled-package suites.

Review gate: manual check of Vim and the pickers in the real app only for
what the tests cannot settle.

**Implementation context.** `src/app/commands/extension.rs`
(`start_next_root`, `settle`, `enqueue_extension_root_for`, `admit_child`,
`cancel_subtree`); key hold in
`src/app/product/key_hold.rs` stays. Packages under `extensions/@knot/`
(`vim/src/extension.ts` `vim.key`/`vim.type`, `find` `refreshing`,
`go-to-file`, `explorer`, `projects`, `notifications`, `word-highlight`);
tests in `crates/knot-ext-vim/tests/`, `crates/knot-ext-workbench/tests/`,
and the languages group.

### 5. Pushed surface callbacks

Deliverables: picker `onHighlight`/`onQuery`, input `onChange`, and tree-edit
`validate` invoked by the native surface as tasks of the owning package;
`beginPicker`/`nextPickerEvent`, `beginInput`/`nextInputEvent`, and
`beginTreeEdit`'s pull replaced by one request for the surface's result plus
callback dispatch; the bootstrap loops removed. A failing callback still
stops further calls and rethrows once the surface settles; a stale answer
(the query or value moved on) is dropped by generation.

Validation: existing picker, input, tree-edit, Go to File, Find, and Vim
prompt tests pass unchanged; new tests that a slow `onQuery` does not block
selection or dismissal, and that answers arriving out of order keep only the
latest.

Review gate: protocol shape and stale-answer handling.

**Implementation context.** Bootstrap loops in
`crates/knot-host/src/js/bootstrap.js` (`showPicker`, `showInput`, tree
`edit`); host operations and `Pulled` in
`src/app/extensions/host/handlers.rs` (`beginPicker`, `nextPickerEvent`,
`picker_update`, `beginInput`, `nextInputEvent`, `input_update`,
`beginTreeEdit`); `PickerUpdate`/`InputUpdate` in
`crates/knot-protocol/src/lib.rs` and `declarations.rs`. Dispatch callbacks
the way provider calls enter a package (`pool.call_provider`,
`start_provider_turn`), keyed by a per-session callback registered in the
bootstrap and released when the surface settles. Native surfaces in
`src/app/extensions/semantics.rs` and `src/app/tree_view.rs`.

### 6. Documentation

Deliverables: [extension host](../architecture/extension-host.md) scheduler
flow, [commands](../architecture/commands.md) extension execution, the pull
loop passages of [semantic UI](../architecture/semantic-ui.md) and its
[decisions](../decisions/semantic-ui.md),
the runtime and providers entries in
[extension-host decisions](../decisions/extension-host.md) and the index,
the D182 references in [bundled packages](../decisions/bundled-packages.md),
the [extension guide](../extension-api/README.md) on concurrency, and
`architecture.md` if ownership wording changes. Remove the scheduling report
and the roadmap item, and this plan once complete.

Validation: `just check`.

Review gate: docs read against the code.
