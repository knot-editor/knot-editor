# D193: Chrome refinements

Status: 🚧 checkpoint 2 awaiting review; checkpoint 3 next. Roadmap item [D193](../roadmap.md).
Builds on the Islands look ([decision](../decisions/platform.md#product-window-chrome-and-failure-notices),
[workbench](../architecture/workbench-and-lifecycle.md), [themes](../architecture/semantic-ui.md#themes-and-color-keys))
and the native `Picker` ([semantic UI](../architecture/semantic-ui.md)).

## Goal

1. The card that holds the focused pane is visibly emphasized.
2. A picker can show scope tabs, switched from the keyboard, and a new
   Search Everywhere command uses them over files and actions.

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

## Exclusions

- A symbol index or a Symbols tab.
- Per-tab remembered state across invocations.
- Dimming unfocused cards.
- Changing the command palette or Go to File to use tabs.

## Deferred decisions

- Default Search Everywhere key binding (decided at checkpoint 3 review).
- How an extension lists actions for the Actions tab if no API exists yet; if
  one is needed, add a small primitive rather than native code.

## Checkpoints

### 1. Focused card emphasis ✅ approved

Done: `card.focusedBorder` outlines the focused pane (see
[themes](../architecture/semantic-ui.md#themes-and-color-keys)), and
`chrome.cardBorder` defaults to 1 so every card has a thin border.

### 2. Picker tabs (native and API) ✅ awaiting review

Done: tab strip, filtering, `onQuery(query, tab)`, and rebindable
`picker.next-tab` / `picker.previous-tab` in a new native `picker` view kind.
Keys typed in a picker with tabs now go through the router, where only
bindings scoped to `picker` apply (decided with the user, over native-only
Tab handling). The fake test host gained `selectTab`. Docs: [semantic
UI](../architecture/semantic-ui.md), [commands](../architecture/commands.md#picker-tabs),
[extension guide](../extension-api.md).

- Deliverable: tab state, strip rendering, filtering, key commands, protocol
  and `knot.d.ts` changes, `onQuery` tab argument, extension guide.
- **Implementation context:** `src/app/picker.rs` (`Picker`, `PickerEntry`,
  `PickerEvent::Queried`, `render`), `src/app/product/` picker driver and
  `key_routing.rs`, the picker key context and keymap in
  `src/app/commands/`, `crates/knot-protocol/src/` (`PickerOptions` and the
  `beginPicker`/`nextPickerEvent` types), `crates/knot-host/src/js/knot.d.ts`,
  `docs/schemas/knot.schema.json`, `src/app/extensions/` picker handlers,
  and `docs/architecture/semantic-ui.md` (picker section).
- Validation: picker unit tests (filter by tab, cycling wraps, tab change
  re-queries, stale answers dropped); a JS extension test through the real
  editor with a tabbed picker; `just check`.
- **Review gate:** a `try.*` command opening a picker with tabs.

### 3. Search Everywhere

- Deliverable: the command and its bundled package changes, All/Files/Actions
  rows with the right ordering and icons, tests.
- **Implementation context:** `extensions/@knot/go-to-file/src/` (reuse
  `files.search` and `rowFor` from `rows.ts`), the command catalog access for
  actions (check the extension API for listing commands; add a primitive if
  missing), `extensions/@knot/go-to-file/test/` for the end-to-end test style,
  the extension guide.
- Validation: extension tests through the real editor for each tab, running
  an action and opening a file; `just check`.
- **Review gate:** the user tries Search Everywhere in daily use and picks
  its key binding.

### 4. Wrap-up

Fold durable behavior into the architecture docs, record the validated
choices in `docs/decisions/` and the index, remove the D193 roadmap entry and
its Next up line, and delete this plan.
