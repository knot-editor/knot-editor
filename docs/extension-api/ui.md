# Tool windows, prompts, and status

Part of the [extension guide](README.md).

## Contribute a tool window

Register one lifecycle-owned view definition before or after windows exist. The contribution ID is stable, namespaced, and doubles as the native view
kind for commands and keybindings:

```js
const view = await workbench.registerView({
  id: "example.outline",
  title: "Outline",
  content: {
    kind: "tree",
    provider: {
      getChildren(parentId, context) {
        // context.window identifies the requesting window; parentId null is the root.
        return [];
      },
    },
  },
  placement: { location: "leftTop", visible: true },
});

await view.invalidate(null);        // every live window
await view.show();                  // every live window
await view.hide();
await view.focus();
await view[Symbol.asyncDispose]();
```

Each window owns at most one native instance. A view is a tool window that
shows in one of six slots (`leftTop`, `leftBottom`, `rightTop`, `rightBottom`,
`bottomLeft`, `bottomRight`; a slot shows one tool window at a time), never
among documents, with a stripe button on the window edge unless the user turned
stripes off. Users can drag them between slots and hide them, and remembered
user layout overrides registration defaults in later windows.
`view.setBadge("info" | "error" | null)` paints a blue or red dot on its stripe
button, as the Notifications package does for unread notices. A view
registered without `content` is a container, which holds tabs opened into it;
see `terminals.open`. Programmatic `show`/`hide`/`focus` affects live windows only and never
changes the fresh-window default. Pass a live `WindowTarget` from
`context.target.windowTarget` or `windows.*` to target one window.
`context.target.viewTarget` is the tab the command was captured on, or null
when no tab was focused.

### Groups, status, and actions

