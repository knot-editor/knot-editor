# Unit test timings

November 2026. Classifies where the test suite spends its time, from a
per-test timing log of 1564 tests (one `<seconds> <binary> <test>` line each).
Timings are per-test durations from a parallel run, so they include
contention; read the ratios, not the absolute numbers.

## Summary

The per-test times add up to 123.7 s. Most of the tests are cheap and a few
are expensive:

- 1331 tests (85%) take under 0.1 s. Only 233 reach 0.1 s, 44 reach 0.5 s,
  and 17 reach 1 s.
- **Two kinds of test make up about 75% of the total**: the
  `knot-ext-workbench` end-to-end tests (41.2 s, 53 tests) and the `knot`
  application tests (51.0 s, 748 tests). Everything else is 31.5 s.
- A single module, `project_sessions` in `knot-ext-workbench`, costs 22.7 s
  across 14 tests, which is 18% of the whole suite.

## By crate

| Binary | Tests | Total | Mean |
| --- | ---: | ---: | ---: |
| `knot` (src/app) | 748 | 51.0 s | 68 ms |
| `knot-ext-workbench` | 53 | 41.2 s | 777 ms |
| `knot-model` | 532 | 12.1 s | 23 ms |
| `knot-ext-vim` | 33 | 9.6 s | 291 ms |
| `knot-ext-languages` | 6 | 4.5 s | 750 ms |
| `knot-core` | 109 | 2.5 s | 23 ms |
| `knot-host` | 70 | 1.5 s | 21 ms |
| `knot-app` bundled packages | 1 | 1.2 s | 1.2 s |
| `knot-protocol`, `knot-handler-macros`, `handler_linking` | 12 | 0.2 s | 17 ms |

## By semantics

### 1. Extension end-to-end tests through the real editor (about 55 s)

Each test boots a `TestEditor` with the real V8 host, loads a bundled
package, and drives it with keys, commands, and the filesystem. The cost is
runtime and package start-up plus real disk and watcher I/O, not the
assertions.

`knot-ext-workbench` by module:

| Module | Tests | Total | Slowest |
| --- | ---: | ---: | ---: |
| `project_sessions` | 14 | 22.7 s | 3.59 s |
| `explorer` | 13 | 6.8 s | 0.76 s |
| `go_to_file` | 4 | 4.7 s | 1.11 s |
| `projects` | 5 | 3.1 s | 0.84 s |
| `find` | 7 | 2.0 s | |
| `word_highlight` | 7 | 1.4 s | |
| others | 3 | 0.5 s | |

- `project_sessions` holds the 10 slowest tests of the suite
  (1.2 s to 3.6 s). They switch projects repeatedly, restart the registry,
  and restore several windows with unsaved documents, so each one is several
  sessions' worth of start-up.
- `go_to_file` pays for building and ranking a workspace file index on disk.
- `explorer` pays for real file creation, rename, and trash operations and
  for waiting on the file watcher.

`knot-ext-vim` (9.6 s) runs one test per topic. `operators`, `visual`,
`motions`, `marks`, `objects`, and `folding` are 0.5 s to 1.1 s each because
each topic is a table of many cases in one test. `knot-ext-languages`
(4.5 s) is the same shape: one test per feature (`highlights`, `folds`,
`comments`, `brackets`, `indents`) that loops over every bundled language,
0.86 s to 0.92 s each. `bundled_packages_load_with_product_discovery`
(1.2 s) loads every bundled package once.

These are single tests over many cases, so a slow test here does not mean one
slow case. They also cannot be parallelized further by the test runner.

### 2. Application product tests (about 35 s of the `knot` total)

The `knot` tests build a gpui test application, so each one pays for window,
workbench, and often an extension pool. The cost is spread thinly:
`app::product::tests::*` is about 28 s across most modules, each module
1 s to 3.7 s, with no single test above 0.65 s except the terminal ones.

Heaviest modules:

| Module | Tests | Total |
| --- | ---: | ---: |
| `product::tests::terminal` | 24 | 3.7 s |
| `terminal::session_tests` | 21 | 3.2 s |
| `terminal::view` | 25 | 3.0 s |
| `product::tests::closure` | 35 | 2.6 s |
| `product::tests::document_watch` | 12 | 2.5 s |
| `extensions::host` | 72 | 2.5 s |
| `product::tests::transient` | 24 | 2.1 s |
| `product::tab_drag` | 19 | 1.7 s |

### 3. Tests that spawn processes or wait on the OS (about 9 s)

Spread over the `knot` terminal modules and `document_watch`. These start a
real PTY and shell, or wait for real file-system events.

- `terminal::session_tests::sustained_output_keeps_foreground_updates_responsive`
  is 1.79 s, the slowest `knot` test. It produces output on purpose.
- `product::tests::terminal::new_terminals_*` start a shell in a directory:
  0.4 s to 0.64 s each.
- `document_watch` tests (0.44 s to 0.56 s) wait on the watcher.
- `extensions::storage::...interrupted_replacement` (0.41 s) re-spawns a
  process to simulate an exit.

### 4. Randomized oracle and property tests (about 2 s)

Deliberate heavy loops, all fast enough to leave alone: `knot-core`
`anchored_range::randomized_correctness_oracle` (0.59 s),
`snapshot_reads_match_buffer_after_random_edits` (0.47 s), and in
`knot-model` the random-edit syntax tests (about 0.19 s each),
`files::equivalence::generated_trees_match_ripgrep` (0.23 s), and the index
random-change test (0.15 s).

### 5. Pure unit tests (about 17 s for 800 or so tests)

`knot-core`, `knot-host`, `knot-model`, `knot-protocol`, and
`knot-handler-macros` tests that mostly take 9 ms to 25 ms. `knot-model`
modules by total: `syntax` 2.6 s (55 tests), `editor` 1.9 s (107),
`files` 1.1 s, `workbench` 0.9 s, `buffer` 0.8 s. The floor is about 9 ms per
test, which is process and harness overhead, so there is nothing to win here.

## Where a speed-up could come from

In order of payoff:

1. **`project_sessions` (22.7 s).** About a fifth of the suite lives in 14
   tests. Check whether the multi-switch tests can share a booted fixture
   or merge their repeated switch steps, as the Vim fixture already reuses
   its editor across loads.
2. **Shared start-up of the `TestEditor` e2e tests (about 55 s).** The
   workbench, vim, and languages binaries each pay package load per test.
   Measure how much of a typical 0.4 s explorer test is boot versus the
   scenario before deciding.
3. **Real I/O waits** in `explorer`, `go_to_file`, and `document_watch`.
   The recent change to prove watch absence by event order rather than
   real-time waits is the model; look for remaining sleeps or polling.
4. Leave the application product tests, the property tests, and the pure
   unit tests alone. They are cheap per test and the total is spread out.

## Method

The log lists `<seconds> <binary> <test>`, ascending. Grouping is by binary,
then by the first module segments of the test path (`knot` is split under
`app::product::tests::<module>`). The semantic classes are assigned by
reading test names and, for the e2e binaries, what each module drives. The
figures for classes 2 and 3 overlap in the terminal and watch modules and
are approximate.
