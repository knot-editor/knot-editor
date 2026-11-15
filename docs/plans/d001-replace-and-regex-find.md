# D001 — Replace, regular expressions, and find in selection

Source: [roadmap D001](../roadmap.md). Current behavior:
[editor bars](../architecture/editor-and-languages.md#editor-bars),
[text search](../architecture/core-and-buffers.md#text-search),
[editor bars decision](../decisions/semantic-ui.md#editor-bars).

## Outcome

The `@knot/find` bar gains, with VS Code and IntelliJ behavior and bindings:

- A **Regex** toggle (`cmd-alt-r` / `ctrl-alt-r`): Rust `regex` syntax, matched
  natively. An invalid pattern shows the parser's message in the bar.
- A **Replace** bar stacked under the find bar, opened by `editor.replace`
  (`cmd-alt-f` on macOS, `ctrl-h` elsewhere). Enter in its field replaces the
  current match and advances (or only advances when the selection is not a
  match); `cmd-enter` / `ctrl-enter` and a button replace all in one undo step
  and report how many.
- A **Find in Selection** toggle (`cmd-alt-l` / `ctrl-alt-l`) that limits
  highlights, stepping, and Replace All to a scope that follows edits and is
  painted beneath the matches.

## Design

### Native matching (`knot-model`)

- `FindOptions` gains `regex`. Regex mode uses the `regex` crate (already in
  the lockfile) in multi-line mode: `^`/`$` match at line boundaries and `\n`
  matches across lines. Case-insensitive regex uses the crate's own folding;
  literal mode keeps the current simple-fold matcher. Whole word applies the
  existing word-class edge check to each regex match.
- With a `range`, a regex match must lie inside it, but anchors and `\b` are
  judged against the text around it (search the range as a span of the
  surrounding text), as whole word already is.
- Empty matches are kept, as in VS Code, so `^` → `// ` prefixes every line.
  After an empty match the search advances one character. Their decorations
  paint nothing; stepping to one places the caret.
- An invalid pattern is a new error, `InvalidQuery`, carrying the parser
  message.

### Native replacement primitive

`buffer.replace(query, replacement, options)` takes `FindOptions` plus an
optional `ifRevision` and returns `{ revision, count, more }`. It matches,
expands, and commits the replacements as one batch (one undo step, one change
notification) in a single foreground step, so it cannot race other edits and
never ships match text across the protocol.

- Regex mode expands `$1`, `${name}`, and `$$` by the crate's rules, and `\n`,
  `\t`, `\\` in the replacement. Literal mode inserts the replacement as typed.
- `limit` caps the replacements (default all, up to the 100,000 cap); `more`
  says whether matches remained.
- A single Replace is the same call with `range` set to the current match and
  `limit: 1`. Vim's `:s` (D150) is the same call over a line range.
- `count: 0` commits nothing and keeps the revision.

### Find in selection (`@knot/find`)

- Per editor session, not shared like the query and options. Turning it on
  records the selection as the scope, or the caret's line when the selection
  is empty. Opening Find with a multi-line selection turns it on.
- The package rebases the scope through each `BufferChange`'s edits (a pure
  function in `search.ts`). An edit that deletes the whole scope turns it off.
- The scope is passed as `range` to `find` and `replace`; stepping wraps inside
  it. It is drawn as a background decoration in a new native color key
  `editor.findScope`, ordered beneath the match decorations.
- Turning the toggle off or closing the bar drops it.

### UI (`@knot/find`)

- The query, replacement, and options (case, whole word, regex) stay shared by
  every editor, as today.
- The Replace bar is a second editor bar, `replace`, opened after `find` so it
  stacks beneath; Tab already moves between fields across bars. It has a field
  and Replace / Replace All buttons. `editor.replace` opens both; `editor.find`
  leaves an open Replace bar alone; Escape in either bar closes both.
- Bindings live in the bar contexts `find` and `replace`, as today's do.

## Exclusions

Moved to the roadmap's lower-priority section: preserve-case replacement and
multi-line query fields. Vim search and substitution remain D150, which will
build on `buffer.find` and `buffer.replace`. Find in selection over several
selections waits for D142.

## Deferred decisions

None material. Bindings and icons follow VS Code and may be adjusted at review.

## Checkpoints

### 1. Regex matching in `buffer.find`

Deliverables: `regex` option and `InvalidQuery` across model, protocol, host
operation, `knot.d.ts`, and the extension guide; model tests for multi-line
anchors, case folding, whole word, range context, empty matches, the limit,
and invalid patterns; an extension-API test of the option and error.

Implementation context:
- `crates/knot-model/src/buffer/find.rs` (matcher; add `regex` to
  `crates/knot-model/Cargo.toml`), tests in `crates/knot-model/src/buffer/tests.rs`
  or a `find` test module beside it.
- `src/app/extensions/host/handlers.rs` — `FindArguments` reader and the
  `find` operation (map the new model error); `BufferError` and `FindResult`
  in `crates/knot-protocol/src/lib.rs`.
- `crates/knot-host/src/js/bootstrap.js` (`Buffer.find` binding),
  `crates/knot-host/src/js/knot.d.ts` (`FindOptions`, error codes),
  `docs/schemas/knot.schema.json` if error codes are listed there,
  `docs/extension-api.md` near the editor-bar example.
- Update [text search](../architecture/core-and-buffers.md#text-search).

Review gate: API shape and regex semantics.

### 2. `buffer.replace`

Deliverables: model function returning the edit batch and count, host
operation committing it through the buffer's normal edit path, `knot.d.ts`
and guide entry; model tests for expansion (`$1`, `${name}`, `$$`, escapes),
literal mode, empty matches, range, limit/`more`, and no-op; host-level tests
for revision conflict, single undo step, and one change notification.

Implementation context:
- Expansion and batch construction in `crates/knot-model/src/buffer/find.rs`
  (or a sibling `replace.rs`), returning plain data.
- Handler beside `find` and `apply_edits` in
  `src/app/extensions/host/handlers.rs`; commit through the same buffer path
  `host.buffers.apply_edits` uses so history, read-only, and change
  notification behave identically.
- `bootstrap.js`, `knot.d.ts`, `docs/extension-api.md`.

Review gate: primitive semantics, ready for D150 to reuse.

### 3. Regex toggle and Replace bar

Deliverables: the Regex toggle with its error message in the bar,
`editor.replace`, the Replace bar, Replace and Replace All commands and
bindings, a "Replaced N" message; extension tests through the real editor
for regex search, invalid patterns, single replace advancing, replace all as
one undo step, and capture expansion; a `post-init.js` with `Try:` commands.

Implementation context:
- `extensions/@knot/find/src/{extension.ts,search.ts}`, tests in
  `extensions/@knot/find/test/extension.test.ts`; native-level workbench
  coverage in `crates/knot-ext-workbench/tests/workbench/find.rs` where key
  routing (Enter, `cmd-enter`, Tab across bars) needs it.
- Icons via `icons.define` (Lucide `regex`, `replace`, `replace-all`).
- Update [editor bars](../architecture/editor-and-languages.md#editor-bars).

Review gate: manual review of Replace in the app.

### 4. Find in selection

Deliverables: the toggle, auto-enable on multi-line selections, scope rebasing,
the `editor.findScope` color key with defaults and theme values, scoped
highlights, stepping, and Replace All; unit tests of the rebase function and
extension tests for scoped search, stepping wrap, edits moving the scope,
deletion clearing it, and scoped Replace All; updated `Try:` commands.

Implementation context:
- `extensions/@knot/find/src/` (session gains `scope`; rebase over
  `BufferChange.edits` in `search.ts`).
- Color key in `crates/knot-model/src/theme.rs` beside `EDITOR_FIND_MATCH`,
  `knot.d.ts` color keys, and `extensions/@knot/themes/src/themes/*.ts`.
- Lucide icon `text-select`.

Review gate: manual review in the app, then record the decision in
[semantic UI decisions](../decisions/semantic-ui.md#editor-bars) and its
index, update the architecture references, and remove D001 and this plan.
