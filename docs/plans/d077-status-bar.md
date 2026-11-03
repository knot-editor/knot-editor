# D077 — Status bar

Status: 🚧 approved; checkpoints 1 to 3 done. Item of the [editor API](editor-api.md)
plan; depends on D109 (done). Follows its brief; this file holds the design.

## Goal

Every product window shows a status bar below the workbench. Extensions fill
it through a public API, and the built-in items are an ordinary bundled
package, `@knot/status-bar`, so personal config can hide or replace any of
them. The bar is the first consumer of editor handles, selection observation,
and document facts, ahead of the Vim mode indicator (D058).

## Design

- **Native bar, extension items.** The shell renders a fixed-height bar with
  a left and a right group. Items sort by priority within a side. An item has
  text, an optional tooltip, an optional command with JSON arguments
  (clicking runs it through the captured-window command route, as tree row
  actions do), and an optional theme color key.
- **`workbench.registerStatusItem(id, options)`** returns a lifecycle-owned
  handle: `update(options)` changes the application-wide value, and
  `update(options, { window })` sets or clears a per-window override.
  Teardown removes the item. Latest registration of an ID wins and disposal
  reveals the previous one, as for languages and themes.
- **The bar follows settings.** `statusBar.visible` (default true) with a
  native Toggle Status Bar command, and `statusBar.hiddenItems`, a list of
  item IDs the bar skips. Hidden items still exist, so a package keeps
  running; personal config hides a bundled item or registers its own.
- **Which editor the bar describes** follows VS Code: the focused editor,
  else the last focused editor while its tab stays visible; a focused
  terminal clears the editor items. Native code owns this rule because the
  shell already tracks focus, and exposes the result as a `statusTarget` fact
  on `WindowInfo`, published like `currentTarget`. The package follows one
  value per window and never reimplements focus rules.
- **A `document` settings fact.** `SettingsScope` gains a `document` fact
  holding the document's UUID, beside `language` and `view`, in the native
  store, the extension API, and `knot.d.ts`. The existing rule applies
  unchanged: the match with the most facts wins and ties go to the latest
  write, so a document value beats a language value, and D089's future `path`
  fact extends the same rule. Views and the effective-indentation lookups
  supply their document in the read context; a view with no document, such as
  a contributed text frame, leaves the fact absent and so matches only scopes
  without one.
- **Document-scoped values belong to the document.** Settings values
  otherwise belong to nobody. Entries carrying a `document` fact are dropped
  when the document is released (`ApplicationDocuments::remove`), so closed
  documents leak nothing. Nothing is persisted: a restore detects again, as
  explicit languages and other view state already do.
- **Indentation through settings.** The per-document `indent_style` field on
  `BufferModel` goes away. When a document enters from a resource, detection
  writes `editor.insertSpaces` scoped to the document, as today's detection
  does once from the first 10,000 lines; it writes nothing when the file has
  no clear style. Any later write to the same scope wins by the ordinary
  rule. "Indent using spaces/tabs" and "change tab size" from the bar are
  ordinary document-scoped writes of `editor.insertSpaces` and
  `editor.tabSize`, available to any extension. Detection still beats
  language and global values, as it does today, because it is the narrower
  scope.
- **Document facts.** `DocumentInfo` gains the effective indentation (style
  and size) resolved for the document. A write changes the store and the
  snapshot comparison publishes `documentChanged`, so the status item needs no
  settings observer. Line ending is not part of this item; it is D149.
- **Convert Indentation** to Spaces and to Tabs are native commands that
  rewrite every line's leading indentation as one undo step.
- **Built-in items** in `@knot/status-bar`, following VS Code: position and
  selection size (click: Go to Line), indentation (click: a picker offering
  indent using spaces or tabs, change tab size, convert indentation), and
  language (click: Set Language Mode). Columns count characters with a tab as
  one, as Go to Line reads them. No dirty, encoding, or line-ending item.
  The position item attaches a handle to `statusTarget` and updates from
  `onDidChangeSelection`.

