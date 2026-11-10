# D173 — Workspace file index

Status: 🚧 checkpoint 1 ✅ implemented, awaiting its review gate. Item of the
[daily-driver plan](daily-driver.md); depends on D008 (watching) and
D081 (ignore policy and enumeration).

## Outcome

Every workspace root has a native in-memory tree of its entries, watched
recursively, so `files.list`, `files.walk`, and `files.isIgnored` answer from
memory for anything inside a root. Go to File (D082) can then walk a root of
about 100k files in tens of milliseconds, end to end to JavaScript. The
extension API does not change.

## Design

### Ownership and scope

- `knot-model` owns a gpui-free `FileIndex`: the tree of one root, its scan,
  its updates, and its queries. An application global
  (`ApplicationFileIndex`) holds one index per root URI.
- Indexes are shared by root URI: two windows that show the same folder share
  one index, and so one scan and one watch. Each window's product shell holds a
  lease for each of its roots. The lease is replaced when the roots change and
  dropped with the shell. The last lease to go drops the index and its watch.
- Each root is indexed on its own. A nested root (a monorepo and one of its
  packages) gets its own index rather than reusing a subtree of its ancestor's
  index, because nested roots are rare.
- The index works over any provider, so tests run on the memory provider.
- Scanning and updating run off the foreground. The foreground only registers
  and releases roots.

### What the tree holds

- Visible entries, files and directories, with their kinds.
- Ignored entries, marked as ignored. An ignored directory is recorded but its
  contents are never scanned (`target/`, `node_modules/`).
- Hidden entries (`.git`, `.DS_Store`) are not stored.
- Each directory keeps the ignore scope that D081's classifier resolved for
  it, so updates can classify new entries without reading ancestor ignore
  files again.

### Queries

- Inside an indexed root, outside unscanned ignored directories:
  - `list` returns the indexed children, with ignored ones marked;
  - `walk` collects the visible files of the subtree;
  - `isIgnored` reads the entry's classification, or its nearest indexed
    ancestor's.
- Everything else, meaning URIs outside every root and the contents of an
  ignored directory, falls through to the current disk implementations in
  `files.rs`.
- While the first scan runs, queries wait for it. The scan costs the same as
  a disk walk, so waiting is never slower than walking, and the tree is not
  read twice at once. Showing partial results during the scan is left to
  D082.
- The index records the `IgnorePolicy` it was built with. A query that carries
  a different policy, because `files.hidden`, `files.ignored`, or
  `files.respectGitignore` changed, starts a full rescan and waits for it. That
  way no settings-change hook is needed.

### Updates

- One recursive watch per root goes through the existing `WatchRegistry`, so
  it shares provider watches with `files.watch` subscriptions the root covers.
- For each coalesced batch, take the distinct parent directories of the
  changed URIs that lie in the scanned part of the tree, re-list each once, and
  compare its children with the index:
  - add new entries, and scan new visible directories;
  - drop removed subtrees, and update entries whose kind changed.
- Watch events get no kinds: re-listing the directory says what happened, and
  notify's kinds are unreliable on macOS.
- Events below an unscanned ignored or hidden directory are dropped.
- A rescan event scans the whole root again in the background and swaps the
  new tree in, while queries keep using the old one.
- Reclassification:
  - a change to `.gitignore` or `.ignore` rescans the subtree of the directory
    that holds it, so newly visible directories are scanned and newly ignored
    ones lose their contents;
  - a change to `.git/info/exclude` rescans that repository's root;
  - a `.git` that appears or disappears is a change to its parent directory's
    entries, and rescans that directory's subtree;
  - a change to the global Git excludes file is not watched, and the next full
    scan picks it up.
- D010's mutations, when they land, update the affected entries of the index
  before they resolve, so a caller that creates a file and then lists the
  directory sees the new file. External changes appear after the watch's
  coalescing window, as in other editors.

### Rejected alternatives

These are recorded as decisions in checkpoint 3.

- An index per window: it would scan and watch the same folder twice.
- `list` on disk with only `walk` from the index: freshness after Knot's own
  mutations comes from updating the index, not from bypassing it.
- A disk fallback or partial answers while the first scan runs: a disk walk
  doubles the I/O, and partial answers break the API's completeness guarantee.
- Lazy scanning of ignored directories, which Zed uses for its project panel:
  nothing needs their contents, because walks skip them and listings inside
  them read the disk.
- Snapshots saved between sessions, as JetBrains does: deferred while a warm
  scan of the kernel takes about a quarter of a second.

## Exclusions and deferred decisions

- No API changes: no streaming or incremental walk, and no root labels. D082
  decides whether the picker needs incremental items. It does so from this
  plan's measurement of how long `files.walk` takes to deliver 100k entries
  to JavaScript.
- No snapshots saved between sessions.
- No reuse of an ancestor index for nested roots.
- No native explorer integration: the native `WorkspaceTree` is replaced by
  D034.
- Mutations: D010 adds the index update described above.

## Checkpoints

### 1. Model index: scan, queries, and event updates ✅

Implemented in `files/index.rs`: `FileIndex` takes a provider and a normalized
root; `scan(policy)` builds or swaps the tree, `list`/`walk`/`is_ignored`
return `None` to fall through, and `apply(batch)` re-lists parents. Scope
decisions: a path through a symlinked or unreadable directory falls through
like an ignored one; `is_ignored` of an entry that is not stored answers only
when it is judged alike as a file and as a directory; a batch that arrives
before the first scan is dropped, and one that arrives during a rescan may be
lost by the swap. Tests: `files/index/tests.rs`.

Deliverables:

