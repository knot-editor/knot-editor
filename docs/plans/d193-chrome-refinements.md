# D193: Chrome refinements

Status: 🚧 checkpoint 1 built, awaiting review. Roadmap item [D193](../roadmap.md).
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

### 1. Focused card emphasis ✅ built, at its review gate

- Deliverable: the color key, the Islands border default, painting, docs
  (themes reference, settings reference if any, decision note), bundled
  themes updated.
- **Implementation context:** `crates/knot-model/src/theme.rs` (color key
  table near line 295, `card_border` default near 357), `src/app/theme.rs`
  and `src/app/product/render.rs` (`render_workbench`'s `card` closure picks
  the border; docks and center are rendered there; maximized pane path too),
  `extensions/@knot/themes/` (both Islands themes), `src/app/workbench.rs`
  (focused pane / area lookup). Mirror how `TAB_ACTIVE_*` keys are resolved.
- Validation: model test for key resolution and defaults; a window test that
  the border color follows focus between center and a dock; `just check`.
- Built: `card.focusedBorder` (default `text.accent`, set by every bundled
  theme), `chrome.cardBorder` defaulting to 1, `focus_ring` in
  `render.rs`. Dock and pane geometry tests now account for the border.
- **Review gate:** `Try: Card focus …` commands in personal `post-init.js`;
  the user judges the look in both themes.

### 2. Picker tabs (native and API)

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
