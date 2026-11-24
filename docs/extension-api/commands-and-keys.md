# Commands, keys, and menus

Part of the [extension guide](README.md).

## Register a command

Import the public API from `"knot"` and register handlers during package
activation:

```js
import { commands } from "knot";

await commands.register("example.insert-greeting", async ({ buffer }) => {
  if (buffer === null) return;
  const snapshot = await buffer.snapshot();
  await buffer.applyEdits([{
    range: { startByteOffset: 0, endByteOffset: 0 },
    text: "Hello ",
  }], { ifRevision: snapshot.revision });
}, { title: "Insert Greeting", description: "Insert a greeting at the start." });
```

The command palette lists the title, which defaults to the command name, with
its description and binding, and still matches the name as typed. Pass
`palette: false` for a command that only makes sense from a key.

The buffer belongs to the command’s captured context. Focus changes during
`await` do not select another buffer. Offsets are UTF-8 bytes; edits require
the revision read from a snapshot. Completion and change callbacks receive an
explicit buffer to use outside commands. Lines are 0-based, and a buffer
answers line lookups (`lineRange`, `lineAt`, `lines`) directly, so a large file
never crosses the boundary whole. `buffer.createMark(offset)` returns a mark
that follows edits.

A command run in a document editor also receives `context.editor`, a handle on
that editor; it is null for terminals, trees, and text frames. It reads and
moves the selection, saves and restores the view, and answers display queries
for motions: rows and the viewport, display cells, and folds.

```js
await commands.register("example.go-to-start", async ({ editor, buffer }) => {
  if (editor === null) commands.unavailable();
  const before = await editor.viewState();
  const line = await buffer.lineRange(0);
  await editor.setSelection(
    { anchor: line.startByteOffset, caret: line.startByteOffset },
    { reveal: "center" },
  );
  if (!(await confirm())) await editor.restoreViewState(before);
});
```

A click on an editor contribution's gutter mark or text runs the
contribution's `command` in the clicked editor, so `context.editor` and
`context.buffer` are that editor and its buffer. Its `arguments`, an object,
gain `range`, the contribution's current range after edits. A click in a text
frame runs the command with a null editor and buffer.

`commands.unavailable()` ends a handler with the `unavailable` outcome, as
native commands report a target they do not apply to. Like `context.target`,
the handle works only while its command runs and rejects `InvalidTarget` once
the editor's tab closes or moves.

An editor can also outlive the command: `workbench.editor(target)` takes the
`ViewNodeTarget` of an editor view, at any depth of a tab's view tree, from
observation, and returns the same editor bound to that exact view. It never
follows focus or a replacement, and rejects a tab target or a view that is not
an editor, so a mode obtains a new handle from the next observation, and
`onDidClose` runs once when the view leaves its tab's tree or its tab closes.
A handle also sets the editor's key context and cursor style, as a modal
package does, and listens for selection changes.

```js
const { snapshot } = await workbench.observe(() => {});
const editor = await workbench.editor(snapshot.windows[0].focusedView);
await editor.setKeyContext("normal", true);
editor.onDidClose(() => { /* drop per-editor state */ });
```

`editor.beginUndoGroup()` makes everything committed to the editor's buffer
until the returned group is disposed one undo step, so a mode that changes text
and then lets the user type, such as Vim's `ciw`, opens the group before the
change and disposes it on leaving insert mode. `editor.record()` records the
keyboard input the editor receives as plain data, so a package can replay it.

```js
const group = await editor.beginUndoGroup();
await buffer.applyEdits([{ range, text: "" }], { ifRevision: revision });
// ...the user types; later, on Escape:
await group[Symbol.asyncDispose]();

const recording = await editor.record();
// ...the user types; later:
const entries = await recording.stop();
for (const { command, arguments: args } of entries) {
  await commands.invoke(command, args); // children target the same editor
}
```

Registering commands, providers, and keymaps is valid during activation;
contextual lookup and command invocation require a running command. Await
disposal and view invalidation to observe completion, and use host error `code`
values when handling failures. The API reference has the precise rules.

## Bind keys and modal contexts

`keymaps.bind` binds a key to a command in the calling package's lifecycle.
Options scope it to a view kind and, within it, to a key context that a
command sets on its editor, which is enough for modal editing:

```js
import { commands, keymaps } from "knot";

const normal = { view: "editor", context: "normal" };
await commands.register("me.normal", ({ editor }) => editor?.setKeyContext("normal", true));
await commands.register("me.insert", ({ editor }) => editor?.setKeyContext("normal", false));
await commands.register("me.typed", ({ arguments: { text } }) => { /* a key nothing binds */ });
keymaps.bind("escape", "me.normal", { view: "editor" });
keymaps.bind("i", "me.insert", normal);
keymaps.bindText("me.typed", normal);
```

