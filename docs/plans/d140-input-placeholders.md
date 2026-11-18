# D140: Placeholder text in input fields

Status: 🚧 approved, not started. Roadmap item [D140](../roadmap.md).

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

### 1. Input placeholder and Go to Line

Deliverables:

- `placeholder` on `InputOptions`, validated in the host and shown in the empty
  field.
- Tab on an empty field accepts the placeholder unless a completion row is
  highlighted.
- `@knot/go-to-line` shows `line:column` as its placeholder and the shorter
  message.
- `knot.d.ts` and the [extension guide](../extension-api.md), if it describes
  input options, document the option and the Tab behavior.

Validation:

- `InputView` tests: the placeholder shows only while the field is empty; Tab
  on an empty field fills it and publishes the change; Tab with a highlighted
  history row accepts the row; Tab on a non-empty field without a list does
  nothing new.
- `go-to-line` extension test: Tab accepts the current position and Enter keeps
  the caret there; the message reads `N of M lines`.
- `just check`.

Review gate: run `Try:` commands for Go to Line and an input with both history
and a placeholder.

**Implementation context:**

- `crates/knot-host/src/js/knot.d.ts` (`InputOptions`, around line 2308) and
  `crates/knot-host/src/js/bootstrap.js` (input option parsing near
  `optionalText(options.prompt)`, around line 1041).
- `src/app/extensions/host/handlers.rs`: `InputArguments` (line 1413) and both
  `InputSpec` constructions in `show_input` and `begin_input` (around lines
  1825 and 1870); `src/app/product/tool_commands.rs:232` builds another
  `InputSpec`.
- `src/app/transient.rs`: add `placeholder` to `InputSpec` (and its test
  builder), set it on the `TextInput` in `InputView::new`, and extend the
  `"tab"` arm of the key handler around `accept_completion`.
- `extensions/@knot/go-to-line/src/extension.ts` and its
  `test/extension.test.ts`.

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
