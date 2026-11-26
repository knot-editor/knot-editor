# Application state, files, and storage

Part of the [extension guide](README.md).

## Inspect and change application state

Use `documents` for metadata and shared buffers, `windows` for live
presentations, and `workspaces` for window-owned roots. Queries work during
activation and callbacks, including before windows exist. Document IDs
survive Save As; across restart only a document restored from automatic recovery
keeps its ID. Retain returned live target objects rather than serializing or
reconstructing them. `windows.focus(target)` activates a window, and
`windows.open({ roots })` opens a new one with those roots, closing it again if
a root is rejected.

A tab's `root` is its tree of views: each `ViewNodeInfo` has a `target`, a
`kind`, its `children`, and for an editor its `document` (null for a buffer
outside document identity), `buffer`, `language`, and `readOnly`. A composite view lists
its views as children. `WindowInfo.focusedView` is the focused leaf of
the focused tab and `statusView` the editor a status display describes, so
follow `statusView` rather than `statusTarget` to see a diff side. Never search
`pane.tabs` for a view: `windows.findView(info, target)` takes a `ViewNodeTarget`
or a tab's `ViewTarget` and returns `{ view, parents, tab, pane }`, with
`parents` nearest first and the tab's root last, or null when the window does
not show it.

```js
const info = await windows.get(window);
const place = info.statusView && windows.findView(info, info.statusView);
const file = place?.view.document ?? null;
```

Use `documents.open` for files, `documents.load(uri)` for a file's document that
a view of yours will show instead of a tab (it closes with the last view that
shows it, and one no view ever shows stays open until closed), `createUntitled` for new documents, and `present`
for another presentation of a live document. These return committed document and
presentation data. `documents.save(id, { uri? })` saves without a dialog, as Save
and Save As do: with `uri` it replaces whatever file is there and the document
becomes that file's. `documents.reload(id)` replaces the text of a document with
its file's as one undoable edit. Pass `{ preview: true }` to `documents.open` when the user is
browsing, as a file list's single click does: the document replaces the pane's
current preview tab instead of adding one, until the user edits, pins, or
double-clicks it. The `workbench.previewTabs` setting turns previews off. Use `workspaces.openFolder` to replace a window's roots with
one folder, or `replaceRoots` with an ordered list. Retained roots keep their IDs,
so add, remove, and reorder by replacing the list read from `workspaces.get`;
duplicates collapse after normalization and nested roots are allowed. Documents
remain independent of workspace containment. A missing file opens as a dirty destination-associated
document rather than failing. Use `workspaces.pickFolder` when a workflow asks
the user for a folder:

```js
const folder = await workspaces.pickFolder();
if (folder.kind === "selected") await workspaces.openFolder(folder.uri);
```

The picker reuses the native directory prompt, owns the window's transient
interaction while open, and returns a provider-normalized directory URI. Dismissal
is a normal result; a missing directory rejects `ResourceNotFound` and a file
rejects `ResourceWrongKind`.

Use `terminals.open` to open a terminal tab, optionally starting in a local
directory:

```js
await terminals.open({ cwd: "file:///Users/me/project/" });
```

Without `cwd` the shell starts in the home directory; choosing a better
directory is the caller's policy. `@knot/chrome`'s New Terminal uses the focused
terminal's directory (`PresentationInfo.directory`), else the root containing
`workbench.lastFocusedDocument(window)`, else the first root.

`location` chooses where the tab opens. `"editor"`, the default, opens it in
the last focused center pane. A container view's ID opens it beside the
captured terminal when that is in the container, else in the container's last
focused pane, which it shows. A container is a view registered without
`content`; its optional `newTab` command runs from the `+` button and when the
window is shown empty.

Pass an explicit live target when acting on a previously selected window, pane,
or presentation. Omitting it captures the command's originating window, or the
active window outside a command, at admission. It never waits for a
future window or follows later focus. Stale targets reject before commit.
Discoverable targets and invocation-scoped command targets are different objects;
consult the declarations for accepted target types.

`workbench.close` uses native protected closure, including dirty-document prompts.
Check its closed/cancelled result. Final document closure invalidates retained
buffers; closing a terminal stops its session. A save already accepted during
closure can remain committed even if the later close cancels.

