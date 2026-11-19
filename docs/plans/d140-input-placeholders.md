# D140: Placeholder text in input fields

Status: 🚧 checkpoint 1 done, checkpoint 2 not started. Roadmap item [D140](../roadmap.md).

## Goal

Extension inputs and pickers can explain themselves inside their empty field.
`workbench.showInput` takes a `placeholder`, and Tab on an empty field accepts
it, as Zed's Go to Line does. Go to Line shows the current position as
`line:column` there. Search Everywhere shows a different hint on each tab.

## Design

- **Input placeholder.** `InputOptions.placeholder?: string` shows as muted
  text in the empty field, through `TextInput::set_placeholder`, as editor bars
  already do. On an empty field, Tab copies the placeholder into the field as
  an ordinary edit, so `onChange` runs and previews follow.
- **Tab precedence.** When a completion row is highlighted (an empty field with
  `history` lists all of it), Tab still accepts that row. The placeholder is
  accepted only when no list is showing, since the highlight is what tells the
  user what Tab does.
- **Picker placeholder per tab.** The picker already shows its `title` only as
  the query field's placeholder. `PickerTab.placeholder?: string` replaces the
  title in the field while that tab is active, and tabs without one fall back
  to the title. The picker has no top-level placeholder option and no
  Tab-to-accept, because Tab switches tabs there.
- **Go to Line.** Its placeholder is the caret position, such as `120:5`, and
  the usage message shrinks to `120 of 300 lines`, since the placeholder now
  carries the position.
- **Search Everywhere.** Each tab gets a placeholder: "Search everything",
  "Search open buffers", "Search files", "Search actions".

## Exclusions

- A separate placeholder for the native fallback command palette.
- Tab-to-accept in pickers.

## Checkpoints

### 1. Input placeholder and Go to Line ✅

Done and approved. `InputOptions.placeholder` and Tab-to-accept are documented
in the [extension guide](../extension-api.md); Go to Line shows the caret
position as its placeholder. Extension tests cannot send Tab, so native Tab is
covered by the `InputView` test only.

### 2. Picker tab placeholders and Search Everywhere

Deliverables:

- `placeholder` on `PickerTab`, carried to the native picker, which swaps the
  field's placeholder when the active tab changes.
- Search Everywhere sets a placeholder on each tab.
- `knot.d.ts` documents the option.

Validation:

- Picker tests: the active tab's placeholder shows, switching tabs swaps it,
  and a tab without one falls back to the title.
- `go-to-file` extension test: opening on Files and on All shows the matching
  placeholder, if the test harness can observe it; otherwise the picker tests
  cover it.
- `just check`.

Review gate: run a `Try:` command that opens Search Everywhere on each tab.

**Implementation context:**

- `crates/knot-protocol/src/lib.rs:465` (`PickerTab`), `bootstrap.js`
  `pickerTab` (line 931), and `knot.d.ts` `PickerTab` (around line 2215).
- `src/app/product.rs` around line 1815 maps protocol tabs to the picker's
  `PickerTab` (`src/app/picker.rs:92`).
- `src/app/picker.rs`: `build` sets the title as placeholder (line 192); move
  that into a helper called from `set_tabs` and on every tab change.
- `extensions/@knot/go-to-file/src/searchEverywhere.ts` (`TABS`) and its
  `test/extension.test.ts`.

## Deferred decisions

None.
