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

- `FindOptions` gains `regex`. Regex mode uses `regex-automata`'s meta regex
  (the engine under the `regex` crate, already in the lockfile, and the one
  that can search a span of a larger haystack) in multi-line mode: `^`/`$` match at line boundaries and `\n`
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

### 1. Regex matching in `buffer.find` ✅

Complete and approved. Behavior: [text search](../architecture/core-and-buffers.md#text-search);
API: `knot.d.ts` and the [extension guide](../extension-api.md#show-an-editor-bar).
Matching uses `regex-automata`'s meta regex for span-aware anchors. A compiled
pattern cache was measured unnecessary (compile and search of a small buffer
took 0.03–0.8 ms). The fake testing host approximates regex with JavaScript
`RegExp`.

### 2. `buffer.replace` ✅

Complete and approved. Behavior: [text search](../architecture/core-and-buffers.md#text-search);
API: `knot.d.ts` and the [extension guide](../extension-api.md#show-an-editor-bar).
`ifRevision` is optional and defaults to the current revision. The fake testing
host expands replacements with JavaScript `RegExp`, an approximation.

### 3. Regex toggle and Replace bar ✅

Complete and approved. Behavior: [editor bars](../architecture/editor-and-languages.md#editor-bars);
keys: `editor.replace` presets in `src/app/commands/keymaps/presets.rs` and the
package's bar bindings. Replacing an empty match with an empty replacement does
not advance (left unhandled).

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