For change-driven features, call `workbench.observe` before querying separately:
it returns an atomic initial snapshot, sequence watermark, and disposer. Later
events describe committed metadata changes; read text through buffer APIs. `documentMoved`, which follows a
`documentChanged`, reports a document whose file changed, as Save As and
`files.rename` do, with the URI it had.
`viewStateChanged` reports a window whose cursor, selection, scroll, focus,
dock sizes, bounds, or structure settled about a second after a change. Packages
activate before startup has restored windows: when the snapshot's `launched` is
false, wait for the one-time `launched` event before reconciling state against
the startup windows.
Listeners can await host work or their own disposal because commits do not wait
for listeners. A query may already see newer state than the event. Dispose and
observe again when a fresh baseline is needed.

To follow every open document, use `documents.track` instead of filtering
`workbench.observe`. Its listener runs for each document already open, then
for each one opened later, and registers `onDidChange` and `onDidClose` on the
tracked document. `onDidChange` reports a change of kind, file, dirty state, or
language with the document now and before; revision-only changes are silent, so
read text through the buffer. Disposing the tracker ends tracking and fires
`onDidClose` for what was still followed. Callbacks arrive in observation order,
one at a time, but are not ordered against a separate `workbench.observe` of
the same package, so reconcile against live queries, not event order. Tracking
does not wait for `launched`.

The bundled [project registry](../../extensions/@knot/projects/src/) is a
complete observer: it waits for `launched`, then tracks windows with `workbench.observe` and documents with `documents.track`.

## Own a buffer

`buffers.create({ text, language, readOnly, persist })` makes a buffer that
belongs to the package rather than to a document: it is never dirty, has no
closing prompt, and Save does not touch it. The usual `Buffer` operations work
on it, and a read-only one rejects edits with `BufferReadOnly`. It ends when
you dispose it (`await using` works) or when the package unloads.

A `persist` key keeps the text in the package's storage under that key, so a
buffer created with it after a restart starts from the stored text instead of
`text`. Writes follow edits after a short delay and are forced at disposal,
unload, and shutdown; disposal keeps the text and `clearPersisted()` removes
it. An empty buffer stores nothing, and a key has one live buffer at a time.

```js
const draft = await buffers.create({ language: "markdown", persist: "commit-message" });
// ... after the commit succeeds:
await draft.clearPersisted();
```

## Compare texts

`diff(old, updated, options?)` returns the line hunks that turn one text into
another. Each side is a string or a live buffer, which is read natively, so its
text never crosses the boundary:

```js
const { hunks, newRevision } = await diff(baseline, buffer);
for (const { old, new: now } of hunks) {
  // Lines old.start..old.end were replaced by now.start..now.end.
}
```

Ranges are 0-based, half-open line numbers, as `buffer.lines` numbers them. An
empty `old` range is an insertion before `old.start`, an empty `new` range a
deletion. Hunks are ascending and disjoint, and the result carries the revision
of each side that was a buffer. Lines compare exactly, terminators included, so
convert a baseline to the working-tree form first (for Git,
`git cat-file --filters`) or pass `{ ignoreWhitespace: true }`, which also
ignores `\r\n` against `\n`. The comparison runs off the foreground, has no
size limit, and reports no changes inside a line.

### Show the differences

A package that requires `@knot/diff` opens a diff tab through its contract. Each
side is an open document, which the tab edits live, or a read-only snapshot. The
tab opens side by side or, as the `diff.layout` setting says, inline, with the
original's changed lines above the modified text; its toolbar and
`diff.toggleLayout` switch it, and `diff.nextHunk` and `diff.previousHunk` move
between hunks. The tab keeps its bands, strip, and scrolling current as either
side changes, and is not restored with its window. `setText` throws for a
document side or a closed tab, and unloading the package closes its tabs.

```js
import { diffs } from "@knot/diff";
const client = await diffs.client();
const id = await client.open({
  original: { label: "HEAD", text: baseline, language: "typescript" },
  modified: { label: "Working copy", document: document.id },
  line: 41,
});
await client.setText({ id, side: "original", text: newBaseline }); // a snapshot side only
await client.close(id);
```

## Read files