- `crates/knot-model/src/files/index.rs`: a `FileIndex` per root that scans
  with the classifier, answers `list`, `walk`, and `is_ignored`, and applies
  batches of changed URIs by re-listing their parent directories.
- Queries wait for the first scan. A rescan event swaps in a fresh tree.

Validation:

- An equivalence test on small generated memory-provider trees. After random
  creates, removes, renames, and kind changes, each followed by applying the
  batch the change produces, the index's `list`, `walk`, and `is_ignored`
  must match the disk implementations for every directory.
- Focused tests:
  - queries that wait for the first scan;
  - a rescan swap;
  - events under ignored and hidden directories being dropped;
  - fall-through outside the root and inside ignored directories.

Automated tests use small trees only. No large repository appears in tests.

Review gate: the shape of the index, its query semantics, and the equivalence
test.

Implementation context:

- Mirror the walk in `crates/knot-model/src/files.rs`: the `visit` and `walk`
  concurrency with `WALK_CONCURRENCY` and `FuturesUnordered`. Reuse
  `Classifier`, `Scope`, `Visibility`, `IgnoreFiles`, and `segments` from
  `crates/knot-model/src/files/ignore.rs`, which this checkpoint may make
  reusable by the index, for example by keeping a directory's `Scope` and
  calling `Classifier::descend` and `Classifier::entry` on updates.
- Enumeration goes through `FileSystemProvider::enumerate`
  (`crates/knot-model/src/filesystem/mod.rs`). Seed fixtures with
  `MemoryFileSystemProvider::seed_file` and `seed_directory`. Run futures in
  tests with `pollster::block_on`, as `files.rs` tests do.
- Batches arrive as `Vec<ResourceUri>`, the `WatchListener` shape in
  `crates/knot-model/src/watch.rs`. A batch that names the root itself is a
  rescan. The index can take batches directly in tests, without a registry.
- Use `files/equivalence.rs` as the model for the equivalence test, against
  `files::list`, `files::walk`, and `files::is_ignored` over the memory
  provider.

### 2. Reclassification and policy changes

Deliverables:

- Changes to `.gitignore` and `.ignore` rescan the subtree of the directory
  that holds them. A change to `.git/info/exclude` rescans the repository
  root. A `.git` that appears or disappears rescans its parent directory's
  subtree.
- A query carrying a different `IgnorePolicy` triggers a full rescan and
  waits for it.

Validation:

- The equivalence test extended with ignore-file edits, `.git` creation and
  removal, `.git/info/exclude` edits, and policy changes.
- Focused tests for a directory becoming visible, which gets scanned, and
  becoming ignored, which loses its contents.

Review gate: reclassification behavior and the policy-mismatch rescan.

Implementation context:

- Hidden entries are not stored, so detect `.git/info/exclude` by the path of
  the changed URI (a `.git` segment followed by `info/exclude`) before
  dropping events under hidden directories.
- The repository root and the `.git` check live in `Classifier::descend` and
  `IgnoreFiles::of` in `files/ignore.rs`.

### 3. Application wiring, measurements, and docs

Deliverables:

- `src/app/file_index.rs`: an `ApplicationFileIndex` global over the
  `knot-model` index registry, with per-root leases.
- The product shell holds leases for its roots, refreshed whenever the roots
  change and dropped with the shell.
- Each index subscribes to its root through `ApplicationFileWatches`.
- The `listFiles`, `walkFiles`, and `isFileIgnored` handlers route through the
  index, which falls through to disk outside indexed roots.
- Measurements on a kernel checkout and, if one is available locally, a
  larger monorepo. Record them in the plan and keep the essential ones in the
  decision. These are one-off manual measurements with a release build, never
  tests:
  - first scan time;
  - resident memory of the index;
  - `walk` time from the index;
  - end-to-end time for `files.walk` to deliver all entries to JavaScript.
- Docs:
  - architecture: the index in the URI providers and watching sections of
    `docs/architecture/documents-and-persistence.md`, and ownership in
    `docs/architecture.md`;
  - decisions: a workspace file index entry in
    `docs/decisions/documents-and-persistence.md`, an update to "File
    watching", which says nothing watches roots, and the index line in
    `docs/decisions.md`;
  - the `files` documentation in `knot.d.ts`, if any guarantee changes.
- A personal-config `post-init.js` with `Try:` commands that time a walk of
  each root, list a directory, and report a file created outside Knot.

Validation:

- Product tests in `src/app/product/tests/files.rs` that exercise `files.list`,
  `files.walk`, and `files.isIgnored` through an extension:
  - over an indexed root;
  - after a change on disk, once the watch delivers it;
  - after a policy setting change;
  - outside the roots.
- A test that removing a root, or closing its last window, releases the index
  and its provider watch (`WatchRegistry::provider_watch_count`).
- `just check`.

Review gate: the measurements against the tens-of-milliseconds target, and
the `Try:` commands in the real app.

Implementation context:

- Root changes go through `replace_workspace_roots` in
  `src/app/product/tools.rs`, which also covers `install_workspace`. The
  shell's `workspace: WorkspaceState` field is in `src/app/product.rs`.
- Follow `src/app/file_watch.rs` (`ApplicationFileWatches`) for the global
  pattern, and register the module in `src/app/mod.rs`.
- The handlers are at `listFiles`, `walkFiles`, and `isFileIgnored` in
  `src/app/extensions/host/handlers.rs`. The policy comes from
  `settings::files_ignore_policy`, and providers from
  `ProductShell::filesystems`.
- Nothing durable is written, so nothing is added to
  `extensions::host::quit_after_storage`.
