# Dock lanes

Status: 🚧 checkpoints 1 to 5 done.

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
  docks, or across the full width under them with
  `workbench.bottomDockLayout`, as Zed's `bottom_dock_layout` and VS Code's
  `panel.alignment` allow. The center may be empty; the placeholder untitled
  document is gone everywhere (final-tab closure, project switch, fresh
  window), and an empty center paints a blank surface that accepts opened
  documents and drops.
- **Docks split.** A dock pane splits like any pane, in either direction, so
  two tools can stack in the left dock or two terminals sit side by side in
  the bottom one.
- **Per-kind placement.** Each tab kind declares the areas it may occupy and,
  for docks, its default dock: documents → center only; tools → docks only,
  default from their definition (`ViewPlacement.location`, explorer left);
  terminals → center and docks, default dock **bottom**. Placement, drops,
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
    pane and position, else its home dock, replacing the other tools in that
    pane (terminals stay); a stripe button's right-click menu offers Show
    Beside, which splits it into a pane of its own, as IntelliJ's split slots
    and VS Code's stacked views do.
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

Carried forward: the workbench decisions still describe tools as dock groups
of one split tree (checkpoint 6).

### 2. ✅ Dock interactions and homes

Done and approved. Docks and tools toggle on one show/focus/hide pattern
(`Workbench::toggle_dock`, `Workbench::toggle_tool`), Move Tab to Left / Right
/ Bottom Dock / Center move tabs through `TabMoveDestination::Area`, and drags
offer targets only where `TabKind::allows` the area; see
[workbench](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).
Choices made along the way:

- A tool's home is the dock it is in or last left, which the tool-only record
  already carries to fresh windows; only the terminal home (checkpoint 3)
  needs a new field.
- The palette has no per-target availability, so move commands a tab's kind
  or current area rules out return Unavailable.
- Toggle Explorer now focuses the explorer it shows.
- Review follow-ups: Close Tab with no focused tab closes the window, and
  `workbench.bottomDockLayout` (default `contained`) offers a full-width
  bottom dock.

### 3. ✅ Terminals in docks

Done and approved. Terminals are allowed in the center and docks with a
remembered home (tool-layout record version 3), Toggle Terminal sits on
`ctrl-``, and New Terminal, New Terminal in Center, and `terminals.open`'s
`location` place them; see
[workbench](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure)
and `crates/knot-model/src/workbench/terminals.rs`. Choices made along the way:

- The workbench ranks dock terminals by recent focus
  (`Workbench::recent_dock_terminal`); Toggle Terminal and New Terminal use it.
- Rebuilding a dock from the tool record (Reset Layout, `arrange_dock`) moves
  its terminals to the center; tools attaching later join a dock that holds
  terminals instead of rebuilding it.
- Moving a terminal into a dock within one window sets the home and publishes;
  a cross-window drop does not.
- Toggle Terminal publishes the layout, as the dock toggles do.

### 4. ✅ Maximize across areas

Done and approved. Dock panes maximize like any pane, and the shell remembers
a dock's maximized pane while the dock is hidden, restoring it when the dock is
shown with that pane focused (`ProductShell::maximized_pane`); see
[workbench](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

### 5. ✅ Tool stripes

Done and approved. Stripes list a button per tool by home dock plus the
terminal button, toggle on click, and rehome by drag or the button's right-click
menu (`Workbench::stripes`, `product/stripes.rs`); see
[workbench](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure)
and [icons](../architecture/semantic-ui.md#icons). Choices made along the way:

- Showing a hidden tool from a toggle replaces the other tools in its pane,
  as in IntelliJ, VS Code, and Zed, whose switchers never stack tabs; the menu's
  Show Beside splits it into a pane of its own. Terminals, tools already
  showing, programmatic shows, and dock toggles keep adding tabs.
- Moving the terminal button moves the dock terminals with it; the terminal
  home defaults to the bottom dock (`DEFAULT_TERMINAL_HOME`).
- The drag uses gpui's in-window drag and drop, not the cross-window tab
  coordinator, since a button never leaves its window.

### 6. ⏳ Documentation and completion

- **Deliverables:** `docs/architecture/workbench-and-lifecycle.md` (workbench,
  layouts, tool-layout memory, relocation, terminals) and
  `docs/architecture.md` ownership sentences describing areas; decisions in
  `docs/decisions/workbench-and-lifecycle.md` replacing "tools are tabs in dock
  groups of the same split tree" and the `ctrl-`` rationale, recording rejected
  free-form docks, tabbed stripe switching, nested terminal panel, floating tools, and the maximize
  preserve rule, with the index updated; `docs/extension-api.md` placement and
  terminal sections; roadmap: add the overlay mode as deferred (D158 removal
  and the D061 rewording landed with checkpoint 1); remove this plan.
- **Validation:** `review-docs`-style pass against the code.
- **Review gate:** documentation review.

Each checkpoint keeps the affected architecture sections current as it lands;
checkpoint 6 finishes the decisions and roadmap and retires the plan.
