# Dock lanes

Status: 🚧 checkpoint 1 implemented, awaiting its review gate. Checkpoint 5's
icons come from the completed icon packs (D055).

## Goal

A window is a **center** area of documents surrounded by **left, right, and
bottom docks** for tools and terminals, as in IntelliJ, VS Code, and Zed.
Every surface has a home, so "toggle X" commands work: Toggle Terminal
shows, focuses, or hides the terminals wherever the user keeps them, and a
dock can be hidden and shown whole, keeping its arrangement. Today's single
free-form split tree, where docks are groups among document panes, goes away.

## Design

- **Areas.** The workbench holds a center split tree and three dock split
  trees, using the same split, pane, and tab types. Docks are not nodes of the
  center tree. Each dock has an absolute size, a visible flag, and its own
  focused-pane memory. The bottom dock sits under the center, between the side
  docks; this is fixed. The center may be empty; the placeholder untitled
  document is gone everywhere (final-tab closure, project switch, fresh
  window), and an empty center paints a blank surface that accepts opened
  documents and drops.
- **Docks split.** A dock pane splits like any pane, in either direction, so
  two tools can stack in the left dock or two terminals sit side by side in
  the bottom one.
- **Per-kind placement.** Each tab kind declares the areas it may occupy and,
  for docks, its default dock: documents → center only; tools → docks only,
  default from their definition (`ViewPlacement.location`, explorer left);
  terminals → center and docks, default dock **right**. Placement, drops,
  move commands, restore, and pruning all check the kind's declaration rather
  than naming kinds.
- **Remembered home.** When the user moves a tool or a terminal tab to another
  dock, that dock becomes the tool's, or the terminal kind's, home. Homes live
  in tool-layout memory, so fresh windows inherit them; Reset Layout returns to
  declared defaults. Only explicit user moves publish, as today.
- **Flat terminal tabs.** Terminals in a dock are ordinary tabs; there is no
  terminal panel and no inner tab strip.
- **Toggles.**
  - *Toggle Tool* (Toggle Explorer, a Toggle Tool… picker, stripe clicks)
    hides just that tool; its pane collapses as any empty pane does, and the
    dock hides when no visible tab remains. Showing it returns it to its last
    pane and position, else its home dock.
  - *Toggle Left / Right / Bottom Dock* hides or shows the whole dock with its
    arrangement.
  - Toggle commands follow one pattern: not visible → show and focus; visible
    but unfocused → focus; focused → hide and return focus to the last center
    pane.
