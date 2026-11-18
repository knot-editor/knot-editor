# D155 — Word and line selection in terminals

Status: 🚧 checkpoint 1 done, checkpoint 2 awaiting review. Roadmap item [D155](../roadmap.md).

## Goal

In a terminal with mouse reporting off (or with Shift forcing local
interaction), a double click selects the word under the pointer and a triple
click selects its line. Dragging after either extends the selection by words
or lines. A single click and drag still select cell by cell.

## Design

- **libghostty owns the gesture.** One selection gesture per terminal view
  (`ghostty_selection_gesture_*`) replaces the view's hand-rolled
  `selection_anchor` and `local_dragged`. Behaviors are single = cell,
  double = word, triple = line, with the emulator's default word separators.
  The emulator decides how a drag extends for each behavior and keeps the
  selection tracked across scrolling as it does today.
- **GPUI counts clicks.** The view passes `MouseDownEvent::click_count` to
  the press. The gesture API takes no count: it counts repeats from press
  times and distances. The press therefore carries a synthetic time that
  repeats exactly when GPUI's count is above one, with the repeat distance
  unbounded, so the emulator needs no clock or distance policy of Knot's
  own.
- **Cell edges follow the emulator.** The gesture reads the pointer's
  position within its cell: a cell joins a drag's selection once the pointer
  passes its middle, and a press and release inside one cell select nothing.
  This is libghostty's rule, adopted rather than kept as the former
  "the cell under the pointer is always included".
- **Boundary.** `knot-terminal` wraps the gesture in a safe type that takes
  viewport cells and the click count and applies the resulting selection
  to the terminal. The view forwards press, drag, and release and notifies;
  it owns no selection policy. Ownership of who gets the press (link
  modifier, application reporting, local) stays in
  [terminal/mouse.rs](../../src/app/terminal/mouse.rs) and `view.rs`.
- **Lifecycle.** Restart, handoff, geometry change, and mode changes already
  invalidate pointer gestures; they also reset the selection gesture.

- **Replayed repeats.** A repeat the view saw while mouse reporting owned the
  press never reached the gesture, so a Shift double click replays the
  earlier presses first. Presses beyond the third keep selecting the line.

## Exclusions

- The output behavior (needs OSC 133, D039), rectangle selection, autoscroll
  tuning beyond what the gesture does by default, configurable word
  separators, and a select-all command.
- Any change to mouse reporting, link activation, or the context menu.

## Checkpoint 1 — Gesture replaces the hand-rolled selection ✅

Approved. Local selection runs through the gesture wrapper in
`crates/knot-terminal/src/grid.rs` (`select_press`, `select_drag`,
`select_release`), owned by `Terminal`; the view in
`src/app/terminal/view.rs` only forwards pointer events. Every click count
still selects by cell. The click-count adapter and the half-cell edge rule
are described under Design.

## Checkpoint 2 — Word and line behaviors 🚧

Deliverable: double click selects a word, triple click a line, drag extends
by that unit; documented.

- ✅ Enable the word and line behaviors and pass the GPUI click count.
- ✅ Validation: `knot-terminal` tests for word selection (default separators,
  paths and URLs) and line selection including soft-wrapped lines;
  `mouse_tests.rs` cases for double and triple click, drag extension in
  both directions, no selection start while application reporting owns the
  press, and Shift forcing local selection.
- ✅ Docs: update the selection sentences in
  [workbench and lifecycle](../architecture/workbench-and-lifecycle.md)
  (terminal selection and mouse paragraphs) and remove this plan and the
  roadmap entry once approved.
- Review gate: ✅ `just check`; ✅ a `try.*` command in the personal
  `post-init.js` opens the mouse fixture terminal so the user can double-
  and triple-click, then drag, on its output.

**Implementation context.** Same files as checkpoint 1, plus
`docs/architecture/workbench-and-lifecycle.md` (~lines 583 and 604) and
`scripts/terminal-mouse-fixture.py` for manual acceptance; personal config
location is `user_config_root` in `src/app/personal_config.rs`.

## Deferred

None beyond the exclusions.
