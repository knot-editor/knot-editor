# D034 — Explorer as a bundled extension

Status: 🚧 approved, not started. Item of the
[daily-driver plan](daily-driver.md); absorbs D053. Depends on D008, D010,
D072, D081, and D122, all done.

## Goal

A bundled `@knot/explorer` package replaces the native explorer. It browses
every root of the window, opens files as previews or regular tabs, follows the
active file, refreshes from file watching, and creates, renames, deletes,
copies, and moves files inline, as VS Code and IntelliJ do. It uses only
public API: every gap it exposes is closed in the API, generally rather than
for the explorer alone, and the native explorer and its private workspace-tree
path are removed.

## Design

### Icon references

Today an extension names a fixed icon (`TreeItem.icon`, `TreeAction.icon`,
`StatusItemOptions.icon`); only native surfaces such as tabs can ask for the
icon a file or folder gets through the icon associations. Picker items and
input completions take no icon.

- Everywhere the API takes an icon, it takes an `IconRef`:
  `string | { file: string } | { folder: string }`. A string is an icon name,
  as today. `{ file }` and `{ folder }` take a name and resolve natively at
  paint time through the existing association rules (`file_icon` and
  `folder_icon` in `crates/knot-model/src/icons.rs`), so pack, override, and
  association changes repaint without the extension.
- Variants are an internal lookup rule, not API: a surface applies a variant
  by trying `name.variant` before `name` for each candidate in turn. Only
  `open` exists, and trees apply it to expanded rows. An expanded `src` folder
  tries `folder.src.open`, `folder.src`, `folder.open`, then `folder`; an
  expanded row with the plain name `X` tries `X.open`, then `X`. Extensions
  never choose open or closed; `folder_icon`'s `open` argument becomes this
  rule.
- `PickerItem` and `InputCompletion` gain `icon?: IconRef`. Go to File and
  Vim's `:e` path completion show file and folder icons.

Other editors agree: VS Code trees take `resourceUri` with `ThemeIcon.File` or
`ThemeIcon.Folder` and resolve the theme icon, including its expanded form;
Zed's icon themes pair collapsed and expanded directory icons. Resolving a name
in JavaScript (`icons.forFile`) was rejected: it goes stale when packs or
associations change and cannot follow expansion.

### Tree API

- **Activation gestures.** A row's command receives how it was activated:
  `click`, `doubleClick`, `middleClick`, `rightClick`, or `enter`. Right click
  also still opens the native context menu built from the row's actions.
- **Row identity.** `TreeItem.arguments` is passed to the row's command, as
  `TreeAction.arguments` is. `workbench.selectedTreeItem()` returns the
  selected row's ID for commands bound to keys in a view's scope (F2, Delete,
  ⌘C) and replaces `selectedText()` for trees; `@knot/projects` moves to it.
- **Reveal.** `ViewRegistration.reveal(idPath, window?)` expands each
  ancestor, waiting for its children, then selects and scrolls to the last
  ID. The extension derives the path from the URI, so no `getParent` callback
  is needed.
- **Speed search** in every tree, as IntelliJ does: typing on a focused tree
  opens a small query field, matches the loaded (expanded) rows, highlights
  the matched text, and jumps to the best match; Up and Down move between
  matches, Enter activates, Escape clears. It highlights and jumps; it does
  not filter rows out.
- **Inline editing,** behaving like VS Code:
  - `ViewRegistration.editLabel(id, { value?, selection?, validate? })` turns
    the row's label into the shared `TextInput`; Enter or focus loss commits,
    Escape dismisses, and a validation message shows under the field and
    blocks the commit. It resolves accepted or dismissed, as `showInput` does.
  - `ViewRegistration.editNewItem(parentId, { icon?, validate? })` expands the
    parent and shows a temporary row with an empty field at the top of its
    children; an empty commit dismisses.

### The explorer package

- `@knot/explorer`, compiled by `knot-ext-workbench`, registers the tree view
  `builtin.explorer` in the left dock, visible by default. It is an ordinary
  tool for tool-layout memory.
