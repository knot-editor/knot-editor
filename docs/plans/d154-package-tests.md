# D154 — Fast, isolated package behavior tests

Status: ✅ checkpoints 1–11 complete. Comes before checkpoint 6 of the
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

### 8. Headless harness — ✅

The session owns the key hold and text-binding routing, with or without a
window ([ordered delivery](../architecture/commands.md#ordered-delivery)), and
a windowless session joins application observation and becomes active once
activated ([product sessions](../architecture/workbench-and-lifecycle.md#product-sessions)).
`src/app/product/tests/harness.rs` starts the bundled packages without a
window and shows each Vim case in one windowless session, in a fresh
workbench laid out at 40 rows by 100 cells. It settles by waiting on the
extension pool's finished turns rather than polling, until no package is
busy and no keys are held. The indent and bracket runners use an `Editor`
without a window. Only the mouse drag and per-key draw count stay windowed.
Status bar and settings changes redraw a window without notifying its
session, so they no longer rebuild the application snapshot. Product tests
keep their databases in memory unless they reopen one as a restart would.
See [development](../development.md).

On Apple M1 Pro/macOS debug builds the 364 Vim cases went from 6.4 s in a
window to 1.7 s, about 4.7 ms per case, and the full suite from 16 s to 14 s.
The window launch opened had cost about 40% of each case by redrawing. What
remains is mostly the package's round trips per key: the test thread waits
on extension turns for about 40% of the run, and half of each turn is the
debug-built host glue rather than V8, which ships as a release library.
Fewer round trips per key is D156.

### 9. Shared case harness and topic files — ✅

`src/app/product/tests/cases.rs` is the one harness: case discovery in
`test/cases/<topic>.jsonc`, JSONC parsing with line numbers, `KNOT_CASES`
name filtering, caret markers, generated `{ "numbered": n }` texts for long
documents, the key notation, and failures that name the case's file and line.
The comment, indent, bracket, and Vim runners all use it, and the editor
runner they share moved into `harness.rs`. Vim cases are split into eight
topic files, each its own parallel test, with a guard test that every file
is listed; a case groups its expectations (`text`, `mode`, `top`) in an
`after` object when it needs more than text. Two TypeScript motion tests that
the cases repeat were removed; the remaining package unit tests check pure
functions with edge cases the cases do not show. See
[development](../development.md).

### 10. Package state reset between cases — ✅

Before each case the Vim runner empties the clipboard and runs the package's
hidden `vim.reset`, which clears the registers and deletes their record, and
forgets marks and the last `f`/`t`. A reset hook won over a fresh lifecycle
by cost: restarting `@knot/vim` takes about 14 ms on Apple M1 Pro/macOS debug
builds, about three times a whole case, while the reset is lost in the noise.
`a_case_starts_without_what_an_earlier_one_left` fails without the reset and
passes with it. The current cases also pass in reverse order with or without
it, so none depended on order. See [development](../development.md).

### 11. Update vim extension plan — ✅

The [vim plan](./d058-vim-mode.md) now describes the windowless Vim cases,
the product tests that stay windowed, and the key router in place of gpui's
pending-sequence timeout, and it no longer refers to D154's checkpoints.
