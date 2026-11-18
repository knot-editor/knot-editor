# D203 — Tool windows, as IntelliJ

Status: 🚧 approved, not started. Source: [roadmap D203](../roadmap.md).

## Goal

Copy IntelliJ's tool-window model. A comparison with VS Code and Zed comes
after this lands, not before.

- Each window has six dock **slots**: left top and bottom, right top and
  bottom, bottom left and right. A slot shows at most one tool window.
- A **stripe click** on a hidden tool window shows and focuses it in its slot
  and hides the one that was there, which keeps its tabs and their views, so
  shells keep running. A click on a visible tool window hides it.
- A **tool window** is the same split tree of tabbed panes the center uses.
  Its bold title sits at the start of its first pane's strip, and its actions,
  options menu (⋮), and hide button sit at the end. Double-clicking the header
  maximizes the window. The options menu holds Move to (six slots), Resize
  (the Stretch commands), and Remove from Sidebar; hiding is the header's `–`
  button.
- Tool windows come in two forms:
  - A **fixed** tool window holds the single tree or text tab it registered
    with (Explorer, Projects, Notifications). It shows no tab strip, just the
    title row.
  - A **container** tool window holds tabs opened into it (Terminal, later
    agents). Every pane shows its tab strip, even with one tab, and a `+` button
    that runs the container's new-tab command. That command also runs when
    the window is shown empty. Closing or moving out its last tab hides it.
    Split Right/Down opens a new tab in a new pane beside the current one, with
    its own strip; Split and Move Right/Down moves the current tab there;
    Unsplit merges the panes back. A pane whose last tab closes or moves out
    goes away. Go to Next/Previous Splitter moves focus between the panes.
- The **center** is the editor area: the same model. A terminal moves between
  the center and its own tool window in both directions: Open as Editor Tab on
  a terminal tab (Move to Editor in the header menu) or a drag to the center
  sends it there, and Return to Tool Window on its editor tab or a drag onto
  its window brings it back.
- Moving a tool window is a stripe-button drag between the six stripe groups,
  or the button menu's "Move to" one of the six slots. The left stripe shows left
  top, a separator, left bottom, and bottom left at its lower end; the right
  stripe mirrors it. A tool removed from the sidebar has no button. The
  "More tool windows" (⋯) button, on the left stripe only, lists such tools;
  picking one shows it in its slot and puts its button back on that slot's
  stripe group. A stripe button's menu offers Move to, Hide, and Remove from
  Sidebar.
- **Esc** in a tool window, a terminal included, focuses the editor and leaves
  the tool window shown. `terminal.escapeToEditor` (default true) sends Esc to
  the shell instead, as IntelliJ's setting does.
- **Default slots:** Explorer and Projects (hidden) left top, Terminal
  bottom left, Notifications right top.
- **Terminal** is a container tool window that `@knot/chrome` registers,
  with New Terminal as its new-tab command. The engine, restore, and split stay
  native.
- **Notifications** live in one place: a fixed tool window whose stripe
  button shows an unread dot, blue for ordinary notices and red when an
  unread one is an error. The status bell goes away.
- **Commands** follow IntelliJ: Toggle <Tool> for each tool, Hide Active Tool
  Window, Jump to Last Tool Window, Hide All Tool Windows, Maximize Tool
  Window, Stretch to Left/Right/Top/Bottom, Move to (slot), Open as Editor
  Tab, Return to Tool Window, the split commands above, and Rename Session.
  Hide All Tool Windows run again restores the windows it hid. Hide Active
  Tool Window from the editor hides the most recently active visible tool
  window.

## Design and boundaries

- `knot_model::workbench` owns slots, tool windows, and each tool window's
  split tree. The pane/tab model, split tree, relocation, closure, and recent
  order are reused as they are, now scoped per tool window instead of per
  dock. Panes keep stable identities, so focus, `CommandView`, and captured
  targets are unchanged.
- Each tool window remembers its own size across its side, as IntelliJ does.
  A side's extent is the size of the window in its first slot (top, or left
  for the bottom side), else of its second slot. Resizing the side sets that
  window's size. Each tool window also remembers its share of its side when
  both slots show; the side splits by the share of the window in its first
  slot. The Stretch commands resize the active tool window's side.
  `workbench.bottomDockLayout` stays.
