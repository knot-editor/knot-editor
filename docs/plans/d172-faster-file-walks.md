# D172 — Faster file walks

Status: 🚧 in progress — checkpoint 1 awaits review. Source: [roadmap D172](../roadmap.md#documents-storage-and-history).
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

- Deliverables:
  - ✅ A benchmark that walks a given local directory with the default policy
    and reports the time and file count, plus a `just` recipe to run it in
    release mode (`just walk-bench <directory> [runs]`).
  - ✅ An equivalence test comparing `files::walk` with `ignore::WalkBuilder` on
    local fixture trees covering nested `.gitignore` and `.ignore`
    overrides, negations, directory-only patterns, `.git/info/exclude`,
    `.ignore` above the repository, hidden and ignored setting patterns, and
    symlinked directories.
  - ✅ Baseline numbers recorded in this plan.
- Validation: the equivalence test passes against the current walk; any
  difference it exposes is either fixed in the classifier or recorded here as
  an intended divergence (for example, the global excludes file is not
  configured in tests).
- Review gate: baseline numbers and any divergences.
- **Result:**
  - Baseline: `just walk-bench` on a shallow clone of the Linux kernel
    (97,336 files, Apple Silicon, release build) takes 1.13–1.26 s per walk,
    about 1.14 s once warm.
  - Fixed: ripgrep consults the `.ignore` files of every level, deepest first,
    before any `.gitignore`; the classifier went level by level. It now
    follows ripgrep, pinned by a policy test.
  - Divergence, to fix in checkpoint 2: walking a directory *above* a
    repository applies no `.gitignore` inside the repository below it, while
    ripgrep does. The equivalence test does not walk from above a repository
    yet.
  - The global excludes file is read by both sides (`git_global`), so the
    test depends on the machine's global excludes only for names the fixtures
    do not use.
- **Implementation context:** the walk is `files::walk` in
  `crates/knot-model/src/files.rs`, and the policy tests live in
  `crates/knot-model/src/files/ignore.rs` (their `memory` fixture uses the
  memory provider; the equivalence test needs real directories under a
  `tempfile` root with `LocalFileSystemProvider`). `ignore` is already a
  `knot-model` dependency. Put the benchmark under `crates/knot-model/examples/`
  and add a recipe to the `justfile`; run it with `pollster::block_on`.

### 2. Rule-stack classifier with per-directory classification

- Deliverables:
  - `Classifier` split into shared policy state and per-directory rule
    stacks.
  - `walk` classifies inside each concurrent directory task.
  - `list` and `is_ignored` resolve a directory's stack once (ancestors from
    the repository root) and classify through the same code.
  - Repositories are detected per directory (see Design), so a walk from
    above a repository applies the repository's rules. The equivalence test
    adds walks from the fixture root and fixtures with sibling and nested
    repositories.
- Validation: the existing policy tests, the walk and listing product test
  (`src/app/product/tests/files.rs`) pass unchanged, and the extended
  equivalence test passes. Benchmark numbers recorded.
- Review gate: the numbers, and the classifier's shape as D173 will reuse it.
- **Implementation context:** `crates/knot-model/src/files/ignore.rs`
  (`Classifier::{new, rules, entry, directory}`, `DirectoryRules`, `verdict`,
  `virtual_path`) and `files.rs` (`walk`, `list`, `is_ignored`). The walk
  loop uses `FuturesUnordered` with `WALK_CONCURRENCY`; keep that and have
  each future return classified files and child directories with their
  stacks. Ignore-file reads stay through the entry's own provider.

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
