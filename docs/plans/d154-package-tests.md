# D154 — Fast, isolated package behavior tests

Status: 🚧 checkpoints 1–7 complete; checkpoint 8 is next. Comes before checkpoint 6 of the
[Vim mode](d058-vim-mode.md) plan. Source:
[roadmap](../roadmap.md) D154 and the
[package tests report](../reports/package-tests.md). Absorbs D153.

## Outcome

Package behavior cases run in milliseconds each, in parallel, without a window,
while still exercising the real key routing, native commands, editor state,
extension host, and packages. Only rendering, IME, pointer input, and the
window adapter keep windowed tests. Every case starts from the same package
state, and all case runners share one harness.

The way there is architectural: commands, key routing, focus, and editor state
stop depending on a gpui window. A window becomes an adapter that renders a
window-independent session and forwards raw input to it.

## Evidence

On Apple M1 Pro/macOS debug builds, 364 Vim cases took 94 s: opening a
product window about 50 ms, one key about 80 ms, closing 10 ms. Package
startup takes 0.6 s once per test.

Sampling showed the foreground thread busy, not waiting on V8. About 75% of
it was `Window::draw`: with `test-support`, gpui draws every dirty window at
the end of every update (`gpui-0.2.2/src/app.rs`, `flush_effects`), so each
host round trip that changes visible state lays out and paints the whole
product shell, all of it unoptimized. Optimizing every dependency in the dev
profile cut the suite to 22 s (open 10 ms, one key 17 ms), with drawing still
70% of the time. The status bar items are updated twice after every key, each
causing a draw.

gpui shapes lines only through `WindowTextSystem`, whose constructor is
crate-private, and its test platform uses `NoopTextSystem`, whose advance is
600 units per UTF-16 unit: tests never shaped real text.

## Design

- **Knot owns key matching.** Windows forward raw keystrokes and typed text to
  a Knot key router, which matches single keys and sequences against the
  effective keymap and the view's live key contexts and owns the one-second
  pending-sequence timeout. A sequence is pending only where some binding of
  it can resolve, which fixes D153 by construction. gpui keeps bindings only
  for menu key equivalents.
- **A session per window.** A window-independent session owns the workbench,
  Knot-side focus (pane, tab, surface, and an open palette or transient
  interaction), the key hold, and the key router. Command targets name the
  session and its identities rather than `AnyWindowHandle` and
  `WeakFocusHandle`. The window renders the session and keeps gpui focus in
  step with it. Native prompts go through a session seam that the window
  answers with gpui prompts and tests answer directly. Every product window is
  a session with a window attached, so startup restoration and
  `replaceWindows` build sessions first.
- **Surfaces split into state and view.** An `Editor` entity holds selection,
  scroll and viewport in lines, key contexts, caret shape, and the native
  editing and movement commands; `EditorView` keeps painting, the input
  handler, the pointer, and shaped hit-testing, and reports its viewport size
  in lines. Tree views, terminals, and the shared `TextInput`, palette, and
  picker follow the same split.
