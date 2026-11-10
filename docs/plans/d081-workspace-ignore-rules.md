# D081 — Workspace ignore rules and file enumeration

Status: 🚧 approved, not started. Item of the [daily-driver plan](daily-driver.md).

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

Implemented; awaiting the review gate.

Deliverables:

- ✅ The three settings with definitions and `docs/settings.md` entries.
- ✅ A native policy built from the settings and evaluated with the `ignore` crate.
- ✅ `files.list` omits hidden entries and returns `ignored`; `files.isIgnored`.
- ✅ `knot.d.ts`, the extension guide, and the URI providers architecture section
  describe the behavior.

Scope decision: the policy lives in `crates/knot-model/src/files/ignore.rs` and
reads ignore files through the provider's own `read` and `stat` (a `Classifier`
over `GitignoreBuilder`), so it needs no new `FileSystemProvider` methods and the
memory and local providers share one implementation. Checkpoint 2 decides
whether the walk uses the same classifier or `WalkBuilder` for local speed.

Validation: `knot-model` tests over a temporary local tree covering hidden
names at any depth, nested `.gitignore` files and negation, `.ignore`,
`.git/info/exclude`, `files.ignored`, and `respectGitignore: false`; a host
handler test that a setting change is seen by the next listing; extension-level
test through the fake host or a bundled package test. `just check`.

Review gate: the defaults and `list` shape feel right for the explorer.

**Implementation context**:

- Add the `ignore` crate to `knot-model`.
- Setting keys and definitions beside `FILES_TRIM_TRAILING_WHITESPACE` in
  `crates/knot-model/src/settings.rs`.
- Policy in a new `crates/knot-model/src/files/ignore.rs` (or beside
  `files.rs`): a plain `IgnorePolicy { hidden, ignored, respect_gitignore }`
  passed into `files::list`, `files::walk`, and `files::is_ignored`. Handlers
  resolve the settings on the foreground and pass the policy in, as
  `list_files` in `src/app/extensions/host/handlers.rs` already resolves
  providers.
- Ignore evaluation is path-based, so the local provider
  (`crates/knot-model/src/filesystem/local.rs`) implements it with
  `ignore::WalkBuilder` (`hidden(false)`, parents and Git excludes on, hidden
  and `files.ignored` patterns applied as an override or `filter_entry`). Add
  the walk to `FileSystemProvider` in `filesystem/mod.rs`; the memory provider
  implements it with `ignore::gitignore::GitignoreBuilder` over its stored
  `.gitignore` files so model tests stay fast. Listing classification can reuse
  a depth-one walk so parent ignore files apply.
- Protocol: add `ignored` to the file entry type in `crates/knot-protocol`, the
  new operations beside the existing files operations, `knot.d.ts`
  (`export const files`), and `docs/schemas/knot.schema.json` if settings are
  listed there.

### 2. `files.walk` enumeration

Deliverables:

- `files.walk(uri)` through the same policy, off the foreground.
- A measurement on a large checkout (for example a Linux kernel tree) recorded
  in this plan, to inform D082's picker decision.

Validation: model tests for recursion, symlinks not followed, ignored
directories not descended into, unreadable entries skipped; a handler-level
test; `just check`.

Review gate: the walk is fast enough for Go to File, or D082 is told it is not.

**Implementation context**: the provider walk from checkpoint 1 with unlimited
depth; parallelize with `WalkBuilder::build_parallel` only if the measurement
needs it. Wire `walk_files` beside `list_files` in
`src/app/extensions/host/handlers.rs`.

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
