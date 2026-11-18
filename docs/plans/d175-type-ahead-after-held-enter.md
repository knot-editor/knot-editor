# D175 — Type-ahead after a held Enter

Status: 🚧 checkpoint 1 implemented, awaiting review. Source: D175 in the [roadmap](../roadmap.md).

## Outcome

Keys typed after accepting an extension command's picker or input are never
lost and never land in the wrong view. Typing `foo⏎bar` in a file picker
whose rows lag behind the query opens the file and types `bar` into it.
Running a command from the palette queues later keys the way a key
binding does.

Today, Enter on supplied rows that do not answer the field sets
`confirm_pending` and the picker drops every key until the rows arrive.
After the picker confirms, focus returns to the previous editor while the
command is still opening its result. No hold is active then, so typing goes
into the old editor. The palette dispatches without a hold.

## Design

Builds on [ordered delivery](../architecture/commands.md#ordered-delivery)
(`src/app/product/key_hold.rs`): the window queues key-downs and held-editor
text, then replays them in order on release.

- **Accepting starts a hold.** When an extension-owned picker or input is
  accepted (Enter on a picker, including the pending-rows case, or an
  accepted input), the window starts a key hold. It lasts until the command
  invocation that owns the interaction (`TransientOwner.invocation`) settles,
  including its child invocations. Interactions without an invocation, and
  native pickers, start no hold, because their effect runs in the same turn.
- **Pending rows.** During the pending-rows wait, the hold queues keys at the
  window, so the picker no longer swallows them. Keys typed while the picker
  waits are replayed after the command settles, not into the frozen field.
- **Held view.** The hold's focus target is the view the picker restores
  focus to, not the picker itself. A command that opens a file moves focus to
  the new tab. The next key then releases the hold under the existing
  focus-left rule, and the queued keys replay into the opened view. If the
  command settles first, the hold releases then.
- **Escape while pending** goes through to the picker: it dismisses the
  picker and discards the queued keys rather than replaying them into the
  restored view.
- **Timeout.** The existing one-second release still applies, counted from
  Enter.
- **Palette.** A palette entry that dispatches an extension command holds
  later keys until that command settles, exactly as a key binding does.
  The existing release when a picker, input, or the palette takes focus keeps
  a command that opens a picker from holding the picker's typing.
- **Settle notification.** `ExtensionCommandBridge`
  (`src/app/commands/extension.rs`) lets a caller wait for an invocation's
  root to settle. The window uses this to release, since the oneshot
  completion returned by `dispatch` belongs to the original dispatcher.

## Exclusions

- No hold for native pickers or prompts, and no change to the one-second
  timeout.
- No hold across extension calls that are not commands (listeners, timers,
  providers). Only command invocations own interactions that take focus.

## Checkpoints

### 1. Hold after an accepted interaction — 🔍 awaiting review

Deliverables:

- ✅ A settle watcher on `ExtensionCommandBridge` for an invocation's root.
- ✅ On an accepted extension picker or input with an invocation, the window
  starts a hold that targets the restored view and releases when the root
  settles or the timeout passes.
- ✅ The pending-rows Enter starts the hold immediately (`PickerEvent::Pending`),
  with the waiting picker as the held view, retargeted to the restored view
  on confirm. Escape while pending dismisses the picker and discards the
  queue. The picker keeps freezing its field while pending, as a guard for
  input that does not pass through the window's key path.
- ✅ Updated [ordered delivery](../architecture/commands.md#ordered-delivery)
  and the supplied-picker paragraph of
  [semantic UI](../architecture/semantic-ui.md); the decision sentence still
  holds.

Validation:

- ✅ Product tests in `src/app/product/tests/key_hold.rs`, driven by a test
  extension command that shows a supplied picker and then opens a document:
  - keys typed after Enter on current rows reach the opened document, not
    the previous editor;
  - keys typed after Enter on pending rows reach the opened document in order;
  - Escape while pending dismisses the picker, and the queued keys are
    discarded;
  - a command that settles without moving focus replays into the restored
    view;
  - the timeout releases a stuck command.

  The restored-view and timeout cases use an input, whose command's clipboard
  write the test holds.
- ✅ Update the picker unit test
  `enter_on_rows_for_older_text_waits_for_the_answer` to the new
  responsibility split. The picker still confirms on the answer, and the
  window owns the queued keys.

Implementation context:

- `src/app/product.rs`: `begin_picker` / `begin_input` subscriptions and
  `finish_transient` (around lines 1541 and 1700–1830). Accepted outcomes
  carry `ActiveTransient.owner` and `restore_focus`. Start the hold in
  `finish_transient` after restoring focus, or at the pending Enter. The
  picker can emit a new `PickerEvent` when it enters the pending state, so
  the window starts the hold before the next key.
- `src/app/product/key_hold.rs`: `start_key_hold` captures
  `focus_target(cx)`. Add a variant that takes the focus target explicitly
  and a release that awaits the bridge's settle watcher.
  `release_keys_for_focus` and `hold_key` must not release when focus is
  still on the pending picker. Escape on a pending picker bypasses the queue
  and drops it.
- `src/app/picker.rs`: `confirm_pending`, `key_down`, `insert_text`. Pending
  keeps freezing tabs and confirming on the answer, but key filtering moves to
  the window.
- `src/app/commands/extension.rs`: `InvocationNode`, `settle`, `root_of`. Add
  watchers keyed by root and fired from `settle`.
- Mirror the existing hold tests and fixture in
  `src/app/product/tests/key_hold.rs`.

Review gate: ✅ `just check`. In the real app, run **Try: Slow picker, then
open a document** (cmd-alt-p) and **Try: Input, then open a document**
(cmd-alt-i). Type ahead through Enter and confirm the text lands in the new
document. In the slow picker, type, press Enter, type, then Escape before the
rows arrive (700 ms) and confirm the typing is discarded.

### 2. Palette dispatches hold

Deliverables:

- Confirming a palette entry for an extension command holds later keys until
  it settles, after focus returns to the origin.
- Update [ordered delivery](../architecture/commands.md#ordered-delivery) to
  say palette dispatches hold too.

Validation:

- Product test: run an extension command from the palette that edits the
  origin editor, type immediately, and assert the typing lands after the
  command's edit.
- Product test: a palette command that opens a picker releases the hold
  when the picker takes focus, and typing reaches the picker.

Implementation context:

- `src/app/product.rs`: the `open_command_palette_from` subscription calls
  `dispatch_command`, which discards the execution. Hold on
  `execution.completion` for extension commands, as
  `src/app/product/key_routing.rs` does around line 392 (`native` check and
  `hold_keys_until`).

Review gate: run `just check`. Manual check of a palette command followed by
immediate typing.

## Deferred

Nothing.
