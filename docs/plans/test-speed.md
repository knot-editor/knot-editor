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
| `knot` `app::product::tests` | 55.5s (584 tests) | About 95ms per test; likely app and extension-host startup per test. Biggest target in aggregate. | 42.7s |
| `knot-core` `buffer::differential_tests` | 10.8s (6 tests) | 1.4–2.2s each; the slowest tests. | 0.7s |
| `knot-ext-vim` `vim::topics` | 10.9s (15 tests) | `search` 1.9s, `operators` 1.4s, `ex` 1.0s. |
| `knot-ext-vcs` | 35.6s (93 tests) | Real `git` subprocesses, mostly 0.7–1.2s each. | 45.6s |
| `knot-ext-workbench` | 15.8s (84 tests) | `project_sessions`, `explorer`, `find` at 3.6–4.0s each. | 11.7s |
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
2. ✅ Add `proptest` and convert the seeded random tests to properties:
   - `knot-core` buffer differential tests: four properties (edits against the
     model, chunk sealing, snapshot immutability, position tracking) plus edit
     points; chunks are 256 bytes under `cfg(test)`. 10.8s down to 0.7s; two
     deliberate bugs are caught and shrunk.
   - The anchored-range oracle: one property over about 10 ranges and 30 edits.
   - The seeded line-index, edit-point, and snapshot stress tests are covered
     by those properties and removed.
   - `knot-model` generated file trees (walk against ripgrep, and the index
     against the disk): proptest supplies the choices the tree generator
     consumes, so failing trees shrink.
   The only seeded generator left is the ignored `snapshot_cost` measurement.
3. ✅ Profile `app::product::tests` setup. A `sample` of a single-process run
   put most busy time in `SettingsStore::default`: `app::settings` rebuilt the
   whole native store on every read when no `ApplicationSettings` global was
   installed, which is the case in most product tests. It is now built once.
   What remains per test is about 27ms of process start (nextest runs one
   process per test) plus V8 isolate and extension-module setup.
4. Look at the per-test cost of the `knot-ext-vcs`, `knot-ext-vim`, and
   `knot-ext-workbench` tests (git repo setup, host boot).
5. Decide whether to add a guard (a budget in CI or a slow-test report) so the
   suite stays under control.

Re-run `just test-timings` after each step and record the new totals here.

## Progress

Summed test time per binary and module, from `just test-timings`. Differences
in areas a step didn't touch are run-to-run noise (about 10%).

| | Baseline | After step 2 | After step 3 |
|---|---|---|---|
| Tests | 2345 | 2341 | 2341 |
| Test run (wall) | 19.0s | 17.1s | 16.9s |
| Summed test time | 184.9s | 161.1s | 171.9s |
| `knot` | 76.1s | 71.0s | 63.9s |
| `knot` `app::product::tests` | 55.5s | 51.2s | 42.7s |
| `knot-ext-vcs` | 35.6s | 32.4s | 45.6s |
| `knot-ext-vim` | 20.6s | 18.4s | 21.3s |
| `knot-ext-workbench` | 15.8s | 14.2s | 11.7s |
| `knot-model` | 16.3s | 16.9s | 17.3s |
| `knot-core` | 13.8s | 2.3s | 2.9s |
| `knot-core` `buffer::differential_tests` | 10.8s | 0.7s | 0.7s |

The step 3 run overlapped a build, so the `knot-ext-*` columns and the summed
total are inflated by contention; only the `knot` rows are comparable.
