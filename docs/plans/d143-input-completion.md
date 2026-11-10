# D143 — Input completion

Status: 🚧 approved; checkpoint 1 awaits its review gate. Item of the
[daily-driver plan](daily-driver.md); depends on D071 (shared picker).

## Goal

`workbench.showInput` can offer completions while the user types. A list
under the field follows the text, Up/Down move through it, and Tab accepts
the highlighted row into the field. The first consumer is Vim's Ex line,
which completes command names and `:set` options.

## Design

- **API: `onChange` returns completions too.** `onChange` may return a
  string (the message, as today), null or nothing, or
  `{ message?: string, items?: readonly InputCompletion[] }`. An item is
  `{ label, value, detail? }`, where `value` is the **whole new field text**.
  Accepting an item replaces the field with `value` and puts the caret at the
  end. Native code knows nothing about tokens, replacement ranges, or the
  caret, and the extension does all filtering and ordering. Native code only
  displays the list in the order it receives. Every caller of the API is in
  this repository, so the shape can change freely.
  - Rejected: replacement ranges plus a caret offset. They add protocol only
    to complete in the middle of a line, which a command line does not need.
  - Rejected: native fuzzy filtering. The extension owns the token rules,
    such as prefix matching and Vim's `no`/`inv` option prefixes.
- **Interaction: menu accept (fish/zsh style).**
  - A non-empty list shows under the field, in the input panel, with its
    first row highlighted. An empty or missing list hides it.
  - Up/Down and Ctrl-N/Ctrl-P move the highlight and wrap around.
  - Tab accepts the highlighted row. The field changes, so `onChange` runs
    again and its next answer replaces the list. Tab with no list does
    nothing.
  - Enter always submits the field text as shown, whatever row is
    highlighted. Escape dismisses the input, as it does today.
  - Rejected: Vim/Helix live cycling, where Tab inserts each candidate as
    you cycle and the list stays frozen. Native code would have to track
    cycling state and suppress `onChange` while cycling, and drilling into a
    folder would need an extra step.
- **Stale feedback is dropped.** The session remembers the value it last
  delivered to the caller. Feedback, both message and items, applies only
  while the field still holds that value. Otherwise native code drops it and
  the next pull delivers the current value. Tab can therefore never accept a
  completion meant for older text.
- **Transport.** The `onChange` pull loop stays as it is.
  `nextInputEvent(session, message)` becomes
  `nextInputEvent(session, message, items)`: `bootstrap.js` validates the
  returned value, and the handler decodes the items into a `knot-protocol`
  type. A return value of the wrong shape stops further calls and is
  rethrown, as today.
- **Vim completion** is a pure function in `ex.ts`,
  `completeEx(line): InputCompletion[]`, with no host calls:
  - The command name, while the line has no space yet: prefix match on the
    full names (`write`, `quit`, `wq`, `xit`, `exit`, `edit`, `set`). Line
    numbers and `$` get no items.
  - `:set` arguments: the word under completion is matched against the full
    option names and short forms, including after a `no` or `inv` prefix,
    which is kept. Only option names are completed, not values. Each item's
    `value` is the line with its last word replaced.

## Exclusions and deferred work

- **`:e` path completion** belongs to D081 (workspace ignore rules), so
  listings skip ignored files from the start; `completeEx`'s shape already
  supports it.
- Completing option values, completion in the middle of the line, and
  mouse selection of rows.
- Picker changes. The picker keeps its own native fuzzy filtering.

## Checkpoints

### 1. Completions in the input API ✅

Deliverables:
- `InputView` holds the items and the highlight, renders the list under the
  field, and handles Up/Down, Ctrl-N/Ctrl-P, and Tab as described above.
- The session drops stale feedback.
- `onChange` may return the object form, typed in `knot.d.ts`.
- The fake test host supports the object form.
- Docs: the `showInput` section of the extension guide, and the input flow
  in the semantic UI reference.

Validation:
- Rust product tests in `src/app/product/tests/transient.rs`:
  - the list shows, and an empty answer hides it
  - the highlight moves and wraps
  - Tab replaces the field and the caret goes to the end, and the next
    answer replaces the list
  - Enter submits the typed text with a row highlighted
  - Tab with no list does nothing
  - stale items are dropped
  - a malformed return rejects the call
