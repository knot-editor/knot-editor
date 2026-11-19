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

Use `documents.open` for files, `createUntitled` for new documents, and `present`
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
events describe committed metadata changes; read text through buffer APIs.
`viewStateChanged` reports a window whose cursor, selection, scroll, focus,
dock sizes, bounds, or structure settled about a second after a change. Packages
activate before startup has restored windows: when the snapshot's `launched` is
false, wait for the one-time `launched` event before reconciling state against
the startup windows.
Listeners can await host work or their own disposal because commits do not wait
for listeners. A query may already see newer state than the event. Dispose and
observe again when a fresh baseline is needed.

The bundled [project registry](../../extensions/@knot/projects/src/) is a
complete observer: it waits for `launched`, then tracks windows and documents.

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

`files.watch(uri, listener, { recursive })` calls the listener with batches of
changed URIs. A batch does not say whether an entry was created, changed, or
removed, so stat or list what you need, and read the current state after
awaiting the watch. The returned `Disposable` and the lifecycle end it.

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
