# D010 — Filesystem mutation API

Status: 🚧 checkpoint 2 awaiting review. Item D010 of the
[daily-driver plan](daily-driver.md), which names it as a dependency of the
explorer extension (D034 / D053).

## Goal

Extensions can create files and directories, rename or move, and delete
through the resource providers: `knot.files.createFile`, `createDirectory`,
`rename`, and `delete`. Open documents follow a rename Knot performed, and a
deletion is reported to the documents it affects. The explorer (D034 / D053)
builds its new, rename, and delete actions on this API.

## Design

- **Provider operations.** `FileSystemProvider` gains `create_directory`,
  `rename`, and `delete`. File creation reuses the existing no-clobber
  `create` with empty bytes. Both providers implement all of them against the
  same typed results and errors, and the shared contract test in
  `crates/knot-model/src/filesystem/mod.rs` covers them.
- **No-clobber, parents created.** `createFile`, `createDirectory`, and the
  destination of `rename` reject `AlreadyExists` when the target exists and
  create missing parent directories. Overwriting is a delete followed by a
  rename, chosen by the caller. A rename whose destination lies inside its
  source rejects. Renaming a file to a different scheme rejects.
- **Delete goes to the trash.** The local provider moves the entry to the OS
  trash through the `trash` crate (a new dependency); `delete(uri, {
  permanent: true })` removes it outright. The memory provider has no trash
  and always removes. A platform without trash support rejects with a
  provider failure instead of deleting permanently. Deleting a directory
  removes its contents.
- **Documents follow Knot-issued renames only.** After the provider rename
  succeeds, the foreground re-associates every document at or under the old
  URI with the matching new URI, clean or dirty, keeping buffer, history,
  views, and identity. Title and language are re-detected as Save As does,
  the document's parent-directory watch moves, and the recorded provider
  version is refreshed. Renames made outside Knot keep the
  [external-change](../architecture/documents-and-persistence.md#detecting-external-changes)
  behavior: the old file vanishes and the document becomes a deleted one.
- **Deletion marks documents deleted.** Documents at or under a deleted URI
  take the existing deleted state: text kept, dirty, associated with the
  missing file, save recreates it. Trash and permanent deletion alike.
- **No prompts, no closing.** The API never confirms, closes documents, or
  refuses because a document is dirty. Callers such as the explorer decide
  whether to confirm. Protected closure and recovery are unaffected, since the
  text stays in an open document.
- **No entitlement.** Like reads, mutations are ungated until D087 defines
  entitlements; D087 will gate these four operations.
- **Index and watches.** The workspace file index and `files.watch` listeners
  learn of the change from the provider watch as for any external change. The
  API adds no direct index update.

## Exclusions

- Copy and duplicate, and undo of a delete (the OS trash is the undo).
- Inferring renames made outside Knot.
- Conditional operations on provider versions.
- Entitlement gating (D087) and any native UI. The explorer owns its prompts.
- Overwriting renames.

## Deferred decisions

- Whether a file that disappears and reappears at a new location in the same
  watch batch should be offered to the user as a rename.

## ✅ Checkpoint 1 — Providers

Approved. `FileSystemProvider` has `create_directory`, `rename`, and
`delete(uri, permanent)`, implemented by the local and memory providers.
`knot_model::files::{create_file, create_directory, rename, delete}` create
missing parents, refuse to overwrite, reject moves into the source or across
schemes, and return the normalized URIs. The local trash tests move real
temporary files to the OS trash.

## 🚧 Checkpoint 2 — Extension API (implemented, awaiting the review gate)

Deliverables:

- Handlers `createFile`, `createDirectory`, `renameFile`, `deleteFile`, the
  protocol declarations, the `knot.files` facade, `knot.d.ts` docs with the
  thrown `HostError` codes, and the schema, over the unscoped providers off
  the foreground.
- Document handling in the shell: a model function over the document
  collection computes which documents move or become deleted for a URI
  prefix; the shell runs it after the provider succeeded, then notifies,
  retitles, re-detects language, and moves document watches.
- Extension guide section, architecture text in
  [documents and persistence](../architecture/documents-and-persistence.md#uri-providers),
  and a decision entry for no-clobber with parent creation, trash default,
  and Knot-issued-only rename following.

Validation: model tests for the prefix computation (file, directory tree,
unrelated sibling with a common name prefix, a nested open document, dirty
documents, an already-deleted document); product tests in
`src/app/product/tests/files.rs` for a document that follows a rename and
keeps its edits and undo history, saves to the new path, deletion of a clean
and a dirty document, and a document watch that moves with the rename;
extension tests through the real editor in `scripts/extensions/testing/` per
the fake host contract, plus `scripts/extensions/contract.test.ts`.

Review gate (commands `Try: Files Create + Open`, then `Try: Files Rename Folder`, then `Try: Files Delete To Trash`, scratch directory `/tmp/knot-d010-try`): a try-command review in the real app. Per the repository rules,
write `try.*` commands in the personal-config `post-init.js` that create a
file and folder under a chosen directory, rename the active file, and delete
a file to the trash, and check that the tab follows and the file watch
behaves.

**Implementation context.** Handlers beside `stat_file` in
`src/app/extensions/host/handlers.rs` (`#[host_operation("...")]`, argument
types like `FileUriArguments`; rename needs a two-URI arguments type); facade
entries in `crates/knot-host/src/js/knot.js` (`files` object) and
`knot.d.ts`; protocol types in `crates/knot-protocol/src/`;
`docs/schemas/knot.schema.json`. Document association in
`crates/knot-model/src/documents.rs` (the `resources` map and
`complete_persistence` show how a document moves to a new URI;
`mark_deleted` is the deleted transition). Shell glue and watch movement in
`src/app/product/save.rs` (Save As re-association) and
`src/app/product/document_watch.rs`. Foreground access to providers through
`ProductShell::filesystems(cx)`.

## Completion

After the review gate, mark D010 ✅ in the daily-driver plan with links to the
architecture, decision, and extension guide sections, and remove this file.
