# D172 — Faster file walks

Status: 🚧 in progress — checkpoint 2 done, checkpoint 3 next. Source: [roadmap D172](../roadmap.md#documents-storage-and-history).
Precedes [D173](daily-driver.md) (workspace file index), whose first scan and
reclassification reuse this classifier.

## Outcome

`files.walk` on a Linux kernel checkout (about 97,000 files) takes at most
0.4 s in a release build, down from about 1.1 s. Classifying entries costs
little next to enumerating them. `files.list` and `files.isIgnored` behave
exactly as they do now and share the faster classifier.

## Design

- **One classifier.** The policy keeps a single evaluator for listings,
  walks, and watching, as
  [workspace ignore policy](../decisions/documents-and-persistence.md#workspace-ignore-policy)
  already decided. We reject a local fast path over
  `ignore::WalkBuilder::build_parallel`: it cannot mark ignored entries, so
  it cannot serve listings or the D173 index, and it would be a second
  implementation to keep in agreement with the classifier.
- **Rule stack per directory.** Each directory the walk visits carries its
  resolved rule stack (`Vec<Arc<DirectoryRules>>`, from the repository root or
  the outermost `.ignore` ancestor down to itself) and its virtual path.
  Classifying an entry means matching `files.hidden` and `files.ignored`,
  then the non-empty rule sets in the stack from the deepest up, then
  `.git/info/exclude` and the global excludes. There are no per-level
  `HashMap` lookups and no segment-vector clones per entry. A directory's
  stack is its parent's stack plus its own `.ignore` and `.gitignore`, read
  once.
- **Repositories found per directory.** A directory whose entries include
  `.git` is a repository root for itself and everything below it: its stack
  starts applying `.gitignore` files from there and carries that repository's
  `.git/info/exclude`. Walking from a workspace root above several
  repositories therefore applies each repository's rules, as ripgrep does,
  and a repository nested in another uses its own exclude file. Walks that
  start inside a repository seed this state by looking up from the walked
  directory; `list` and `is_ignored` always do.
- **Classification runs where the directory is read.** Each concurrent
  directory task enumerates the directory, reads its ignore files, and
  classifies its entries. The policy-wide state (compiled `hidden` and
  `ignored` patterns, the repository depth, the exclude file) is shared
  read-only behind an `Arc`, so the old sequential classification loop goes
  away.
- **Cheaper local enumeration.** The local provider builds each child URI by
  appending the name to the already-normalized parent URI, instead of a
  path → `Url::from_file_path` → normalize → scope check round trip per
  entry. The walk no longer stats each directory twice
  (`enumerate_directory` stat, then `enumerate`'s own).
- **Validation.**
  - A release-mode benchmark walks a path passed on the command line and
    prints the elapsed time and the file count.
  - An equivalence test checks the walk against `ignore::WalkBuilder` (set
    up with `parents`, `git_ignore`, `git_exclude`, `ignore` files, symlinks
    not followed, and an entry filter applying `files.hidden` and
    `files.ignored`) on generated local fixture trees. This checks the
    ripgrep semantics the policy claims.

## Exclusions

- Streaming or incremental walk results: D173 answers walks from its index,
  and D082 decides how the picker loads items.
- Changing the public API or the settings. Walk semantics change only to
  follow ripgrep (repositories found per directory).
- Faster providers other than the local one, beyond what the shared
  classifier gives them.

## Deferred decisions

None. D173 decides how the index uses the classifier: first scan,
reclassification on ignore-file changes, lazy ignored directories.

## Checkpoints

### 1. Benchmark and equivalence baseline ✅

Approved. `just walk-bench <directory> [runs]` times `files::walk` with the
default policy (`crates/knot-model/examples/walk_bench.rs`), and
`crates/knot-model/src/files/equivalence.rs` compares the walk with
`ignore::WalkBuilder` on hand-written and generated local trees.

- Baseline: a shallow Linux kernel clone (97,336 files, Apple Silicon,
  release build) walks in 1.13–1.26 s, about 1.14 s warm.
- The classifier now applies `.ignore` files of every level before any
  `.gitignore`, as ripgrep does.
- Both sides read the machine's global excludes, so the test assumes they
  match none of the fixtures' names.

### 2. Rule-stack classifier with per-directory classification ✅

Approved. `Classifier` holds the shared policy state and a `Scope` holds one
directory's rules; each walk task builds its scope from its entries and
classifies them, and repositories are found per directory. `list`,
`is_ignored`, and the walk root resolve their scope through
`Classifier::resolve`. The equivalence test covers walks from above
repositories, sibling repositories, and nested ones.

- Warm walk of the kernel clone: about 0.62 s, down from 1.14 s.

### 3. Cheaper local enumeration

- Deliverables:
  - Child URIs are derived from the normalized parent URI in
    `LocalFileSystemProvider::enumerate`.
  - The walk enumerates subdirectories without the extra
    `enumerate_directory` stat (the first directory is still checked so a
    file or missing root reports its error).
- Validation: local provider tests (including
  `enumerate_skips_entries_a_listing_cannot_represent` and the scoped-root
  tests), the equivalence test, and the benchmark meeting the ≤ 0.4 s
  target. If it doesn't, profile and record findings here before going on.
- Review gate: final numbers against the target.
- **Implementation context:** `crates/knot-model/src/filesystem/local.rs`
  (`enumerate`, `path_to_uri`, `path_to_unrestricted_uri`,
  `normalize_file_uri`) and `crates/knot-model/src/open.rs`
  (`enumerate_directory`). A child of a normalized, in-scope directory is in
  scope, so the per-child scope check can go.

### 4. Documentation

- Deliverables:
  - [Workspace ignore policy](../decisions/documents-and-persistence.md#workspace-ignore-policy):
    the new measurements, and the rejected `WalkBuilder` fast path with its
    reason.
  - [URI providers](../architecture/documents-and-persistence.md#uri-providers):
    any flow change (directories classified concurrently).
  - Remove D172 from the roadmap and the D172 note from D173 in the
    daily-driver plan, then remove this plan.
- Validation: `just check`.
- Review gate: final docs.
