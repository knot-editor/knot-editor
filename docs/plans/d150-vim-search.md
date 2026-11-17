# D150: Vim search and substitution

Status: 🚧 in progress (checkpoints 1–3 done, 4 awaiting review). Roadmap item [D150](../roadmap.md).

## Goal

Vim mode searches and substitutes the way Vim users expect:

- `/` and `?` prompt for a pattern, highlight matches and preview the first
  one while typing, and jump to it on Enter; `n`, `N`, `*`, `#`, `g*` and `g#`
  step through matches. All of them are jumps, work as operator targets
  (`d/foo<cr>`, `cn`) and extend a visual selection.
- Every match stays highlighted (`hlsearch`) until `:noh`, through edits, in
  every Vim editor showing the buffer.
- `:s` and `:%s` substitute over line ranges with the flags daily editing
  uses, each substitution one undo step.
- Patterns are written in Vim's syntax by default; a setting switches to the
  Rust syntax that Find's regex mode uses.
- Earlier entries are a keystroke away: the `/`, `?` and `:` prompts list
  past entries, and Up and Down in the Find bar's fields recall past queries
  and replacements.

## Design

### Matching

Search lives in `@knot/vim` on public API, as the
[Vim mode decision](../decisions/bundled-packages.md#bundled-vim-mode)
requires, and matches with the buffer's native `find` and `replace`
([text search](../architecture/core-and-buffers.md#text-search)), always in
regex mode. Vim keeps its own last pattern, direction and replacement,
independent of the Find bar's query.

- **Pattern syntax.** `vim.regex` is `"vim"` (default) or `"rust"`. In Vim
  syntax a pure translator (`pattern.ts`) turns Vim's magic syntax into a Rust
  pattern: `\(` `\)` `\|` `\+` `\=` `\?` `\{n,m}` `\{-}` `\<` `\>` (to
  `\b{start}` and `\b{end}`), `\_s`-style classes, `\v` and `\V`, `\c` and
  `\C`, and literal `(`, `)`, `|`, `+`, `?`, `{`. Atoms Rust's engine cannot
  express, such as `\zs`, `\ze` and lookaround, fail with a Vim-style error.
  In Rust syntax the pattern goes to the engine as typed.
- **Case.** `vim.ignoreCase` and `vim.smartCase`, both on by default, and
  `:set ic`/`scs` with their `no`/`inv`/`?` forms. Smartcase makes a pattern
  with an uppercase letter case-sensitive. `\c`/`\C` override both; `*` and
  `#` ignore smartcase, as in Vim.
- **Replacement.** In Vim syntax `\0`–`\9`, `&`, `\&`, `\r` and `\n`, and a
  literal `$` are translated to the native expansion (`${1}`, `${0}`, line
  break, `$$`); `\u` `\U` `\l` `\L` `\e` `\E` pass through to the native case
  escapes added by checkpoint 1. In Rust syntax the replacement is native
  syntax as typed.

### Native primitives

Two generic additions to `BufferModel::find`/`replace` and the extension API,
useful beyond Vim:

- `perLine` on `find` and `replace`: at most one match starting on each line;
  after a multi-line match the next candidate line is the one the match ends
  on, past its end. `:s` without `g` stays one native, revision-checked
  replace.
- Case escapes in a pattern's replacement: `\u` and `\l` change the next
  character, `\U` and `\L` everything up to `\E` or `\e`, applied to literal
  text and expanded groups alike. Find's regex Replace gains them too, as VS
  Code's has.

### Search state and highlights

- The last pattern and direction are shared by every Vim editor, and the
  pattern is the `/` register, persisted with the other registers. `n` and `N`
  read it, `*` and `#` set it.
- Search wraps, showing "search hit BOTTOM, continuing at TOP" (and the
  reverse) in the mode text; a miss shows `E486: Pattern not found`.
- Highlights are each Vim editor's own decorations in `editor.findMatch`,
  capped at 1,000 matches like Find's, refreshed on buffer changes the way
  Find refreshes them. They show in every followed editor while the search is
  active, and `:noh` hides them until the next search or `n`.
- Incremental search runs in the `/` prompt's `onChange`: highlights and a
  previewed caret on the first match from the starting point; Escape restores
  the view state from before the prompt, an invalid pattern shows its message
  under the field.

The prompt is `workbench.showInput` with prompt `/` or `?`, as the Ex line is.

### Substitution

- Ex ranges: `N`, `.`, `$`, `%`, `'x` marks including `'<` and `'>`, and
  `+N`/`-N` offsets, with `,` between two addresses. `:` in a visual mode
  opens the Ex line with `'<,'>` and leaves the visual mode, as Vim does.
- `:s/pat/rep/flags` with any delimiter that is not alphanumeric, `\`, `"` or
  `|`; empty pattern means the last search pattern, and the substituted
  pattern becomes the last search pattern. Flags: `g`, `i`, `I`, `n` (count
  only) and `e` (no error on a miss). A count after the flags (`:s/a/b/ 3`)
  is not supported.
- One `buffer.replace` over the range's bytes with `perLine` unless `g`. The
  caret goes to the start of the last substituted line, and `:s` reports
  `N substitutions on M lines` above Vim's reporting threshold.

### Input history

History is a generic input feature, adopted by Find and Vim with separate
lists. Vim already keeps `/` and `:` histories apart, and a Vim pattern
recalled into Find's literal search would be noise. Each package owns its
lists: it adds a submitted entry, persists the lists in its storage, and caps
them (most recent first, a repeated entry moving to the front).

- **Prompts** (`workbench.showInput`): a `history` option. Its entries
  matching the typed text, through the shared matcher and kept newest first,
  head the completion list, before what `onChange` returns; on an empty field
  the list is the history alone. Up and Down move the highlight and Tab puts
  the entry in the field as they do today, so there are no new keys; Enter
  still submits the typed text. `/` and `?` share one list, since the pattern
  is the same; `:` has its own, ahead of command, option and file
  completions.
- **Bar fields**: a `history` option per field. Bars have no completion list,
  so Up and Down replace the field's text with the previous and next entries
  whose start matches the text typed before recalling began, as Vim's
  command line does, and Down past the newest restores that text.

## Exclusions

- The `c` confirm flag, `&`, `g&`, `:&&`, and `~` in patterns and
  replacements.
- Search offsets (`/pat/e`, `/pat/;/other`).
- `\zs`, `\ze`, lookaround and other atoms Rust's engine lacks.
- `:g`, `:v` and other range commands stay with the Ex commands item.
- Highlights past the first 1,000 matches, which Find shares.

## Release notes

Drafts for the `Release-note:` trailers (see
[development](../development.md#nightly-releases)); revise them to what
shipped. Checkpoint 1 and 2 add no user-visible behavior beyond what these
cover; checkpoint 3 carried the Find history note.

- Release-note: Vim mode can search with `/`, `?`, `n`, `N`, `*` and `#`, with live match highlighting that stays until `:noh`.
- Release-note: Vim mode supports `:s` and `:%s` substitution with ranges and flags, as a single undo step.
- Release-note: The Vim `/`, `?` and `:` prompts list earlier entries.


## Deferred decisions

- Whether highlights follow the viewport instead of the buffer's first
  1,000 matches; revisit for both packages if daily use hits the cap.

## Checkpoints

### 1. Native per-line matching and replacement case escapes ✅

Done and approved: `perLine` on find and replace, and `\u \U \l \L \e \E` in
regex replacements, are documented under
[text search](../architecture/core-and-buffers.md#text-search) and in the
[extension guide](../extension-api.md).

### 2. Input history infrastructure ✅

Done and approved: the `history` option on `showInput` and on bar fields is
documented in the [extension guide](../extension-api.md) and, for bar keys,
under [editor bars](../architecture/commands.md#editor-bars). A prompt's
`highlighted` counts only `onChange` items and is null on a history row; a bar
field's history is the snapshot passed when the bar was opened.

### 3. History in Find ✅

Done and approved: `@knot/find` keeps query and replacement histories in its
storage (100 entries each, newest first, added on step, Replace and Replace
All) and passes them as the bar fields' `history`. A field that opens with an
entry, as Find does with its last query, recalls from that entry instead of
filtering by it
([extension guide](../extension-api.md)).

### 4. Search motions, with prompt history ✅ (awaiting review)

Implemented: `pattern.ts` (translation, smartcase, replacement translation, `wordPattern`),
`search.ts` (settings, prompt, motions), the `/` register and the `/` and `:`
histories persisted with the registers, and `test/cases/search.jsonc`. Not
implemented by design: highlights (checkpoint 5). A key typed in the same
instant as a prompt's Enter is dropped when the command then enters insert
mode (`c/foo<cr>x`), as for any command that awaits a prompt.

Deliverables:

- `pattern.ts`: Vim-to-Rust translation of patterns and replacements and the
  smartcase decision, with unit tests over each atom and the unsupported ones.
- `vim.regex`, `vim.ignoreCase` and `vim.smartCase`, and `:set ic`/`scs`.
- `/`, `?`, `n`, `N`, `*`, `#`, `g*`, `g#` with counts, as jumps, operator
  targets (exclusive charwise) and visual extensions; the `/` register; wrap
  and miss messages. No highlights yet.
- The search history for `/` and `?`, filled by submitted prompts and by
  `*`/`#`, and the Ex history for `:`, both persisted with the registers.
- Cases in a new `test/cases/search.jsonc`, through the real editor,
  including recalling a search and an Ex line through Tab from the list.

Implementation context: commands and motions in
`extensions/@knot/vim/src/keys.ts` (parser), `normal.ts` (`run`, jumps via
`JumpList`), `visual.ts`; the prompt mirrors `commandLine` in
`command-line.ts`, passing the `history` option from checkpoint 2; settings
declared by module augmentation as `vim.enabled` is in `extension.ts`; `:set`
parsing in `ex.ts`; registers in `registers.ts`. Mirror
`matchFrom`/`matchBefore` from `extensions/@knot/find/src/search.ts`. Case
topics run from `crates/knot-ext-vim/tests/vim.rs`.

Review gate: try search in the real app through `Try:` commands; `just check`
clean.

### 5. Highlights and incremental search

Deliverables:

- Highlights in every followed editor after a search, kept current through
  edits, hidden by `:noh` and shown again by the next search, `n` or `N`.
- Incremental highlights and caret preview in the prompt; Escape restores the
  view; invalid patterns show their message.
- Product tests in `crates/knot-ext-vim/tests/vim.rs` reading the editor's
  decorations, since the case format checks only text and caret.

Implementation context: refresh loop and decoration calls mirror
`refresh`/`clearHighlights` in `extensions/@knot/find/src/extension.ts`; the
followed-editor map and per-editor `State` in `extension.ts`/`normal.ts`;
`viewState`/`restoreViewState` on the editor handle for Escape.

Review gate: try in the real app; `just check` clean.

### 6. Substitution

Deliverables:

- Ex range parsing and `:s` parsing in `ex.ts`, with unit tests; `:` from
  visual mode with `'<,'>`.
- `:s` running as one native replace with flags, last-pattern reuse, the
  caret placement and the report.
- Cases in `test/cases/ex.jsonc` (or a new `substitute.jsonc`) covering
  ranges, flags, `\1`/`&`/`\r`/`\U` replacements, undo as one step, and
  Rust syntax.

Implementation context: `parseEx`/`ExCommand` and `completeEx` in
`extensions/@knot/vim/src/ex.ts`; `runEx` in `command-line.ts`; marks in
`Shared.marks` and the visual marks in `State.previous`; `":"` handling in
`keys.ts`.

Review gate: try in the real app; then record the regex-syntax, substitution
and input-history decisions in
[bundled packages](../decisions/bundled-packages.md#bundled-vim-mode) and the
[decisions index](../decisions.md), remove D150 from the roadmap, and
`just check` clean.
