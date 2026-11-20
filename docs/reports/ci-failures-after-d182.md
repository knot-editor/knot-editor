# CI failures after the D182 work

Status as of 2026-11-20. Cause not found; nothing here is fixed.

## Summary

CI went red at `809d9248` ("Remove the completed D182 plan", a docs-only
commit). The last green CI run is `02e9f40c` ("Remove the completed D151
plan"). The 48 commits in between are mostly D182 (interleaved extension
commands and tasks), plus the D087 entitlement commits at the end. The failing
tests all exercise extension commands, cancellation, or the extension host, so
a D182 regression is the working hypothesis. It is not confirmed.

## Runs

| Run | Commit | Result |
| --- | --- | --- |
| 44813150835 | `809d9248` | failed: 1 test (`public_open_rejects_pane_or_window_loss_and_command_cancellation_without_leaks`) |
| 44816633320, attempt 1 | `6cb0670d` | failed: 1 test (`a_command_runs_an_inline_and_a_cross_package_child_at_once`) |
| 44816633320, re-run of failed job | `6cb0670d` | failed: 3 tests (the two above and `knot-ext-workbench` `explorer::a_document_saved_under_a_new_name_is_selected_in_the_explorer`) |

`6cb0670d` only adds scenario names to the assertion messages in the first
test; it does not change behavior. The failure count grew from one to three
between runs, so the failures are not a single rare race.

## Failing tests

1. `knot app::product::tests::operations::public_open_rejects_pane_or_window_loss_and_command_cancellation_without_leaks`
   - Fails at `src/app/product/tests/operations.rs:781` in ~0.13 s: the
     assertion that no document exists for `mem://product/notes.txt` fails, so
     an open that should have been rejected or cancelled committed a document.
   - It failed in the first run (before the assertions were labelled) and in
     the re-run. The re-run log has the scenario label; it was not read.
2. `knot app::product::tests::interleaving::a_command_runs_an_inline_and_a_cross_package_child_at_once`
   - Times out after 10 s with "the condition never held" at
     `src/app/product/tests.rs:516`, which is `wait_for_command`. The only
     `wait_for_command` in the test is on the hanging command after
     `cancel_active_test_commands`, so the cancelled command never reported
     `Cancelled`.
3. `knot-ext-workbench workbench explorer::a_document_saved_under_a_new_name_is_selected_in_the_explorer`
   - Times out after ~10 s with "the condition never held" at
     `crates/knot-ext-workbench/tests/workbench/explorer.rs:619`. Seen only in
     the re-run. Not investigated.

## What was checked

- **Local runs pass.** On the 10-core macOS machine the first test passed in 5
  sequential runs and 60 parallel runs; it and the interleaving tests passed 40
  parallel copies; the `knot` package passed 3 full runs; and
  `cargo nextest run --locked --workspace --all-targets --profile ci` (CI's
  exact command) passed 1968 tests. The local machine does not reproduce any of
  the three failures.
- **Open path read, no race found.** `start_resource_operation`
  (`src/app/product/open.rs`) rechecks the caller (`OperationCaller::is_live`),
  `open_generation`, `window_is_live`, and `matches_presentation` before it
  applies a read result. `GatedFileSystemProvider::read` takes its gate when
  `read()` is called, and the test waits for that before the pane, window, or
  cancel step.
- **Cancel path read, no race found.** `cancel_subtree`
  (`src/app/commands/extension.rs`) marks a whole invocation tree and queues
  `CancelExtension` for started nodes. The isolate remembers a cancel that
  arrives before its command runs (`cancelled_commands` in
  `crates/knot-host/src/engine/capsule.rs`).
- **Open question in the open path.** `OperationCaller.invocation` is an
  `Option`. If `request.invocation` were `None`, `is_live` would never observe a
  cancel and the open would commit. That would fail every time, so it does not
  fit an intermittent failure, but it was not checked.

## Observations

- The failures appear only on CI. CI runs on a smaller Linux runner; both the
  OS and the core count differ from the local machine. Neither was isolated.
- Tests 1 and 2 both depend on `cancel_active_test_commands`
  (`src/app/extensions/host.rs`). That is a shared dependency, not a shown
  shared cause. Test 3 involves a save and the explorer, with no cancel.
- Two of the three fail with the same "condition never held" timeout, which
  points at an expected completion that never arrives rather than a wrong
  value.

## Next steps

1. Read the labelled failure of test 1 in run 44816633320 (the re-run attempt)
   to learn which scenario (`pane`, `window`, `cancel`) fails.
2. Reproduce under CI-like conditions: fewer CPUs and more test processes than
   cores (`cargo nextest run -j <n>` with a high `n`, or a Linux container
   limited to 2 CPUs). The attempt to run `-j 1`, `-j 2`, and `-j 40` was
   interrupted before it ran.
3. Bisect `02e9f40c..809d9248` on CI or in a CPU-limited container, starting
   with the D182 commits that changed command start, invocation completion, and
   cancellation: `16394c36`, `ae8f05b1`, `0b516518`, `7a504651`.
4. Read test 3's log and source to see whether it shares a cause with the other
   two.
