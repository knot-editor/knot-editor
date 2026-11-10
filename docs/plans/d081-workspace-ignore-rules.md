# D081 — Workspace ignore rules and file enumeration

Status: 🚧 checkpoint 1 done; checkpoint 2 in review; checkpoint 3 not started. Item of the [daily-driver plan](daily-driver.md).

## Outcome

Knot has one native ignore policy that matches VS Code, Zed, and IntelliJ out of
the box, and extensions consume it through `knot.files`:

- **Hidden** entries (`.git`, `.DS_Store`, …) never appear in a listing or walk.
- **Ignored** entries (`.gitignore` and friends, plus configured patterns) still
  appear in listings, marked so the explorer (D034) can dim them, but are skipped
  by walks, and so by Go to File (D082), search (D085), and watching (D008).
- `files.walk` enumerates every visible, non-ignored file under a directory.
- Vim's `:e` completes paths.

## Design

### Policy and settings

Three unscoped D070 settings, defined natively in `knot-model`:

| Setting | Default | Meaning |
| --- | --- | --- |
| `files.hidden` | `[".git", ".hg", ".svn", ".jj", "CVS", ".DS_Store", "Thumbs.db"]` | never shown (Zed's `file_scan_exclusions`, VS Code's `files.exclude`) |
| `files.ignored` | `[]` | extra ignored patterns |
| `files.respectGitignore` | `true` | apply `.gitignore`, `.ignore`, `.git/info/exclude`, and the global excludes file |

Both pattern lists use gitignore syntax, so a bare name matches at any depth
and there is no second pattern language. Repository ignore files follow
ripgrep's semantics because native code evaluates them with the `ignore` crate,
which is what D085 relies on when it runs `rg` with only `files.ignored` passed
as globs.

The policy is global and tied to the filesystem (repositories), not to
workspace roots: it applies to any directory `knot.files` lists or walks.
Evaluation is native, so watching, walking, and listing never call JavaScript.

### Public API (`knot.files`)

- `list(uri)` omits hidden entries and marks each remaining `FileEntry` with
  `ignored: boolean`. This revises D122's "no filter" choice: hidden is policy,
  not a caller filter syntax.
- `walk(uri)` resolves every file below a directory, recursively, excluding
  hidden and ignored entries and not following symlinks, as one array. Streaming
  or paging is left to D082 unless the measurement below shows it is needed.
- `isIgnored(uri)` reports whether a path is hidden or ignored, for one-off
  checks; D008 uses the same native predicate directly.

### Exclusions

- No extension-contributed rule registration; settings cover configuration, and
  a registration waits for a real consumer.
- No API exposing the effective rules: `rg` applies repository ignore files
  itself, and `files.ignored` is readable through settings.
- The native explorer is not changed; D034 replaces it.
- Watching integration lands with D008.
- Workspace or project-local overrides wait for D089.

### Deferred decisions

- Whether the picker needs incremental items for very large repositories (D082).
- Whether `.gitignore` applies outside a Git repository (ripgrep's `require_git`);
  start with ripgrep's default and revisit on real use.

## Checkpoints

### 1. Ignore policy, settings, and annotated listings ✅

Approved. The policy, the three settings, the annotated `files.list`, and
`files.isIgnored` are in place; see [settings](../settings.md), the
[URI providers](../architecture/documents-and-persistence.md#uri-providers)
section, and `crates/knot-model/src/files/ignore.rs`. The policy reads ignore
files through each provider's own `read` and `stat` (a classifier over
`GitignoreBuilder`) rather than new `FileSystemProvider` methods; checkpoint 2
decides whether the walk reuses it or uses `WalkBuilder` for local speed.

### 2. `files.walk` enumeration 🔍

Implemented, awaiting review.

Deliverables:

- `files.walk(uri)` through the same policy, off the foreground.
- A measurement on a large checkout (for example a Linux kernel tree) recorded
  in this plan, to inform D082's picker decision.

Validation: model tests for recursion, symlinks not followed, ignored
directories not descended into, unreadable entries skipped; a handler-level
test; `just check`.

Review gate: the walk is fast enough for Go to File, or D082 is told it is not.

Done ✅:

- `knot_model::files::walk` reuses the checkpoint 1 classifier over the
  provider (`ResourceEntry` gained `symlink`), reads up to 16 directories at once,
  and is wired as `walk_files` in `handlers.rs`, the bootstrap, `knot.d.ts`, and
  the fake host. `WalkBuilder` was not needed.
- Tests: model tests (memory and local tempdir), the product handler test, and
  the fake-host test.

**Measurement** (release build, Linux kernel `--depth 1` checkout: 97,336 files,
macOS/APFS, warm cache, three runs): about 1.1 s with the default policy, of
which about 0.4 s is enumeration alone (`bare` policy). Sequential enumeration was
2.6 s with a thread per read; a shared pool of at most 16 workers, a `stat` only
for symlinks, and 16 directories read at once gave a stable 1.1 s (64 at once
was slower, 1.3 s). `rg --files` takes 0.15 s on the same tree. The remaining cost is
per-entry classification on one task. Result for
D082: a kernel-sized repository takes about a second to enumerate once, which is
acceptable for a one-shot Go to File index; typical repositories are one to two
orders of magnitude smaller. Cheaper classification, a native local fast path, and
streaming are deferred as D172 in the [roadmap](../roadmap.md).

### 3. Vim `:e` path completion

Deliverables: `:e` completes paths from the same base `:e` opens against
(`edit` in `command-line.ts`), listing the typed directory with `files.list`
and dropping ignored entries; directories complete with a trailing `/`.

Validation: `extensions/@knot/vim/test/ex.test.ts` and `cases/ex.jsonc` cases,
run through the real editor per the package test runner; a `post-init.js`
`Try:` command for manual review. `just check`.

Review gate: completing paths in the Ex line feels like Vim.

**Implementation context**: extend `completeEx` in
`extensions/@knot/vim/src/ex.ts`, which already completes command names and
`:set` options through `completion.match`; reuse `resolvePath` and
`directoryOf`. `completeEx` needs the base directory, so pass the state from
the `onChange` in `command-line.ts`.

## Completion

Record the decisions (native walk over `rg --files`, settings over a rule
registration, `list` hiding by default, defaults aligned with VS Code and Zed)
in `docs/decisions/documents-and-persistence.md` beside Extension file reads,
revising its "no filter" sentence, and update the daily-driver item.