- Placement rules:
  - Documents live only in the center.
  - A fixed tool's tab lives only in its own window.
  - A tab opened into a container belongs to it: it lives in that window or
    in the center, as an IntelliJ terminal lives in Terminal or the editor.
    Any other tool window offers no drop target. Move to Editor sends it to
    the center, and dragging it onto any of its window's strips brings it
    back.
- Tabs are labelled natively: a terminal is named after its shell (`zsh`),
  with the lowest free ` (N)` suffix from 2 when the name is taken in its
  window, and reads the same in the tool window and the center. Rename Session
  sets a name, and names persist across restarts.
- Native code registers a `<viewId>.toggle` command, titled "Toggle <Title>",
  for every registered tool and removes it when the tool goes. It uses the
  usual toggle pattern: show and focus, focus, or hide when focused. ctrl-`
  binds to `knot.terminal.toggle`.
- Window layouts and the tool-layout record describe slots, tool windows,
  each tool window's size, side share, and tree, and which tools are off the
  sidebar. A terminal tab still records its directory and name and restores
  natively. Layout entries in a container that has not registered yet wait for
  it, as late-registering tools already do. Both records bump their version,
  and old records are ignored.

### Extension API changes

- `registerView`:
  - `content` becomes optional. Without it the view is a container.
  - A container may set `newTab`, a command ID that `+` and an empty show run.
  - `placement.location` becomes one of the six slots: `leftTop`,
    `leftBottom`, `rightTop`, `rightBottom`, `bottomLeft`, `bottomRight`.
- `terminals.open`: `location` becomes `"editor"` or a container view ID.
  `cwd` stays. Without `cwd` the shell starts in the home directory, because
  the directory policy now lives in the caller.
- New `workbench.lastFocusedDocument(window)` lets JavaScript reproduce the
  directory rule.
- New `ViewRegistration.setBadge("info" | "error" | null)` paints a blue or
  red dot on the tool's stripe button.
- `WindowLayout` and `LayoutDock` describe slots and tool-window trees.

## Exclusions

- Floating, windowed, undocked, and sliding/overlay tool-window modes, as
  the existing decision keeps out.
- The container ⌄ menu, which lists shell profiles in IntelliJ; Knot has no
  profiles.
- The side-by-side layout of a side's two slots, and the option to give all
  tool windows one size instead of remembering each.
- Tab kinds beyond tree, text, and terminal; agent tabs bring their own.
- Keybinding changes beyond keeping the existing ones working.
- Terminal titles from the shell (D041).

## Checkpoints

### 1. Slots and tool windows

Deliverables:
- Docks become six slots of tool windows, each owning a split tree, with the
  title, actions, ⋮ menu, and hide in the header and double-click to
  maximize. Existing tools are fixed tool windows.
- Stripes are grouped by slot. A click on a hidden window shows it, replacing
  the one in its slot, and a click on a visible one hides it. Drag and Move to
  move a window between slots. Remove from Sidebar and the ⋯ button work,
  and picking from ⋯ restores the button. Show Beside goes away.
- Per-tool-window sizes and side shares, and the side extent rule.
- Native code registers the Terminal container window for now, with the
  placement rules above, shell-name labels, and New Terminal opening into it.
  Closing or moving out its last tab hides it.
  The terminal stripe button, terminal home dock, and dock-terminal recency go
  away.
- Relocation, directional focus, maximize, resize (side size and slot ratio),
  closure, Reset Layout, window layouts, and the tool-layout record follow the
  new model. `placement.location`, `WindowLayout`, and `LayoutDock` change
  as above, and bundled packages use the default slots above.

Validation: model tests for slot transitions, replacement, hidden tool windows
keeping their tabs, stripe click show/hide, sizes, sidebar removal, placement
rules, container empty show and last-tab hide (closed or moved out), labels,
layout capture/restore pruning,
and record install. Product tests for stripes, the ⋯ list, headers and their
menu, double-click maximize, drag between slots and between container and
center, and closure.
Update `src/app/product/tests/{stripes,dock_header,tool_layout,navigation,terminal}.rs`.

Review gate: arrange tools in all six slots, swap tools within a slot, move
the Terminal window to right top, split inside it, move a terminal to the
editor and back, restart, open a fresh window.

**Implementation context:**
- Model: `crates/knot-model/src/workbench.rs` (`Area`, `TabKind::allows`) and
  `workbench/{tools,stripes,arrange,resize,relocate,close,closure,terminals}.rs`.
  `ToolFrame`, `ToolPlacement`, and `StoredDock`/`StoredNode` become slot and
  tool-window records. `show_tool_replacing` becomes the only show, and
  `terminals.rs` shrinks to the container's terminal placement.
- App: `src/app/workbench.rs` (view retention for hidden tool windows) and
  `src/app/product/{render,stripes,tools,tab_drag,navigation,commands,tool_picker}.rs`.
  `render_pane` splits into the tool-window title row and the pane tab strips.
  Persistence is in `src/app/window_layout.rs` and `src/app/tool_layout.rs`.
- Contract: `crates/knot-host/src/js/knot.d.ts` (`ViewLocation`,
  `WindowLayout`, `LayoutDock`), `docs/schemas/knot.schema.json`, and
  `extensions/@knot/{explorer,find,notifications}`.

### 2. Terminal in `@knot/chrome`

Deliverables:
- The container-view API (`content` optional, `newTab`),
  `terminals.open({ location })`, and `workbench.lastFocusedDocument`.
- `@knot/chrome` registers `knot.terminal` (bottom left) and New Terminal and
  New Terminal in Editor. New Terminal is the `newTab` command and owns the
  directory rule: the focused terminal's directory, else the root containing
  the last focused document, else the first root, else home.
- Native registration of Terminal, `terminal.new`, `terminal.new-in-center`,
  `terminal.toggle`, and `knot-model/src/terminal_directory.rs` go away.

Validation: package tests through the real editor for New Terminal placement,
the directory rule, empty show, toggle, and restore. Rust tests for the
removed native paths are deleted.

Review gate: daily terminal use, including restart with terminals in a tool
window and in the center.

**Implementation context:**
- `extensions/@knot/chrome/src/` (mirror `menus.ts` and `extension.ts`).
- `src/app/extensions/host/handlers.rs` and `crates/knot-protocol/src/`,
  mirroring `registerView` and `terminals.open`.
- `src/app/product/commands.rs` (terminal commands) and the window's
  last-focused-document tracking that `terminal_directory.rs` reads today.

### 3. IntelliJ commands

Deliverables:
- Generated Toggle <Tool> commands, Hide Active Tool Window (from the editor
  too), Jump to Last Tool Window, Hide All Tool Windows, Maximize Tool Window,
  Stretch to Left/Right/Top/Bottom, Move to (slot), Open as Editor Tab, Return
  to Tool Window, Split and Move Right/Down, Unsplit, Go to Next/Previous
  Splitter, and Rename Session, with Hide All restoring what it hid. The ⋮
  menu gains Resize.
- Esc in a tool window focuses the editor, and `terminal.escapeToEditor`
  (default true) lets a terminal send Esc to the shell instead.
- The dock toggle and Move Tab to Left/Right/Bottom Dock and Center commands go
  away. Existing keybindings and Vim `ctrl-w` keep working.

Validation: command tests for each, plus Esc from a tree tool and from a
terminal under both values of the setting.

Review gate: drive the workbench from the palette alone.

**Implementation context:** `src/app/product/commands.rs` (the dock macros
at its end), `src/app/commands/` keymaps, `src/app/product/tool_picker.rs`
(Toggle Tool…), `extensions/@knot/vim`, `src/app/terminal/input.rs` (Esc),
and terminal settings in `knot.d.ts` and `docs/schemas/knot.schema.json`.

### 4. Notifications in one place

Deliverables:
- `ViewRegistration.setBadge`. `@knot/notifications` shows the unread dot,
  red when an unread notice is an error, on its stripe button and drops the
  status bell. Popups are unchanged.

Validation: a package test for the dot following read state and severity,
and a product test for the dot paint.

Review gate: raise, read, and clear notices.

**Implementation context:** `extensions/@knot/notifications/src/`,
`src/app/product/stripes.rs`, `crates/knot-model/src/workbench/stripes.rs`,
and `knot.d.ts`.

### Finish

Update `docs/architecture/workbench-and-lifecycle.md` (workbench, relocation,
tool-layout memory, terminals, notifications), `docs/extension-api.md`, and
the [workbench decisions](../decisions/workbench-and-lifecycle.md): docks as
slots instead of split trees, terminals in container tool windows instead of
flat dock tabs, and stripe replacement within a slot. Then remove this plan.
