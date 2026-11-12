# Terminal chrome and resize limits

Status: ✅ checkpoints 1 and 2 complete; checkpoint 3 (restart
keeps output) not started.
Implements the terminal usability report (2026-11-12): drop
the dev mini-header, lift arbitrary resize caps.

## Goal

While a terminal runs, it shows only its grid — no status bar, no
`close`/`restart` text buttons, no `mouse:` indicator. A terminal that is no
longer running still offers a visible way back. Docks and center splits can
be resized up to the window edge; only the minimums stay.

## Agreed choices

- Header: remove the bar entirely. Add a `terminal.restart` palette command
  and show a restart affordance only when the session is exited, failed, or
  closed. Close stays on the tab bar, as today.
- Resize: remove upper bounds only (`DockLocation::MAX_SIZE`, split-ratio and
  drag maximums). Minimums stay: dock `MIN_SIZE` (80px), split `MIN_RATIO`
  (0.1), drag `MIN_DRAGGED_PANE_SIZE` (120px).
- The `mouse: on|off` indicator goes away with the bar. The
  `ToggleMouseReporting` command stays palette-accessible with no visible
  state; mouse reporting itself is unchanged (programs such as vim/tmux
  keep receiving mouse events while it is on).
- Scope: all three docks and center splits (covers dock terminals, center
  terminals, and document panes).

## Consequence of keeping the minimums

Keeping per-pane minimums means a center pane maxes out at ~90% of the
split (or window minus 120px by drag) — it cannot literally fill the window
while the sibling pane exists. Full-bleed by collapsing the sibling is
deferred, not part of this plan.

## Exclusions

- No new keybindings (restart is palette-only).
- No change to `BottomDockLayout` contained/full, maximize, or tab-drag
  relocation.
- No change to mouse-reporting behavior, only to the visibility of its state.

## ✅ Checkpoint 1 — Drop the header, add restart command and exited overlay (complete)

A running terminal paints only its grid; the exited/failed/closed
overlay (status badge, verbatim exit text, restart affordance) renders
only in the non-running states and steals no grid space. `terminal.restart`
is palette-only, reusing the terminal-focus participant pattern; close
stays on the tab bar. Manual review gate passed (exit shell, overlay and
palette command both revive, no header while running).

## ✅ Checkpoint 2 — Lift the upper resize bounds (complete)

Docks drag to the window edge with only the 80px minimum
(`DockLocation::MAX_SIZE` removed; `area_rects` clamps painted docks to
the window). Center-split drags move past 0.9 until the shrinking side
hits its 120px floor via `Workbench::set_split_ratio_bounded`; the ratio
path (keyboard resize, restore) keeps the 0.1 share floor on both sides —
dropping `clamp_ratio`'s upper bound outright would have let keyboard
resize collapse a pane, against the plan's minimums. Model
workbench/resize and product drag/restore suites green; the terminal pty
and sandbox filesystem/watch failures seen during validation reproduce
on the untouched base and are unrelated.

## Checkpoint 3 — Restart keeps prior output visible

Deliverable: pressing restart (overlay or `terminal.restart`) starts a
fresh shell without wiping what was there. The exited session's grid
content remains readable — carried into the new session's scrollback —
so `ls`, Ctrl-D, restart still shows the `ls` output above the new
prompt. New shell output continues below it.

Implementation context: `TerminalSession::restart` (`src/app/terminal/session.rs`
~L467) replaces the grid with a fresh `Term`; `TerminalState::Stopped`
still holds the old grid at that point. Move its visible lines plus
scrollback into the new terminal as scrollback before (or as part of)
the swap. Open design point, settle at implementation: exact carry-over
mechanics (replay lines into the fresh `Term` vs. snapshot handoff).

Validation: a session that prints a marker then exits still contains
the marker in its scrollback after restart; new output appends below.
Existing session/view/mouse tests stay green.

Review gate: run `ls`, Ctrl-D, restart; the `ls` output is still
visible and scrollable above the fresh prompt.

## Verification

`just check` clean (warnings included), focused test suites above green,
`cargo fmt` once at the end. Commit checkpoint 1 before starting
checkpoint 2.