- **Vertical movement by display cells.** The wanted column is measured in
  `unicode-width` cells with tabs expanded, as soft wrap already measures
  rows, instead of shaped x. This changes the validated
  [views decision](../decisions.md#views-and-extension-ui): it removes the
  window from movement and makes tests check what users get, at the cost of a
  cell's drift beside proportional fallback glyphs and on right-to-left lines.
- **Headless tests.** A harness starts the application, packages, and sessions
  without a window, types through the router, and settles by awaiting the key
  hold and host idleness rather than polling.
- **Speed in the remaining paths.** gpui and taffy build optimized in the dev
  profile; equal status bar values change nothing; packages skip redundant
  round trips per key. These also help the windowed tests, the debug app, and
  real typing latency.

## Exclusions

- TypeScript case modules (D133).
- A fake host for case tests: cases keep exercising native commands.
- Moving rendering, IME, or pointer tests off windows.

## Deferred decisions

- Where the roadmap places this work, and whether parts become their own
  items, once the tasks are settled.
- How package state resets between cases: a fresh lifecycle or a reset hook,
  chosen by measured cost in checkpoint 10.

## Checkpoints

Checkpoints 1, 2, 3, and 10 are independent of the others; 4 through 8 build
on each other in order. Each checkpoint updates the documentation it changes.

### 1. cargo-nextest — ✅

Rust tests run under `cargo nextest run --all-targets`, one process per test,
locally (`just test`) and in CI (the `ci` profile, which does not fail fast);
doc tests stay on `cargo test --doc`, and the handler-platform jobs keep their
filtered `cargo test` runs. See [development](../development.md).

On Apple M1 Pro/macOS debug builds, tests already built, the suite went from
131 s under `cargo test` to 110–112 s; the 95 s Vim case test bounds it until
later checkpoints. On CI the test step takes about 170–230 s. Parallel test
processes on the CI runner exposed fixtures that assumed a fast machine: test
polling deadlines are now 10 s, and a syntax-query count test sets an
unbounded parse budget.

### 2. Optimized gpui and taffy in debug builds — ✅

`gpui` and `taffy` build at `opt-level = 3` in the dev profile, beside
`tree-sitter` in `Cargo.toml`. On Apple M1 Pro/macOS debug builds the Vim
cases went from 93 s to 22 s (open 13 ms, one key 14 ms), the same as
optimizing every dependency, so the font stack stays unoptimized; level 1
gave 27 s and gpui alone 25 s. The full suite went from 110 s to 35 s.

### 3. Fewer redraws and round trips per key — ✅

Host requests no longer notify the extension host entity, which every
product shell observes, so a request, read-only ones included, no longer
redraws every window. A status update that repeats its value changes
nothing ([status bar](../architecture/workbench-and-lifecycle.md#status-bar)),
and `@knot/vim` and `@knot/status-bar` send only changed items and skip
redundant reads. `src/app/work_counts.rs` counts draws and host requests
under test, and `a_key_costs_few_draws_and_requests` guards them per key.

On a two-line document, a motion went from 51 draws and 22 requests to 3
and 16, a typed character in insert mode from 29 and 11 to 3 and 7. On
Apple M1 Pro/macOS debug builds the Vim cases went from 22 s to 9 s and the
full suite from 35 s to 17 s. The remaining requests per motion are the
package's own reads and caret write, the window lookup for the focused
editor, the tab size, two selection notifications, and the status bar
package reading the caret's line in its own isolate; removing the lookup
needs the view in the command context, which checkpoints 4 and 5 reshape.

### 4. Knot-owned key matching — ✅

A key router (`src/app/commands/key_router.rs`) matches keys and sequences
against the keymap with the focused view's live contexts, holding a key only
where some binding beginning with it resolves to a command; one interceptor
in `src/app/product/key_routing.rs` holds keys behind extension commands and
then routes them, and gpui keeps bindings only for menu key equivalents. See
[commands](../architecture/commands.md#keymaps) and the keymaps decision.
D153 is fixed and removed from the roadmap. The router lives on the product
shell until checkpoint 5 moves it into the session. A key that begins a chord
in the focused context now waits for its next key, so a modal package's key
that shares a chord prefix arrives after the one-second timeout.

### 5. Window-independent sessions — ✅

A product session (`ProductShell`) owns the workbench, Knot-side focus by
identity, the transient interaction and palette, the key hold, and the key
router, and runs without a window; the application registry in
`src/app/product.rs` records each session's workbench and attached window.
Command targets name the session and focus identities, native prompts go
through the session, and launch, tab transfer, and session replacement build
the session before its window. See
[product sessions](../architecture/workbench-and-lifecycle.md#product-sessions),
[commands](../architecture/commands.md#dispatch), and the workbench layout
decision. `commands_run_in_a_session_without_a_window` covers a command
against a windowless session. The window remains the adapter for raw input:
keys and typed text reach the router through it, and tab drags, gutter
marker clicks, and tree row actions carry it as pointer input.

### 6. Editor state and view — ✅

An `Editor` entity holds one presentation's selection, scrolling and
viewport in rows, wrapping, IME preedit, key contexts, caret shape, and
completion, and runs the native editing and movement commands without a
window; `EditorView` renders it and keeps focus, painting, pointer input,
the platform input handler, shaped hit testing, and horizontal scrolling.
Tabs own the view, which owns its editor; commands, editor handles, and
completion reach the `Editor`. Vertical movement keeps a display column in
`unicode-width` cells. See
[editor views](../architecture/workbench-and-lifecycle.md#editor-views) and
the views decision. Editor state tests build an `Editor` without a window,
and `commands_run_in_a_session_without_a_window` edits and moves. Held input
and text bindings still route through the view's input handler and the
window until checkpoint 8.

### 7. The other surfaces — ✅

Trees, the explorer, terminals, pickers, prompts, and `TextInput` handle the
keys the keymap leaves unclaimed in a window-free `key_down`, called by the
session for the surface with Knot-side focus instead of gpui key listeners;
a key that surface ignores becomes typed text, and a session without a
window takes keys and text directly. Tree row commands name the session and
tree. These surfaces stay one entity each: their state needed no window, and
a terminal's state already lives in its session. See
[keymaps](../architecture/commands.md#keymaps),
[product sessions](../architecture/workbench-and-lifecycle.md#product-sessions),
and [semantic UI](../architecture/semantic-ui.md#extension-transient-interactions).
`src/app/product/tests/headless.rs` drives each surface through a windowless
session. Without a window, keys after an extension command are not held yet;
that moves into the session in checkpoint 8.

### 8. Headless harness

Deliverables: a harness that starts the application, packages, and sessions
without a window, types keys and text, and settles without polling; the Vim,
comment, indent, and bracket runners on it; windowed tests kept only for
rendering, IME, pointer input, and the adapter.

Validation: the case suites pass and their per-case times are recorded here.

Implementation context: `src/app/product/tests/vim.rs` (`Fixture`,
`wait_until`, `parse_keys`), `language_editing.rs`
(`start_bundled_packages_with_post_init`), `language_indents.rs`,
`language_brackets.rs`, and `product_window` in `src/app/product/tests.rs`.

Review gate: cases take milliseconds.

### 9. Shared case harness and topic files

Deliverables: one harness for case discovery, JSONC parsing, markers, name
filtering, and failures with file and line; the key notation beside it;
`test/cases/` topic files, each its own test; expectations grouped in objects
and shared or generated starting texts for long documents; parser tests
limited to what cases cannot show.

Validation: every runner uses the harness; a failing case reports its file
and line.

Implementation context: `case_files`, `unmark`, and `CARET` in
`language_indents.rs`; the copies in `language_editing.rs`; `parse_keys` and
`marked` in `vim.rs`; `extensions/@knot/vim/test/cases.jsonc` and
`keys.test.ts`.

Review gate: the case format reads well for a new topic.

### 10. Package state reset between cases

Deliverables: each case starts with a clean clipboard and package state
(registers, last `f`/`t`, global marks, the persisted register record),
by a fresh lifecycle or a reset hook, chosen by cost.

Validation: a case that depends on a register left by an earlier case fails
without the reset and passes with it.

Implementation context: Restart Extension in
`src/app/product/extension_restart.rs`, package storage under
`src/app/extensions/`, `extensions/@knot/vim/src/registers.ts`.

Review gate: case order no longer matters.

### 11. Update vim extension plan

Ensure [vim plan](./d058-vim-mode.md) is up-to-date with the new testing
framework.
