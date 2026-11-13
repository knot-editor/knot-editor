# Watch test clock

## Goal

Make the `watch` debounce tests instant without weakening what they assert.
Target: the three tests now burning ~0.22s in real waits go under 10ms total.

## Why a refactor, not smaller constants

`WatchRegistry::new` spawns a delivery thread running `deliver`, which computes
deadlines with `Instant::now()` and blocks in `Receiver::recv_timeout`
(`crates/knot-model/src/watch.rs`). The tests prove absence ("no second batch")
with `recv_timeout(WINDOW * 4)` — 160ms of real waiting per assert. Shrinking
`WINDOW` only scales the waits down; any real clock keeps a wait. There is no
clock abstraction or mock timer anywhere in the workspace.

A fake `Instant` alone is not enough either: the thread still blocks in real
time inside `recv_timeout`. The seam must be the wait itself.

## Approach

- Add a small `Wait` trait next to `WatchRegistry`, e.g.
  `wait(&self, receiver: &Receiver<Message>, timeout: Duration) -> Result<Message, RecvTimeoutError>`.
  The production impl calls `recv_timeout`; `deliver` takes it as a parameter.
- Production behavior is byte-identical: same thread, same `COALESCE_WINDOW`
  (100ms), same `WatchRegistry::new(window)` signature (default impl inside).
- Tests inject a manual impl: drain with `try_recv`, and when empty advance a
  virtual clock past the deadline and return timeout. Seed events still travel
  the real channel, so batching/dedup logic runs unchanged — only time is fake.
- Convert the three slow tests (`a_burst_of_writes...`, `a_non_recursive_...`,
  `watches_under_a_recursive_ancestor...`) to the manual impl. Keep `WINDOW`
  at 40ms and keep every negative assert.

## Steps

1. ✅-gate: introduce `Wait`, thread through `new`/`deliver`, prod impl only.
   Validation: `cargo nextest run -p knot-model --lib -- watch` passes with
   timings unchanged; `cargo clippy -p knot-model --all-targets`.
2. ✅-gate: add manual test impl, convert the three tests, assert sub-10ms.
   Validation: same command shows the three tests near-zero; full
   `just test` clean (modulo the known sandbox-only pty failures).

## Non-goals

No change to `COALESCE_WINDOW`, delivery threading, or the file-watching
contract in `docs/decisions.md#file-watching`. No fake timers for V8/extension
hosts — out of scope.