- **Terminal commands.**
  - *Toggle Terminal* (`ctrl-``): with no dock terminal, create one in the
    terminal home dock; otherwise show and focus the most recently used dock
    terminal, or hide its dock when it is focused. Center terminals are
    ignored.
  - *New Terminal*: from a terminal, a new tab beside it in its directory, as
    today; otherwise beside the most recently used dock terminal, else in the
    home dock, showing that dock.
  - *New Terminal in Center*: a terminal tab in the last focused center pane.
  - `terminals.open` gains `location?: "dock" | "center"`; omitted, it follows
    New Terminal.
- **Moving between areas.** Generic *Move Tab to Left Dock / Right Dock /
  Bottom Dock / Center*, each offered only when the tab's kind allows that
  area, replace Move Tool to Dock…. Dragging follows the same rule: workbench
  edges and dock panes accept dock-capable kinds, the center accepts
  center-capable kinds, and anything else rejects the drop.
- **Navigation scope.** Directional focus and moves cross between areas by
  laid-out rectangles over visible docks. Cycling, first/last pane, Close
  Other Panes, and Equalize stay within the focused pane's area. Vim's
  `ctrl-w` keys stay in the center, which resolves D158.
- **Maximize.** Toggle Maximize Pane applies to any focused pane, center or
  dock; the pane fills the whole workbench. Hiding a dock while one of its
  panes is maximized remembers it, and showing that dock again restores the
  maximize, as VS Code's `preserve` and Zed's zoom do. Focusing another pane by
  any other means ends maximize, as today. Maximize stays unrecorded.
- **Tool stripes.** IntelliJ-style thin stripes on the left and right window
  edges show a button for every tool whose home is that side, visible or
  hidden, with visible ones highlighted; bottom-dock tools' buttons sit at the
  bottom end of the left stripe. The terminal kind has a button that runs
  Toggle Terminal. Click toggles; dragging a button to another stripe moves the
  tool and rehomes it. `workbench.toolStripes` turns stripes off. Buttons use
  icons from the icon pack plan; a tool without one shows its title's initial.

## Exclusions

- Floating or undocked tool windows, and a top dock.
- IntelliJ's overlay ("undock") mode, where a tool slides over the center
  without resizing it: deferred to the roadmap.
- A start surface in the empty center: stays D061, reworded for an empty
  center.
- Icon formats, packs, and extension-supplied icons: owned by the icon pack
  plan.

## Deferred decisions

- Whether an empty dock pane survives as an empty drop target. Empty panes
  collapse, as today, until use says otherwise.
- Default keys for the dock toggles and New Terminal in Center, beyond
  `ctrl-`` for Toggle Terminal; chosen at checkpoint 3 review against the
  existing presets.

## Checkpoints

### 1. ✅ Center and dock areas

Done and approved. The workbench is a center tree plus three dock trees, each
tab kind declares the areas it allows (`TabKind::allows`), the center may be
empty, and window layouts and tool-layout memory are version 2; see
[workbench](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure) and
`crates/knot-model/src/workbench.rs`. Choices made along the way:

- An empty window has no focused pane, so command and open targets carry an
  optional tab, and window and quit closes plan from the session
  (`closure::CloseOrigin`).
- Focus falls back to the pane's own area, then the center, else the empty
  center; docks never take focus by fallback (review: cmd-w in a fresh window
  closed a tool).
- Tools attaching one by one rebuild a stored dock tree while the dock still
  matches the record (`Workbench::restore_stored_tool`).
- Vim's `ctrl-w` keys pass `{ withinArea: true }`; D158 was removed and D061
  reworded with this checkpoint.

Carried forward: drop targets still preview disallowed drops and refuse them
on release (checkpoint 2); the workbench decisions still describe tools as
dock groups of one split tree (checkpoint 6).

### 2. ⏳ Dock interactions and homes

Docks become directly manageable: whole-dock toggles, the shared toggle
pattern, remembered homes, move-to-area commands, and drag targets by kind.

- **Deliverables:** Toggle Left / Right / Bottom Dock; Toggle Explorer and a
  Toggle Tool… picker on the show/focus/hide pattern; tool hide restoring last
  pane and position, else home; remembered homes published on explicit moves
  and inherited by fresh windows; Move Tab to Left Dock / Right Dock / Bottom
  Dock / Center replacing Move Tool to Dock…; drag drop targets by allowed
  area; Reset Layout to declared defaults.
- **Validation:** product tests for each toggle state transition and focus
  return, hide/show restoring position, homes published only by explicit moves
  and inherited by a fresh window, move commands availability per kind, drop
  rejection of disallowed kinds (`product/tab_drag/tests.rs`).
- **Review gate:** try commands that move tools between docks, toggle docks,
  and open a fresh window to check inherited homes.
- **Implementation context:** commands in `src/app/product/commands.rs`
  (`toggle-explorer`, `toggle-bottom-dock`, `move-tool-to-dock`,
  `reset-layout`), `product/move_tool.rs`, `product/tools.rs`;
  drop targets in `product/tab_drag.rs`; homes in `src/app/tool_layout.rs`
  beside the existing publish rules; keymap presets in
  `src/app/commands/keymaps/presets.rs`.

### 3. ⏳ Terminals in docks

Terminals live in docks by default, flat, with their own home.

- **Deliverables:** terminal kind allowed in center and docks with default dock
  right; terminal home in tool-layout memory; Toggle Terminal on `ctrl-``,
  New Terminal, New Terminal in Center; `terminals.open` `location`; dock
  terminals restored by window layouts while fresh windows inherit only the
  home; splitting a dock terminal starting a new session.
- **Validation:** product tests in `src/app/product/tests/terminal.rs` for each
  Toggle Terminal state, New Terminal placement from a terminal, a tool, and
  the center, home change on move, restore of dock terminals; keymap tests;
  an extension package test for `terminals.open` with both locations.
- **Review gate:** try commands that toggle the terminal, open several,
  split one, move one to the bottom dock and to the center, and open a fresh
  window.
- **Implementation context:** `NEW_TERMINAL_COMMAND` and its peers in
  `src/app/product/commands.rs`; terminal creation and directory rule in
  `src/app/terminal/directory.rs` (unchanged rule, new destination);
  `terminals.open` handler in `src/app/extensions/host/handlers.rs` and its
  declaration in `knot.d.ts`; `ctrl-`` in
  `src/app/commands/keymaps/presets.rs` and `keymaps/tests.rs`.

### 4. ⏳ Maximize across areas

- **Deliverables:** Toggle Maximize Pane for dock panes; maximize remembered
  per dock across hide and restored on show by any route.
- **Validation:** product tests in `src/app/product/tests/navigation.rs` for
  maximizing a dock pane, hiding and reshowing it maximized via Toggle
  Terminal and dock toggle, and maximize ending on other focus changes.
- **Review gate:** try command maximizing a dock terminal and toggling it.
- **Implementation context:** maximize state in `product/navigation.rs` and
  its painting in `product/render.rs`.

### 5. ⏳ Tool stripes (blocked on the icon pack plan)

- **Deliverables:** left and right stripes with buttons per home, highlight
  for visible tools, bottom-dock group on the left stripe, terminal button,
  click to toggle, drag between stripes to rehome, `workbench.toolStripes`
  setting, initial-letter fallback.
- **Validation:** product tests for stripe contents by home and visibility,
  click toggling, and rehoming by stripe drag; settings test for the switch.
- **Review gate:** try the stripes with explorer, contributed tools, and
  terminals.
- **Implementation context:** rendering beside the workbench in
  `product/render.rs`; drag through `product/tab_drag.rs`'s coordinator;
  setting registered with the native settings and listed in
  `docs/settings.md`; icon lookup per the icon pack plan.

### 6. ⏳ Documentation and completion

- **Deliverables:** `docs/architecture/workbench-and-lifecycle.md` (workbench,
  layouts, tool-layout memory, relocation, terminals) and
  `docs/architecture.md` ownership sentences describing areas; decisions in
  `docs/decisions/workbench-and-lifecycle.md` replacing "tools are tabs in dock
  groups of the same split tree" and the `ctrl-`` rationale, recording rejected
  free-form docks, nested terminal panel, floating tools, and the maximize
  preserve rule, with the index updated; `docs/extension-api.md` placement and
  terminal sections; roadmap: add the overlay mode as deferred (D158 removal
  and the D061 rewording landed with checkpoint 1); remove this plan.
- **Validation:** `review-docs`-style pass against the code.
- **Review gate:** documentation review.

Each checkpoint keeps the affected architecture sections current as it lands;
checkpoint 6 finishes the decisions and roadmap and retires the plan.
