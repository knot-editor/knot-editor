# D183: Deterministic indentation after Enter and closing tokens

Status: 🚧 checkpoint 1 awaiting review. Roadmap item [D183](../roadmap.md).
Builds on [syntax state](../architecture/core-and-buffers.md#syntax-state),
[editor views](../architecture/editor-and-languages.md#editor-views), and the
[indentation decision](../decisions/editing.md#indentation).

## Goal

Enter and typing a closing or branching token (`}`, `else`, Python's
`else:`) indent the line the same way regardless of file size or typing
speed. Today the indent request waits for a syntax tree at its exact
revision; when the 1 ms foreground reparse overruns, a key typed before the
background parse finishes drops the request and the line keeps the wrong
indentation.

## Design

- **A larger foreground budget for indenting commits.** After Enter, and after
  any typing commit on a line that has indentation (a line at column 0 can't
  outdent), the editor asks the buffer to bring its tree current within a
  larger indent budget before applying the pending indent. Every typed
  character goes through `outdent_typed_line`, so this applies to typing in
  general on indented lines, as Zed and Neovim parse synchronously after every
  edit. The cost is the parse time itself, and only when a reparse falls between
  1 ms and the indent budget; the work is the same as the background parse it
  replaces.
- **The budget comes from a benchmark.** Incremental reparses for Enter and
  `}` edits on large Rust, TypeScript, Python, and Markdown-with-fences files
  set the indent budget near their p99, capped at about half a frame (8 ms).
- **Take over a running background parse.** A foreground parse is skipped
  while a background parse runs, which is exactly the large-file case (a paste
  or an earlier keystroke overran first). The indent-budget parse cancels the
  running background parse, restores the edits it took plus those committed
  since, and reparses in the foreground from the current edited trees. The
  cancelled result is rejected by its parse ID as any stale result is.
- **The deferred request remains beyond the budget.** If the indent-budget
  parse also overruns, the request waits for the background parse as today,
  and a later commit drops it. Rebasing the request over later typing stays
  rejected (see the roadmap entry).
- **API.** `BufferModel::parse_syntax_now(budget) -> bool` (cancel, then parse
  in the foreground) is the one new model entry point; the commit's own 1 ms
  attempt still runs first, and its work is discarded on overrun.

## Exclusions

- Raising the budget for other commits (paste, extension edits, undo,
  language changes, initial parse); they keep the 1 ms budget and background
  continuation.
- Textual pre-filters guessing whether a typed character can close a node.
- Enter between an auto-closed pair needs no change beyond sharing the same
  path.

## Deferred decisions

- None.

## Checkpoint 1: benchmark and indent budget ✅ (awaiting review)

Deliverables:

- An `#[ignore]` benchmark test in `knot-model`, run with
  `cargo test --release -p knot-model -- --ignored --nocapture <name>`, that
  builds large generated files (about 10k–50k lines) in Rust, TypeScript,
  Python, and Markdown with code fences, and times the incremental reparse
  after an Enter edit and a typed `}` at many positions, printing median, p99,
  and maximum per language and size.
- Results recorded in this plan, and the indent budget chosen from them as a
  `knot-model` constant beside `FOREGROUND_BUDGET`.

Validation: the benchmark runs locally in release mode; `just check` passes.

Results ✅ (`indent_reparse_costs_on_large_files` in `syntax/tests.rs`, release,
200 positions per row, the edit reverted between samples; "close" is `}`, or
`else:` typed on a whitespace line for Python):

| Language | Lines | Enter median / p99 / max | Close median / p99 / max |
| --- | --- | --- | --- |
| Rust | 7k | 1.1 / 1.9 / 1.9 ms | 1.1 / 1.9 / 1.9 ms |
| Rust | 35k | 7.6 / 9.5 / 9.9 ms | 7.4 / 9.5 / 12.6 ms |
| TypeScript | 12k | 0.3 / 0.7 / 0.7 ms | 0.3 / 0.8 / 0.9 ms |
| TypeScript | 60k | 2.6 / 4.0 / 4.3 ms | 2.7 / 4.4 / 5.3 ms |
| Python | 11k | 0.8 / 1.3 / 1.3 ms | 10.0 / 18.7 / 19.0 ms |
| Python | 55k | 4.8 / 13.3 / 14.3 ms | 60.3 / 99.5 / 100.1 ms |
| Markdown | 14k | 6.4 / 8.0 / 28 ms | 6.4 / 13.3 / 14.3 ms |
| Markdown | 70k | 39.4 / 48.7 / 147 ms | 39.6 / 49.1 / 50.3 ms |

Findings: reparse time grows with file size for flat files (Rust, Markdown),
not just edit locality. Indent budget chosen: `INDENT_BUDGET` = 8 ms, the cap.
It covers Enter and `}` through roughly 30k Rust lines, TypeScript at any
measured size, and Python Enter; Python `else:` (the dedent re-lexes much of
the file) and the largest Markdown files exceed it and use the deferred
fallback.

Review gate: the user reviews the numbers and the chosen budget.

**Implementation context:** `FOREGROUND_BUDGET` and
`SyntaxState::parse_in_foreground` in `crates/knot-model/src/syntax.rs`; the
benchmark can drive `BufferModel` directly (`from_text`, `set_syntax`,
`set_syntax_budget(Duration::ZERO)` to keep commits from parsing, then time
`parse_in_foreground` with an unbounded budget) the way
`crates/knot-model/src/buffer/tests.rs` drives syntax tests. Grammars come from
the compiled-in languages in `crates/knot-model/src/languages.rs`; mirror
how existing syntax tests obtain a `SyntaxState` with an indent query.

## Checkpoint 2: indent-budget parse and take-over

Deliverables:

- `SyntaxState` can cancel a running background parse and restore its edits:
  `RunningParse` keeps a copy of the edits `start_background_parse` took, and
  cancellation sets its flag and restores `edited` to those plus `pending`.
- `BufferModel::parse_syntax_now(budget)` cancels any running parse and
  parses in the foreground within `budget`, returning whether the tree is
  current.
- `EditorState::insert_newline` calls it with the indent budget before
  `apply_pending_indent`; `outdent_typed_line` does too when the caret's line
  has leading whitespace.
- Architecture, decision, and roadmap updates (below).
- A personal-config `post-init.js` with `Try:` commands that open a large
  generated file in each covered language for manual typing.

Validation:

- Model tests: with the commit budget at zero and a background parse running,
  Enter and a typed `}` indent immediately and the stale background result is
  rejected; an indent-budget overrun keeps the deferred request and the
  background parse continues; typing on a column-0 line doesn't escalate.
- A gpui editor test that types Enter then another key with no
  `run_until_parked` between them while commits overrun, asserting the
  indentation, alongside the existing editing driver cases.
- `just check` passes.

Review gate: the user types quickly in large files through the `Try:`
commands and confirms Enter and closing tokens indent consistently with no
noticeable typing latency.

**Implementation context:** `RunningParse`, `start_background_parse`,
`finish_background_parse`, and `edit` in `crates/knot-model/src/syntax.rs`;
`observe_committed_edits`, `start_syntax_parse`, and `finish_syntax_parse` in
`crates/knot-model/src/buffer.rs` (new method beside them, budget tests in
`buffer/tests.rs` around the existing `set_syntax_budget` cases);
`insert_newline`, `outdent_typed_line`, and `apply_pending_indent` in
`crates/knot-model/src/editor/indentation.rs`, with `type_text` in
`editor/input.rs` as the caller. The application side
(`Buffer::continue_syntax_parse` in `src/app/model.rs`) needs no change: a
cancelled parse's result fails `finish_syntax_parse` and the next notify
restarts a parse if the tree still lags. Editor tests live in
`src/app/editor/tests.rs` (see the case that sets a 60 s budget) and the
driver in `src/app/product/driver/editing.rs`. Docs: the Enter paragraph of
[editor views](../architecture/editor-and-languages.md#editor-views), the
syntax-state flow in
[core and buffers](../architecture/core-and-buffers.md#syntax-state), and the
Enter paragraph of the [indentation decision](../decisions/editing.md#indentation)
with the benchmark numbers; remove D183 from the roadmap when done.
