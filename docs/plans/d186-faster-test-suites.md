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

### 1. Measurement and the quick wins

- A `just test-timings` recipe (backed by a small script under `scripts/`)
  that runs nextest with per-test timings and prints the sorted log and the
  totals per binary and per module, in the format of `test.log`.
- Cache built-in glyph bytes in `icons.rs`.
- One blocking `wait_until`; route `wait_for_product_startup` and every local
  copy through it.
- Record the baseline and the new totals in this plan.

Validation: `just check`; the timing totals before and after.

Review gate: measured effect of the quick wins; whether the wait strategy
holds without flakiness over several full runs.

**Implementation context:** `src/app/icons.rs` (`svg_bytes`, `glyph_exists`,
`AllAssets`); `src/test_support/mod.rs` (`wait_until`);
`src/app/product/driver/setup.rs` (`wait_for_product_startup`);
`src/app/product/driver/mod.rs` (`settle`, the pattern to mirror with
`pool.activity()` and `pool.wait_for_turn`); local copies such as
`src/app/product/tests/document_watch.rs::wait_until` and
`src/app/product/tests.rs::wait_for_api_condition` (grep `yield_now` in
`src/`). nextest config in `.config/nextest.toml`; recipes in `justfile`.

### 2. Headless driver and launch

- A test-only option at `launch::show_window` that leaves the `KnotWindow`
  without a native window; `TestProduct::launch` uses it by default, with a
  windowed variant for tests that need a native window.
- `TestWindow` over an optional native window: keys go through
  `KnotWindow::press_key` without one, as in `TestEditor`; window identity
  uses `window_id(entity)` as `TestEditor::status` already does.
- A headless counterpart of `setup::product_window` for the library's own
  tests, and `product_window` kept for windowed ones.
- Convert `knot-ext-workbench` and the driver's own tests; keep windowed only
  what needs prompts, pointer input, or typed input.
- Record the three tiers in [development](../development.md), replacing the
  current windowed-versus-headless sentences there.

Validation: `just check`; workbench totals before and after.

Review gate: the driver API shape before the library sweep copies it.

**Implementation context:** `src/app/product/launch.rs` (`show_window`,
`open_window`); `src/app/product.rs` (`ApplicationWindows`, `attach`,
`window_id`, `test_window_id`, `native_window`);
`src/app/product/driver/{application,window,setup,editor}.rs`;
`install_protected_window_close` and `observe_extension_host`, which a
headless window must replace with what `TestEditor::over` does
(`observe_application`, `activate`); `crates/knot-ext-workbench/tests/workbench/`.

### 3–6. Library sweep, one area at a time

For each area: switch tests to headless windows, move tests that need no
gpui into `knot-model` (turning the code they test into model functions where
it still lives in `src/app`), keep windowed only the tier-3 tests, and measure.
One commit per area; split an area further if it grows.

3. `src/app/product/tests/` part one: `closure`, `transient`, `keybindings`,
   `editor_api`, `commands`, `operations`, `editing`, `key_hold`.
4. `src/app/product/tests/` part two: `persistence`, `observation`,
   `sessions`, `recovery`, `extension_api`, `startup`, `document_watch`,
   `window_restore`, `tool_layout`, `tools`, `files`, `workbench`, and
   `src/app/product/tab_drag/` (which keeps windows where it tests
   cross-window delivery).
5. Terminal: `src/app/terminal/session_tests.rs`, `view/tests.rs`,
   `mouse_tests.rs`. Session behavior goes headless; paint and mouse stay
   windowed.
6. Views and the rest: `src/app/editor/tests*`, `tree_view/tests.rs`,
   `picker.rs`, `text_input.rs`, `icons/tests.rs`, `theme.rs`,
   `recovery/tests.rs`, `extensions/`. Also move the plain `#[test]`s in
   `src/app` that need no gpui to `knot-model`.

Validation per area: `just check`; that area's module totals before and
after.

Review gate per checkpoint: the moved and converted tests still assert the
same behavior, and what stayed windowed is justified.

**Implementation context:** the headless helpers from checkpoint 2;
`src/app/product/tests.rs` (shared helpers such as
`two_dirty_untitled_documents_in_one_window`, `toggle_test_tool`); 
`knot-model` modules named in the [repository map](../../AGENTS.md#repository-map)
for moved logic; grep `product_window(` and `add_window` for the call sites.

### 7. Package subsets and the remaining end-to-end cost

- Let end-to-end tests start only the packages they exercise, resolving
  `requires`; apply it to `knot-ext-workbench`.
- Re-profile `project_sessions` and decide whether to merge its repeated
  switch scenarios.
- Re-profile a Vim topic and decide whether settling after each key needs
  work.

Validation: `just check`; workbench and Vim totals.

Review gate: whether any remaining cost justifies more work in D186.

**Implementation context:** `crates/knot-ext-workbench/src` (`PACKAGES`);
`TestProduct::start` and `launch` in `src/app/product/driver/`;
`host.set_bundled_packages` in `src/app/extensions/host.rs`; package
discovery in `crates/knot-model/src/extensions/package.rs`.

### 8. Close

- Final measurements in the
  [extension authoring and tests](../decisions/extension-host.md#extension-authoring-and-tests)
  decision, or a new testing entry if the tiers warrant their own, with its
  index line in [decisions](../decisions.md).
- Remove the test timings report and `test.log`, and this plan once its
  results are preserved; mark D186 done in the roadmap.