The binding whose scope requires the most facts wins, so `i` in the `normal`
context beats a plain editor binding, and the latest write breaks ties.
`bindText` receives typed text, after keyboard layout and IME, that nothing
binds as a key, so `$` arrives as `$` on any layout; without it, the context
types natively. Keys typed while a key-invoked extension command runs wait
for it and then apply in order, so a mode switch is in effect for the next
key however fast it is typed. A mode usually also sets
`editor.setCursorStyle("block")` and clears it on leaving, and may read the
clipboard with `workbench.readClipboardText()`, which resolves to `null` when it
holds no text. The bundled [Vim package](../../extensions/@knot/vim/) is a
complete modal editor on these primitives.

Keybinding registration through `keymaps.bind` and `keymaps.unbind` is
synchronous; mutations from one V8 turn apply as one atomic batch, and invalid
input fails the batch. Bindings sit above the selected keymap preset and the
latest write wins, so personal overrides of package bindings belong in
`post-init`. Show Keybindings lists every binding with its owner.

A preset is a named set of default bindings that replaces Knot's when the
`keymap.preset` [setting](settings.md#read-and-write-settings) selects it; every
`keymaps.bind` still wins over it. Presets are code, so derive one by
spreading another and branch on `platform` for OS-specific keys:

```js
import { keymaps, platform, settings } from "knot";

const knot = await keymaps.getPreset("knot");
const mod = platform === "macos" ? "cmd" : "ctrl";
await keymaps.registerPreset({
  name: "mine",
  title: "Mine",
  bindings: [
    ...knot.bindings,
    { key: `${mod}-shift-d`, command: "me.insert-date", view: "editor" },
    { key: `${mod}-k down`, command: null },
  ],
});
await settings.set("keymap.preset", "mine");
```

A binding needs `command`, which is a name, a `{ name, arguments }` object, or
`null` to unbind. The latest registration of a name wins and teardown reveals
the previous one; `"knot"` cannot be replaced. Knot resolves the selection
when startup completes and again whenever the setting changes, so registering
the selected preset later takes effect at the next change, and an unregistered
name selects Knot.

Two top-level facts describe the environment beside the namespaces above.
`platform` is `"macos"`, `"windows"`, or `"linux"`, for OS-specific
configuration like the preset example. `homeUri` is the user's home directory
as a normalized `file:` URI, in the same form document and resource URIs use,
so plain string comparison applies; it is `null` when no home directory is
known.

## Compose commands

`await commands.invoke(name, arguments)` routes a child through the originating
command's captured target and returns a structured outcome. Native and registered
JavaScript commands use the same route. Check `outcome.kind`; command return values
do not become semantic results. A participant can decline, and a valid invocation
can return `unavailable` for a command unsupported by its captured surface. Calling
from activation or unrelated callbacks rejects `ContextUnavailable`.

`await commands.list(view?)` returns the commands the palette lists as `{ name,
title, description, key }`, sorted by title, so a package can offer them in its
own picker and run the chosen one with `invoke`. `key` is the binding shown for
`view`'s kind of view, formatted for display, or null.

The checked [activation example](../../scripts/extensions/examples/activation.js)
includes `guide.compose`, which awaits another JavaScript command, and
`guide.select-all`, which awaits the native `editor.select-all` command. The
contributed view's `copy` registration shares the native command name and handles
it only when that view instance claims the command. Dispose these registrations
when removing the feature; native definitions retain their application ownership.

## Concurrency

A package's code runs as Node's does: one task at a time, and other work runs
whenever yours is parked at an `await`. A command waiting on a picker, a timer,
or a host call does not block your listeners, providers, or other commands,
and two commands, including two of your own package, may overlap. State read
before an `await` may have changed after it, so revalidate it, and a package
whose commands must not overlap (modal key handling, for example) takes turns
itself, for example by chaining invocations on a promise. A command keeps its
captured target across awaits, and a listener or timer never has one. A
request a command leaves in flight when it settles runs without command
context.

## Add menu items

`menus.add(location, item)` puts a command in a menu and returns a handle;
disposing it, or unloading the package, removes the item. The command may be
native or a package's own, and one command can appear in several places with
different arguments.

```js
await menus.add("tab", {
  command: "my.copy-title",
  title: "Copy Title", // defaults to the command's title
  group: "3_copy", // groups sort alphabetically, with a separator between them
  order: 0, // within a group; then by title
  view: "editor", // optional: only on editor tabs
});
```

The `location` is `"tab"` (the right-click menu of a tab header, in the center
or a dock), `"content"` (the right-click and Shift+F10 menu inside an editor or terminal),
or a menu bar menu (`"menubar.file"`,
`"menubar.edit"`, `"menubar.view"`, `"menubar.go"`); any other name is a
`TypeError`. `view` (`"editor"`, `"terminal"`, or a contributed view ID) and
`context` (an editor key context, with `view`) show an item only on matching
surfaces, as for keybindings; menu bar locations reject both. Ungrouped items
come last, and registration order never affects a menu.

A menu runs its command on the surface that was clicked, without focusing it
or changing the active tab, so a command reads `context.editor` and
`context.buffer` rather than the window's active file. A command that does not
apply returns `commands.unavailable()` and the click does nothing; menus have
no disabled state. Bundled `@knot/chrome` fills the tab and content menus this way.
