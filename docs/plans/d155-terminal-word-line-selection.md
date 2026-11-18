# D155 — Word and line selection in terminals

Status: 🚧 checkpoint 1 awaiting review. Roadmap item [D155](../roadmap.md).

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

## Exclusions

- The output behavior (needs OSC 133, D039), rectangle selection, autoscroll
  tuning beyond what the gesture does by default, configurable word
  separators, and a select-all command.
- Any change to mouse reporting, link activation, or the context menu.

## Checkpoint 1 — Gesture replaces the hand-rolled selection ✅

Deliverable: single click and drag select exactly as before, through the
gesture, with the cell behavior for every click count.

- ✅ Spike first: confirm the event options, position and viewport semantics,
  and the click-count input against the pinned libghostty, and note any
  finding in this plan.
- ✅ Safe wrapper in `knot-terminal`; remove `start_selection`/`extend_selection`
  state from the view.
- ✅ Validation: `knot-terminal` tests for press/drag/release selecting the
  expected text, including a drag across scrollback; existing
  `mouse_tests.rs` selection tests pass (shift-forced local, wide glyphs,
  reporting toggle), with drag ends moved past the cell middle.
- Review gate: ✅ `just check`; confirm in the real app that click-drag
  selection feels right with the half-cell rule.

**Implementation context.** `crates/knot-terminal/src/grid.rs` (selection
helpers: `select`, `extend_selection`, `grid_ref`, `set_selection`) is where
the wrapper lives next to `Terminal`; bindings are already in
`crates/knot-terminal/src/ffi.rs` (`ghostty_selection_gesture_*`). View
state is `selection_anchor`/`local_dragged` in `src/app/terminal/view.rs`
(`start_selection`, `extend_selection`, `mouse_down`, `mouse_up`, reset
around line 321). Tests mirror `src/app/terminal/mouse_tests.rs`. The
gesture must be freed with the terminal (see how `Terminal` frees other FFI
objects).

## Checkpoint 2 — Word and line behaviors ⬜

Deliverable: double click selects a word, triple click a line, drag extends
by that unit; documented.

- Enable the word and line behaviors and pass the GPUI click count.
- Validation: `knot-terminal` tests for word selection (default separators,
  paths and URLs) and line selection including soft-wrapped lines;
  `mouse_tests.rs` cases for double and triple click, drag extension in
  both directions, no selection start while application reporting owns the
  press, and Shift forcing local selection.
- Docs: update the selection sentences in
  [workbench and lifecycle](../architecture/workbench-and-lifecycle.md)
  (terminal selection and mouse paragraphs) and remove this plan and the
  roadmap entry when done.
- Review gate: `just check`; a `try.*` command in the personal
  `post-init.js` opens the mouse fixture terminal so the user can double-
  and triple-click, then drag, on its output.

**Implementation context.** Same files as checkpoint 1, plus
`docs/architecture/workbench-and-lifecycle.md` (~lines 583 and 604) and
`scripts/terminal-mouse-fixture.py` for manual acceptance; personal config
location is `user_config_root` in `src/app/personal_config.rs`.

## Deferred

None beyond the exclusions.
