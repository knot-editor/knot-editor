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
  first). Go to File opens Search Everywhere on Files and the palette opens it
  on Actions, so there is a single implementation.
- Tabs are the only way to choose a scope; there are no `>` or `@` prefixes.
  The one query syntax is `:N` (and `file:N`), which adds a "Go to line N" row
  in any tab that lists files; it is a parameter, not a mode.
- A Buffers tab lists open documents, most recently used first. In All, an open
  document is listed once, in the buffers position, not again as a file.
- The palette's own features move into the extension picker's rows: recently
  used commands first, the key binding on each row, and enabled state. The
  native palette is then deleted.

## Exclusions

- A symbol index or a Symbols tab.
- Per-tab remembered state across invocations.
- Dimming unfocused cards.
- A Symbols tab and any query prefix other than `:N`.

## Deferred decisions

- Default Search Everywhere key binding (decided at checkpoint 3 review).
- How an extension lists actions for the Actions tab if no API exists yet; if
  one is needed, add a small primitive rather than native code.

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

- `initialTab` (native picker and API); Go to File becomes a Search Everywhere
  entry on Files.
- Buffers tab (open documents once, most recent first; add a small primitive if
  `documents` cannot list them), and `:N` / `file:N` line rows (check whether
  an open-at-line API exists).
- Validation: extension tests for each tab and entry, `just check`.
- **Review gate:** the user uses the pickers daily and picks Search Everywhere's
  key binding.

### 5. Replace the native palette

- Read the native palette first and size what only it provides (recents, key
  binding display, enabled state); move each into the extension picker rows.
- The palette command opens Search Everywhere on Actions; delete the native
  palette code.
- Validation: tests covering recents ordering, key binding display, and running
  a command; `just check`.
- **Review gate:** the user confirms the palette feels the same or better.

### 6. Wrap-up

Fold durable behavior into the architecture docs, record the validated
choices in `docs/decisions/` and the index, remove the D193 roadmap entry and
its Next up line, and delete this plan.
