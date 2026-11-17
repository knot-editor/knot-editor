# D202: Fast typing in large files

Status: 🚧 approved, not started. Roadmap item [D202](../roadmap.md).

## Goal

Typing in a large file feels as immediate as in a small one, whatever the
language. The benchmark files are the documents the `Try: Indent …` commands
generate: Markdown 70k lines (5,000 units of a heading, prose, and a `rust`
fence), Rust 35k, TypeScript 60k, and Python 55k. Plain text of the same size
is the control, with no syntax at all.

The outcome is about how fast typing feels, not about syntax in particular.
Syntax state is the leading suspect, but every foreground cost per keystroke
is in scope. Highlights may lag the text while typing, but they settle soon
after typing stops.

## Targets

On a release build, measured by the checkpoint 1 benchmark over typing in
prose, in a fence or function body, near the start of the file, and near its
end:

- Foreground time per keystroke, from key dispatch through the next frame's
  layout and paint, stays within 4 ms at p99 on every benchmark file. Work
  per keystroke does not grow with file size or with the number of injection
  layers.
- Highlights settle within one background parse of the last keystroke.

The checkpoint 1 review can adjust these numbers once the baseline is known.

## Design

Measure first and fix second. Checkpoint 1 decides which of the provisional
checkpoints below happen, in what order, and whether others are needed.
Reading the code suggests these costs per keystroke (none measured yet):

- `Layer::edit` calls `Tree::edit` on every injected layer after the edit. A
  70k-line Markdown file has tens of thousands of layers, because inline
  content is injected too.
- `SyntaxState::parse_in_foreground` clones the whole layer tree, and every
  `Tree` copy allocates its included ranges. It then spends the 1 ms budget
  on a reparse that cannot finish and throws the result away.
- `finish_background_parse` rebases over the pending edits with the same walk
  over every layer, tries the foreground again, overruns again, and starts
  the next background parse.
- Costs outside syntax are unknown: line index, wrap map, highlight queries
  after a generation change, recovery capture, document observation, and
  extension observation. The plain-text control isolates them.

Fixes keep the existing [syntax state](../architecture/core-and-buffers.md#syntax-state)
contract unless the measurements show it must change: trees follow commits,
background results are rebased or rejected, and painting never calls
JavaScript.

## Exclusions

- Fewer extension host round trips per key stays
  [D156](../roadmap.md). If checkpoint 1 shows host traffic dominates, the
  review decides whether to pull D156 forward instead.
- Type-ahead after a held Enter stays [D175](../roadmap.md).
- Indentation budget changes. D183 showed a larger foreground budget doesn't
  change typing latency.

## Deferred decisions

- Which fixes to make, and how. This depends on checkpoint 1.
- Whether a timing regression check runs in CI, or the benchmark stays an
  ignored test with deterministic tests guarding the mechanisms.

## Checkpoints

### 1. Measure typing per keystroke

Deliverables:

- An ignored release benchmark that starts the real product headless, opens
  each benchmark file and the plain-text control, types bursts of characters
  at the four positions, and reports p50, p99, and max foreground time per
  keystroke, including drawing the window.
- An ignored model-level benchmark that splits one keystroke on the Markdown
  file into stages: commit and edit-log observers, `SyntaxState::edit`, the
  foreground parse attempt including its clone, the rebase of a background
  result, and a visible-range highlight query. It also counts layers.
- A `samply` profile of the release app typing in the `Try: Indent Markdown`
  document, covering costs the headless benchmark cannot see, such as text
  shaping and platform painting.
- A findings section in this plan with the baseline numbers, the ranked
  costs, and the resulting fix checkpoints. The provisional checkpoints below
  are rewritten to match.

Validation: both benchmarks run with
`cargo test --release … -- --ignored --nocapture`, and their invocations are
documented on the tests as the existing ones are.

Review gate: agree on the findings, the targets, and the fix checkpoints
before any optimization.

**Implementation context**

- Fixture generators: port the `units` of the personal-config
  `post-init.js` behind the `Try: Indent …` commands into a shared Rust test
  helper, plus a plain-text unit.
- Product benchmark: mirror the headless driver in `src/app/product/driver/`.
  `editor.rs` has `type_input`, and `VisualTestContext` draws the window. Open
  an untitled document and set its language the way the driver's existing
  language tests do. Place it beside the product tests under `src/app/` as an
  `#[ignore]` test.
- Model benchmark: extend `crates/knot-model/src/syntax/tests.rs` next to
  `indent_reparse_costs_on_large_files` and
  `injection_costs_on_a_large_markdown_file`, and reuse their `Fixture`. Time
  the stages through `BufferModel` in `crates/knot-model/src/buffer.rs`
  (`observe_committed_edits`, `start_syntax_parse`, `finish_syntax_parse`).
- Real app: `cargo build --release`, then profile with `samply record` and
  run the existing `Try:` commands.

### 2. Syntax costs that do not grow with layers (provisional)

The likely shape, to be confirmed by checkpoint 1:

- Skip the foreground attempt when it cannot finish. One way is to remember
  the last reparse duration of the state and go straight to the background
  when it exceeded the budget.
- Make `edit`, `clone`, and rebase leave untouched layers alone, for example
  by shifting layers after the edit lazily rather than editing each tree.

Deterministic tests cover each mechanism, such as layers outside an edit
staying untouched and an overrunning state skipping the foreground. The
benchmarks confirm the targets.

**Implementation context**: `crates/knot-model/src/syntax.rs`
(`SyntaxState::{edit, parse_in_foreground, finish_background_parse}`,
`Layer::{edit, parse, reinject}`), `crates/knot-model/src/buffer.rs`
(`observe_committed_edits`, `finish_syntax_parse`), and the parse driver in
`src/app/model.rs`. Update
[core and buffers](../architecture/core-and-buffers.md#syntax-state) if the
contract changes.

### 3. Costs outside syntax (provisional)

The costs per keystroke that checkpoint 1 finds outside syntax, such as
layout, wrap map, highlight queries, recovery capture, or observation, each
made independent of file size. Tests sit at the boundary that owns each fix.

### 4. Settle highlights sooner (provisional)

Only if checkpoint 1 shows highlights settling late, for example because of
the roughly 40 ms incremental Markdown reparse: shorten the background parse
chain or the reparse itself.

### 5. Close out

Record the final numbers and any validated choice in
[decisions](../decisions/semantic-ui.md#syntax-highlighting) and the relevant
architecture references. Write a personal-config `post-init.js` with `Try:`
commands for the final manual check, then remove this plan.
