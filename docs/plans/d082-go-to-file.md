# D082 — Go to File

Status: 🚧 approved, not started. Item of the
[daily-driver plan](daily-driver.md#phase-4--files-and-workspace); promotes
[D118](../roadmap.md) (query-driven picker) in its smallest form.

## Outcome

⌘P (ctrl-P elsewhere) opens a Go to File picker. Typing fuzzy-matches the
paths of every file under the window's roots, through the D173 index, and
Enter opens the highlighted file. Recently active files come first, as in
IntelliJ: with an empty query the picker lists them, most recent first, and
with a query the recent files that match lead the results.

## Design

### Query-driven picker

`workbench.showPicker` gains an `onQuery(query)` option, exclusive with
`items`. It returns the rows for that query, each a `PickerItem` with
optional UTF-16 match ranges for its label and description. With `onQuery`
the native picker does no matching of its own: it shows the rows in the
order given and highlights the supplied ranges. Calls follow `onChange`'s
rules: the first is for the empty query, calls are serial, queries that
arrive while one runs collapse into the latest, and an answer applies only
while the field still holds the query it answers. A throw stops further
calls and is rethrown once the picker settles. `onHighlight` works as it
does with static items.

The transport reuses the pulled picker session (`beginPicker` /
`nextPickerEvent`): a pull reports either a query change or a highlight
change, and the next pull carries the rows answering the last query. The
static-items picker stays the common case and is unchanged.

Rejected: Go to File on `showInput` completion, because Enter submits the
field and rows have no description; sending every path up front, which
D173 ruled out.

### Multi-root search

`files.search` accepts one directory or an ordered list. A list is ranked
natively as one search, with the same tiers (file-name matches before
directory-only matches), the matcher's order, then root order and path
order. Each hit names the searched directory it came from. One call per root
merged in JavaScript was rejected: hits carry no score, so a strong match in
the second root would sink under weak matches in the first.

### The package

A bundled `@knot/go-to-file` package in `knot-ext-workbench` registers
`workspace.go-to-file`, bound to `secondary-p` in the Knot preset.

- A row's label is the file name and its description the directory below
  its root; with more than one root the description starts with the root's
  name. The search's path ranges are split between the two.
- Choosing a row opens it with `documents.open` in the captured pane.
- A window without roots offers only recent files.
- Recent files are the files that became active in an editor, whatever
  opened them, recorded from `activeTargetChanged` application events into
  a `history` list (50 entries). The list is global; the picker shows only
  entries under the window's roots, so it behaves per project.
- Empty query: recent files under the roots, most recent first, without the
  current file, so Enter returns to the previous one.
- Typed query: recent files matching through `completion.match` on their
  displayed path come first, then the `files.search` results without
  duplicates.

D083's Open Recent… reuses this package's recent list rather than keeping a
second one, joining the package (and renaming it if needed) when it lands.

## Exclusions

- `file:line` suffixes and previewing the highlighted file.
- A separate ranking boost for open documents: they are almost always
  recent.
- Pruning recent entries whose files were deleted.
- A "more results" indicator: the search's `more` flag is unused.

## Deferred decisions

- None of consequence; ranking tweaks wait for daily use.

## Checkpoints

### 1. Query-driven picker and a basic Go to File on ⌘P

Deliverables:

- `onQuery` in `showPicker`, declared in `knot.d.ts`, and the
  native picker's supplied-rows mode.
- The `@knot/go-to-file` package and its ⌘P binding. It searches each root
  with one `files.search` call and concatenates them in root order, until
  checkpoint 2 merges them.

Validation:

- Picker unit tests: supplied rows keep their order and ranges, a stale
  answer is dropped, confirming names the supplied row.
- Product transient tests: an `onQuery` picker round trip, collapse of
  superseded queries, rethrow after a throw.
- Contract tests in `scripts/extensions/contract.test.ts`; package tests
  under `extensions/@knot/go-to-file/test`.
- `knot-ext-workbench` integration test: ⌘P in a window over a temporary
  tree, type, Enter opens the file.

Review gate: press ⌘P in the real app on the Knot checkout and open files.

**Implementation context:**

- JS facade: `showPicker` in `crates/knot-host/src/js/bootstrap.js` (mirror
  the `showInput` `onChange` loop beside it); types in
  `crates/knot-host/src/js/knot.d.ts` (`PickerOptions`, `PickerItem`).
- Host operations: `begin_picker` / `next_picker_event` and
  `PickerArguments` in `src/app/extensions/host/handlers.rs`, with
  `TransientSession` carrying query changes as `beginInput` /
  `nextInputEvent` carry values; UTF-16 to byte range conversion as input
  completion already does.
- Native: `src/app/picker.rs` (`Picker::refilter` is skipped in supplied
  mode; add a setter that applies rows only when the query still matches),
  `ProductShell::begin_picker` in `src/app/product.rs`,
  `src/app/transient.rs`; tests in `src/app/product/tests/transient.rs`.
- Package: copy `extensions/@knot/go-to-line` (`knot.jsonc`, `src/main.ts`,
  `src/extension.ts`, `test/`, `tsconfig.json`); add it to
  `crates/knot-ext-workbench/build.rs`, `crates/knot-app/tests/bundled_packages.rs`,
  and an integration test `crates/knot-ext-workbench/tests/workbench/go_to_file.rs`
  (mirror `go_to_line.rs`, registered in `main.rs`).
- Binding: `src/app/commands/keymaps/presets.rs` (command constant beside
  `GO_TO_LINE_COMMAND`, `BUNDLED_PACKAGE_COMMANDS`, a `secondary-p` row) and
  its tests in `src/app/commands/keymaps/tests.rs`.
- Docs: picker section of `docs/extension-api.md`;
  [extension transient interactions](../architecture/semantic-ui.md#extension-transient-interactions)
  and its [decision](../decisions/semantic-ui.md#extension-transient-interactions),
  with the `decisions.md` index line; remove D118 from `docs/roadmap.md` and
  its mentions in the dependency list; the bundled package list in
  `README.md`.

### 2. Multi-root `files.search`

Deliverables:

- `files.search` takes one directory or a list, ranks them as one search,
  and reports each hit's directory.
- Go to File makes one call for all roots and labels rows by the hit's root.

Validation:

- Model tests for cross-root ranking (a name match in the second root beats
  a directory match in the first; ties fall to root order) for both the
  index and the disk walk.
- Product test for the host operation with two roots, one unindexed.
- Package and integration tests with two roots.

Review gate: in a two-root window, results from both roots interleave by
quality and show their root. A `post-init.js` `Try: Add Second Root`
command sets up the window.

**Implementation context:**

- `crates/knot-model/src/files/search.rs`: `Ranker` candidates carry a
  directory index, and `finish` takes the directories; `SearchHit` gains the
  directory.
- `crates/knot-model/src/files/index.rs` (`search`, one `Ranker` offered by
  each root's index in turn) and the disk fallback `search` in
  `crates/knot-model/src/files.rs`.
- `search_files` and its arguments in `src/app/extensions/host/handlers.rs`
  (an index lookup per directory, falling back to the walk per directory);
  protocol types in `crates/knot-protocol/src/lib.rs`; tests in
  `src/app/product/tests/files.rs`.
- `files.search` and `FileSearchHit` in `knot.d.ts` and `bootstrap.js`; the
  [workspace file index](../architecture/documents-and-persistence.md#workspace-file-index)
  reference.

### 3. Recent files first

Deliverables:

- The package records active files and orders rows as described in
  [the package](#the-package).

Validation:

- Package tests for ordering: empty query lists recent files under the roots
  without the current one; a query puts matching recent files first and
  removes duplicates; entries outside the roots are hidden.
- Integration test: activate two files, ⌘P then Enter returns to the
  previous one; recency survives a package restart through `history`.

Review gate: daily use of ⌘P switching between files in the real app.

**Implementation context:**

- `workbench.observe` (`knot.d.ts` `WorkbenchObservation`,
  `activeTargetChanged`), resolving the active view to its document URI from
  the window's presentations; `history.record` / `history.recent`.
- `completion.match` for the recent paths; the `@knot/projects` package for
  an existing observer of application events.
- Update the [D082 item](daily-driver.md#phase-4--files-and-workspace) to ✅
  and reduce this plan per `docs/README.md` once done.
