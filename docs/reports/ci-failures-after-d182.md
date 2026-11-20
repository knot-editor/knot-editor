# CI failures after the D182 work

Status as of 2026-11-20. Two D182 regressions found and fixed; one older
explorer flake remains open.

## Summary

CI went red at `809d9248` ("Remove the completed D182 plan"). The last green
run is `02e9f40c`. Three tests failed across two runs (44813150835 and
44816633320, both attempts). Two are separate D182 cancellation bugs; the third
is an explorer test that predates D182 and flakes under load.

All three reproduce locally once the machine is loaded: 10 busy-loop processes
(`yes > /dev/null`) and 120 copies of the test binary, 30 at a time. On an
idle machine they pass.

## Fixed

1. `operations::public_open_rejects_pane_or_window_loss_and_command_cancellation_without_leaks`
   (`cancel` scenario) — fixed in `a0f659ad`.
   - `ae8f05b1` made `ExtensionCommandBridge::is_cancelled` treat a settled
     invocation as not cancelled. When the cancelled command settled before
     its gated read finished, `OperationCaller::is_live` saw no cancel and the
     open committed its document.
   - The bridge now remembers invocations that settled after a cancel. A
     command that settled normally still leaves its requests running, as
     `ae8f05b1` intended.
   - The test's `cancel` scenario now waits for the command to settle before
     releasing the read, which forces the failing order.
2. `interleaving::a_command_runs_an_inline_and_a_cross_package_child_at_once`
   — fixed in `31f137ff`.
   - `RuntimePool::cancel_command` queued a cancel turn only while some
     command execution was active in the isolate. An inline child runs in its
     root's frame without its own execution, so once the root's execution had
     finished (here, the root handled its own cancel first) the child's cancel
     only set a flag nothing read. The child never aborted and the command
     never settled.
   - Logging confirmed it: every failing run had the inline child cancelled
     with no active execution. It failed 8 in 120 runs under load before the
     fix and 0 in 240 after.
   - `cancel_command` now always queues the turn.
     `an_inline_child_is_cancelled_after_its_parent_handler_returns` covers it
     deterministically: a root handler that returns while its inline child
     runs cancels the child after the root's execution is gone.

## Open: explorer flakes under load

- `explorer::a_document_saved_under_a_new_name_is_selected_in_the_explorer`
  (failed on CI; 1 in 120 locally under load) and
  `explorer::rename_selects_the_stem_and_open_documents_follow` (2 in 120
  locally under load, before and after the fixes) time out waiting for tree
  state.
- Both tests predate D182 (`1e88c774` for the first) and involve no
  cancellation. Reading `followMoved`, the watcher refresh, and the tree
  view's `reveal`, `invalidate`, and `apply_response` found no race.
- Next step: log the tree's rows, loading state, and pending reveal when the
  wait times out, and stress the test until it fails.
