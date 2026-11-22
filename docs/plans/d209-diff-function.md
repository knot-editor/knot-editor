# D209 — Diff function for extensions

Status: 🚧 approved, not started. Source: [roadmap D209](../roadmap.md), part of
the [version control plan](vcs.md).

## Outcome

Extensions can compare two texts natively: `knot.diff(old, new, options?)`
returns the line hunks that turn `old` into `new`. Either side is a string or
a live `Buffer`, read natively, so a buffer's text never crosses the boundary.
The change gutter (D211) and the diff view (D197) build on it; when and what
to compare stays package policy, as with `buffer.find`.

## Design

- **One top-level, stateless function.** `knot.diff(old, new, options?)`, each
  side `string | Buffer`. Rejected: a `buffer.diff(text)` method beside a
  separate text-to-text function (two entry points for one operation), and a
  native text handle or headless baseline buffer that would spare resending a
  baseline (state, and a new buffer kind; revisit only if D211 measures the
  copy as a problem).
- **Result.** `{ hunks, oldRevision, newRevision }`. Each hunk is
  `{ old: LineRange, new: LineRange }` with 0-based, half-open
  `{ start, end }` line ranges, numbered as `Buffer.lines` numbers them. An
  empty `old` range is an insertion before `old.start`; an empty `new` range is
  a deletion. Hunks are ascending and disjoint. A revision is the buffer
  side's revision when diffed, absent for a string side. No byte ranges:
  consumers with a buffer side get them from `lineRange`.
- **Comparison.** imara-diff's Histogram algorithm with line post-processing,
  as reload uses today, factored into one shared function so reload and
  `knot.diff` agree. Lines compare exactly by default, terminators included;
  converting a baseline to the working-tree form is the provider's job (e.g.
  `git cat-file --filters`).
- **`ignoreWhitespace` option.** Boolean, default false. When true, two lines
  are equal when they match after removing all whitespace (as `git diff -w`),
  which also ignores `\r\n` against `\n`. Lines that differ only in whitespace
  then fall inside no hunk.
- **Execution.** Buffer sides are snapshotted on the foreground; the diff runs
  on the background executor, as language registration compiles queries, and
  honors invocation cancellation. No size cap.

## Exclusions

- Changes inside a line (word or character refinement): D197 adds it as an
  option when the diff view needs it.
- Other whitespace modes (trim only, ignore blank lines), move detection, and
  a computation time limit.
- Any caching of baselines or incremental rediffing.

## Deferred decisions

- Whether intra-line refinement extends `knot.diff` or is its own function
  (D197).
- Whether resending baselines needs a native handle (D211's measurements).

## Checkpoints

### 1. Native line diff with ignore-whitespace

Deliverables: a `knot-model` diff module returning line hunks for two texts
with `ignoreWhitespace`; reload's `line_edits` built on it.

Validation: unit tests for insertions, deletions, modifications, empty texts,
trailing-newline changes, CRLF, whitespace-only changes with and without the
option, and the existing reload tests unchanged.

Review gate: the hunk type and the whitespace semantics.

**Implementation context.** Add `crates/knot-model/src/diff.rs` (export it
from the crate's `lib.rs`) with a `LineHunk { old: Range<u32>, new: Range<u32> }`
and `line_hunks(old, new, DiffOptions)`. Move the `InternedInput` /
`Diff::compute(Algorithm::Histogram, …)` / `postprocess_lines` sequence out of
`crates/knot-model/src/reload.rs`, which keeps `line_edits` by mapping hunks
through its `line_starts`. For `ignoreWhitespace`, intern a per-line key with
whitespace removed (an imara-diff `TokenSource` over lines yielding the
normalized key) instead of the raw line; hunk indices stay line indices of
the original texts.

### 2. `knot.diff` in the extension API

Deliverables: the host operation, the JavaScript binding and declaration, the
extension test harness's implementation, and documentation.

Validation: host handler tests for string/string, buffer/string, and
buffer/buffer diffs, revisions reported, closed buffers, invalid arguments,
and cancellation; an extension-level test through `scripts/extensions/testing`;
`knot.d.ts` type checks.

Review gate: the API declaration and guide text; then record the decision.

**Implementation context.**
- Protocol: add `LineRange`, `DiffHunk`, and `DiffResult` (camelCase serde)
  beside `FindResult` in `crates/knot-protocol/src/lib.rs`, with a
  `HostSuccess` impl in `crates/knot-protocol/src/declarations.rs`.
- Handler: `#[host_operation("diff")]` in
  `src/app/extensions/host/handlers.rs`, mirroring `FindArguments`/`find`
  for decoding and `host.buffers.read_model` for buffer sides. Each side
  decodes as text or buffer handle. Snapshot in `context.foreground`, then
  `context.application.background_executor().spawn(...)` as the language
  registration handler does, checking `commands.is_cancelled` around it.
- Binding: `diff` in the `knot` object in `crates/knot-host/src/js/bootstrap.ts`,
  validating sides (string or a buffer proxy's handle) and `ignoreWhitespace`,
  freezing the result like `find`; declare `diff`, `DiffOptions`,
  `DiffResult`, `DiffHunk`, and `LineRange` in `crates/knot-host/src/js/knot.d.ts`
  with the contract in the doc comment.
- Test harness: a plain-JavaScript diff in `scripts/extensions/testing/host.ts`
  alongside its `findMatches`, matching the native hunks on the tested cases.
- Docs: a short "Compare texts" section in
  `docs/extension-api/application.md`, after "Inspect and change application
  state"; a paragraph in
  `docs/architecture/core-and-buffers.md` next to `BufferModel::find` naming
  the shared diff module and its reload and API users; a decision entry in
  `docs/decisions/core-and-buffers.md` with its index line (native, stateless
  diff over strings or buffers; rejected text handles and per-buffer methods),
  which also records the plan's "diff stands on its own" and "native
  primitive" choices so `vcs.md` can drop them later.
