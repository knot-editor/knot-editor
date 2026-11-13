# D186 — Much faster Rust test suites

Status: 🚧 planned. Tracked in the [roadmap](../roadmap.md); the starting
measurements are in the [test timings report](../reports/test-timings.md).

## Goal

Make every test cost only what it exercises, so that the suite stays fast as
the product grows. A test that checks logic runs as a plain `knot-model`
test; a test of application behavior runs the product without a native
window; only tests of painting, pointer input, layout, and native prompts
open one. No test spins a core while it waits.

The outcome is lower total CPU per run, which is what decides wall time: the
suites are CPU-bound, and the workspace run is about the summed per-test time
divided by the cores.

## What the profiles found

Time Profiler, Apple Silicon, debug build, each binary run serially in one
process, 2026-11-13:

| Where CPU goes | `knot-ext-workbench` (53 tests, 13.6 s) | `knot` lib (748 tests, ~13 s CPU, 26.7 s wall) |
| --- | ---: | ---: |
| gpui `Window::draw` of test windows | 27% | 44% |
| – icons read from disk on every draw (`icons::svg_bytes`) | 10% | 4.5% |
| – terminal paint and snapshot capture | – | ~20% |
| V8 turns | 24% | 5% |
| Product startup (globals, host, packages, launch) | ~22% | ~10% |
| – isolate creation and bootstrap | 6% (bootstrap 4%) | 4.6% |

- 312 call sites open a test window through `product_window`, and gpui lays
  out and paints every dirty window after each update.
- `AllAssets::get` from `gpui-kit-assets` canonicalizes and opens the SVG file
  on every call in debug builds, and `AppIcon` calls it on every render. The
  debug app pays the same cost.
- `test_support::wait_until`, its copies, and `wait_for_product_startup` loop
  on `run_until_parked` and `yield_now`, so a waiting test keeps a core busy
  that other tests' extension workers need. `driver::settle` already blocks
  on `wait_for_turn`.
- A nextest process costs about 10 ms, so process-per-test isolation stays.

## Chosen design

- **Cheap tests, not shared products.** Each test keeps its own process and
  `TestAppContext`. Sharing a started product across tests would need resets
  at every layer (documents, windows, sessions, stores, package state) and
  would work against nextest and gpui's per-test contexts. The table-driven
  suites (Vim, languages) keep their one product per topic, and other tests
  merge scenarios only where several tests repeat the same expensive steps.