## Exclusions

- Line ending in `DocumentInfo`, its item, and conversion (D149).
- Non-modal notices and progress (D117), tooltips richer than text, and
  item-level context menus.
- The `path` scope fact and EditorConfig (D089, D090), and persisting
  document-scoped values across restarts.

## Deferred decisions

- Whether the bar later gains a middle group or click-and-drag resizing. Two
  groups until a consumer asks.

## Checkpoints

### 1. Document settings scope and indentation ✅

Approved. Behavior: the `document` settings fact and its release on close
([extension host](../architecture/extension-host.md#settings)), indentation
resolved through document-scoped `editor.insertSpaces` and `editor.tabSize`
with Convert Indentation commands
([editor views](../architecture/workbench-and-lifecycle.md#editor-views)), and
the rationale in [decisions](../decisions.md). `DocumentInfo.indentation`
publishes `documentChanged` on any settings write. Under the fact-count rule a
two-fact scope such as `{ language, view }` overrides detection, which writes
only the document fact.

### 2. Active editor per window ✅

Approved after checking the rule on real windows: switching between editors,
a tool, a terminal, a hidden editor tab, and closing the last editor. Behavior:
`WindowInfo.statusTarget` is the focused document tab, else the last focused
document tab while it is the selected tab of its pane, and none while a terminal
is focused ([extension host](../architecture/extension-host.md)). Closing the
last-focused tab resolves to none; the product test does not cover closure, which
was checked by hand.

### 3. Native bar and item API ✅

Approved after a real-window pass over the look, item order, per-window
override, click focus, tooltip, Toggle Status Bar, and `statusBar.hiddenItems`.
Behavior: [extension guide](../extension-api.md#show-status-items),
[workbench and lifecycle](../architecture/workbench-and-lifecycle.md#status-bar),
and [decisions](../decisions.md#status-bar). Settings gained a `strings` type
for `statusBar.hiddenItems`, and `console` output now goes to standard error
under the extension's name. The product test covers one window, so the click's
window comes from the clicked shell; gpui debug bounds outlive their elements,
so hiding is asserted by layout shifts.

### 4. `@knot/status-bar` package ✅

Implemented, awaiting the review gate (a daily-use pass). Behavior is in
[workbench and lifecycle](../architecture/workbench-and-lifecycle.md#status-bar)
and [decisions](../decisions.md#status-bar). Scope decisions made while building it:

- An item with empty text is not shown, which lets a window's missing editor hide
  the items there.
- The three click commands act on the window's focused view, so Go to Line,
  Set Language Mode, and the Convert Indentation choices are unavailable while a
  tool has focus; the Indentation picker's settings writes use `statusTarget` and
  work anywhere.
- Selection size counts characters up to 1 MiB of selected text and shows
  kilobytes beyond.

The remaining plan steps wait for the gate: remove this plan in a separate
final commit, and update the references to D077 in the
[editor API plan](editor-api.md) and the [roadmap](../roadmap.md).

- Deliverables: the position, indentation, and language items, their click
  pickers and commands, tab-size and indent-style changes as document-scoped settings writes,
  and bundling; docs and decisions updated, with the plan reduced to a
  completion note and then removed per the editor API plan's rule.
- Validation: `node:test` package tests on the fake host for each item and
  picker; a Rust product test through the real host showing the caret item
  follows selection and `statusTarget`.
- **Implementation context:** mirror `extensions/@knot/go-to-line`
  (`knot.jsonc`, `src/extension.ts`, `src/main.ts`, `test/`), register in
  `src/app/extensions/bundled.rs`, and rely on `build.rs` to compile it.
  Document the behavior in `docs/architecture/workbench-and-lifecycle.md` and
  `docs/architecture/extension-host.md`, and record rationale in
  `docs/decisions.md`; update `docs/architecture.md` only if boundaries move.
- Review gate: daily-use pass over the bar before D058 builds on it.
