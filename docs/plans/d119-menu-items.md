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
- **Menu contents live in JS, in `@knot/chrome`.** One bundled package in the
  workbench group holds the window chrome that packages fill in: the
  status-bar items (today's `@knot/status-bar`, renamed and extended), the
  context-menu contents, and later any toolbar widgets that move to JS. A
  package per surface was rejected as too fine-grained. `@knot/chrome` adds the tab items (Close, Close Others, Close to the Right,
  Close All, Split Right, Split Down), the editor items (Cut, Copy, Paste), and
  the terminal items (Copy, Paste). The explorer adds Copy Path, Copy Relative
  Path, Reveal in Explorer View, and Reveal in Finder to editor tabs. Native code
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
  gpui-component's `PopupMenu` supports it cheaply (checked in checkpoint 3).
- Whether an `editor`-scoped explorer item set belongs in `content` too (Copy
  Path in the editor's own menu). Revisit at checkpoint 5's review.

## Checkpoints

### 1. One native context menu and popup focus ✅

Done and approved. The tree row and stripe button menus open through
`src/app/context_menu.rs`, and an open menu gives the window the
`Focus::Popup` layer. See [keymaps](../architecture/commands.md#keymaps) and
the [tree context menu](../architecture/semantic-ui.md).

### 2. `@knot/chrome` from `@knot/status-bar` ✅

Done and approved. The bundled package `extensions/@knot/chrome/` holds the
status-bar items and takes the menu contents of later checkpoints. See the
[status bar](../architecture/workbench-and-lifecycle.md#status-bar).

### 3. Menu registry, `menus.add`, and the tab context menu ✅

Done and approved. Items live in `knot_model::menus` and the
`ApplicationMenus` global; `workbench.close-tabs-to-right` and the tab menu
use a target captured for the clicked tab. An open menu matches only global
keys. See [context menus](../architecture/commands.md#context-menus), the
[status bar and tab menu](../architecture/workbench-and-lifecycle.md#status-bar),
and the [extension guide](../extension-api.md#add-menu-items).

### 4. Editor and terminal content menus ✅

Done and approved. Editors and terminals open a `content` menu on right-click,
and `workbench.show-context-menu` (Shift+F10) opens the editor's at the caret.
See [context menus](../architecture/commands.md#context-menus), the
[editor views](../architecture/editor-and-languages.md#editor-views), and the
[terminal notes](../architecture/workbench-and-lifecycle.md).

### 5. Explorer items in the tab menu 🔍 ready for review

The explorer adds Copy Path, Copy Relative Path, Reveal in Explorer View, and
Reveal in Finder to editor tabs, acting on the captured tab's document.

- **Implementation context:** `extensions/@knot/explorer/src/clipboard.ts`
  and `commands.ts` (`explorer.reveal-active-file`) currently read the active
  file through `activeFile` in `reveal.ts`. New tab-scoped commands read the
  captured `ctx.editor`/`ctx.buffer` document instead. If the API cannot read
  the URI of a captured buffer, add that accessor (host handler and
  `knot.d.ts`).
- **Status:** ✅ the explorer registers the four items for editor tabs, and
  their commands read the captured tab's document (`capturedUri`). ✅ Tests
  cover the menu contents, each item run from an unfocused tab, and
  unavailable on an untitled document.
- **Validation:** explorer package tests in
  `crates/knot-ext-workbench/tests/workbench/explorer.rs` run each item from
  an unfocused tab's menu, and an untitled document returns unavailable.
- **Review gate:** manual check, and decide the deferred `content` question.

### 6. Menu-bar contributions ✳️ not started

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
