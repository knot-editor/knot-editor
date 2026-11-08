# D160 — Current gpui and gpui-component

Status: 🚧 checkpoint 1 complete; checkpoint 2 implemented, awaiting its review gate. Roadmap item [D160](../roadmap.md);
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

✅ Complete and approved after the macOS smoke pass. Knot runs on
`gpui-pre` and `gpui-pre-platform` 0.3.11, declared once under
`[workspace.dependencies]` as `gpui` and `gpui_platform`; `knot-app` creates
the application ([architecture](../architecture.md#system-map)). CI passes
on Linux (full suite) and builds the product and runs the handler checks on
macOS and Windows; the full suite and a manual pass ran only on macOS.

- **Port report.**
  - Build errors, 59 in the library and 23 more in tests, by category:
    - Async contexts are infallible. `AsyncApp::update` and entity access
      return the value, not a `Result`; about 25 sites dropped dead `Err`
      branches and 15 a `let _ =`. `WeakEntity` and window updates still
      return `Result`.
    - Focus and blur take the app: `FocusHandle::focus(window, cx)`,
      `Window::focus(handle, cx)`, `Window::blur(cx)`.
    - Keystroke notation is OS-aware. `Keystroke::parse` rejects `secondary`
      without an OS, `unparse` takes one, and `KeyBinding::new` takes a
      `&'static str` and panics on `secondary`, so menu shortcuts and the
      config-error Quit bind through `KeyBinding::load` with the host OS.
    - `ShapedLine::paint` takes an alignment and an alignment width (5 call
      sites); `ScrollWheelEvent` and `on_scroll_wheel` became `ScrollEvent`
      and `on_scroll`.
    - Small ones: `on_window_closed` passes the window id, `Menu` has
      `disabled`, `flex_shrink` takes a value, `ArenaClearNeeded::clear`
      takes the app, and `track_scroll` borrows its handle.
    - The application comes from `gpui_platform::application()`, which
      `knot-app` calls and passes to `knot::run`. Linking the platform crate
      into the library made every test process pay ~100 ms or more at load.
      gpui's macOS constructor initializes `NSView`, and that scans the
      executable's directory, `target/debug/deps`, for a bundle. With 69k
      stale files the suite took 105 s; with only the binary linking the
      platform, test binaries never load it.
  - The change that mattered: gpui's new test scheduler fails a test when
    another thread wakes a foreground task, unless parking is allowed. 225
    tests failed this way, because extension workers, PTY readers, the local
    filesystem thread, and storage threads all wake foreground tasks.
    `allow_thread_wakeups` in `src/app/mod.rs` allows it where the extension
    pool, terminal sessions, and the product globals start. Storage tests
    now run on gpui's `ThreadedDispatcher`, because a Mac platform can only
    be created on the main thread. Tests use gpui's own test platform and
    need no headless application from `gpui_platform`.
  - Untouched: editor layout, painting other than `paint`'s new arguments,
    the text system, the key interceptor, and tab drag beyond the
    window-closed callback.
  - Workarounds rechecked:
    - D030: gpui now has a public AccessKit bridge (`Element::a11y_role`,
      `write_a11y_info`), so the deferral's reason no longer holds.
      [Semantic UI](../architecture/semantic-ui.md) now says so; the roadmap
      entry is revised in checkpoint 4.
    - D062: unchanged. gpui still only resizes an existing window.
    - Physical keypad keys: still no key identity for them.
    - The macOS draw before activation in tab relocation stays until the
      smoke pass shows it is unnecessary.
  - Cost, measured on Apple M1 Pro macOS against the pre-port tree with a
    fresh `target`: the full suite takes 10.0 s against 10.8 s, and the
    release binary is 93.3 MB against 91.7 MB (+1.7%).
### 2. gpui-component, the theme bridge, and tooltips

🚧 Implemented; awaiting the review gate.

- Deliverables:
  - ✅ gpui-component 0.9.0 (`default-features = false`) is a workspace
    dependency pinned beside gpui-pre. `components::init` in
    `src/app/components.rs` initializes it at the start of `application.run`
    in `src/app/product/launch.rs` and in the test globals.
  - ✅ The theme bridge: `set_theme` in `src/app/theme.rs`, which every theme
    application goes through, calls `components::apply_theme`. It switches
    gpui-component to the theme's appearance, which loads its built-in
    colors for that appearance, then sets each field of the `COLORS` table
    from its native key. Fields with no fitting key (charts, button
    variants, sidebar, tables) keep the built-in colors.
  - ✅ Status bar item tooltips render gpui-component's `Tooltip` view
    through gpui's own `.tooltip()`, at `text_xs`; the `StatusTooltip` view is
    gone. gpui-component's managed tooltips need its `Root`, which Knot does
    not use.
- Validation:
  - ✅ `components_take_the_bundled_theme_colors_and_follow_a_switch`
    (`crates/knot-ext-workbench/tests/workbench/themes.rs`): with the bundled
    themes, every mapped field, the popover token, and the mode match One
    Dark, then One Light after `theme.mode` switches.
  - ✅ `hovering_an_item_with_a_tooltip_shows_it_on_a_popover`
    (`src/app/product/tests/status_bar.rs`): hovering an item with a tooltip
    paints one quad on the theme's popover background; an item without one
    paints none. The existing status bar tests pass unchanged.
  - ✅ Full suite and `clippy --all-targets -D warnings` on macOS.
- Scope decisions:
  - The mapping table lives in `src/app/components.rs`, not `knot-model`:
    it names gpui-component's fields, and `knot-model` cannot see gpui.
  - gpui-component's `border` takes the general `border` key rather than
    `popover.border`, so later tables and lists get dividers; tooltips and
    menus therefore border with `border`. No native key was added.
  - In debug builds gpui-component's inspector binds Cmd-Alt-I
    (Ctrl-Shift-I elsewhere) globally; Knot's keymap still matches first.
- Review gate (pending): tooltips look native in both appearances, and the
  theme bridge has no field that would need a hand-picked color.

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
