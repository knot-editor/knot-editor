# D181 — Native operation failures as notices

Status: 🚧 approved, not started. Source: [roadmap D181](../roadmap.md).

## Outcome

When a native operation fails and nothing else on screen shows it (Save, Open,
Reopen, Add Folder, recovery, Restart Extension, tab relocation, shell
commands), Knot raises a notice instead of a native OK-only prompt. The popup no
longer interrupts typing, and the Notifications panel keeps the notice after it
is dismissed. Prompts remain only where the user must choose something: dirty
close, save conflict, file changed outside Knot, File or Folder, Restore or
Discard, and confirmations.

## Design

- One window helper replaces `KnotWindow::show_notice(PromptLevel, title,
  detail)`, plus a free function for call sites that hold only a native window,
  such as `start_open_resource` and `run_shell_command`. It raises a
  `NoticeSpec` through `notifications::raise` with `source: "Knot"` and the
  window's `window_id` as `target`, so the popup shows in the window where the
  operation ran. The title stays the title and the detail becomes the message;
  when there is no detail, the title becomes the message and the notice has no
  title. The OK-only prompt helper `product::show_notice` is deleted.
- Severity, using the store's default `auto_dismiss` (only `info` hides on its
  own):
  - `error` for failed operations: Save failed, Open failed, Reopen failed,
    Cannot add folder, Discard failed, Recovering X failed, X failed to
    restart, Could not create destination window, and shell command failures.
  - `warning` for something unavailable: Recovery is unavailable, Extensions
    are unavailable, and the startup "N unsaved documents can be recovered".
  - `info` for "No unsaved documents to recover" and "Restarted X". A restart
    has no other visible effect, so this success notice stays.
- Actions appear only where the next step is obvious:
  - The startup recovery notice gets **Recover…**, which runs `file.recover`.
  - Save failure notices get **Save As…**, which runs `file.save-as`.
- Extension command outcomes do not change. `HandlerFailure` still carries the
  message, so a package that runs Save still gets the failure as a result.

## Exclusions

- No change to the prompts that ask for a decision.
- No "Open Log" action and no new notice API. The notification store, popups,
  and panel stay as they are.

## Checkpoint 1 — Failures raise notices

Deliverables:

- The helper, with every OK-only prompt site converted using the severity and
  actions above.
- Product tests that answer `"OK"` to a failure prompt now assert the raised
  notice: its severity, title, message, target, and actions. Add a small test
  helper that reads the store's latest notice. Add a test for each kind of
  site: a Save failure with its Save As action, an Open failure, the startup
  recovery notice with its Recover action, and the info "No unsaved documents
  to recover".
- Docs: rewrite the
  [product window chrome and failure notices](../decisions/platform.md#product-window-chrome-and-failure-notices)
  decision and its `decisions.md` index line to say that silent native failures
  are notices. Update the notifications section of
  [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#notifications)
  and the recovery notice in
  [documents and persistence](../architecture/documents-and-persistence.md).

**Implementation context:**

- Helper: `src/app/product.rs`. Replace `KnotWindow::show_notice` (around line
  2248) and the free `show_notice` (around line 2300). Model it on
  `notifications::raise_config_warning` in `src/app/notifications.rs` for how
  to build a spec with actions (`StatusCommand` with `Null` arguments). Get the
  target from `window_id(entity_id)`.
- Call sites in `src/app/product/`: `save.rs` (99, 158, 276), `open.rs` (160,
  186, 284; the File or Folder prompt at 40 stays), `reopen.rs:116`,
  `roots.rs` (21, 48), `recovery_listing.rs` (31, 42, 109, 136, and the
  `notice` helper at 159; the Restore or Discard prompt stays),
  `extension_restart.rs` (18, 64), `relocate.rs:191`, `launch.rs` (78, 502;
  `recovery_listing::startup_notice` feeds both), and `commands.rs:698`
  (`run_shell_command`). Leftover `PromptLevel` imports go away. Also drop the
  stale "Status line for a window…" doc line above `startup_notice`.
- Tests: `src/app/product/tests/{recovery,persistence,extension_restart}.rs`
  use `simulate_prompt_answer("OK")`; `src/app/product/tests/notifications.rs`
  shows how to read the store.

Validation: `just check`, then the product tests named above.

Review gate: a manual check in the real app using `Try:` commands from
`post-init.js`: a failing Save, a failing Open, and Recover with nothing to
recover. The startup recovery notice can only be checked by hand.
