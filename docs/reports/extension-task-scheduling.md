# Extension task scheduling

November 2026. Examines why a package's callbacks run one at a time to
completion, what that costs, and what moving to JavaScript's ordinary task
semantics would take. Supports [D182](../roadmap.md#architecture-and-technical-debt).
Current behavior is owned by
[extension host](../architecture/extension-host.md#runtime-and-scheduler-flow)
and [commands](../architecture/commands.md#extension-execution).

## Summary

A root is one entry from the editor into a package: a command invocation, a
listener call, a timer callback, or a provider request. A package runs one
root at a time, and the next root starts only when the current root's promise
has settled. Microtasks behave as in any JavaScript engine; the difference is
at the task level, where Node and browsers run the next ready task whenever
the current one is parked at an `await`.

The code shows three consequences, the second and third of which the
architecture references do not state:

1. **Head-of-line blocking inside a package.** While a root awaits, the
   package's listeners, timers, and providers wait.
2. **A root that awaits its own package deadlocks it.** Awaiting a timer, or
   any promise only another root resolves, parks the package until it is
   restarted.
3. **Extension commands are serial across all packages**, not per package.

The recommendation is to interleave roots at awaits, carry each root's
identity through its continuations instead of reading it from global state,
and keep commands ordered where typing needs it.

## How it works today

- **Scheduler** (`crates/knot-host/src/scheduler.rs`). Each extension has a
  FIFO of pending roots and one `active: Option<ActiveRoot>`. `next_turn`
  pops a pending root only when no root is active; otherwise it runs the
  active root's continuations. `enqueue_continuation` rejects any root but
  the active one (`WrongRoot`).
- **Root completion** (`inspect_active` in
  `crates/knot-host/src/engine/capsule.rs`). After each turn the engine
  inspects the root's promise. A pending promise yields `AwaitingHostWork`
  whether or not a host request is outstanding, and the root stays active.
- **Timers** (`timer_table` in `crates/knot-host/src/engine/pool.rs`). An
  expiry is `enqueue_root`, so it waits behind the active root.
- **Attribution in JavaScript** (`crates/knot-host/src/js/bootstrap.js`).
  `request()` attributes a host call to `activeCommandFrames.at(-1)`, a
  global stack, and the native side records a turn-wide `current_root`. Both
  are correct only because nothing else runs while a command is parked.
- **Foreground bridge** (`src/app/commands/extension.rs`).
  `ExtensionCommandBridge` holds one `active_root` and one `roots` queue for
  the whole application, and `start_next_root` starts a command only when
  none is active.

## Consequences

### Blocking inside a package

A command awaiting `showInput` for thirty seconds freezes that package's file
watches, selection listeners, tree refreshes, and completion answers for
thirty seconds. The picker, input, and tree-validation pull loops exist
because a pushed notification would queue behind the awaiting command
([decision](../decisions/semantic-ui.md)). Each new interactive surface needs
another loop.

### Self-deadlock

```ts
commands.register("demo.wait", async () => {
  await new Promise((resolve) => setTimeout(resolve, 100));
});
```

The command's promise is pending, so its root stays active. The timer expiry
is a new root and never starts. The package then accepts no further work
until Restart Extension terminates it; because commands are globally serial,
later extension commands of other packages queue behind it too. The same
holds for awaiting an event (`await once(onDidChange)`), or any promise a
listener or provider resolves. This follows from the code above; no test
exercises it, and the timer tests only await inside timer callbacks.

### Commands serial across packages

A slow command in one package delays every other package's commands. A Vim
key handled by one package waits behind another package's command that is
awaiting a process or a picker; the key hold releases typing after a second,
but the command itself stays queued.

### What cannot be built

A language client (D067) receives responses out of order, notifications
while a request is pending, and server-to-client requests that must be
answered before the server replies. Streaming the output of a spawned process (spawning
arrives with D087) and progress reporting (D117) have the same shape. None fits one root at a time.

## What serial roots provide

The recorded reason is that serial roots "simplify ownership"
([decision](../decisions/extension-host.md#extension-runtime)). In the code
that means:

- one answer to "which command does this host call act for", read from the
  top of a global stack;
- captured presentation authority that a listener cannot borrow, since no
  listener runs while a command is parked;
- cancellation and teardown that settle one root's pending requests;
- typing order: a key that runs a command holds later keys until it ends.

## Proposed direction

1. **Scheduler: several live roots per extension.** Replace
   `active: Option<ActiveRoot>` with a set of live roots, each with its own
   continuation queue. Turns stay serial per isolate; a turn belongs to one
   root. A new root may start whenever no turn is running. Completion,
   cancellation, and `Stopping` settle every live root.
2. **Engine: per-root execution records.** `local_state.active` becomes a map
   by root, and `inspect_active` inspects every live root after a turn, since
   a turn of one root can settle another's promise.
3. **Attribution that follows the continuation.** Replace the global
   `activeCommandFrames` stack and turn-wide `current_root` with identity
   carried by the async chain. V8 exposes continuation-preserved embedder
   data (`set_continuation_preserved_embedder_data` in the `v8` 152 crate),
   which survives `await` and promise reactions; the frame, its invocation,
   and its root travel there. A continuation of command A that runs inside a
   turn of listener B still acts with A's authority, and B never acquires it.
   This is the piece to prototype first, since everything else depends on it.
4. **Root lifetime.** A root with a pending promise and no outstanding host
   request no longer blocks anything, so the self-deadlock disappears. Its
   completion still settles when the promise does.
5. **Command ordering.** Keep the key hold as it is: it waits for the
   command's completion, not for the scheduler. Make the bridge's serial
   queue per package rather than application-wide, and decide whether two
   commands of one package may overlap. Keeping them serial per package
   preserves today's guarantee for modal input and costs little; cross-package
   children already await foreground completion.
6. **Simplifications that follow.** Picker highlight and query, input
   change, and tree validation can become ordinary callbacks invoked as roots
   while the command awaits. The pull loops can stay until a consumer is
   touched.

## Risks

- **Authors lose run-to-completion across awaits.** State read before an
  `await` may be stale after it. Native code already revalidates after every
  await, and this is what JavaScript authors expect, but the bundled packages
  must be audited for module state they assume is stable across an await
  (`@knot/find`'s `refreshing` flag already guards against this).
- **Cancellation fan-out.** `reject_pending_for_root`, provider cancellation,
  and termination wakeups address the single active root and need a root
  argument everywhere.
- **Keybinding batches.** The engine holds earlier responses until a batch
  settles; with several roots the hold must be scoped to the batch's root or
  remain isolate-wide by decision.
- **Fairness.** A package can now have many live roots. D019 (quotas and
  timeouts) becomes more relevant, but is not a prerequisite.

## Suggested checkpoints

1. Prototype continuation-preserved attribution in the engine with the
   scheduler unchanged, and move `request()` onto it. No behavior change.
2. Multi-root scheduler and engine records, with tests for awaiting a timer
   and an event inside a command, and for a listener running while a command
   awaits a picker.
3. Per-package command queue in the foreground bridge.
4. Audit the bundled packages; update the extension guide, the architecture
   references, and the runtime decision.
5. Optional: replace the pull loops with callbacks.
