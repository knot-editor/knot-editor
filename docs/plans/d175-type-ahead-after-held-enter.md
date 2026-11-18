# D175 — Type-ahead after a held Enter

Status: 🚧 checkpoint 1 done; checkpoint 2 awaiting manual review. Source: D175 in the [roadmap](../roadmap.md).

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
- **Timeout.** The existing one-second release still applies. For pending
  rows it starts when the picker confirms, so rows slower than a second do
  not lose the keys; Escape still leaves a picker that never answers.
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

### 1. Hold after an accepted interaction — ✅ done

Accepting an extension picker or input holds keys until the owning command
tree settles, through a root settle watcher on `ExtensionCommandBridge`.
Enter on pending rows holds from that Enter (`PickerEvent::Pending`), and
Escape there drops the held keys; that hold's timeout starts when the picker
confirms. See
[ordered delivery](../architecture/commands.md#ordered-delivery). The picker
still freezes its field while pending, as a guard for input that bypasses the
window's key path. Covered by product tests in
`src/app/product/tests/key_hold.rs`. Manually checked in the app: keys typed
after Enter on current rows, and on rows arriving two seconds late, land in the
opened document. The Escape case was checked only by the tests.

### 2. Palette dispatches hold — 🔍 awaiting manual review

Deliverables:

- ✅ Confirming a palette entry for an extension command holds later keys until
  it settles, after focus returns to the origin
  (`KnotWindow::dispatch_holding_keys`, shared with key dispatch's native check).
- ✅ Update [ordered delivery](../architecture/commands.md#ordered-delivery) to
  say palette dispatches hold too.

Validation:

- ✅ Product test: run an extension command from the palette that edits the
  origin editor, type immediately, and assert the typing lands after the
  command's edit.
- ✅ Product test: a palette command that opens an input releases the hold
  when the input takes focus, and typing reaches the input.
- The palette key opens Search Everywhere, whose chosen action runs as a child
  of its own command, so checkpoint 1's accepted-picker hold already covers it;
  this checkpoint covers the fallback palette shown without that package.

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