`files` reads stored resources through the registered resource providers, for
any URI with a provider, whether or not a workspace root contains it. It reads,
lists, and stats; [changing files](#change-files) is separate.

```js
const root = (await workspaces.get(target)).roots[0].uri;
const entries = await files.list(root);
const names = entries.filter((entry) => entry.kind === "file").map((entry) => entry.name);
if ((await files.stat(`${root}/.editorconfig`)) !== null) {
  const text = await files.readText(`${root}/.editorconfig`);
}
```

`list` omits hidden entries and marks entries that the ignore files or the
`files.ignored` setting match with `ignored: true`, so an explorer can dim them.
`files.search(uri, query, { limit })` matches the query against the paths of the
visible, non-ignored files below a directory, so a picker searches again on
every keystroke rather than holding the list of files.

`files.watch(uri, listener, { recursive, includeHidden })` calls the listener
with batches of changed URIs. Entries the `files.hidden` setting names are left
out unless `includeHidden` is set, as a watch of `.git` needs. A batch does not
say whether an entry was created, changed, or removed, so stat or list what you
need, and read the current state after awaiting the watch. The returned `Disposable` and the lifecycle end it.

`readText` decodes UTF-8 and drops a leading byte-order mark. It reads what is
saved, never an open document's buffer, so unsaved edits are invisible; use
`documents` and `Buffer.snapshot` for live text. `list` returns one directory in
no particular order, so sort and filter in JavaScript. `stat` resolves `null`
for a missing resource instead of rejecting.

## Change files

`files.writeText(uri, text)` creates or replaces a file, and any missing parent
directories, without touching an open document of it. `files.createFile(uri)`,
`createDirectory(uri)`, `rename(from, to)`, `copy(from, to)`, and
`delete(uri, { permanent })` change stored resources through the same
providers. Apart from `writeText`, nothing is overwritten and missing parent
directories are created: creating, renaming, or copying onto an existing entry
rejects `ResourceAlreadyExists`,
and replacing one is a `delete` followed by a `rename` the caller chooses.
`delete` moves the entry to the operating system's trash unless `permanent` is
set, and rejects instead of deleting for good where the platform has no trash.
None of them asks for confirmation, so an explorer confirms before calling.

Open documents follow what Knot did: after a `rename`, every document at or
under the old URI keeps its text, history, and views under the new URI, and
`delete` turns the documents it affects into deleted ones, which keep their
text and recreate the file when saved. A rename made by another program is not
followed; its documents become deleted ones.

```js
const created = await files.createFile(`${directory}/notes/todo.md`);
const renamed = await files.rename(created, `${directory}/notes/done.md`);
const copy = await files.copy(renamed, `${directory}/notes/copy.md`);
await files.delete(renamed);
```

`files.reveal(uri)` shows a local file or folder in the platform's file
manager.

## Run programs

A package that declares the `process` entitlement runs programs with
`processes`. One that did not rejects `EntitlementNotDeclared`; the user decides
on the entitlement before the package loads, so declaring it is enough.
`processes.run` resolves once the program exits, with its exit code (`null`
when a signal ended it), the signal, and its output:

```js
const { exitCode, stdout, stderr } = await processes.run({
  command: "git",
  args: ["status", "--short"],
  cwd: "file:///Users/me/project/",
  env: { GIT_OPTIONAL_LOCKS: "0", LANG: null },
  stdin: "",
});
```

`command` is looked up on `PATH`. Knot resolves the environment the programs
inherit from the user's login shell when it starts, so a Finder launch finds
Homebrew's tools; `env` is merged over it, and `null` unsets a variable. `cwd`
is a file URI and defaults to the home directory. A missing program rejects
`ProgramNotFound`, a missing directory `WorkingDirectoryNotFound`.

`processes.spawn` takes the same options without `stdin` and returns a handle
to stream the output and control the program:

```js
const child = await processes.spawn({ command: "tail", args: ["-f", log] });
child.onStdout((text) => append(text));
await child.write("input");
await child.kill();
const { exitCode, signal } = await child.exited;
```

Output is UTF-8 text. A listener's chunk is acknowledged before the next is
read, so a slow listener slows the program instead of buffering its output, and
output that arrives before a listener attaches waits for one. A program that
writes more than its pipe holds does not exit until its output is read.

Each program starts in its own process group, which `kill` signals and which
Knot kills when the package unloads or restarts or Knot quits, so helpers a program started die with it. Cancelling a command does
not kill its programs; abort its `signal` to stop them, as in the
[processes example](../../scripts/extensions/examples/processes.js).

A handle that is no longer needed is released with `await child[Symbol.asyncDispose]()`,
which kills the group when it still runs. A finished handle whose streams were
both read to their ends is released automatically; anything else keeps its entry
until the package unloads. So a program that spawns many processes disposes
each one, as the example does.

## Acknowledge cleanup and handle failures

Await disposal to observe native release. It makes an object locally inactive
immediately and repeated calls share the same result, including failure. A
callback may await its own disposal. Await tree invalidation for native acceptance;
fetching and painting happen separately. Lifecycle teardown releases registrations
without requiring a final callback.

Handle `KnotHostError` by stable `code`, as in the
[error example](../../scripts/extensions/examples/errors.js), rather than matching
message text. Narrow optional diagnostic `details` before reading fields. On
`RevisionConflict`, choose whether to abandon work or deliberately read a new
snapshot. Cancellation does not roll back a commit that already succeeded.

If a package stops responding, the palette's Developer: Restart Extension…
stops and reloads it, re-reading an installed package from disk.

## Durable storage

`storage` keeps private, application-global JSON records for the package's
canonical identity; personal config has its own owner. Storage works during
activation without a window and survives reload, updates, and uninstall.

```js
import { storage } from "knot";
const result = await storage.read("preferences");
const preferences = result.found ? result.value : { schema: 1 };
await storage.write("preferences", preferences);
await storage.write("optional", null);
await storage.delete("obsolete");
const keys = await storage.list();
```

Missing records differ from stored null. Keep related state in one record for
atomic replacement: read/modify/write across calls is not atomic. Extensions own
value shapes and schema versions. Awaited writes acknowledge commit; rejection
following cancellation does not establish whether a write committed. Damaged or
incompatible databases fail without replacement.

The bundled [project registry](../../extensions/@knot/projects/src/) keeps
its records in storage through public APIs only. Precise JSON validation,
ordering, cancellation, and failure contracts live in the TypeScript reference;
SQLite ownership and shutdown belong to
[host architecture](../architecture/extension-host.md#extension-record-storage).

## Retained document captures

`recovery` keeps durable copies of open documents' text for workflows that must
restore them later, such as project sessions. Native unsaved-document protection
works without it; use it when your own records need to reference exact text.

```js
import { recovery, storage } from "knot";
const capture = await recovery.retain(document.id); // durable once resolved
await storage.write("session", { schema: 1, capture: capture.id });
await recovery.release(previous.capture); // only after the new record committed
const restored = await recovery.restore(capture.id);
```

Captures belong to your package's canonical identity; you never pass an owner,
and other packages cannot see or release yours. They are immutable, survive
unload, failure, restart, and uninstall, and are removed only by `release` or by
the user discarding them from File › Recover Unsaved Documents…, which lists
them with their owner. Reconcile `recovery.list()` with your records to release
captures a cancelled call left unreferenced. Restore opens an ordinary document
with an empty undo history and never writes the file; its `outcome` reports
whether the text replaced nothing (`restored`) or opened beside a live document
(`preserved`). Exact outcomes and errors live in the TypeScript reference.

## Project sessions

`workbench.replaceWindows` closes a set of windows and opens their replacements
in one native commit, without prompts. It is how `@knot/projects` switches
projects, and any package can build the same workflow. The caller first retains
the text of every document whose last view it removes, captures each window's
layout, records both durably, and then asks for the replacement:

```js
const capture = await recovery.retain(document.id);
const layout = await workbench.captureLayout(window.target);
await storage.write("session", { windows: [{ entries: [{ uri, document: document.id, capture: capture.id }], layout }] });
const result = await workbench.replaceWindows({
  suspend: [{ document: document.id, capture: capture.id }], // every document in the released windows
  windows: [{ roots: [root], entries: destinationEntries, layout: destinationLayout }],
  release: [window.target],
});
```

Every destination window is created with its layout and geometry, and every
released window closes as any window does, stopping its terminals. A layout is
plain versioned JSON: store it verbatim. Its document tabs find their entry by
document identity, else by file; unmatched documents and tools no longer
registered are pruned, splits of one document come back as independent views of
one shared document, and terminal tabs return in place as fresh shells. Each
window reports whether its layout was `applied` or `ignored`, the latter for an
unsupported version, in which case its entries open as without a layout.

Files load and captures restore before the commit, which then rechecks that
nothing changed since your captures: an edit after retain, an unexpected tab, or
a closed window rejects with `SessionConflict` and leaves every window intact, so
retain again and retry. Each entry reports its outcome. A live document matching
the entry's identity or file always wins over its capture, so a replacement
never opens a second copy of an open document. Missing clean files and entries that
fail to load are skipped and reported while the rest commits. When `protected`
is true, restored dirty text is already covered by automatic recovery and the
captures you restored from can be released once your new record is durable.

The bundled `@knot/projects` package builds project switching on these calls;
its behavior is described under [projects](../architecture/workbench-and-lifecycle.md#projects).
