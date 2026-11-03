# Editor API and Vim mode

Status: 🚧 phase 1 (D110) and D111 complete. Promotes D058, D066 (folding), D077, D109,
D110, D111, D112, and D114 (decoration styles) from the
[roadmap](../roadmap.md) and adds D141–D143. Vim features left for later are
D113, D142, D144, and D146.

## Goal

Give extensions the editing half of a Neovim-depth API and prove it with a
Vim mode that its author uses every day, built as a bundled JavaScript
package on public primitives only. The status bar is the first consumer: its
built-in items move to a bundled package on the same API.

The [extensibility report](../reports/extensibility.md) maps Vim's needs onto
these primitives and sets the guardrails: primitives stay generic enough for
a Helix or Kakoune mode, Vim's vocabulary (modes, registers, operators)
stays inside the extension, and native movement is exposed as parameterized
commands rather than layout data.

## How this plan is executed

This plan is an index, executed like the [daily-driver](daily-driver.md)
plan. Each item below is a brief, not a design. Before implementing an item,
analyze it in its own session and write its design in a separate plan,
`docs/plans/d0xx-<slug>.md` or `d1xx-<slug>.md`, however small. Link that
plan from the item here and never add design detail to this file. Mark
progress with ✅ and 🚧. Remove each item's plan once it is complete, and
remove this plan once every item's durable behavior lives in the
architecture references and decisions.

Every primitive lands together with a consumer that uses it: the status bar,
word occurrence highlighting, Find, or the Vim package. No primitive is
built ahead of its consumer.

Nothing that exists today is a constraint; Knot has no users.

## Scope

In scope for the Vim mode: normal, insert, visual, and visual-line modes;
operators with counts and motions; plain text objects (`iw`, `i(`, `ip`, and
the like); registers backed by the clipboard; marks; `/`, `?`, `n`, `N`, `*`,
`#` with search highlights; folding commands; relative line numbers; a mode
indicator in the status bar; a cursor style per mode; and the core Ex set
(`:w`, `:q`, `:wq`, `:e`, `:s` and `:%s` with ranges, `:<line>`, `:noh`, and
`:set` for a few options).

Excluded, and staying deferred in the roadmap:

- Visual-block mode and multiple or rectangular selections (D142), which need
  the piece-tree buffer (D092).
- `.` repeat and `q` macros (D113).
- Tree-sitter text objects and the public syntax-tree query API (D144).
- Language servers (D067) and the presentation and hook items that would be
  designed against a real server: virtual text and floating panels (D145),
  lifecycle hooks (D115), and progress (D117).
- The broad Ex set (D146).

## Relation to the daily-driver plan

This plan runs now, ahead of the daily-driver files-and-workspace phase. It
schedules three daily-driver items where Vim needs them, and they keep their
briefs and plans there:

- D031, Current-file Find, with or after D114 here, so its match highlights
  use a decoration style.
- D080, Word occurrence highlighting, after D109 here, which supplies the
  cursor observation it asks for, and D114.
- D078, Zoom, font size, and soft wrap, before D112 here, which moves by its
  visual lines.

D077 moved here from the daily-driver plan.

## Ordering rationale

- **Key delivery first.** The D110 spike decides whether modes are native key
  contexts, native key buffering, or both, which shapes commands, the Vim
  package, and later recording. Its measurements need no other item.
- **Handles and observation before every consumer.** The status bar, word
  occurrences, and Vim all follow an editor beyond one command.
- **The status bar as the first consumer.** It exercises handles,
  observation, and document facts on a small surface before Vim depends on
  them.
- **Decoration styles before Find, occurrences, and Vim search.** All three
  paint match highlights.
- **Soft wrap before visual-line movement.** D112 moves by the visual lines
  D078 defines.
- **Vim last.** It is the acceptance test for everything above it.

## Phase 1 — Key input

