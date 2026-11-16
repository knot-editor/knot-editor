# D193: Chrome refinements

Status: 🚧 checkpoints 1–3 approved; 4–5 added to build one tabbed picker. Roadmap item [D193](../roadmap.md).
Builds on the Islands look ([decision](../decisions/platform.md#product-window-chrome-and-failure-notices),
[workbench](../architecture/workbench-and-lifecycle.md), [themes](../architecture/semantic-ui.md#themes-and-color-keys))
and the native `Picker` ([semantic UI](../architecture/semantic-ui.md)).

## Goal

1. The card that holds the focused pane is visibly emphasized.
2. A picker can show scope tabs, switched from the keyboard, and a new
   Search Everywhere command uses them over files and actions.
3. One picker implementation: Go to File, the command palette, and Search
   Everywhere are the same tabbed picker, differing only in the tab they start
   on. Tabs gain Buffers, and `:N` jumps to a line.

## Design

### Focused card

- A new theme color key `card.focusedBorder` (in `crates/knot-model/src/theme.rs`
  beside `CARD_BACKGROUND`) is painted as an outline over the focused pane,
  so a card with split panes shows which
  one has focus; cards keep `border`. Defaults come from the
  accent, at about 44% alpha for both Islands themes, and a theme may set it equal to `border` to
  disable the emphasis.
- Emphasis needs a border to paint, but `chrome.cardBorder` defaults to `0`.
  The Islands defaults therefore change to a thin border (1 px) on every card,
  so a focus change recolors a border instead of shifting layout.
- The focused pane is the workbench's (center or dock). While a transient
  surface (palette, picker) has focus, the pane that would receive focus
  keeps the outline.

### Picker tabs

- `PickerOptions.tabs?: { id: string; label: string }[]`. The first tab is
  active initially (convention: an "All" tab).
- Local items gain `tabs?: string[]`, the tab ids a row belongs to; a row
  without tags shows in every tab. The native `Picker` filters its entries by
  the active tab before matching.
- `onQuery` is called as `(query, tab)` with the active tab's id, and a tab
  change re-queries like a query change (the same pull loop and
  stale-answer rule, with the tab as part of what an answer applies to).
- `Tab` / `Shift+Tab` cycle tabs in a picker with tabs; they are commands in
  the picker key context, so they are rebindable. A picker without tabs
  leaves them unclaimed. The strip renders under the query field and tabs are
  also clickable.
- Search Everywhere is a new command (bundled package, likely beside
  `go-to-file`) with tabs All, Files, Actions, using `onQuery` because files
  are asynchronous. Symbols is not offered until a symbol source exists.

### One picker

- `PickerOptions.initialTab?: string` picks the starting tab (default: the
  first). `workspace.search-everywhere` takes an optional `{ tab }` argument.
  Go to File calls the same function on Files.
- Tabs: All, Buffers, Files, Actions. Tabs are the only way to choose a scope;
  there are no `>` or `@` prefixes.
- **Buffers** lists open documents (untitled ones included), most recently
  active first, with the current buffer last. The package keeps a second
  `history` list of document IDs, recorded in `recordActive` beside recent
  files, and orders `documents.list()` by it. Rows match on the title and open
  with `documents.present(id)`.
- **All** shows buffers, then files, then actions (capped at 8). An open
  document is listed once, as a buffer: file rows whose URI matches an open
  document's `resourceUri` are dropped. With an empty query, All shows buffers
  and recent files and no actions.
- **Line jump**, the one query syntax, is a parameter, not a mode:
  - A bare `:N` adds a "Go to line N" row for the current editor, in every tab.
  - `text:N` matches on `text`, and opening any buffer or file row then goes
    to line N.

  Going to a line is `documents.open`/`present`, then `presentations.editor`,
  `lineRange`, and `setSelection` with a centered `reveal`; no new primitive.
- **Actions** take over the palette's behavior:
  - Matching: `completion.match` over titles and over names, merged with the
    best score kept, so the matching setting applies and a name matches though
    it is not shown.
  - Recents: commands chosen here are recorded with `history.record`, and
    `history.order` puts them first, so recency also breaks equal scores.
    Native `CommandHistory` and its state-database record are deleted.
  - Key bindings: `commands.list(view?)` gains `key: string | null`, the
    binding shown for that view's kind, already formatted for display (the
    native `display_key`). Search Everywhere passes the view it opened from.
  - The commands that open this picker (Search Everywhere, Go to File, Show
    Command Palette) are left out of Actions: `commands.invoke` refuses to run
    an ancestor again.
- **Palette command and fallback.** `workbench.show-command-palette` stays
  native and keeps its key binding. When `workspace.search-everywhere` is
  registered, it runs that command with `{ tab: "actions" }`. Otherwise
  (extensions failed to load or the package is disabled) it opens a minimal
  native fallback, enough to diagnose a JavaScript failure and reach commands
  such as the extension report or Close: the registered commands by title in
  the shared `Picker`, matched natively, with no key bindings, descriptions,
  or recents. The native palette shrinks to that.

## Exclusions

- A symbol index, a Symbols tab, and any query prefix other than `:N`.
- Per-tab remembered state across invocations.
- Dimming unfocused cards.
- Folding Open Recent into the picker: it also lists recent files outside the
  window's roots, which Files does not.
- A disabled (greyed out) state for commands: the palette never had one.

## Deferred decisions

- Default Search Everywhere key binding (decided at checkpoint 4 review).

## Checkpoints

### 1. Focused card emphasis ✅ approved

Done: `card.focusedBorder` outlines the focused pane (see
[themes](../architecture/semantic-ui.md#themes-and-color-keys)), and
`chrome.cardBorder` defaults to 1 so every card has a thin border.

### 2. Picker tabs (native and API) ✅ approved

Done: a picker shows scope tabs, filters local rows by tab, and passes the tab
to `onQuery` (see [semantic UI](../architecture/semantic-ui.md) and the
[extension guide](../extension-api.md)). Tab and Shift+Tab run
`picker.next-tab` / `picker.previous-tab` in a native `picker` view kind, so
they are rebindable ([commands](../architecture/commands.md#picker-tabs)).

### 3. Search Everywhere ✅ approved

Done: `workspace.search-everywhere` ("Search Everywhere…") in
`extensions/@knot/go-to-file` over All, Files, and Actions tabs. Files reuse
Go to File's rows (`fileRows.ts`); actions come from the `commands.list()`
primitive (see the [extension guide](../extension-api.md)) and run with
`commands.invoke`. Actions have no icon, as in the palette. An empty All tab
lists recent files only, and All caps actions at 8. The key binding is chosen
at checkpoint 4.

### 4. One picker: entries, Buffers, line jump

- `initialTab` (native picker and API) and Search Everywhere's `{ tab }`
  argument; Go to File opens it on Files (its binding and toolbar button stay).
- Buffers tab and its document-ID history; All lists buffers first and drops
  file rows of open documents.
- `:N` and `text:N` line jumps.
- Validation: extension tests through the real editor for each tab, both
  entries, the buffer order (current last, untitled included), the All
  de-duplication, and both line-jump forms; `just check`.
- **Review gate:** the user uses the pickers daily and picks Search
  Everywhere's key binding.

### 5. Palette on the single picker

- `commands.list(view?)` reports each command's displayed `key`.
- Actions: `completion.match` over titles and names, `history` recents, key
  bindings on rows, and the picker's own commands left out.
- `workbench.show-command-palette` runs Search Everywhere on Actions when it is
  registered, else the minimal native fallback. Delete `CommandHistory` and its
  state-database record, and strip the native palette to the fallback (titles
  only: no recents, binding hints, or descriptions).
- Validation: extension tests for recents ordering and tie-breaking, name-only
  matches, the key binding shown for the opening view, and running a command
  against the captured target. Native tests for the fallback when the package
  is absent, and the existing palette key binding tests still pass; `just check`.
- Check that opening and typing feel as fast as the native palette (it now
  waits on host round trips) before the gate.
- **Review gate:** the user confirms the palette feels the same or better.

### 6. Wrap-up

Fold durable behavior into the architecture docs (including the
`docs/architecture.md` state-database line that names the palette's recent
commands), record the validated choices in `docs/decisions/` and the index
(including the native fallback palette), remove the D193 roadmap entry and its
Next up line, and delete this plan.
