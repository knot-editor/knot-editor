# Tab, editor, and extension menu items

Status: 🚧 approved, not started. Roadmap item [D119](../roadmap.md).

## Goal

Right-clicking a tab or an editor shows a context menu, Shift+F10 opens the
focused surface's menu from the keyboard, and packages add commands to these
menus and to the application menu bar. Context menus have no hard-coded
contents: bundled packages fill them through the same API every extension
uses.

## Design

- **Menu items are registrations.** `menus.add(location, item)` returns a
  `Disposable`; the item is `{ command, title?, arguments?, group?, order?,
  view?, context? }`. The title defaults to the command's title. Items are owned
  by the lifecycle that registered them, so teardown removes them. The registry
  is a `knot-model` module modeled on `status_bar.rs`, and an application
  global wraps it. Native commands can be placed too, including from personal
  config. A separate call, rather than a field of `CommandOptions`, lets a
  command appear in several locations with different arguments.
- **Locations are a fixed, validated set.** An unknown location is a
  `TypeError`, and `knot.d.ts` types the names as a union:
  - `tab`: right-click on a tab header, in the center or a dock.
  - `content`: right-click inside a surface. In this item it covers editors
    and terminals. Tree rows keep their provider-owned row actions (see
    Exclusions).
  - `menubar.file`, `menubar.edit`, `menubar.view`, `menubar.go`: the
    application menu bar.
- **Context menu items are scoped like keybindings.** For `tab` and `content`,
  `view` (a view kind: `editor`, `terminal`, or a contributed view ID) and
  `context` (an editor key context) select the surfaces where an item shows.
  They are matched against the target surface's kind and live contexts, using
  the key router's scope rule. Menu-bar locations reject `view` and `context`,
  because the menu bar is global and menus already ignore key contexts.
- **Ordering is deterministic.** Items are sorted by `group`, a string
  compared alphabetically (`"1_close"`, `"2_split"`), then by `order`, then by
  title. A separator goes between groups. Native menu-bar items sit in fixed
  named groups that contributions can join or follow. Activation order never
  affects the menu.
- **A menu acts on what was clicked, without focusing it.** The menu builds a
  `ProductCommandTarget` for the clicked tab (pane, tab, surface,
  presentation) instead of the focused one and dispatches its commands with
  that target. Focus and the active tab don't change. Native commands already
  act on their captured tab. JS commands read `ctx.editor` and `ctx.buffer`,
  never the window's active file. An unfocused extension view captures
  `view: null`, as it does today.
- **An unavailable command does nothing.** There are no disabled states and no
  `when` language. A command such as Copy Path on an untitled document returns
  `unavailable`.
- **Menu contents live in JS.** A new bundled `@knot/menus` package in the
  workbench group adds the tab items (Close, Close Others, Close to the Right,
  Close All, Split Right, Split Down), the editor items (Cut, Copy, Paste), and
  the terminal items (Copy, Paste). The explorer adds Copy Path, Copy Relative
  Path, Reveal in Explorer, and Reveal in Finder to editor tabs. Native code
  owns the menu surface, the locations, and the commands.
- **The menu bar stays native.** It must exist before extensions start and
  where none run (macOS with no window, the config-error window), so its
  current items stay native and contributions are appended to them. The menu
  bar is set again when its registrations change, as it already is when the
  keymap changes.
- **One native context menu and a Knot-side popup focus.** One helper owns
  building the gpui-component `PopupMenu`: focus, the dismiss and blur
  subscriptions, and restoring focus. The tree, stripe, tab, and content menus
  all use it. An open menu sets `Focus::Popup` in the window instead of relying
  on the key path's `popup_focused` exception. Global and extension bindings
  still match first, and leftover keys go to the menu's own bindings.
- **Editor right-click.** A right-click outside the selection moves the caret
  to the click point, and one inside the selection keeps it. The menu then
  opens at the pointer. `workbench.show-context-menu` replaces
  `tree.show-context-menu`. It stays on Shift+F10 and opens the focused tree
  row's menu, or the editor's menu at the caret. Terminals keep Shift+F10
  unbound. A terminal opens its menu on right-click only while its program
  doesn't own the mouse.

