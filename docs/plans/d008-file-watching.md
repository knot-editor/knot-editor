# D008 — File watching and automatic reload

Status: 🚧 approved, not started. Item of the [daily-driver plan](daily-driver.md#phase-4--files-and-workspace).

## Outcome

Knot notices when files change outside it:

- A clean document whose file changes reloads in place. Only the changed lines
  are edited, so cursors, selections, folds, decorations, and syntax outside
  the changed hunks stay where they were, and Undo restores the previous text.
- A dirty document whose file changes is marked as changed on disk. When it is
  (or next becomes) the focused view, a native prompt offers Reload or Keep.
- A document whose file is deleted keeps its tab and text and becomes dirty,
  so saving recreates the file.
- Knot rechecks every open document when the application becomes active.
- Extensions watch any URI through `files.watch(uri, { recursive })` and get
  coalesced batches of changed URIs.

## Design

### Watching

- **Who watches.** Native code watches the file of every persisted open
  document. Extensions watch what they ask for through `files.watch`; nothing
  watches workspace roots by default. The D034 explorer watches its roots, and
  the [workspace file index](daily-driver.md#phase-4--files-and-workspace)
  (D173) becomes one more caller watching roots recursively.
- **Provider boundary.** `FileSystemProvider` gains a `watch` method. The local
  provider uses the `notify` crate (FSEvents on macOS, inotify on Linux). The
  memory provider reports its own writes, so lifecycle tests run without the
  OS.
- **Sharing.** A `knot-model` watch registry reference-counts OS watches by
  URI and recursion. A watch already covered by a recursive watch of an
  ancestor reuses it, so a document inside a watched root or two extensions
  watching the same root cost no extra OS watch.
- **Document files.** A document watches its parent directory non-recursively
  and filters events to its file, because Git, formatters, and atomic saves
  replace files by rename, which loses a watch on the file itself.
- **Events.** A batch of changed URIs with no created, changed, or removed
  kind, coalesced over about 100 ms. Listeners stat or list what they care
  about. When the OS reports dropped events, the batch names the watched URI
  itself. Entries the `files.hidden` setting names are dropped, so watching
  agrees with `files.list`; ignored entries are still reported, because the
  explorer shows them marked. Recursive watching does not descend into hidden
  directories where the platform watches per directory.
- **Lifecycle.** `files.watch` resolves a `Disposable` owned by the calling
  lifecycle, like `settings.observe`. Unload and restart dispose it.

### Detecting document changes

- An event that names a document's file, and application activation for every
  persisted document, stats the file and compares the provider version with
  the document's recorded one. An equal version is Knot's own save or no
  change, and nothing happens; there is no time window for ignoring saves.
- When the version differs, Knot reads the file. Text equal to the saved
  content only records the new version (a `touch`, or a write of the same
  bytes).
- Otherwise a clean document reloads, and a dirty one is marked as changed on
  disk.

### Reload

- Reload computes a line diff (`imara-diff`) between the buffer and the file
  and applies the hunks through `BufferModel::apply_edits` as one commit and
  one undo step, then records the new provider version and revision as the
  saved state.
- It revalidates the document and the buffer revision captured before the
  read, as the conflict Reload does today; edits that race the read win, and
  the document is then dirty and marked as changed on disk.
- Save's conflict Reload uses the same path.

### Dirty and deleted documents

- The changed-on-disk mark is document state that records the version it was
  raised for. The tab shows it.
- When the marked document is focused, or already focused when the mark is
  raised, a native prompt offers Reload or Keep. Reload discards the buffer's
  changes through the same diff reload, which Undo can restore. Keep leaves
  the buffer, clears the mark, and does not prompt again for that version; the
  recorded provider version is unchanged, so the next save reaches the
  existing conflict prompt.
- A deleted file turns a clean document into a dirty document associated with
  a missing file, the state Open uses for a missing file; the tab shows it as
  deleted and stays open. A dirty document stays dirty and is marked. When the
  file reappears, the ordinary comparison applies.

## Exclusions

- A Show Difference option in the Reload or Keep prompt. Deferred until a diff
  view exists.
- Refreshing the native workspace tree. D034 replaces it.
- Watching workspace roots by default and a native file index (D173).
- Rename and deletion performed through Knot (D010).
- Created, changed, and removed kinds in watch events.
- A setting to turn off automatic reload.

## Decisions to record

When validated, add a "File watching" entry to the
[documents and persistence decisions](../decisions/documents-and-persistence.md)
and its index line: watches requested by documents and extensions rather than
recursive root watches (the IDEs watch roots because they keep a tree snapshot,
which D173 adds), version comparison rather than a save time window, a full
line diff rather than whole-text replacement or common prefix and suffix
(VS Code and Emacs), and a prompt on focus rather than silence until save
(VS Code) or an immediate prompt.

## Checkpoint 1 — Watch registry and `files.watch`

Deliverables:

- `FileSystemProvider::watch` on the local and memory providers.
- The shared, reference-counted watch registry with coalescing, rescan
  batches, and hidden-entry filtering.
- `files.watch(uri, { recursive })` in `knot.d.ts`, the JavaScript facade, and
  a host handler, owned by the calling lifecycle.
- A `try.watch` personal-config command that watches the first root and logs
  batches.

Validation:

- `knot-model` tests over the memory provider: coalescing, sharing under a
  recursive ancestor, disposal releasing the OS watch, hidden filtering, and
  the rescan batch.
- A local-provider test over a temporary directory, including a rename-based
  replace.
- A bundled-package test (`scripts/extensions` runner) that watches a
  directory, writes a file, receives the batch, and stops receiving after
  dispose and after unload.

Review gate: run `try.watch`, edit and create files in a terminal, and check
batches arrive coalesced without `.git` noise.

**Implementation context**

- Provider trait and types: `crates/knot-model/src/filesystem/mod.rs`;
  implementations in `filesystem/local.rs` (which already owns a blocking I/O
  pool) and `filesystem/memory.rs`. Add `notify` to the workspace
  `Cargo.toml` and `crates/knot-model/Cargo.toml`.
- Hidden filtering reuses the ignore policy in `crates/knot-model/src/files/ignore.rs`,
  built by `crate::app::settings::files_ignore_policy`.
- Expose the registry as an application gpui `Global` registered from
  `src/app/mod.rs`; batches reach the foreground through a channel drained by
  a foreground task.
- Mirror `settings.observe` for the API: `observeSetting` and
  `__knotDispatchSettingChange` in `crates/knot-host/src/js/bootstrap.js`,
  the `observeSetting`/`unobserveSetting` handlers in
  `src/app/extensions/host/handlers.rs`, and `observe_setting` in
  `src/app/extensions/host.rs`, including lifecycle teardown.
- Document the contract beside `files` in `crates/knot-host/src/js/knot.d.ts`
  and regenerate `docs/schemas/knot.schema.json` if it covers the namespace.
  Update [URI providers](../architecture/documents-and-persistence.md#uri-providers)
  and the extension guide's files section.

## Checkpoint 2 — Clean reload and recheck on activation

Deliverables:

- Every persisted document holds a native watch from insertion or a first
  save until removal, and moves it when Save As changes its resource.
- Change detection by version, then content, on events and on application
  activation.
- Diff reload through `apply_edits` as one undo step; Save's conflict Reload
  uses it.
- A `try.reload` command that rewrites the focused document's file outside the
  buffer.

Validation:

- `knot-model` tests for the line diff to edits conversion, including CRLF
  buffers, trailing newline changes, and empty files.
- Product tests over the memory provider: an external write reloads a clean
  document, keeps a cursor and fold below an unrelated hunk, and is one undo
  step; Knot's own save triggers nothing; a same-content write only records the
  version; an edit racing the read wins; activation recheck reloads a document
  whose event was missed.

Review gate: edit an open file with another tool, run `git checkout` on an
open file, and confirm cursor, folds, and Undo behave.

**Implementation context**

- Document state and persistence: `crates/knot-model/src/documents.rs`
  (`DocumentState`, `persisted_version`, `begin_persistence`,
  `complete_persistence`) and its application wrapper `src/app/documents.rs`,
  which already notifies recovery and observation on persistence changes.
- Put the diff-to-edits conversion and the detection decision (same version,
  same content, reload, mark) in `knot-model` as plain functions; the shell
  reads, revalidates, and applies.
- Reload today: `start_conflict_reload` in `src/app/product/save.rs`, which
  shows the revalidation and `finish_persistence` sequence to keep.
- Multi-edit commits: `BufferModel::apply_edits` in
  `crates/knot-model/src/buffer.rs`.
- Application activation: `cx.observe_window_activation` in
  `src/app/product.rs` (`observe_extension_host`) is the existing hook.
- Read with `load_resource` from `src/app/product/open.rs`.
- Update [saving](../architecture/documents-and-persistence.md#saving), which
  says watching is deferred.

## Checkpoint 3 — Dirty and deleted documents

Deliverables:

- The changed-on-disk mark and the deleted state, shown on the tab.
- The Reload or Keep prompt when a marked document is focused.
- Deletion of a clean document's file turns it into a dirty missing-file
  document; reappearance re-runs the comparison.
- `try.conflict` and `try.delete` commands that dirty the focused document and
  change or delete its file.

Validation:

- `knot-model` tests for the mark: raised per version, cleared by Keep, not
  raised again for a kept version, raised again for a newer one.
- Product tests: Keep leaves the buffer and the next save reaches the conflict
  prompt; Reload is undoable; a deleted clean file keeps the tab and makes the
  document dirty; saving recreates it; a dirty document is marked rather than
  reloaded.
- `just check`.

Review gate: run the `try.*` commands, confirm the prompt appears only on
focus and only once per version, then finish the plan: update the
architecture reference, record the decision, mark D008 ✅ in the daily-driver
plan, and remove this plan.

**Implementation context**

- Tab rendering: `src/app/product/render.rs` already appends the dirty marker
  from `DocumentDirty::is_dirty`.
- Prompts use the native warning prompt, as `prompt_for_save_conflict` in
  `src/app/product/save.rs` does.
- The missing-file state is the destination-associated document that Open
  creates for a missing file (`insert_destination` in
  `crates/knot-model/src/documents.rs`).
- Focus changes: the session's focus routing in `src/app/product/` and the
  `activeTargetChanged` observation path show where focus moves are known.