- Every root of the window is a top-level row; a window without roots shows
  an Open Folder… row.
- Folders first, then files, by name ignoring case with numbers in natural
  order (`localeCompare` with `numeric: true, sensitivity: "base"`).
- Gestures: click opens with the preview hint (`documents.open` with
  `preview: true`, which the `workbench.previewTabs` setting may turn into a
  regular tab), double click and Enter open a regular tab, middle click opens
  a regular tab without focusing it. Clicking a folder toggles it.
- One recursive `files.watch` per root invalidates the loaded folders that
  contain changed entries; a batch naming the root rescans it.
- Ignored entries are shown dimmed; a toggle (setting and command, default
  shown) hides them.
- Reveal Active File command, and `explorer.autoReveal` (default on) reveals
  the active document whenever it changes.
- Toggle Explorer moves to the package and replaces `workbench.toggle-explorer`.
- File actions, as row actions, context menu entries, and keys in the view's
  scope: New File, New Folder (typing `a/b/c.txt` creates missing folders; a
  new file opens in a regular tab), Rename (stem selected, folders whole),
  Delete (to the trash after a confirmation), Copy, Cut, Paste (one item;
  pasting over an existing name picks `name copy`), Copy Path, Copy Relative
  Path, Copy File Name, and Reveal in Finder.

### Exclusions

Each is a roadmap entry: drag-and-drop in trees (D176), multi-select in trees
(D177), compact folders (D178), and Git decorations (D126). The native
explorer's behavior is not a constraint.

### Deferred decisions

- The default key for Toggle Explorer stays D169.
- The new-copy naming scheme (`name copy`, `name copy 2`) may follow whichever
  of VS Code or IntelliJ reads better in review.

## Checkpoints

### 1. Icon references

- `IconRef` in the protocol and `knot.d.ts`, resolved natively wherever an
  icon is painted; the `open` variant rule for expanded tree rows; `icon` on
  picker items and input completions.
- Go to File shows file icons; Vim's `:e` completion shows file and folder
  icons.
- Validation: model tests for the variant fallback order and `{ file }` /
  `{ folder }` resolution; tree, picker, and completion rendering tests; Go to
  File and Vim package tests that observe the icons.
- Review gate: icons in a personal-config `Try:` tree, Go to File, and `:e`.

**Implementation context:** `crates/knot-model/src/icons.rs` (`file_icon`,
`folder_icon`, `first_defined`), `src/app/icons.rs` (`icon_definition`,
`file_definition`, `folder_definition`), icon-bearing types in
`crates/knot-protocol/src/lib.rs`, painting in `src/app/tree_view.rs`
(`icon`), `src/app/picker.rs`, `src/app/transient.rs` (completion list), and
`src/app/status_bar.rs`; contract in `crates/knot-host/src/js/knot.d.ts`;
`extensions/@knot/go-to-file/src/rows.ts`,
`extensions/@knot/vim/src/ex.ts` (`completePath`); tests beside
`src/app/icons/tests.rs`, `src/app/product/tests/icons.rs`,
`crates/knot-ext-workbench/tests/workbench/go_to_file.rs`, and
`crates/knot-ext-vim/tests/vim.rs`.

### 2. Tree API

