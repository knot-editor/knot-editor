# Test suite speed

Status: 🚧 in progress. Goal: keep the suite fast and tackle the slowest tests
one at a time. Measure with `just test-timings` (per-test, per-binary, and
per-module totals; per-test numbers come from a parallel run, so compare runs
on the same machine).

## Baseline (2026-11-27, debug, 2345 tests)

- 19s test run, 34s wall including the build; 185s summed test time.
- 1530 tests under 50ms, 628 at 50–200ms, 174 at 0.2–1s, 13 at 1s or more.
  The slowest single test is 2.2s.

| Area | Total | Notes |
|---|---|---|
| `knot` `app::product::tests` | 55.5s (584 tests) | About 95ms per test; likely app and extension-host startup per test. Biggest target in aggregate. |
| `knot-core` `buffer::differential_tests` | 10.8s (6 tests) | 1.4–2.2s each; the slowest tests. |
| `knot-ext-vim` `vim::topics` | 10.9s (15 tests) | `search` 1.9s, `operators` 1.4s, `ex` 1.0s. |
| `knot-ext-vcs` | 35.6s (93 tests) | Real `git` subprocesses, mostly 0.7–1.2s each. |
| `knot-ext-workbench` | 15.8s (84 tests) | `project_sessions`, `explorer`, `find` at 3.6–4.0s each. |
| `knot-app` `bundled_packages_load_with_product_discovery` | 1.3s | One test. |

## Findings

- **Differential tests are oracle-bound, not optimization-bound.**
  `opt-level = 3` for `knot-core` in dev changed nothing. Every one of the 900
  steps rereads the whole text (`check_buffer`) and rescans every piece
  (`validate()`). Validating every 10th step saves about 40%, at the cost of
  localizing failures to a window of steps.
- The 900 steps exist to push the buffer past `ADD_CHUNK_SIZE` (16 KiB) so
  chunks seal; the test asserts that it did. It is a size requirement, not a
  sequence-length one.
- The hand-rolled xorshift with six fixed seeds replays the same runs on every
  CI run and gives no shrinking: a failure is a 900-step trace.
- `anchored_range::tests::randomized_correctness_oracle` (0.7s) has the same
  shape: one fixed seed, 200 ranges, 2000 edits, hand-rolled `Rng`.
- Other hand-rolled `Rng` users: `buffer/tests.rs`, `buffer/snapshot/tests.rs`,
  `knot-model` `files/equivalence.rs`, and the `buffer_bench` example.

## Steps

1. ✅ Review `anchored_range::tests::randomized_correctness_oracle`: it replays
   2000 insert, delete, and replace edits over 200 ranges and compares every
   range against an offset-transform oracle after each edit (0.7s, one fixed
   seed). The sizes are far beyond what the rules need (the rules involve at
   most a few ranges around one edit), so it joins the proptest conversion
   with about 10 ranges and 30 edits.
2. 🚧 Add `proptest` as a dev-dependency and convert the buffer differential
   tests to focused properties (agreed direction). ✅ Buffer differential tests
   converted (four properties: edits against the model, chunk sealing,
   snapshot immutability, position tracking; chunks are 256 bytes under
   `cfg(test)`): 10.8s down to 0.7s, and two deliberate bugs (chunks never
   sealing, a wrong UTF-16 length) are caught and shrunk. Remaining: the
   anchored-range oracle.
   - edit sequences against the `String` model: short sequences (about 20–50
     edits) with occasional full-oracle checks, relying on shrinking;
   - chunk sealing as its own property, with a test-only way to make chunks
     small so sealing happens within a few edits;
   - snapshots and position tracking as separate properties instead of one
     interleaved loop.
   Convert the anchored-range oracle the same way. Decide then whether the other `Rng` users are worth converting.
3. Profile `app::product::tests` setup (shared fixture, app and host startup
   per test) and cut the per-test cost.
4. Look at the per-test cost of the `knot-ext-vcs`, `knot-ext-vim`, and
   `knot-ext-workbench` tests (git repo setup, host boot).
5. Decide whether to add a guard (a budget in CI or a slow-test report) so the
   suite stays under control.

Re-run `just test-timings` after each step and record the new totals here.
