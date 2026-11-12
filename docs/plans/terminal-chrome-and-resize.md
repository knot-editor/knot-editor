# Terminal chrome and resize limits

Status: 🔨 checkpoint 1 implemented, awaiting its review gate; checkpoint 2
not started. Implements the terminal usability report (2026-11-12): drop
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

## Checkpoint 1 — Drop the header, add restart command and exited overlay

Deliverable: `TerminalView::render` paints only the terminal grid while the
session runs; when the status is exited, failed, or closed it paints a small
overlay with the status and a restart affordance. New `terminal.restart`
palette command restarts the focused terminal.

Implementation context: `src/app/terminal/view.rs` (`Render` impl ~L688-761,
`TerminalCommand`, `close`/`restart` handlers); register the command beside
`terminal.new`/`terminal.toggle` in `src/app/product/commands.rs` with the
terminal-focus participant pattern used by copy/paste
(`execute_terminal`, view.rs ~L1154). The overlay must not steal grid space
while running — render it only in the non-running states.

Validation: existing terminal view/session/mouse tests
(`src/app/terminal/`, `src/app/product/tests/terminal.rs`); add coverage
that a running terminal renders no header chrome and that restart works
from the command and from the overlay in the exited state.

Review gate: run a terminal, exit the shell, confirm the overlay and the
palette command both revive it; confirm no header while running.

## Checkpoint 2 — Lift the upper resize bounds

Deliverable: a dock (bottom terminal included) drags out to the window edge;
a center split drags until the shrinking side hits its minimum. The 480px
dock cap is gone; the 80px dock minimum, 0.1 split minimum, and 120px drag
minimum remain.

Implementation context: `crates/knot-model/src/workbench/tools.rs`
(`DockLocation::MAX_SIZE`, `set_dock_size` clamp); `crates/knot-model/src/workbench.rs`
(`WorkbenchLayout::clamp_ratio` — keep the lower bound, lift the upper);
`src/app/product/tab_drag.rs` drag limits (~L711-713, keep the shrinking-side
floor, drop the growing-side ceiling); `src/app/product.rs`
(`MIN_DRAGGED_PANE_SIZE`). `area_rects` (`resize.rs`) already clamps docks
to the window, which becomes the effective maximum. Update
`dock_sizes_clamp_and_survive_hide_reopen` and any ratio tests asserting
the old upper clamp.

Validation: model workbench tests (`tools.rs`, `resize.rs`), product
drag/relocate tests; add a test that a dock size above 480 survives
`set_dock_size` and that a split ratio favors the growing pane past 0.9
down to the sibling's minimum.

Review gate: drag a bottom-dock terminal and a side-by-side doc pair to
their new extremes; confirm minimums still hold and layouts restore.

## Verification

`just check` clean (warnings included), focused test suites above green,
`cargo fmt` once at the end. Commit checkpoint 1 before starting
checkpoint 2.