- **D110 — Key contexts and ordered key delivery** ✅
  - Complete: lifecycle-owned key contexts on editor views, bindings and
    text bindings scoped to a context and resolved against the live
    contexts, and ordered delivery of keys after key-invoked extension
    commands. Behavior in [commands](../architecture/commands.md#key-contexts),
    rationale and latency in
    [decisions](../decisions.md#commands-and-keymaps), and a small modal
    package in `examples/extensions/@example/modal`, to be removed when
    D058 lands.

## Phase 2 — Editor handles and the status bar

- **D109 — Editor handles and selection observation** ✅
  - Complete: `workbench.editor(target)` returns a long-lived handle bound
    to the exact document tab, with key contexts, `onDidClose`, and coalesced
    payload-free `onDidChangeSelection`. Behavior in
    [commands](../architecture/commands.md#extension-execution) and
    [extension host](../architecture/extension-host.md#editor-handle-notifications),
    rationale in [decisions](../decisions.md#editor-handles), and a small
    follower in `examples/extensions/@example/modal`, to be removed when D058
    lands. One selection per view; several and rectangular selections are
    D142.

- **D077 — Status bar** (depends on D109) ✅
  - Complete: a native per-window bar with `workbench.registerStatusItem`,
    the `statusBar.visible` and `statusBar.hiddenItems` settings, and Toggle
    Status Bar; the bundled `@knot/status-bar` package shows position,
    indentation, and language. Behavior in
    [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#status-bar)
    and the [extension guide](../extension-api.md#show-status-items),
    rationale in [decisions](../decisions.md#status-bar). Windows publish the
    editor a bar describes as `statusTarget`, indentation is a
    document-scoped setting, and line ending is D149.

## Phase 3 — Editing primitives

- **D111 — Undo groups across edit batches** ✅
  - Complete: `editor.beginUndoGroup()` holds a lifecycle-owned group open on
    the buffer, so batches, native typing, and caret moves in between are one
    undo step. Behavior in
    [core and buffers](../architecture/core-and-buffers.md) and
    [commands](../architecture/commands.md#extension-execution), rationale in
    [decisions](../decisions.md#editor-handles), and a change command in
    `examples/extensions/@example/modal`, to be removed when D058 lands.

- **D141 — Editor view controls**
  - 🚧 [Plan](d141-editor-view-controls.md)
  - Extend the D109 handle with what modal editing needs: cursor style,
    the visible line range, scrolling without moving the caret, a reveal at
    the top or bottom (Vim's `zt`, `zb`), undecorated anchored positions
    for marks, and clipboard read.

- **D114 — Decoration styles**
  - Contribution decorations choose a style beside their color key:
    background, straight underline, or today's wavy underline. Find (D031),
    word occurrences (D080), Vim search, and visual-mode feedback consume
    it. Virtual text and anchored floating panels are D145.

## Phase 4 — Layout and folding

D078 from the daily-driver plan lands before this phase.

- **D112 — Visual-line movement and layout queries** (depends on D078, D109)
  - Native movement as parameterized commands: by visual line, by page,
    and to screen positions, so Vim's `j`/`k`, `gj`/`gk`, `H`/`M`/`L`,
    and `zz` need no shaping data.

- **D066 — Folding** (depends on D109)
  - Per-language `folds` queries in language registrations beside
    `indents`, native fold state per view, a fold lane in the gutter, and
    Fold, Unfold, and Toggle Fold commands. The editor handle folds and
    unfolds arbitrary ranges, so Vim's `zf` creates manual folds.
  - Movement, search, and editing treat folded lines as VS Code does.
    Restorable view state records folds.
  - The public syntax-tree query API and text objects are D144.

## Phase 5 — Vim mode

- **D143 — Input completion**
  - `workbench.showInput` offers completion items as the text changes,
    through the pulled input session `onChange` already uses, for the Ex
    command line.

- **D058 — Vim mode** (depends on every item above)
  - A bundled `@knot/vim` package implementing the scope above on public
    APIs only. A `vim.enabled` setting, off by default, turns it on and off
    at runtime, and scoped values can turn it off for a language or view. Any gap it finds is
    closed as a generic primitive, never a private bypass.
