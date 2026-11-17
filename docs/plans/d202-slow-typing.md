# D202: Fast typing in large files

Status: 🚧 checkpoint 1 ready for review. Roadmap item [D202](../roadmap.md).

## Goal

Typing in a large file feels as immediate as in a small one, whatever the
language. The benchmark files are the documents of
`knot_model::large_documents`, 5,000 repeated units each: Markdown 50k lines
(a heading, prose, and a `rust` fence per unit), Rust 35k, TypeScript 40k,
and Python 40k. Plain text of 50k lines is the control, with no syntax.

The outcome is about how fast typing feels, not about syntax in particular.
Every foreground cost per keystroke is in scope. Highlights may lag the text
while typing, but they settle soon after typing stops.

## Targets

On a release build, measured by the product benchmark over typing at the
start, middle, and end of each file and in Markdown prose:

- Foreground time per keystroke stays within 4 ms at p99 on every benchmark
  file, both for the key itself (commit and frame) and for the work after it
  (finishing a background parse and drawing its result). Work per keystroke
  does not grow with file size or with the number of injection layers.
- Highlights settle within one background parse of the last keystroke.

## Design

Measure first and fix second. Fixes keep the existing
[syntax state](../architecture/core-and-buffers.md#syntax-state) contract
unless the measurements show it must change: trees follow commits,
background results are rebased or rejected, and painting never calls
JavaScript.

## Exclusions

- Fewer extension host round trips per key stays [D156](../roadmap.md).
- Type-ahead after a held Enter stays [D175](../roadmap.md).
- Indentation budget changes. D183 showed a larger foreground budget doesn't
  change typing latency.

## Deferred decisions

- Whether a timing regression check runs in CI, or the benchmarks stay
  ignored tests with deterministic tests guarding the mechanisms.

## Findings

Baseline on an M-series Mac, release build.

**Product benchmark** (foreground thread time per key, background parses
excluded; "after" is the work once the window settles after the key):

| File | Key p50 / p99 | After p50 / p99 |
| --- | --- | --- |
| Markdown, 50k lines | 45 / 48 ms | 42 / 48 ms (90 ms when a parse is rebased over a second key) |
| Rust, 35k | 30 / 34 ms | 29 / 32 ms |
| TypeScript, 40k | 32 / 34 ms | 32 / 33 ms |
| Python, 40k | 28 / 29 ms | 27 / 29 ms |
| Plain text, 50k | 1.4 / 2.1 ms | 0.04 / 0.1 ms |

The cost is the same at the start, middle, and end of each file, so it
scales with the file, not with the edit.

**Real-app profile** (`samply`, release, `Try: Large Markdown` then
`Try: Type 200 Characters`): 82% of the foreground while typing is
`EditorState::fold_marks` → `BufferModel::fold_regions` →
`syntax::fold::collect`. The gutter asks for fold marks on every paint, and
the regions are cached per revision and syntax generation, which every key
and every finished background parse change. Each paint then runs the
`folds` query over the whole tree of every layer, and reads and splits the
whole text to trim trailing blank lines. Text shaping and Metal drawing are
under 1%.

**Model benchmark** (Markdown, 15,001 layers): the rest of the syntax work
per key grows with layers and with distance from the end of the file.

| Stage | p50 / p99 |
| --- | --- |
| Commit with syntax (plain text: 6 µs) | 1.8–4.9 / 2.6–7.8 ms |
| `SyntaxState::edit`, near the start / end | 3.2 / 0.02 ms |
| Layer clone, before each foreground attempt and background start | 0.33 / 0.6 ms |
| Foreground attempt that cannot finish | 1.7 / 2.4 ms |
| Rebase over one key plus a new foreground attempt | 3.5–11.9 / 5.6–14.2 ms |
| Background parse (off the foreground) | 8.5–11.6 / 20–56 ms |
| 60-line highlight query | 0.1 / 0.3 ms |

Smaller costs seen in the profile: bracket matching through
`SyntaxState::layers_at` (2%), and dropping cloned layers (2%).

**Ranked costs and the checkpoints that follow:**

1. Fold regions over the whole file on every paint: 25–40 ms per key and
   per settle in every language. Checkpoint 2.
2. Syntax work over every layer (edit, clone, failing foreground attempt,
   rebase): 2–14 ms per key in Markdown. Checkpoint 3.
3. Whatever remains, including bracket matching and settle time, measured
   again after 2 and 3. Checkpoint 4.

## Checkpoints

### 1. Measure typing per keystroke ✅ (review pending)

- ✅ Benchmark documents in `knot_model::large_documents`, exported to
  product tests through `knot::test_support::large_documents`.
- ✅ Product benchmark `typing_costs_on_large_documents` in
  `crates/knot-ext-languages/tests/languages/typing.rs`, through
  `TestEditor::key_costs`. In tests gpui runs background tasks on the
  foreground thread, so the driver measures thread CPU time and subtracts
  the background parses `work_counts` times.
- ✅ Model benchmark `typing_stage_costs_on_the_large_markdown_document` in
  `crates/knot-model/src/syntax/tests.rs`.
- ✅ `samply` profile of the release app, recorded with
  `samply record --save-only --unstable-presymbolicate target/release/knot`
  while `Try: Type 200 Characters` typed into `Try: Large Markdown`.
- ✅ Findings above, and checkpoints 2–5 rewritten to match.

Review gate: agree on the findings, the targets, and the fix checkpoints
before any optimization.

### 2. Fold marks that do not grow with the file

The gutter's fold marks cost only the visible lines: the `folds` query runs
over the visible range (a query cursor range) and only on layers that
overlap it, and trailing blank lines are trimmed by reading only the lines a
region ends on. Commands that need every region (Fold All, toggling a
recursive fold) keep computing them for the whole file on demand.

Deterministic tests: marks over a range match the whole-file regions
restricted to it, across languages with and without a `folds` query and
with injections; a layer outside the range is not queried. The product
benchmark confirms the per-key drop in every language.

**Implementation context**: `crates/knot-model/src/syntax/fold.rs`
(`SyntaxState::fold_regions`, `collect`), `crates/knot-model/src/buffer/folds.rs`
(`BufferModel::fold_regions` and its cache, `indentation_regions`),
`crates/knot-model/src/editor/folding.rs` (`fold_marks` and the commands
that use all regions), and the gutter in `src/app/editor/paint.rs`.

### 3. Syntax work that does not grow with layers (provisional)

Re-measure after checkpoint 2, then, for what remains:

- Skip a foreground attempt that cannot finish: remember that the last
  reparse of the state overran the budget and go straight to the background.
- Make `edit`, the clone before a parse, and the rebase leave untouched
  layers alone, for example by shifting layers after the edit lazily rather
  than editing each tree.

Deterministic tests cover each mechanism, such as layers outside an edit
staying untouched and an overrunning state skipping the foreground.

**Implementation context**: `crates/knot-model/src/syntax.rs`
(`SyntaxState::{edit, parse_in_foreground, start_background_parse,
finish_background_parse}`, `Layer::{edit, parse, reinject}`),
`crates/knot-model/src/buffer.rs` (`observe_committed_edits`,
`finish_syntax_parse`), and the parse driver in `src/app/model.rs`. Update
[core and buffers](../architecture/core-and-buffers.md#syntax-state) if the
contract changes.

### 4. Remaining costs (provisional)

Whatever the benchmarks still show over target after checkpoints 2 and 3,
such as bracket matching through `layers_at`, or highlights settling late
after the Markdown background parse. Skipped if both targets hold.

### 5. Close out

Record the final numbers and any validated choice in
[decisions](../decisions/semantic-ui.md#syntax-highlighting) and the relevant
architecture references, refresh the `Try:` commands for the final manual
check, then remove this plan.