## Exclusions

- Contributing items to another package's tree rows. Row-specific entries,
  such as Stage on unstaged rows and Unstage on staged ones in a git view, are
  the owning provider's row actions (`menuOnly` where they belong only in the
  menu). A contribution could only filter by view, so it has no consumer yet.
- An `addEditorAction`-style helper combining command, keybinding, and menu
  item. A shared TS helper can come later if the boilerplate grows.
- A language scope on items or keybindings, until a package needs one.
- Submenus, checkmarks and toggles, icons, disabled states, and `when`
  expressions.
- Stripe button menus (layout management, not commands), gutter menus, and
  status-bar item menus.

## Deferred decisions

- Whether context menus show each command's key binding. Include it only if
  gpui-component's `PopupMenu` supports it cheaply (checked in checkpoint 2).
- Whether an `editor`-scoped explorer item set belongs in `content` too (Copy
  Path in the editor's own menu). Revisit at checkpoint 4's review.

## Checkpoints

### 1. One native context menu and popup focus ✳️ not started

Extract the shared context-menu helper. Move the tree row menu and the stripe
button menu onto it, and give an open menu its own Knot-side `Focus::Popup`,
removing the key path's popup exception. Behavior doesn't change for users.

- **Implementation context:** `src/app/tree_view.rs` (`ContextMenu`,
  `open_context_menu`, `close_context_menu`, render at the
  `context_menu` child) and `src/app/product/stripes.rs` (`StripeMenu`,
  `open_stripe_menu`) duplicate the build, focus, subscribe, and track steps.
  Put the helper beside `track_popup`/`popup_focused` in
  `src/app/components.rs`, or in a new `src/app/context_menu.rs` registered in
  `src/app/mod.rs`. Add `Popup` to `enum Focus` in `src/app/product.rs`, set it
  when a menu opens, and restore it on dismissal. `KnotWindow::key_down` in
  `src/app/product/key_routing.rs` checks `Focus::Popup` instead of
  `popup_focused`. Keep the router-first ordering
  ([keymaps](../architecture/commands.md#keymaps)).
- **Validation:** the existing `src/app/product/tests/tree_context_menu.rs`
  and `stripes.rs` tests pass unchanged. Add tests for a global binding
  winning while a menu is open, for leftover keys reaching the menu, and for
  focus returning on dismissal and on blur.
- **Review gate:** tree and stripe menus behave as before in the real app.
  Update the keymaps section of `docs/architecture/commands.md` and the tree
  context menu paragraph of `docs/architecture/semantic-ui.md`.

### 2. Menu registry, `menus.add`, and the tab context menu ✳️ not started

Add the registry and the extension API, the clicked-tab command target,
`workbench.close-tabs-to-right`, the tab context menu, and the `@knot/menus`
package with the tab items.

- **Implementation context:**
  - Model: `crates/knot-model/src/menus.rs` (declared in `lib.rs`), modeled on
    `status_bar.rs`. It holds locations, items with lifecycle owners, scope
    matching through the key router's scope rule
    (`crates/knot-model/src/commands/key_router.rs`), and ordering into
    groups.
  - Application: an `ApplicationMenus` global in `src/app/menus.rs`.
    Lifecycle teardown removes an owner's items alongside the other
    registries (see how `src/app/status_bar.rs` is cleaned).
  - Host: a `menus.add` handler annotated in
    `src/app/extensions/host/handlers.rs`, the bootstrap wrapper in
    `crates/knot-host/src/js/bootstrap.js`, types in
    `crates/knot-host/src/js/knot.d.ts`, and `docs/schemas/knot.schema.json`
    if it covers the API.
  - Target: a `capture_tab_target(pane, tab)` beside
    `capture_command_target` in `src/app/product.rs`, dispatched through
    `dispatch_command`.
  - Tab menu: a right-click handler on the tab header in
    `src/app/product/render.rs`, next to the middle-click close, opening the
    shared helper with the registry's `tab` items for the tab's view kind
    (`target_view_kind`).
  - Command: `workbench.close-tabs-to-right` in `src/app/product/commands.rs`,
    with a new `ProtectedCloseKind` resolved in
    `crates/knot-model/src/workbench/closure.rs`.
  - Package: add `extensions/@knot/menus/` to
    `crates/knot-ext-workbench/build.rs`.
- **Validation:**
  - Model tests: ordering, separators, scope matching, lifecycle removal,
    unknown-location and menu-bar-scope rejection.
  - Product tests in a new `src/app/product/tests/context_menus.rs`:
    right-clicking an unfocused tab runs Close Others or Split for that tab
    while focus stays put; terminal and tool tabs get only the items scoped
    to them; protected closure prompts for Close to the Right.
  - End-to-end package test in `crates/knot-ext-workbench/tests/workbench/`
    through the real editor.
- **Review gate:** the user checks tab menus through `Try:` commands in
  `post-init.js` that add items from personal config. Document `menus.add`
  in `docs/extension-api.md`, and record the menu ownership in
  `docs/architecture/commands.md` and `workbench-and-lifecycle.md`.

### 3. Editor and terminal content menus ✳️ not started

Right-click and Shift+F10 menus for editors, right-click menus for terminals,
with `@knot/menus` adding Cut, Copy, Paste, and the terminal's Copy and Paste.

- **Implementation context:**
  - Editor: right-click handling in `src/app/editor/input.rs`
    (`on_mouse_down`). Caret placement is model code in
    `crates/knot-model/src/editor/` with a plain test.
  - Terminal: `src/app/terminal/mouse.rs`. Open the menu only when the
    program doesn't own the pointer.
  - Keyboard: rename `tree.show-context-menu` (`src/app/tree_view.rs`) to
    `workbench.show-context-menu`, which picks the focused surface's menu.
    Update the presets in `src/app/commands/keymaps/presets.rs`, keeping the
    terminal unbind.
  - Menu: the helper from checkpoint 1, with targets from
    `capture_command_target`.
- **Validation:**
  - Editor tests: caret placement inside and outside the selection.
  - Product tests: menus open on right-click and Shift+F10 and run against
    the right editor, and the terminal menu is skipped under mouse reporting.
  - Package test for the items.
- **Review gate:** manual check through `Try:` commands. Update the editor
  views section of `docs/architecture/editor-and-languages.md` and the
  terminal notes in `workbench-and-lifecycle.md`.

### 4. Explorer items in the tab menu ✳️ not started

The explorer adds Copy Path, Copy Relative Path, Reveal in Explorer, and
Reveal in Finder to editor tabs, acting on the captured tab's document.

- **Implementation context:** `extensions/@knot/explorer/src/clipboard.ts`
  and `commands.ts` (`explorer.reveal-active-file`) currently read the active
  file through `activeFile` in `reveal.ts`. New tab-scoped commands read the
  captured `ctx.editor`/`ctx.buffer` document instead. If the API cannot read
  the URI of a captured buffer, add that accessor (host handler and
  `knot.d.ts`).
- **Validation:** explorer package tests in
  `crates/knot-ext-workbench/tests/workbench/explorer.rs` run each item from
  an unfocused tab's menu, and an untitled document returns unavailable.
- **Review gate:** manual check, and decide the deferred `content` question.

### 5. Menu-bar contributions ✳️ not started

Contributed items join the native menu bar under `menubar.*`.

- **Implementation context:** `product_menus` and `install_menus` in
  `src/app/product/launch.rs`. Give the native items named groups, merge
  registry items, and set the menus again when `ApplicationMenus` changes
  (observe it as the keymap is observed). Menu items carry arguments through
  `crate::app::commands::dispatch::menu_action`, which may need them. A
  contributed item with no window open is disabled, like other window
  commands.
- **Validation:** tests of the menu list that `product_menus` builds,
  covering merged groups, lifecycle removal, and arguments reaching the
  command.
- **Review gate:** manual check of a personal-config menu-bar item. Update
  the [menus](../architecture/commands.md#menus) section and record the menu
  design in `docs/decisions/` once validated.
