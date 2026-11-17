# D202: Fast typing in large files

Status: 🚧 checkpoints 1–3 complete; checkpoint 4 only if needed, then 5. Roadmap item [D202](../roadmap.md).

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

### 1. Measure typing per keystroke ✅

Approved with the findings, targets, and checkpoints above. The benchmarks
are `typing_costs_on_large_documents` in
`crates/knot-ext-languages/tests/languages/typing.rs` (through
`TestEditor::key_costs`, which subtracts background parses because tests run
them on the foreground thread) and
`typing_stage_costs_on_the_large_markdown_document` in
`crates/knot-model/src/syntax/tests.rs`, over `knot_model::large_documents`.
The profile was recorded with
`samply record --save-only --unstable-presymbolicate target/release/knot`;
it types through `Try: Type 200 Characters`, which edits through the buffer
API and so skips key routing, cheap in the benchmark.

### 2. Fold marks that do not grow with the file ✅

Approved: noticeably faster in the app, though not yet immediate. The
gutter asks `BufferModel::fold_starts` for the visible lines only; it queries
the visible rows of the layers that reach them, and the whole-text
`fold_regions` stays for the fold commands. Tested against the whole-text
regions over every bundled fold case and the shortened benchmark documents.

Product benchmark after it (p99 per key / after the key, fast pace):

| File | Key | After |
| --- | --- | --- |
| Markdown | 5.5–10.6 ms | 17.7 ms |
| Rust | 2.8 ms | 4.4 ms |
| TypeScript | 2.5 ms | 4.1 ms |
| Python | 2.5 ms | 4.7 ms |
| Plain text | 0.9 ms | 0.06 ms |

Markdown's remaining cost is the per-layer syntax work of checkpoint 3; the
other languages' after work at the fast pace (a rebased parse) is just over
target.

### 3. Syntax work that does not grow with layers ✅

Approved: typing feels much better in the real app. An edit touches only the
root layer and injections catch up when read or reparsed; a state that
overran the foreground budget skips the foreground until a background parse
fits it; layer lookups test the host span first; and replaced layers are
dropped on the application's background executor. See
[core and buffers](../architecture/core-and-buffers.md#syntax-state).

Product benchmark after it (p99 per key / after the key, both paces; the
test executor's background work is subtracted):

| File | Key | After |
| --- | --- | --- |
| Markdown | 1.4–2.0 ms | 1.2–2.4 ms |
| Rust | 1.0–1.1 ms | 1.0–2.0 ms |
| TypeScript | 1.0–1.2 ms | 1.1–2.3 ms |
| Python | 0.9–1.0 ms | 1.0–1.9 ms |
| Plain text | 0.8–0.9 ms | 0.07 ms |

Model benchmark (Markdown, 15,001 layers): syntax edit 2 µs, rebase 40 µs,
whatever the edit position.

### 4. Remaining costs (provisional)

Both targets hold after checkpoint 3 in the benchmark, so this is skipped
unless the manual check finds more. Whatever remains over target,
such as bracket matching through `layers_at`, or highlights settling late
after the Markdown background parse. Skipped if both targets hold.

### 5. Close out

Record the final numbers and any validated choice in
[decisions](../decisions/semantic-ui.md#syntax-highlighting) and the relevant
architecture references, refresh the `Try:` commands for the final manual
check, then remove this plan.
