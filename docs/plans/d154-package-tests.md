# D154 — Fast, isolated package behavior tests

Status: 🚧 checkpoint 1 complete; checkpoint 2 next. Comes before checkpoint 6 of the
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
- Whether the font stack joins gpui and taffy in the optimized profile,
  decided by measurement in checkpoint 2.

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

### 2. Optimized gpui and taffy in debug builds

Deliverables: `[profile.dev.package.gpui]` and `[profile.dev.package.taffy]`
at an optimization level chosen by measurement, beside the existing
`tree-sitter` entry.

Validation: Vim suite wall time and per-case open and key times, compared
against all-dependency optimization (22 s) and the baseline (94 s).

Implementation context: `Cargo.toml` `[profile.dev.package.tree-sitter]`.
Time with temporary `Instant` prints around `Fixture::open_selected`,
`press`, and `close` in `src/app/product/tests/vim.rs`; profile with
`sample` on the test process.

Review gate: the narrowed profile captures most of the gain.

### 3. Fewer redraws and round trips per key

Deliverables: an equal status bar value changes nothing and notifies
nothing; `@knot/vim` and `@knot/status-bar` skip unchanged updates and
repeated reads per key; a test-only count of draws and host requests per key
that guards the result.

Validation: counts per key before and after for a motion, an operator, and
insert-mode typing; a product test that an unchanged status update causes no
redraw.

Implementation context: `ApplicationStatusBar::update` in
`src/app/status_bar.rs` (returns true even when equal; it is reached through
`cx.default_global` in `update_status_item`,
`src/app/extensions/host/handlers.rs`), `extensions/@knot/vim/src/extension.ts`
(mode and keys items updated after every key),
`extensions/@knot/status-bar/src/extension.ts`, and the Vim motion path in
`extensions/@knot/vim/src/normal.ts` and `motions.ts`.

Review gate: per-key draws and round trips are as low as the behavior allows.

### 4. Knot-owned key matching

Deliverables: a key router that receives keystrokes and text, matches
sequences against the effective map and live contexts, holds pending
sequences only where they can resolve, and times them out after one second;
windows forward raw input to it; gpui bindings remain only for menu key
equivalents. D153 is removed from the roadmap.

Validation: router unit tests for sequences, contexts, timeouts, and the
unbound-sequence case; the D153 two-package product test; existing keymap and
key-hold tests pass.

Implementation context: `src/app/commands/keymaps.rs` (`rebuild` registers
every binding with gpui; `resolve`, `resolve_in`, and `resolve_text` already
resolve in Knot), `src/app/product/render.rs` (`dispatch_keybinding`,
`dispatch_source`), `src/app/product/key_hold.rs` and
`src/app/commands/key_hold.rs`, the editor input handler in
`src/app/editor/input.rs`, and the terminal's own key path in
`src/app/terminal/view.rs`. Update [commands](../architecture/commands.md)
and the keymaps decision.

Review gate: keys, sequences, menus, and typing behave the same in the app.

### 5. Window-independent sessions

Deliverables: a session that owns the workbench, Knot-side focus, the key
hold, and the router; command targets that name the session; the product
window as an adapter that renders it and follows its focus; a prompt seam;
session-first window creation, restoration, and replacement.

Validation: existing command, closure, relocation, restoration, and
session-replacement tests pass; a test runs a command against a session with
no window.

Implementation context: `ProductCommandTarget` in
`src/app/commands/dispatch.rs` (`window`, `focus`, `validate_tab`),
`ProductShell` in `src/app/product/` (about 38 files use `AnyWindowHandle`
and 15 `ProductCommandTarget`), `src/app/workbench/`,
`src/app/product/launch.rs`, `sessions.rs`, and `closure.rs`. Update
[workbench and lifecycle](../architecture/workbench-and-lifecycle.md),
[commands](../architecture/commands.md), and
[architecture](../architecture.md) ownership.

Review gate: ownership reads correctly in the architecture docs and the app
behaves unchanged.

### 6. Editor state and view

Deliverables: an `Editor` entity with the state and native commands, an
`EditorView` that renders it, cell-based vertical movement, a viewport in
lines set by the view, and editor handles resolving to the `Editor`.

Validation: editor, editing, wrap, and editor-handle tests pass at the
model boundary; movement tests over tabs, CJK, and emoji in cells; the Vim
and language cases pass.

Implementation context: `src/app/editor/` (`layout.rs` `shape_row`,
`x_for_index_dir`, and `fully_visible_lines`; `editing.rs`;
`native_commands.rs`; `wrap.rs` already measures cells), the editor handle in
`src/app/extensions/host/handlers.rs`. Update the views decision,
[editor views](../architecture/workbench-and-lifecycle.md#editor-views), and
the soft-wrap decision's movement paragraph.

Review gate: editing in the app is unchanged except for the movement change.

### 7. The other surfaces

Deliverables: state and view splits for tree views, terminals, and the
shared `TextInput`, palette, and picker, driven through the router and
session.

Validation: existing tree, terminal, palette, picker, and input tests pass,
and each surface has a test without a window.

Implementation context: `src/app/tree_view.rs`, `src/app/terminal/view.rs`
(sessions already outlive views), `src/app/text_input.rs`, the palette and
picker under `src/app/product/`. Update
[semantic UI](../architecture/semantic-ui.md).

Review gate: every product surface works headless.

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
