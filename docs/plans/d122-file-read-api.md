# D122 — Extension file read, list, and stat API

Status: 🚧 in progress (checkpoint 2 awaits its review gate). Item of the
[daily-driver plan](daily-driver.md#phase-4--files-and-workspace).

## Goal

Extensions can read a file's text, list one directory, and stat a URI through
the registered resource providers, as VS Code's `workspace.fs` does. This
unblocks project-local settings (D089), EditorConfig (D090), project inference
(D123), Vim's `:e` completion (D143), and the explorer (D034 / D053).

## Design

A new `knot.files` namespace in `crates/knot-host/src/js/knot.d.ts`, which
D010 (mutation) and D081 (enumeration) extend later:

- `readText(uri): Promise<string>` reads the stored resource through its
  provider and decodes it as UTF-8, dropping a leading BOM. It never reads
  an open document's buffer: unsaved edits are ignored, and the doc comment
  says so and points to `documents` and `Buffer.snapshot` for live text.
  Errors: `InvalidUri`, `ResourceNotFound`, `ResourceWrongKind` for a
  directory, `InvalidUtf8`, and `ProviderFailure`.
- `list(uri): Promise<readonly FileEntry[]>`, where `FileEntry` is
  `{ uri, name, kind: "file" | "directory" }`, lists exactly one directory,
  in unspecified order. Callers sort and filter in JavaScript. Errors:
  `InvalidUri`, `ResourceNotFound`, `ResourceWrongKind` for a file, and
  `ProviderFailure`.
- `stat(uri): Promise<FileStat | null>`, where `FileStat` is
  `{ kind: "file" | "directory" }`, resolves `null` for a missing resource
  instead of rejecting. Symlinks report their target's kind.

Boundaries:

- Any URI with a registered provider is readable, independently of workspace
  roots, through the same unscoped providers product Open uses. No
  entitlement gates reading: D087 gates processes and the network, which are
  what could send file contents anywhere, so reading alone exposes nothing a
  package can leak. Workspace trust (D124) remains deferred.
- Enumeration skips entries a listing cannot represent (broken symlinks,
  sockets and other special files, and names that are not UTF-8) instead of
  failing the whole directory. This lives in the providers, so the native
  workspace tree gains it too.
- The meaning of each operation (normalization, kind checks, decoding, and
  error mapping) is model code in `knot-model` with plain tests. The handlers
  only run it off the foreground and encode the result.

## Exclusions

- **Bytes.** No `readBytes` and no binary transport: no consumer needs
  non-UTF-8 content. Deferred as D171 in the roadmap.
- **Recursion.** No `walk`. D081 adds the ignore-aware walk, with a filter
  that uses D089's glob syntax and the ignore rules, where filtering actually
  saves work. D122's consumers look upward (EditorConfig, project settings),
  check markers with `stat` (project inference), or list one directory
  (explorer, `:e`).
- **Sort and filter options on `list`.** Both are one line of JavaScript, and
  a host-side contract would either fix an order the explorer wants to own or
  add a second pattern syntax beside D089's.
- **Richer stat.** No size, modification time, or version token; change
  detection belongs to D008.
- **Mutation and watching.** D010 and D008.

## Deferred decisions

None.

## Checkpoints

### 1. ✅ Listings skip entries they cannot represent

- `LocalFileSystemProvider::enumerate` skips broken symlinks, special files,
  and non-UTF-8 names instead of rejecting the directory; other I/O errors
  still reject. The memory provider is unaffected.
- Automated validation: local provider tests in a temporary directory with a
  dangling symlink, a FIFO or socket, and a non-UTF-8 name (macOS rejects
  such names on APFS, so gate that case to platforms that allow it, or skip
  it).
- **Implementation context:** `crates/knot-model/src/filesystem/local.rs`
  (`enumerate`, and its `metadata` helper, which maps a dangling symlink to
  `NotFound`); existing tests at the bottom of that file. The native
  workspace tree reaches the same path through `enumerate_directory` in
  `crates/knot-model/src/open.rs`.
- Review gate: none beyond `just check`; continue to checkpoint 2.
- ✅ Done: the test uses a Unix socket for the special file.

### 2. 🔍 The `files` namespace end to end

- Model operations in `knot-model` that take the provider registry and a URI
  and return `readText`, `list`, and `stat` results as data with typed
  errors. They reuse `enumerate_directory` and the UTF-8 decoding in
  `open.rs`, plus BOM removal.
- Three host operations, their `bootstrap.js` requests, the frozen `files`
  export in `knot.js`, and the declarations with full doc comments in
  `knot.d.ts`, including the disk-only rule on `readText`.
- A minimal `files` fake in `scripts/extensions/testing/host.ts` over a map of
  seeded files, so package tests for later consumers can arrange files.
- Automated validation: model tests over `MemoryFileSystemProvider` for each
  operation and error, including the BOM and invalid UTF-8. Product tests
  through the real host using the driver's served files: read, list, stat of
  a file, a directory, and a missing URI, the wrong-kind rejections, and
  `readText` returning disk content while the document is open and dirty.
  The extension contract test covers the new exports.
- **Implementation context:** mirror `pickFolder` and `resolve_directory`
  (`src/app/extensions/host/handlers.rs` near `pick_folder`,
  `src/app/product/operations.rs`) for running provider work off the
  foreground with `ProductShell::filesystems`; map `ResourceError` and
  `OpenResourceError` to the existing `InvalidUri`, `ResourceNotFound`,
  `ResourceWrongKind`, `InvalidUtf8`, and `ProviderFailure` codes the way
  `OpenFailure::host_error` does. Arguments decode with
  `ArgumentReader::text`; results encode as JSON. The JavaScript side follows
  `workspaces` in `crates/knot-host/src/js/{bootstrap,knot}.js`. Product tests
  sit beside `src/app/product/tests/workspaces.rs` and use `ServedFiles` from
  `src/app/product/driver/product.rs`. Handler declarations are validated at
  startup by `host::declarations`.
- ✅ Implemented: `knot-model/src/files.rs`, the `readFileText`, `listFiles`, and
  `statFile` host operations, the `files` export and declarations, the
  `seedFile` fake, and tests at each level. Remaining: the review gate.
- Review gate: run the `Try:` commands from personal-config `post-init.js`:
  read a file of the open workspace, list the root, and stat a missing path,
  each reporting its result with `showMessage`; then edit an open file without
  saving and confirm `readText` returns the saved text.

### 3. Documentation

- A "Read files" section in `docs/extension-api.md` with a short example and
  the disk-only rule.
- `docs/architecture/documents-and-persistence.md` (URI providers) gains the
  extension read path and the enumeration skip rule.
- A decision entry under documents and persistence for ungated reads and
  text-only reads, with its index line in `docs/decisions.md`.
- `docs/roadmap.md`: add D171 (binary file reads) to the deferred register,
  with the trigger "a consumer needs non-UTF-8 file content", and drop D122
  from the D090 deferral reasons.
- Review gate: remove this plan and mark D122 ✅ in the daily-driver plan.
