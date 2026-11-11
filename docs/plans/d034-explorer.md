# D034 — Explorer as a bundled extension

Status: 🚧 in progress; checkpoints 1 to 4 done. Item of the
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

### 1. Icon references ✅ done

`IconRef` (`crates/knot-protocol`, `knot.d.ts`) resolves at paint time through
`IconRegistry::first_defined` and `src/app/icons.rs` (`ref_definition`); tree
rows apply the `.open` variant when expanded; picker items and input
completions carry an icon, used by Go to File and Vim's `:e`.

### 2. Tree API ✅ done

Gestures (`TreeGesture` in `src/app/tree_view.rs`; a double click delivers
`click` then `doubleClick`, expandable rows toggle on click and Enter and run
no command, `rightClick` reaches only rows without actions because actions move
focus to the context menu), `TreeItem.arguments`, `selectedTreeItem()`,
`ViewRegistration.reveal`, and speed search drawn by the tree itself (not
`TextInput`; matching ignores case).

### 3. Explorer browsing, native explorer removed ✅ done

`extensions/@knot/explorer/` browses the roots of each window; the native
workspace tree, its tool registration, and `workbench.toggle-explorer` are
gone. General API added for it: `documents.open(…, { focus: false })`,
`ViewRegistration.toggle`, and `TreeItem.muted`. `workspace.remove-folder` and
`workspace.move-folder-up`/`-down` acted on the native selection and were
removed with it (Add Folder to Workspace stays). The knot crate's own tests
load no packages, so its workbench tests register a test tool
(`ensure_test_tool`) in place of the explorer.

### 4. Inline editing and file actions ✅ done

`editLabel` and `editNewItem` in the native tree (`src/app/tree_view.rs`); New
File, New Folder, Rename, and Delete in the explorer. An edit settles through
the host's input pull loop, so `validate` is asked asynchronously and a commit
waits for the answer to the current text; focus loss commits unless a message
is showing, then it dismisses. The explorer's actions are `menuOnly` (a
`TreeAction` flag), so rows show no buttons; two toolbar buttons create files
and folders. Rename, Delete, and F2/Delete keys act on the selected row; from
the palette they act on the active file. Created, renamed, and deleted entries
refresh the workspace file index at once (`FileIndex::apply_changes`), because
a listing right after a mutation otherwise lagged the watch by its coalescing
delay. The tree's context menu and edit field render inside its focus subtree,
and the session follows focus entering a view, so a menu action works when
another view had focus before the right click.

### 5. Clipboard and paths 🔎 awaiting review

- ✅ `files.copy` through providers (`FileSystemProvider::copy`; the local
  provider keeps permissions and copies symlinks as links) and the public API;
  `files.reveal` (Finder) as a small native operation.
- ✅ Copy, Cut, Paste, Copy Path, Copy Relative Path, Copy File Name, and
  Reveal in Finder in the explorer (`src/clipboard.ts`), as menu entries, palette
  commands, and ⌘C/⌘X/⌘V in the view's scope.
- ✅ Validation: model tests for `files.copy` (directories, clobbering, cycles,
  permissions, links); a host test; package tests for paste after copy and cut,
  naming, and the copied texts.
- Review gate: `Try: Explorer Clipboard` walks through the actions.

Scope decisions: the taken item lives in the extension, not the system
clipboard. Cut moves on the first Paste and refuses a taken name; Copy picks
`name copy`, `name copy 2` before the extension.

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