- The contract test and a fake-host test of the object form.

Implementation context:
- **Native view:** `src/app/transient.rs`. Add an items field and a selected
  index to `InputView`, plus `set_feedback(message, items)` in place of
  `set_message`. Handle the new keys in `key_down` after the field has
  declined them. Render the rows the way `src/app/picker.rs` does: the
  `LIST_SELECTED_BACKGROUND` theme key, and muted `detail` text. Cap the
  visible rows, around 8, and scroll the highlight into view. Accept with a
  non-test `TextInput::set_text` in `src/app/text_input.rs` (the existing one
  is `#[cfg(test)]`), which already emits `Changed` and moves the caret to
  the end.
- **Keys:** check that the transient key context's keymap leaves Up, Down,
  Tab, Ctrl-N, and Ctrl-P unclaimed (`TRANSIENT_KEY_CONTEXT` in
  `src/app/mod.rs`, and the keymap registry).
- **Handler:** `src/app/extensions/host/handlers.rs`.
  - `InputSessionArguments` decodes an optional items array at argument 4.
  - `TransientSession` records the last delivered value, set in
    `input_update` / `next_change`.
  - `next_input_event` applies the feedback only when
    `input.read(cx).input().read(cx).text()` still equals that value.
  - Add `InputCompletion` next to `InputUpdate` in
    `crates/knot-protocol/src/lib.rs`, if the decoding needs a serde type.
- **JavaScript:** `showInput` in `crates/knot-host/src/js/bootstrap.js`
  splits the return value into message and items, and validates items the
  way `pickerItem` does. Add the types to `InputOptions.onChange` in
  `crates/knot-host/src/js/knot.d.ts`, and update
  `docs/schemas/knot.schema.json` if it covers transient options.
- **Fake host:** the `showInput` fake in `scripts/extensions/testing/host.ts`
  records returned items next to `inputMessages()`, for example through an
  `inputFeedback()` accessor. Add a test in
  `scripts/extensions/testing/transient.test.ts`.
- **Docs:** `docs/extension-api.md` (the `onChange` example near line 744),
  and `docs/architecture/semantic-ui.md` (the input paragraph near line 417):
  feedback carries items, stale feedback is dropped, and Tab accepts.

Review gate: a `post-init.js` `Try: Input completion` command that completes
from a fixed word list shows the list, highlight, and Tab behavior in the
real app.

### 2. Vim Ex line completion

Deliverables:
- `completeEx` in `extensions/@knot/vim/src/ex.ts`.
- `commandLine` in `extensions/@knot/vim/src/command-line.ts` passes
  `onChange: (line) => ({ items: completeEx(line) })`.

Validation:
- Unit tests in `extensions/@knot/vim/test/ex.test.ts`:
  - name prefixes, a unique prefix, and no match
  - line numbers get no items
  - `:set` words, short forms, and `no`/`inv` prefixes
  - an earlier argument is preserved in `value`
- End-to-end cases in `extensions/@knot/vim/test/cases/ex.jsonc`, run by
  `crates/knot-ext-vim/tests/vim.rs`:
  - `:se<tab> noet<cr>>>` indents with a tab
  - `:set noexp<tab><cr>>>` indents with a tab
  - `:s<down><tab>…` picks the second match
  - Enter with a highlighted row runs the typed text

Implementation context:
- Reuse the `NAMES`, `BOOLEANS`, and `NUMBERS` tables in `ex.ts`. List each
  option once, under its full name, with its short form as `detail`.
- Typing keys through the real editor depends on the asynchronous
  `onChange` answer arriving before `<tab>`. Check how `vim.rs` waits
  between keys, and make the driver settle pending extension work before
  the next key if it does not already.

Review gate: in the real app, `:se<Tab>`, `:set nu<Down><Tab>`, and Enter
behave as designed. Then finish the item:
- Fold the durable behavior into the architecture references.
- Add a decision entry for the menu-accept model and the whole-value items
  in `docs/decisions/`, with its index line.
- Mark D143 ✅ and remove this plan.
