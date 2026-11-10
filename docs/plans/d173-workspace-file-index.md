# D173 — Workspace file index

Status: 🚧 checkpoints 1–3 ✅ approved; checkpoint 4 ✅ implemented, awaiting its review gate; checkpoints 5–6 planned. Item of the
[daily-driver plan](daily-driver.md); depends on D008 (watching) and
D081 (ignore policy and enumeration).

## Outcome

Every workspace root has a native in-memory tree of its entries, watched
recursively, so `files.list` and `files.isIgnored` answer from memory for
anything inside a root, and a new `files.search` matches a query against the
root's paths natively and returns the best hundred or so. Go to File (D082)
then never receives, or makes Knot build, a URI per file: it asks for the top
results on every keystroke. `files.walk` is removed, because nothing needs the
complete list. The tree stores only a name, kind, and flags per entry, so a
100k-file root stays small.

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

- Visible entries, files and directories, with their kinds. An entry holds
  its name, kind, symlink flag, and ignored flag; its URI is built from its
  parent's when a query returns it, and never stored.
- Ignored entries, marked as ignored. An ignored directory is recorded but its
  contents are never scanned (`target/`, `node_modules/`).
- Hidden entries (`.git`, `.DS_Store`) are not stored.
- Each directory keeps the ignore scope that D081's classifier resolved for
  it, so updates can classify new entries without reading ancestor ignore
  files again.

### Queries

- Inside an indexed root, outside unscanned ignored directories:
  - `list` returns the indexed children, with ignored ones marked;
  - `search` matches the visible files of the subtree (see below);
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

### Search

`files.search(uri, query, { limit })` matches `query` against the visible
files below the directory `uri` and resolves `{ results, more }`:

- Each result is `{ uri, path, name, ranges }`: `path` is relative to `uri`,
  `name` is the last segment, and `ranges` are the UTF-16 ranges of `path`
  that matched. `more` is true when more files matched than `limit`, which
  defaults to 100.
- Matching uses the shared matcher and the user's `completion.styles`. A file
  whose name matches ranks above one that matches only through its directories;
  within each tier the matcher's order applies, then path order. An empty query
  matches everything, so the first files in path order come back with `more`.
- The index scores every visible file and keeps the best `limit` in a bounded
  heap, so no URI is built for any other file.
- A directory outside every indexed root, or below an ignored one, is searched
  by walking the disk with the same matching, so results do not depend on the
  index.
- A call covers one directory. Multi-root labels and ranking open and recent
  files first belong to D082, which merges the per-root results.

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
- Returning every file from the index to JavaScript, as `files.walk` did, or
  returning root-relative paths for it to match: both ship 100k strings per
  keystroke, and a URI string per file costs memory and time before
  JavaScript reads one.
- Incremental delivery of the full list: bounded results make it
  unnecessary.
- A numeric bonus for matches in the last path segment: it needs matcher
  internals and a tuned constant, where ranking by tier does not.
- Storing each entry's URI, or caching it on first use: a search or walk
  touches every file, so the cache fills and the memory returns.
- Lazy scanning of ignored directories, which Zed uses for its project panel:
  nothing needs their contents, because walks skip them and listings inside
  them read the disk.
- Snapshots saved between sessions, as JetBrains does: deferred while a warm
  scan of the kernel takes about a quarter of a second.

## Exclusions and deferred decisions

- No streaming or incremental delivery, and no root labels: a call returns
  at most `limit` results, and D082 merges roots.
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

### 2. Reclassification and policy changes ✅

Implemented in `FileIndex::apply`: a changed `.gitignore`, `.ignore`, or
`.git` rescans its parent directory's subtree, and a `.git/info/exclude` path
rescans the repository root's subtree; the root itself is a full scan. The
policy-mismatch rescan landed with checkpoint 1. The equivalence test includes
ignore-file, `.git`, and exclude edits and policy flips. Note: with
`respect_gitignore` off the disk `is_ignored` and `list` judge only the
nearest directory's ancestry, so the index answers only through directories
the disk judges visible and falls through otherwise.

### 3. Application wiring, measurements, and docs ✅