- Activation gestures, `TreeItem.arguments`, `selectedTreeItem()` (removing
  `selectedText()`'s tree path and moving `@knot/projects` to it), `reveal`,
  and speed search.
- Validation: `TreeView` tests for each gesture, reveal through unloaded
  ancestors and stale generations, and speed search matching, navigation, and
  dismissal; product tests for the activation event's arguments and
  `selectedTreeItem()`; the projects package tests.
- Review gate: a `Try:` tree exercising gestures, reveal, and speed search.

**Implementation context:** `src/app/tree_view.rs` (`TreeViewEvent::InvokeCommand`,
click handling, `key_down`, `expanded`, `apply_response`) and its tests in
`src/app/tree_view/tests.rs`; the foreground bridge in
`src/app/extensions/semantics.rs`; `contextual_selected_text` and the view
handlers in `src/app/extensions/host/handlers.rs`; `TreeItem` in
`crates/knot-protocol/src/lib.rs`; the speed-search field reuses
`src/app/text_input.rs`; context-menu tests in
`src/app/product/tests/tree_context_menu.rs`;
`extensions/@knot/projects/src/commands.ts` and `tree.ts`.

### 3. Explorer browsing, native explorer removed

- `@knot/explorer` with browsing, sorting, gestures, watch refresh, the
  ignored toggle, Reveal Active File, `explorer.autoReveal`, and Toggle
  Explorer.
- Remove `src/app/workspace_tree.rs`, the native explorer tool
  (`EXPLORER_TOOL_ID`, `ensure_explorer`, `explorer_frame`, the
  first-run explorer special cases), `workbench.toggle-explorer`, and the
  `tool.knot.explorer` icon; tool-layout memory treats the explorer as any
  contributed view.
- Validation: package tests through the real editor for multi-root rows,
  sorting, each gesture's resulting tab, watch refresh after external
  changes, ignored rows, and reveal; workbench and tool-layout tests updated
  for the removed native tool.
- Review gate: daily use of the explorer in a multi-root window.

**Implementation context:** new `extensions/@knot/explorer/` mirroring
`extensions/@knot/projects/` (`knot.jsonc`, `src/`, `test/`), compiled by
`crates/knot-ext-workbench` (`build.rs`, `src/`) with tests in
`crates/knot-ext-workbench/tests/workbench/`; `files.list`, `files.watch`,
`documents.open`, and `workbench.observe` (`activeTargetChanged`,
`workspaceChanged`); native removal across `src/app/product/tools.rs`,
`src/app/product/roots.rs`, `src/app/product/commands.rs`,
`src/app/product/sessions.rs`, `src/app/workbench.rs`,
`src/app/tool_layout.rs`, `src/app/window_layout.rs`,
`crates/knot-model/src/workbench/tools.rs`, `crates/knot-model/src/icons.rs`,
and the product tests that name the explorer.

### 4. Inline editing and file actions

- `editLabel` and `editNewItem` in the native tree; New File, New Folder,
  Rename, and Delete in the explorer.
- Validation: `TreeView` tests for commit, dismissal, focus loss, and
  validation; package tests for each action, nested creation, the opened tab,
  open documents following a rename, and delete confirmation.
- Review gate: `Try:` walk through the file actions.

**Implementation context:** the field is `src/app/text_input.rs`, as
transient prompts use it (`src/app/transient.rs`); row rendering and key
routing in `src/app/tree_view.rs`; mutation through `files.createFile`,
`files.createDirectory`, `files.rename`, and `files.delete` (`crates/knot-model/src/files.rs`,
`rename_file` and siblings in `src/app/extensions/host/handlers.rs`).

### 5. Clipboard and paths

- `files.copy` through providers and the public API; Copy, Cut, Paste, Copy
  Path, Copy Relative Path, Copy File Name, and Reveal in Finder (a small
  native operation in the public API).
- Validation: model tests for `files.copy` (directories, clobbering, cycles);
  package tests for paste after copy and cut and for the copied texts.
- Review gate: `Try:` walk through the clipboard actions.

**Implementation context:** `crates/knot-model/src/files.rs` beside `rename`,
the provider in `crates/knot-model/src/resource.rs`, a handler beside
`rename_file`, `workbench.writeClipboardText`.

### Completion

Update [semantic UI](../architecture/semantic-ui.md) (icons, extension trees),
[documents and persistence](../architecture/documents-and-persistence.md)
(`files.copy`), [workbench and lifecycle](../architecture/workbench-and-lifecycle.md)
(no native explorer), the [extension guide](../extension-api.md),
[settings](../settings.md), and the decisions for icon references and the
extension explorer; mark D034 done in the daily-driver plan and remove this
plan.
