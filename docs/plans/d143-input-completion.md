# D143 — Input completion

Status: 🚧 approved; checkpoints 1 to 3 are implemented and await their
review gates, and checkpoint 4 is not started. Item of the
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
  caret, and the extension chooses the candidates and their order, usually through
  `completion.match`. Native code only displays the list in the order it
  receives. Every caller of the API is in
  this repository, so the shape can change freely.
  - Rejected: replacement ranges plus a caret offset. They add protocol only
    to complete in the middle of a line, which a command line does not need.
  - Rejected: native filtering of the field text. The extension owns the token
    rules, such as Vim's `no`/`inv` option prefixes, and passes only the
    word under completion to the matcher.
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
- **One native matcher, with user-selectable styles.** Matching is a Rust
  module in `knot-model` that the picker, input completion, and any extension
  share, so one setting changes how everything matches.
  - A style is built in: `prefix`, `substring`, `flex` (fuzzy, as the picker
    matches today), or `orderless` (space-separated components in any order,
    each a substring). Smart case. The matcher takes a query and candidate
    strings and returns the matches ordered, with the matched ranges.
  - The setting `completion.styles` is an ordered list of styles, default
    `["prefix", "flex"]`. A candidate matches if any style matches it; results are
    ordered by the first style that matches, then by that style's own score,
    then by the caller's order, so a picker's recent items lead among equal
    matches. Prefix matches list first and fuzzy ones after, as ido does.
  - It is native because the picker filters on every keystroke, and native
    painting and raw input never call JavaScript synchronously. Extensions
    reach the same matcher through a host call, `completion.match`.
  - The picker keeps its tiers (label, then description, then match text) and
    calls the matcher per field.
  - Input completion items carry optional `matches` ranges over their label,
    which the list highlights as the picker does.
  - Rejected: styles registered as JavaScript functions. They cannot drive
    the picker, and the built-in set with an ordered list covers what ido and
    Emacs users customize. A consumer with unusual needs matches in its own
    `onChange`.
  - Rejected: a per-category override of the styles, until a consumer needs
    one. A call may still pass explicit styles.
- **Vim completion** is a pure function in `ex.ts`,
  `completeEx(line): InputCompletion[]`, with no host calls:
  - The command name, while the line has no space yet: `completion.match` on
    the full names (`write`, `quit`, `wq`, `xit`, `exit`, `edit`, `set`). Line
    numbers and `$` get no items.
  - `:set` arguments: the word under completion is matched, through
    `completion.match`, against the full option names and short forms,
    including after a `no` or `inv` prefix, which is kept. Only option names are completed, not values. Each item's
    `value` is the line with its last word replaced.

## Exclusions and deferred work

- **`:e` path completion** belongs to D081 (workspace ignore rules), so
  listings skip ignored files from the start; `completeEx`'s shape already
  supports it.
- Completing option values, completion in the middle of the line, and
  mouse selection of rows.
- Per-category style overrides and user-registered JavaScript styles.

## Checkpoints

### 1. Completions in the input API ✅

Implemented and committed; the review gate is open. The `onChange` return
shape, `InputView` list and keys, stale-feedback dropping, fake-host
`inputFeedback()`, tests, and docs landed. Review gate: run
`Try: Input completion` from `post-init.js`; the list, highlight, Tab, and
Enter behave as designed.

### 2. Shared matcher and `completion.styles` ✅

Deliverables:
- The matcher module in `crates/knot-model/src/` (`prefix`, `substring`,
  `flex`, `orderless`; merge and order as in the design), with plain unit
  tests.
- The `completion.styles` setting, documented in `docs/settings.md`; unknown
  style names are ignored, and a list with none selects the default.
- The picker filters through the matcher instead of calling nucleo
  directly; its fuzzy matching is the `flex` style.
- The `completion.match` host call, typed in `knot.d.ts`, with the contract
  test; the fake host implements the styles in TypeScript.
- `InputCompletion.matches` through the protocol, `bootstrap.js`, the typed
  surface, and the rendered list. Ranges follow the existing range
  conventions for text crossing the host boundary.
- Docs: the extension guide (`completion.match`, `matches`) and the
  matching paragraph of the semantic UI reference.

Validation:
- Matcher unit tests: each style, smart case, merge order with several
  styles, empty query keeps the caller's order, ranges on multibyte text.
- Picker tests: current picker tests pass unchanged under the default; a
  `["prefix"]` and a `["substring"]` case.
- A product test that `completion.match` follows a changed setting.
- A fake-host test of `completion.match` and of `matches`.

Review gate: a `post-init.js` with `Try: Match styles` shows a picker and an
input completion over the same word list; change `completion.styles` in the
settings file and check that both follow it, with matched characters
highlighted.

### 3. Extension history ✅

`history.recent`, `history.record`, and `history.order` keep a named list of
recently chosen IDs per package in its storage, so extensions order picker
items and completion candidates by recency without a native picker option. The
picker and the matcher already keep the caller's order among equal matches,
which is all recency needs: it never outranks a better match, as in IntelliJ.
The helper lives in `bootstrap.js` over `storage`; the fake host implements the
same three calls. The review gate is `Try: Match picker` and
`Try: Match input`, which now remember picks.

### 4. Vim Ex line completion

Deliverables:
- `completeEx` in `extensions/@knot/vim/src/ex.ts`, matching through
  `completion.match`.
- `commandLine` in `extensions/@knot/vim/src/command-line.ts` passes
  `onChange: (line) => ({ items: completeEx(line) })`.

Validation:
- Unit tests in `extensions/@knot/vim/test/ex.test.ts`:
  - name matches, a unique match, and no match
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