- **Three test tiers**, recorded in [development](../development.md):
  1. *Model tests*: plain `#[test]` in `knot-model` (or `knot-core`) for
     anything that needs no gpui, as the
     [thin-shell invariant](../architecture.md#invariants) already asks.
  2. *Headless product tests*, the default for application behavior: a
     `KnotWindow` with no native window, driven through its key router and
     commands, as `TestEditor::open` already does.
  3. *Windowed tests*, only for painting, pointer input, IME, layout, the
     window adapter, and native prompts.
- **Headless launch.** `TestProduct::launch` starts the whole application
  with headless windows by default, through a test-only option at
  `launch::show_window`, the single place startup opens a native window.
  `TestWindow` drives a `KnotWindow` whose native window is optional; a test
  asks for a native window when it needs one.
- **Blocking waits.** One `wait_until` in `test_support` blocks between
  checks: on the extension pool's next turn when a package is busy, otherwise
  briefly on the clock for OS events such as the watcher and PTYs. Local copies
  are removed.
- **Icons are read once.** The bytes of each built-in glyph are cached, so a
  draw never touches the disk.
- **Only the packages a test uses.** An end-to-end test starts the packages
  it exercises, not its crate's whole `PACKAGES` set.
- **A repeatable measurement.** One command produces the per-test timing log
  and the per-binary and per-module totals that the report used, so each
  checkpoint states its effect in numbers.

## Exclusions

- No shared or reset-in-place product across tests beyond the existing
  per-topic reuse.
- No change to nextest or the libtest harness.
- No V8 startup snapshot or code cache. Bootstrap is about 4% of
  end-to-end CPU and only per process; [D139](d139-isolate-floor.md) stays a
  deferred memory item.
- No change to product behavior except the icon cache.

## Deferred decisions

- Whether `project_sessions` merges its repeated switch scenarios: decide in
  checkpoint 7 from what it costs after the earlier steps.
- Whether settling after each key needs to get cheaper (the roadmap's last
  point): decide from the profile after the headless sweep, since drawing
  currently hides it.

## Checkpoints

Each checkpoint, and each area within a sweep, is its own commit. Each one
measures before and after with the command from checkpoint 1 and records the
result here.

### 1. Measurement and the quick wins ✅

Done and approved. `just test-timings` (`scripts/test-timings.py`) produces
the per-test log and the per-binary and per-module totals with wall and CPU
time; built-in glyph bytes are cached in `src/app/icons.rs`; and every wait in
`src/app` goes through the blocking `app::driver::wait_until`, exported by
`test_support`. Baseline, warm build, whole workspace: summed per-test time
123.3 s, wall 13.3 s, CPU 71.4 s. After: 125.1 s, 13.5 s, 67.3 s. A blocking
wait frees a core but does not shorten a test, so the gain is CPU only; the
large costs (drawing test windows, product startup) belong to the later
checkpoints.

### 2. Headless driver and launch ✅

Done and approved. `TestProduct::launch` opens windows with no native window
(`HeadlessWindows`, honored at `launch::show_window`), and
`TestProduct::launch_windowed` opens native ones. `TestWindow` holds an
optional native window and takes keys and text through `KnotWindow::press_key`
and `type_text`; `driver/setup.rs::headless_window` is the counterpart of
`product_window`. Without a native window the window records its focused tab,
activates itself, and hands activation to the last opened window when it
closes. The [development](../development.md) guide records the three tiers.
43 of 53 `knot-ext-workbench` tests are headless; the ten that answer native
prompts or edit a tree row in place (which takes the native window's focus)
use `Projects::start_windowed` and `Explorer::start_windowed`. Workbench run,
warm build: CPU 13.6 s → 12.0 s, wall 6.3 s → 6.0 s. Drawing was a smaller
share than the serial profile suggested; startup and V8 turns dominate
(checkpoint 7).

### 3–6. Library sweep, one area at a time

For each area: switch tests to headless windows, move tests that need no
gpui into `knot-model` (turning the code they test into model functions where
it still lives in `src/app`), keep windowed only the tier-3 tests, and measure.
One commit per area; split an area further if it grows.

3. ✅ `src/app/product/tests/` part one: `closure`, `transient`, `keybindings`,
   `editor_api`, `commands`, `operations`, `editing`, `key_hold`. Done and
   approved. Tests reach a window through `WindowHandle` (`tests.rs`), with
   `Headless` for a window that has no native window, and `press_keys` and
   `type_text` stand in for `simulate_keystrokes` and `simulate_input`.
   Windowed on purpose: native prompts (`closure`, `transient`), the menu
   action and native-keybinding adapters (`commands`, `keybindings`), IME,
   mouse, and focus through `Window` (`editing`, `key_hold`), viewports
   (`editor_api`), and anything starting from a native startup. Summed time of
   the 145 tests: 9.95 s → 8.19 s. No test needed moving to `knot-model`.
4. ✅ `src/app/product/tests/` part two: `persistence`, `observation`,
   `sessions`, `recovery`, `extension_api`, `startup`, `document_watch`,
   `window_restore`, `tool_layout`, `tools`, `files`, `workbench`, and
   `src/app/product/tab_drag/` (which keeps windows where it tests
   cross-window delivery). Done and approved. Windowed on purpose: tests
   that identify a window by its native handle (observation, public API
   capture, replacement sessions), answer native prompts or press keys through
   gpui (`simulate_keystrokes`, IME, tree rows), or draw (`tools` geometry),
   the native close and failed-launch paths of `window_restore`
   (`Launch::windowed`), and all of `tab_drag`, which is pointer-driven.
   Run serially, the 135 tests of the twelve modules went from 8.58 s to
   7.95 s: these modules mostly wait on stores, the watcher, and V8 rather
   than draw, so the gain is small. No test needed moving to `knot-model`.
5. ✅ Terminal: `src/app/terminal/session_tests.rs`, `view/tests.rs`,
   `mouse_tests.rs`. Session behavior goes headless; paint and mouse stay
   windowed. Done and approved. The session tests already run without a
   window, apart from the two focus tests, which depend on gpui focus. The
   mouse and view tests are pointer and layout driven, so all stay windowed. The one change
   is the sustained output test, whose shell stream went from 20000 to 4000
   lines: 1.2 s → 0.12 s, keeping the check that foreground updates stay
   responsive during output. The 70 terminal-module tests now sum to 4.1 s
   serially. No test needed moving to `knot-model`.
6. ✅ Views and the rest: `src/app/editor/tests*`, `tree_view/tests.rs`,
   `picker.rs`, `text_input.rs`, `icons/tests.rs`, `theme.rs`,
   `recovery/tests.rs`, `extensions/`. Also move the plain `#[test]`s in
   `src/app` that need no gpui to `knot-model`. Done and approved. The
   editor, `text_input`, `icons`, and `theme` tests that open a window test
   layout, painting, pointer input, or IME, so they stay windowed; the other
   modules already run without a window, and the extension host tests (2.7 s,
   93 tests) spend their time in V8, which checkpoint 7 covers. Moved to
   `knot-model`: launch and open requests (`entry`), the terminal start
   directory (`terminal_directory`), and the status bar registry
   (`status_bar`). Still in `src/app` because they need gpui, alacritty, or `knot-host`:
   `convert`, `window_layout`, the terminal `links`, `input`, `layout`, and
   `mouse`, and `personal_config`. No time changed measurably; the moved
   tests now build and run with `knot-model`.

Validation per area: `just check`; that area's module totals before and
after.

Review gate per checkpoint: the moved and converted tests still assert the
same behavior, and what stayed windowed is justified.

**Implementation context:** the headless helpers from checkpoint 2;
`src/app/product/tests.rs` (shared helpers such as
`two_dirty_untitled_documents_in_one_window`, `toggle_test_tool`); 
`knot-model` modules named in the [repository map](../../AGENTS.md#repository-map)
for moved logic; grep `product_window(` and `add_window` for the call sites.

### 7. Test file watches ✅

Test file watches. The end-to-end tests spent most of their wall time idle:
starting an operating system file watch (`FSEventStreamStart`) blocks for
about 100 ms, `project_sessions` starts nine per test, and the starts of
parallel test processes queue behind each other, which also inflated the
summed per-test times. Test providers
(`LocalFileSystemProvider::unrestricted_with_scratch_trash`) now report no
file events, and `TestProduct::watch_files` turns them on for the two tests
that wait for an external change (`explorer` and `go_to_file`). Warm build,
whole workspace: wall 13.4 s → 7.8 s, summed per-test time about 130 s →
77 s, CPU unchanged at about 61 s. The product-side cost is roadmap item
D187.

The second profile (wall 8.0 s, summed 79.7 s) shows no test above 2 s; the
largest remaining costs are the Vim topics (7.4 s for 11 tests),
`knot-ext-workbench` (13.8 s for 53), and the `knot-ext-languages` fixture
tests (0.7 s each). Checkpoints 8–10 take them in that order.

### 8. Package subsets for the workbench tests ✅

- ✅ Each `knot-ext-workbench` test starts only the packages it exercises,
  through `bundled::only` in the test crate (no test needed `requires`
  resolution, since the bundled workbench packages are independent). A
  trivial test halves (0.14 s → 0.06 s). Workbench summed time 13.8 s → 9.1 s;
  whole workspace 79.7 s → 73.0 s. `knot-app` keeps every package on purpose:
  its test checks product discovery of all of them.
- ✅ `project_sessions` (14 tests, 4.1 s) is not merged: its scenarios differ,
  no test exceeds 0.8 s, and merging would trade clear failures for little.

Validation: `just check`; workbench totals.

**Implementation context:** `crates/knot-ext-workbench/src` (`PACKAGES`);
`TestProduct::start` and `launch` in `src/app/product/driver/`;
`host.set_bundled_packages` in `src/app/extensions/host.rs`; package
discovery in `crates/knot-model/src/extensions/package.rs`.

### 9. Language fixture tests ✅

- ✅ Profile showed the 0.6–0.7 s of each of the five fixture tests is
  starting the language packages, almost all of it tree-sitter query analysis
  (`ts_query__perform_analysis`, already optimized); the cases take
  milliseconds. The five tables now run on one product in one test
  (`bundled_language_cases_run_on_their_registered_languages`), each topic
  exposing `check`. `knot-ext-languages` summed time 3.6 s → 0.7 s.

Validation: `just check`; `knot-ext-languages` totals.

### 10. Vim topics ✅

- ✅ `topics::operators` and `topics::visual` run alone in about 0.5 s of
  mostly CPU: startup is 40 ms and the rest is about 5 ms per case (100 and
  73 cases), the V8 turns of each key. The 1.5–1.8 s in a full run is
  contention among parallel processes. Settling after each key is a real
  round trip rather than avoidable waiting, so no change.

Whole workspace after checkpoints 8–10, warm build: wall 7.2–7.7 s, summed
per-test time 72.7 s, CPU about 58 s (from 13.4 s, about 130 s, and 61 s
before checkpoint 7). No test runs over 1.6 s.

Validation: `just check`; Vim totals.

Review gate: whether any remaining cost justifies more work in D186.

### 11. Close

- Final measurements in the
  [extension authoring and tests](../decisions/extension-host.md#extension-authoring-and-tests)
  decision, or a new testing entry if the tiers warrant their own, with its
  index line in [decisions](../decisions.md).
- Remove the test timings report and `test.log`, and this plan once its
  results are preserved; mark D186 done in the roadmap.