Implemented in `src/app/file_index.rs` (`ApplicationFileIndex`,
`FileIndexLease`): leases refreshed in `replace_workspace_roots`, the
`listFiles`, `walkFiles`, and `isFileIgnored` handlers routed through the
index, and a root whose provider cannot be watched left unindexed. Measured on
a synthetic 100,000-file tree (no kernel checkout was available): disk walk
117–136 ms, first scan 115 ms, index walk 6–8 ms, about 42 MB resident. Docs
are in the [file index architecture](../architecture/documents-and-persistence.md#workspace-file-index)
and its decision.

### 4. Compact entries ✅ implemented, awaiting its review gate

Implemented in `files/index.rs`: `Entry` holds a name, kind, symlink flag,
ignored flag, and scanned directory, and `list` and `walk` build each URI from
the directory's. The provider's URIs are dropped after classifying. Measured on
the synthetic 100,000-file tree (release build, warm): resident memory 42 MB →
11 MB, index walk 6–8 ms → 14–16 ms, first scan unchanged (131–137 ms). The
equivalence and focused tests pass unchanged.

Details:

Deliverables:

- `crates/knot-model/src/files/index.rs`: entries hold their name, kind,
  symlink flag, ignored flag, and scanned directory, not a `ResourceEntry`.
  A query builds each returned URI from the URI of its directory and the name.
  `scan` and `refresh` build the URIs they need to read subdirectories the
  same way, and drop the provider's URI after classifying.
- The directory URI is not stored either. A directory's URI is built when
  something reads it.
- `list` and the index's internal `walk` return the same values as before.

Validation:

- The equivalence test and the focused tests pass unchanged.
- A one-off release measurement of resident memory and the index walk on the
  100,000-file tree, recorded here against the checkpoint 3 numbers (about
  42 MB and 6–8 ms).

Review gate: memory against the earlier numbers, and the walk time.

Implementation context:

- `Entry` and `Dir` in `files/index.rs`. `ResourceEntry` and `child_uri` come
  from `crates/knot-model/src/filesystem/`. `Children` already stores the name
  once.
- `read_directory`, `classify`, `scan`, `refresh`, and `scan_again` use
  `entry.entry.uri`; replace them with a URI built from the directory being
  read.

### 5. Native search

Deliverables:

- `FileIndex::search(uri, query, limit, policy)` in `files/index.rs` returns
  the hits and the `more` flag, or `None` to fall through, as the other
  queries do. A hit carries the relative path, name, ranges, and kind, and the
  URI is built for the returned hits only.
- Ranking by tier, then the matcher's order, then path order; a bounded heap
  of `limit` hits and a count of the rest.
- `files::search` over the disk, which walks and matches the same way, for
  directories the index does not cover. Both go through one ranking function.

Validation:

- A property test on generated trees: for random queries and limits, the
  index's results equal the ranking of a brute-force match over every
  visible file from `walk`, including `more` and the empty query, and the disk
  search equals the index.
- Focused tests for the tiers (a name match beats a directory match), a limit
  smaller and larger than the number of matches, and results that follow a
  batch of changes.
- A one-off release measurement of `search` on the 100,000-file tree for
  short, long, and non-matching queries, recorded here.

Automated tests use small trees only.

Review gate: the ranking and the search time against a keystroke budget.

Implementation context:

- `crates/knot-model/src/matching.rs` (`Query`, `Matcher::find`, `Found`) is
  the matcher; the caller passes the styles from `completion.styles`. Look at
  `Matcher::rank` for how the style order and score combine.
- `files/equivalence.rs` and `files/index/tests.rs` are the models for the
  property test.
- Byte ranges convert with `matching::utf16_ranges`.

### 6. Extension API, removal of `files.walk`, and docs

Deliverables:

- `files.search(uri, query, options?)` in `crates/knot-host/src/js/knot.d.ts`,
  `bootstrap.js`, and `knot.js`, with a `searchFiles` handler in
  `src/app/extensions/host/handlers.rs` that takes the leased index covering
  the URI and the policy and styles from the settings, and a protocol type in
  `crates/knot-protocol/src/lib.rs`. `limit` defaults to 100.
- `files.walk` and the `walkFiles` handler are removed from the host, the
  declarations, the docs, and the fake host; `files::walk` and
  `FileIndex::walk` stay as internal pieces (the disk search and the
  equivalence oracle).
- The fake host in `scripts/extensions/testing/host.ts` implements
  `files.search` with its plain-name matching, with a test in
  `files.test.ts`.
- Product tests in `src/app/product/tests/files.rs` for `files.search` over an
  indexed root, after a change on disk, after a policy change, and outside the
  roots; the walk tests become search tests.
- Measurement: the end-to-end time of `files.search` from JavaScript on the
  100,000-file tree, recorded here and in the decision.
- Docs:
  - `docs/extension-api.md` and `docs/settings.md` replace `files.walk`;
  - the architecture's workspace file index section describes search and
    compact entries;
  - the workspace file index decision records the choice of native bounded
    search, the rejected alternatives above, tiered ranking, and the numbers;
  - the D082 bullet in `docs/plans/daily-driver.md` drops the question about
    incremental items.
- `Try:` commands in the personal `post-init.js`: time a search of each root
  for a typed query, and report `more`.

Validation:

- `just check`, including the extension toolchain tests.

Review gate: the API shape and the search timings, and the `Try:` commands in
the real app.

Implementation context:

- The operation declarations: `#[host_operation("listFiles")]` and the
  `FileUriArguments` decoder in `src/app/extensions/host/handlers.rs`;
  `indexed_uri` there finds the covering index.
- Settings: `settings::files_ignore_policy` and the `completion.styles` reader
  the completion handler uses.
- Everything that mentions `walkFiles` or `files.walk`: `knot.js`,
  `bootstrap.js`, `knot.d.ts`, `scripts/extensions/testing/host.ts` and its
  test, the docs listed above, and `src/app/product/tests/files.rs`.
