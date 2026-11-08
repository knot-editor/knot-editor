# D160 — Current gpui and gpui-component

Status: 🚧 approved, not started. Roadmap item [D160](../roadmap.md);
[D055](d055-icon-packs.md) waits for it.
Order: D161 (complete; see the [crate boundaries decision](../decisions/platform.md#crate-boundaries)) →
**D160** → [D162](d162-thin-gpui-shell.md). The paths below follow D161's
crate moves. The port report from checkpoint 1 feeds
D162.

## Goal

Knot runs on current gpui instead of the year-old `gpui` 0.2.2 release, and
uses gpui-component for leaf widgets it would otherwise build by hand. The
first adopted widgets are visible: themed tooltips and a right-click and
keyboard context menu on extension tree rows. Knot keeps its own workbench,
docks, editor, terminal, single-line text input, and key routing.

## Background

- `gpui` 0.2.2 (2025-10-22) is Zed's latest crates.io release. Zed has not
  published since, while gpui kept changing in its repository.
- `gpui-pre` is a weekly crates.io snapshot of Zed's gpui, each version
  naming its Zed commit, published by Longbridge's maintainer. 0.3.11 adds
  separate platform crates (application creation through `gpui-pre-platform`;
  wgpu on Linux instead of blade), AccessKit accessibility, parent-anchored
  popup windows for menus and tooltips, gestures, and a profiler.
- gpui-component (repository `longbridge/gpui-kit`, about 20k stars, weekly
  0.x releases, 400k downloads in 90 days) powers Longbridge Pro. Two
  maintainers write most of it. 0.9.0 pins `gpui-pre =0.3.11`, so the gpui
  port is a precondition rather than a choice.

## Design

- **Dependency route.** Depend on `gpui-pre`, `gpui-pre-platform`, and
  `gpui-component` at exact versions, upgraded together and deliberately when
  gpui-component releases. Depend on `gpui-component` directly, without the
  `gpui-kit` umbrella, and enable only the features the adopted widgets need.
  If the snapshots stop, pinning a Zed git commit carries the same code, but
  gpui-component would then need a fork.
- **What Knot adopts.** Leaf widgets whose state is purely presentational:
  tooltips, popup and context menus, and later dropdowns, popovers,
  notifications, tables, virtual lists, and markdown rendering, each when a
  feature needs it. No component owns buffer text, documents, persisted
  state, layout, or command dispatch.
- **What Knot keeps.** The workbench and docks, the editor, the terminal,
  the command palette and pickers, and the single-line `TextInput`. Sessions
  without a window drive `TextInput` directly, and windowless package tests
  depend on that, which gpui-component's `Input` cannot provide.
- **Keys: the keymap first.** Knot's interceptor keeps matching every
  key-down before gpui sees it. When a gpui-component popup holds focus, the
  keymap still matches first, so global bindings keep working; a key the
  keymap does not claim continues to gpui dispatch, so the component's own
  actions (arrows, Enter, Escape) run. Components that take keyboard focus
  are adopted only for surfaces that need no windowless path.
- **Theme bridge.** On every theme application, Knot fills
  gpui-component's theme from the resolved Knot theme through one mapping
  table from their fields to native color keys. A new native key is added
  only where no existing key fits; extensions see no new API.

## Exclusions

- gpui-component's dock, code editor, `Input`, and theme registry.
- Accessibility roles for Knot's own surfaces: the bridge arrives with the
  port, but making the editor, trees, and terminal accessible is D030.
- Any web, iOS, or hot-reload use of the new platform crates.
- A context menu on the workspace tree, which has no file commands yet.

## Deferred decisions

- Whether D062 (positioning existing windows) becomes a gpui call on the new
  platform crates. Check during the port and update its entry.
- Whether D055 paints through gpui-component's `Icon` element or its own.
  Decide when D055 resumes after this plan.

## Checkpoints

### 1. Port to gpui-pre

- Deliverables: Knot builds and runs on `gpui-pre` 0.3.11 with the same
  behavior on macOS, Windows, and Linux. This plan records a short port
  report under this checkpoint: build errors by category, the API changes
  that mattered, and anything gpui now does that Knot had worked around.
- Validation: the full suite under `just test` and the extension checks;
  CI's Linux, macOS, and Windows jobs; a manual macOS smoke pass over editing,
  IME, splits, tab drag, terminal, zoom, and window restoration.
- **Implementation context:** `Cargo.toml` swaps the three `gpui` entries
  (normal, dev with `test-support`, Linux with `wayland`) for `gpui-pre` and
  `gpui-pre-platform`, renamed to `gpui` with `package =` so `use gpui::`
  stays unchanged. Application creation moves from `Application::new()` in
  `src/app/product/launch.rs:393` to `gpui_platform::application()`, and test
  setup to its headless counterpart. The work most likely to break is
  text-system and painting code (`src/app/editor/paint.rs`,
  `src/app/editor/layout.rs`, `src/app/terminal/`), the key interceptor in
  `src/app/product/key_routing.rs`, and tab drag in
  `src/app/product/tab_drag.rs`. Re-check the gpui 0.2.2 workarounds noted
  in `docs/architecture/` and the D062 and D030 roadmap entries.
- Review gate: the port report and the smoke pass. Nothing feels
  different, and the measured cost does not change the decision.

### 2. gpui-component, the theme bridge, and tooltips

- Deliverables: gpui-component initialized at startup and in test
  applications, the theme bridge applied with every theme application, and
  status bar item tooltips rendered through gpui-component's tooltip.
- Validation: a theme test that every mapped field takes its Knot key's
  color in a dark and a light bundled theme and follows a theme switch; a
  status bar test that hovering an item with a tooltip shows it; the existing
  status bar tests unchanged.
- **Implementation context:** initialize beside the other globals in
  `src/app/product/launch.rs`. The mapping table lives in
  `crates/knot-model/src/theme.rs` next to `native_color_definitions`, and is applied from
  `themes::apply` in `src/app/themes.rs`. The current tooltip is the
  `StatusTooltip` view used at `src/app/product/render.rs:404`.
- Review gate: tooltips look native in both appearances, and the theme
  bridge has no field that would need a hand-picked color.

### 3. Tree row context menu and the key bridge

- Deliverables: right-clicking an extension tree row selects it and opens a
  gpui-component context menu listing the row's actions, which run through
  the existing captured-window command route. A `tree.showContextMenu`
  command, bound to Shift+F10 in the tree key context, opens it at the
  selected row. While the menu has focus, unclaimed keys reach it; keymap
  bindings still win; dismissal returns focus to the tree. A row without
  actions opens no menu. The key interceptor gains the general rule that a
  focused gpui-component popup receives keys the keymap leaves.
- Validation: product tests for opening by pointer and by command, running
  an action with its arguments, Escape returning focus to the tree, a global
  binding firing while the menu is open, and no menu for a row without
  actions; key-routing tests for the pass-through rule; a windowless session
  where the command does nothing.
- **Implementation context:** row actions and their dispatch already exist in
  `src/app/tree_view.rs` (`TreeView::render`, action focus, `TREE_KEY_CONTEXT`
  from `src/app/mod.rs:67`). The pass-through goes in the interceptor and
  `FocusTarget` handling of `src/app/product/key_routing.rs`. Register the
  command and its default binding through `crates/knot-model/src/commands/catalog.rs` and
  `src/app/commands/keymaps/presets.rs`.
- Review gate: a personal-config `post-init.js` with a `try.*` command
  that contributes a tree whose rows have actions; the menu works by
  pointer and keyboard, and Vim mode and global bindings behave as before.

### 4. Documentation

- Deliverables: a decision in `docs/decisions/platform.md` with its index
  line (the gpui-pre route and its risk, selective adoption, native text
  input, the keymap-first rule, the theme bridge); `docs/architecture.md` and
  the commands and semantic UI references updated for the component layer
  and key pass-through; `docs/development.md` for any toolchain change; the
  roadmap's D030 and D062 entries revised from the port findings, D055's
  order moved after D160, and D160 removed; this plan removed.
- Validation: the `review-docs` checks for the touched files.
- Review gate: the docs describe the shipped behavior only.