An item is `item` by default. A `group` is a non-activatable header that
organizes child rows: the native view fetches its children immediately, shows no
disclosure, and keeps them visible. Items carry optional muted text under the label,
`detail`, an optional `statusText` at the row's top right, and a
`badge` naming a [color key](languages-and-themes.md#register-a-theme-and-define-colors), such as
`warning` or `text.muted`, and either kind may declare command-backed `actions`. A
contribution's own `actions` populate the frame toolbar. An item or action may
name an [icon](languages-and-themes.md#define-icons-packs-and-associations); an action with an `icon`
renders as an icon button and shows its `label` as a tooltip.

```js
const view = await workbench.registerView({
  id: "example.changes",
  title: "Changes",
  content: {
    kind: "tree",
    provider: {
      getChildren(parentId) {
        if (parentId === null) return [{
          id: "changed", label: "Changes", collapsibleState: "none", kind: "group",
          actions: [{ label: "Stage all", command: "example.stage-all" }],
        }];
        if (parentId === "changed") return [{
          id: "src/main.rs", label: "src/main.rs", collapsibleState: "none",
          statusText: "M", badge: "warning",
          actions: [{ label: "Stage", command: "example.stage",
                      arguments: { path: "src/main.rs" } }],
        }];
        return [];
      },
    },
  },
  actions: [{ label: "Refresh", command: "example.refresh" }],
});
```

Wherever an icon is accepted it may be an `IconRef`: a name, or `{ file: name }`
or `{ folder: name }` to get the icon a file or folder of that name gets through
the associations. Those resolve natively at paint time, show the open variant of
an expanded row, and follow pack and association changes, so a file browser
names icons without resolving them. Picker items and input completions take the
same `icon`.

Actions invoke existing registered commands. Their JSON `arguments` become the
handler's `context.arguments`, so an action retains the row identity it was
created for even after focus changes. Clicking an action never activates the
row, and the captured window is the one where the click happened. Rows remain
usable without a pointer: move the selection, press Right to enter its actions,
cycle with Up/Down, press Enter to invoke, and Escape to return.

### Activation, selection, and editing in trees

A row's `command` receives `arguments` (the row's `arguments`) and a `gesture`:
`click`, `doubleClick`, `middleClick`, `rightClick`, or `enter`. Expandable
rows toggle on click and Enter instead. A command bound to a key with
`keymaps.bind(key, command, { view })` learns the selected row from
`workbench.selectedTreeItem()`. `ViewRegistration.reveal(idPath)` expands each
ancestor, waiting for its children, then selects and scrolls to the last ID. A
row may be `muted` or `bold`, and an action `menuOnly` or `showOnHover`. Every
tree has speed search: typing on a focused tree highlights and jumps to
matching loaded rows.

`editLabel(id, { value, selection, validate })` turns a row's label into a text
field and `editNewItem(parentId, { icon, validate })` shows an empty temporary
row under a parent. Both resolve `accepted` with the text or `dismissed`;
`validate` returns a message to show under the field, which blocks the commit.
The bundled `@knot/explorer` package is the reference use of all of this: it
browses every root, follows the active file, and creates, renames, deletes,
copies, and moves files with only the public API.

### Dragging rows

A `draggable` row can be dragged onto a `dropTarget` row of its tree, which
runs the view's `drop` command with a `TreeDrop`:
`{ source: { id, arguments }, target: { id, arguments }, copy }`, where `copy`
is true when Option was held. The command decides what the drop means and
throws to reject one it cannot do. A draggable row with a `resource` URI can
also be dropped onto the editor area, which opens it as a tab in the pane or
new split under the pointer. The explorer moves files by dropping them on
folders and copies them with Option.

### Read-only text frames

A text frame owns a private read-only buffer rendered by the native editor; it is
not a document and has no resource, persistence, or document tab. Replace the
whole text with `setText`. The last text is retained for windows created later.

```js
const log = await workbench.registerView({
  id: "example.log",
  title: "Log",
  content: { kind: "text" },
  placement: { location: "bottomRight" },
});

await log.setText("first line\n");
await log.setText("replacement text\n");
```

`invalidate` applies to tree frames and `setText` to text frames; the wrong kind
rejects with `ContentKindMismatch`. Selection and copy work through the normal
editor surface, but edits are rejected. Pass `language` with the registration to
highlight the frame's text as a registered language. A frame has no gutter
unless the registration passes `gutter: true`; its line numbers then follow
`editor.lineNumbers` scoped to the frame's view ID, so
`settings.set("editor.lineNumbers", "off", { view: "example.log" })` drops the
numbers, and `editor.folding` set to `false` the fold lane; with both off, only
the marker lane remains.

## Open a composite view

A composite view is a center tab whose layout of child views you describe and
the user cannot rearrange. Reserve a kind with `workbench.registerComposite`,
then open instances of it; the tab belongs to your package and closes when you
close or dispose the instance or the registration, or when the package unloads.
Instances are not restored with their window.

```js
const kind = await workbench.registerComposite({ id: "example.compare" });
const [left, right] = await Promise.all([buffers.create({ text: "a\n" }), buffers.create({ text: "b\n" })]);
const instance = await kind.open({
  title: "Compare",
  icon: "git-compare",
  layout: {
    type: "row",
    children: [
      { type: "editor", key: "left", buffer: left },
      { type: "editor", key: "right", buffer: right, size: { pixels: 320 }, focused: true },
    ],
  },
  toolbar: [{ label: "Next change", icon: "arrow-down", command: "example.next" }],
});
```

The layout is a `row` or `column` of members: an `editor` on an
[extension buffer](application.md#own-a-buffer) or on a `document`, a `tree` or
`text` frame of a view registered with `embedded: true`, nested rows and
columns, and `tabs`, which show one titled member at a time. A member's `size`
is a `weight` (a share of what the `pixels` members leave, 1 by default) or
`pixels`. `focused` names the member that has focus on open; reselecting the
tab restores the view focused last. Every view, nested groups included, is in
the [view tree](application.md#inspect-and-change-application-state), so
commands, keymaps, and editor-driven packages such as Vim work in each editor.
An editor on a document counts the tab as a view of that document, once however
many editors show it, so closing the tab asks about unsaved changes when it is
the document's last view.

`instance.update({ title, icon, resource, layout, toolbar })` changes what the
tab shows; a field left out keeps its value. A member with a `key` that the new
layout also has, with the same buffer, document, or view, keeps its view and so
its handle, text position, history, and focus; members without a key are new
views. The view that had focus keeps it, and the layout's `focused` member takes
over when it is removed. An update that would drop the final view of a document
with unsaved changes rejects `DocumentUnsaved`.

The `toolbar` is a bar of icon buttons above the layout. A button's `label` is
its tooltip, and it runs `command` with `arguments` when clicked, with the
instance's tab as the command's target and the view focused in it as its focus,
wherever focus is, so a handler finds its instance in `target.viewChain`.
`toggled` shows the button pressed and `disabled` dims it and ignores clicks.
An update replaces the whole list, so send it again to change a button's state;
`toolbar: []` removes the bar.

A `scrollLink` makes two editors scroll together, wherever they sit in the
layout:

```js
const hunks = (await diff(left, right)).hunks;
await instance.update({
  scrollLink: {
    first: "left",
    second: "right",
    map: hunks.map(({ old, new: updated }) => ({ first: old, second: updated })),
  },
});
```

`first` and `second` are the `key`s of two editor members, and `map` lists the
regions where their texts differ, ascending and disjoint, as `diff` returns
them. Lines between regions match one for one, and the lines of a region map
proportionally onto the other side's. Scrolling either editor, by wheel, caret,
or code, scrolls the other to match. Sending a link brings the second editor in
line with the first, so send a new map after each rediff; `scrollLink: null`
removes it. A link must name two different editors of the layout the tab shows:
`open` and `update` reject `InvalidTarget` otherwise, including an update whose
new layout lacks a key the kept link names.

A `connector` member draws the strip between two editors of the layout, with a
band from the lines of one to the lines of the other:

```js
layout: {
  type: "row",
  children: [
    { type: "editor", key: "left", buffer: left },
    {
      type: "connector",
      first: "left",
      second: "right",
      links: [{ first: { start: 3, end: 5 }, second: { start: 3, end: 8 }, color: "diff.modified.background" }],
    },
    { type: "editor", key: "right", buffer: right },
  ],
}
```

`first` and `second` are the `key`s of two different editor members, and each
link's line ranges are the lines its band spans at that editor's edge, in the
theme `color` key it is filled with. The strip follows both editors'
scrolling. It is 32 pixels wide unless `size` says otherwise, and it is not a
view: no key, focus, or node in the view tree, and not a member of `tabs`.
Links do not follow edits, so send the layout again with new links after each
rediff; the keyed editors keep their views. A connector that does not name two
different editors of its layout is a `TypeError`.

## Ask the user transiently

`workbench` exposes window-local pickers, text input, and a native confirmation
prompt. Each operation captures an explicit `WindowTarget` or the
admitting command's window, so focus changes during an `await` cannot retarget
it. Results are typed: acceptance carries a value, dismissal is a normal value,
and operational failure is a host error.

```js
const picked = await workbench.showPicker({
  title: "Switch repository",
  items: [{ id: "knot", label: "knot", description: "/src/knot-editor" }],
});
if (picked.kind === "selected") await select(picked.id);

const named = await workbench.showInput({ title: "Rename", value: "knot" });
if (named.kind === "accepted") await rename(named.value);

const sure = await workbench.showConfirmation({ title: "Remove knot?" });
if (sure.kind === "confirmed") await remove();
```

Picker items are matched locally in the user's `completion.styles`, so the
complete list is supplied up front. When the rows depend on the query or are too
many to send, pass `onQuery(query)` instead of `items`, which returns the rows
for each query with the ranges to highlight:

```js
const picked = await workbench.showPicker({
  title: "Go to File",
  onQuery: async (query) => (await search(query)).map((hit) => ({
    id: hit.uri, label: hit.name, description: hit.directory, labelMatches: hit.ranges,
  })),
});
```

Pass `tabs` for scope tabs under the field. With `onQuery`, the callback
receives the active tab's ID as its second argument and runs again when the user
switches tabs. Tab and Shift+Tab switch tabs through the `picker.next-tab` and
`picker.previous-tab` commands, which `keymaps.bind(key, command, { view:
"picker" })` rebinds.

```js
const picked = await workbench.showPicker({
  title: "Search Everywhere",
  tabs: [{ id: "all", label: "All" }, { id: "files", label: "Files" }],
  onQuery: async (query, tab) => (await search(query, tab)).map(toRow),
});
```

Pass `onHighlight` to preview the highlighted item as the user moves through
the list. The callback runs inside the awaiting command, one call at a time, and
undoing a preview after dismissal is the caller's job:

```js
const before = await settings.get("theme.dark");
const picked = await workbench.showPicker({
  title: "Preview",
  items,
  onHighlight: async (id) => { if (id !== null) await settings.set("theme.dark", id); },
});
if (picked.kind === "dismissed") await settings.set("theme.dark", before);
```

An input's `onChange` works the same way for its text, starting with the
initial value. Return a string to show it as a muted line under the field, or
nothing to clear it:

```js
const named = await workbench.showInput({
  title: "Port",
  onChange: (value) => (/^\d*$/.test(value) ? null : "Type digits only."),
});
```

An `onChange` may instead return `{ message?, items? }` to show a completion
list under the field; each item's `value` is the whole new field text, and you
do the filtering and ordering. Match with `completion.match` rather than your
own filter, so the list follows the user's `completion.styles` setting, and
pass its `ranges` as an item's `matches` to highlight them:

```js
const words = ["alpha", "alps", "beta"];
await workbench.showInput({
  title: "Word",
  onChange: async (value) => ({
    items: (await completion.match(value, words)).map(({ index, ranges }) => ({
      label: words[index],
      value: words[index],
      matches: ranges,
    })),
  }),
});
```

To list recently chosen items first, keep a history under a name of your
choice, which is private to the package. `history.order` puts recorded items
first, so recent items lead the empty list without outranking a better match:

```js
const items = await history.order("projects", allProjects); // by item.id
const picked = await workbench.showPicker({ title: "Projects", items });
if (picked.kind === "selected") await history.record("projects", picked.id);
```

To recall earlier entries in an input, pass `history`, the package's own list
of earlier submissions, newest first; keeping and persisting it is the
package's job:

```js
const found = await workbench.showInput({ prompt: "/", title: "Search", history: searches });
if (found.kind === "accepted") searches = [found.value, ...searches.filter((s) => s !== found.value)];
```

A `prompt` such as `":"` shows before the field and makes Backspace on an empty
field dismiss the input, as a command line does; a `placeholder` shows muted in
the empty field. Confirmation and message use the native platform prompt.

One extension transient interaction may own a window at a time. A second
request rejects `OperationInProgress`. The command palette is independent and
may be open at the same time. Window closure, extension unload, and caller
cancellation settle a pending interaction as cancellation; focus returns to the
surface that was focused before it opened.

## Show an editor bar

A bar is a non-modal strip above an editor's text, for workflows that keep the
editor usable, such as Find. Open it on an editor handle; opening the same ID
again replaces what it shows, and fields that remain keep their text.

```js
const editor = await workbench.editor(target);
await editor.openBar("find", {
  fields: [{ id: "query", placeholder: "Find" }],
  toggles: [{ id: "case", icon: "find.case", tooltip: "Match Case", active: false,
              command: { name: "editor.find-toggle-case" } }],
  message: "3 of 17",
  buttons: [{ id: "close", icon: "find.close", command: { name: "editor.find-close" } }],
});
await editor.focusBarField("find", "query", { text: "foo" });
await editor.onDidEditBar(({ bar, fields }) => search(bar, fields.query));
```

A field may carry `history`, the package's earlier entries for it, newest
first, as of the call that opened the bar. Up replaces the focused field's text
with the next older entry that starts with the text typed before recalling
began (when that text is itself an entry, from that entry on), and Down
steps back toward newer ones and finally to the typed text; each replacement is an edit that `onDidEditBar` reports. A field without
history leaves Up and Down to the text.

Icons are names from `icons.define`. A click runs the command with the bar's
editor captured; flip a toggle by opening the bar again with the new `active`.
While a field has focus, bind keys with `keymaps.bind(key, command, { view:
"bar", context: "<bar id>" })`; Tab moves between fields. `buffer.find` supplies
native matching for highlights and navigation, literal or, with `regex: true`,
a Rust `regex` pattern in multi-line mode (an invalid one rejects
`InvalidQuery` with the parser's message in `details.message`); `perLine: true`
keeps at most one match per line.
`buffer.replace(query, replacement, options)` takes the same options and
replaces the matches natively in one commit (one undo step, one change
notification), expanding `$1`, `${name}` and `$$` in regex mode, with `\u` `\l`
and `\U` `\L` ... `\E` changing the case of what follows, and returns
`{ revision, count, more }`. The bundled
[`@knot/find`](../../extensions/@knot/find/) package is a complete example; the
precise contracts are in `knot.d.ts`.

## Show status items

Every window shows a status bar below the workbench. A package fills
it with `workbench.registerStatusItem`, which returns a handle; disposing it,
or unloading the package, removes the item.

```js
const position = await workbench.registerStatusItem("position", {
  text: "Ln 1, Col 1",
  tooltip: "Go to line",
  side: "right",
  priority: 10,
  command: { name: "editor.go-to-line" },
});
await position.update({ text: "Ln 4, Col 2", side: "right", priority: 10 });
```

An item has text, which hides it while empty and without an icon, and optionally an `icon` name
painted before the text (in the item's color), a tooltip, a command with JSON arguments that a
click runs in the window it was clicked in, and a theme color key for its
text. Its `placement` is the status bar (the default) or the `"toolbar"`, the row
at the top of the window; an item with only an icon makes a button. It sits on the `left` or `right` side; within a side, a higher `priority`
sits further left and equal priorities keep registration order. `update`
replaces the whole value, so pass every field each time. With
`{ window }` it sets that window's own value, which shadows the shared one
until `update(null, { window })` clears it. The latest registration of an ID
defines the item, and disposing it reveals the previous one, so personal
config can replace a bundled item by registering the same ID in `post-init`.

The `statusBar.visible` setting and the Toggle Status Bar command show or hide
the bar, and `statusBar.hiddenItems` lists IDs the bar skips. A hidden item
keeps its registration and its package keeps running. To follow the editor the
bar describes, observe `statusView` on each window, and locate it with
[`windows.findView`](application.md#inspect-and-change-application-state).

## Decorate resources

A package can color and badge any resource URI. Trees paint a decoration on
every row whose `resource` is that URI, and document tabs tint their title and
show the tooltip. Create a source, then set the URIs it decorates:

```js
const source = await decorations.createSource();
await source.set({
  "file:///repo/src/main.rs": {
    color: "diff.modified", // a theme color key; tints the label
    badge: { text: "M" },   // or { icon: "square-dot" }
    tooltip: "Modified",
    propagate: true,        // folders above take the color
  },
  "file:///repo/old.rs": null, // removes that URI's decoration
});
await source.clear();
```

`set` changes only the URIs it names, and disposing the source, or unloading
the package, removes everything it decorated. A decoration applies in every
window. Every field is optional. A text badge is one or two characters, and an
icon badge names an icon, so a pack can restyle it. When sources overlap,
`error`, `warning`, and `info` colors win, then the most recent source.
Decorations don't touch pickers or the status bar, and tabs show no badge. The
rules are in
[Resource decorations](../architecture/semantic-ui.md#resource-decorations).

## Tell the user without interrupting

`workbench.showNotice` raises a notice: a popup card in the window it targets,
or in every window when `target` is omitted, and an entry in the notification
history that outlives the popup and the package. It resolves with the notice's
ID once the notice is recorded and does not wait for the user. A question that
needs an answer is `showConfirmation`.

```js
await workbench.showNotice({
  title: "Indexing finished",
  message: "1,204 files in the workspace.",
  severity: "info", // "info" (default), "warning", or "error"
  actions: [{ title: "Open Log", command: { name: "app.open-log" } }],
});
```

An action dispatches its command in the popup's window and closes the popup.
`markNotificationsRead`, `dismissNotification`, and `clearNotifications` change
the history, which does not persist across restarts.

`workbench.observe` carries the history, so a package can mirror the notices in
its own surface; setting `notifications.popups` to false records notices without
native popups, so such a package can replace them with its own presentation.
