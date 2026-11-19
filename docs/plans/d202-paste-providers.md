# D202: Language paste providers, and URL-over-selection links in Markdown

Status: 🚧 approved, not started. Roadmap item [D202](../roadmap.md).

## Outcome

In a Markdown document, pasting a URL while text is selected turns the
selection into `[selection](url)` in one undo step, from any paste path:
the paste key, Edit ▸ Paste, the palette, and Vim's `p`/`P` over a visual
selection. Pasting a URL with nothing selected, over a selection that is
itself a URL, or inside a fenced code block inserts it as is. Plain-text
documents are unaffected.

The behavior comes from a generic, language-scoped paste hook: any package
can shape what pasting inserts in a language. Markdown links are its first
user.

## Design

- **Provider named by language.** Native paste looks up the latest
  registration of `editor.paste.<languageId>`, where the language is the one
  at the selection start (`BufferModel::editing_at`'s deepest syntax layer,
  as brackets and comments use). Inside a Markdown paragraph that is
  `markdown_inline`; in a code fence it is the fence's language. With no
  registration, paste stays synchronous and never touches the extension
  host, so languages without a provider pay nothing.
- **Contract.** The provider's `providePaste(request)` receives
  `{ document, revision, text, range: [start, end], selectedText }` (UTF-8
  offsets, as `CompletionRequest`) and returns the text to insert, or `null`
  to insert `text` unchanged. A throw or invalid result falls back to `text`.
  The edit stays the editor's: one undo step, caret after the inserted text.
  The provider never edits.
- **Revalidation.** The editor applies the provider's answer only if the
  buffer revision and the selection are unchanged when it arrives. Otherwise
  it pastes `text` plainly at the current selection. This mirrors how
  completion discards stale responses.
- **Editor API.** `Editor.preparePaste(text, range?)` runs the same lookup
  and provider call for `range` (default: the selection) and returns the text
  to insert, without editing. Vim uses it for every register's text put
  characterwise over a characterwise visual selection, then keeps its own
  replace, register, and caret logic. Linewise and block puts and
  normal-mode `p` keep Vim's own behavior.
- **Markdown package.** `@knot/language-markdown` registers one provider
  under both `editor.paste.markdown` and `editor.paste.markdown_inline`. It
  returns `[selectedText](text)` when the selection is non-empty, the
  selected text is not itself a URL, and the trimmed `text` is a URL. A URL
  is one whitespace-free token matching `scheme://…` or `mailto:…`, checked
  with a small regex. The `url` crate is Rust-side, and it accepts any
  `scheme:` string, so it is a poor fit for recognition. The JS host has no
  `URL` global.

## Exclusions

- Plain-text and other languages: no provider.
- Rich clipboard formats (HTML, images): the provider sees text only.
- Chaining several providers for one language: the latest registration wins,
  as `providers.get` does.
- Reindenting pasted text remains D135.

## Deferred

- None. The checkpoint 1 review may rename the method or request fields.

## Checkpoint 1 — Native paste hook and Markdown links

Deliverables:
- Native paste asks `editor.paste.<languageId>` when one is registered,
  revalidates, and applies the answer as one undo step.
- `providePaste` request and result types in `knot.d.ts`, documented in the
  extension guide next to `editor.completion`.
- `@knot/language-markdown` registers the provider for both Markdown
  languages.

Validation:
- knot-model unit tests: building the request, applying an answer, falling
  back on null, an invalid result, or a stale revision or selection.
- Product test (`src/app/product/tests/providers.rs` or `editing.rs`):
  personal config registers a provider, and paste through the command uses
  its answer. A paste in a language with no provider stays synchronous.
- Markdown package test (end to end through the real editor): write the
  clipboard, select text in a `.md` document, run `editor.paste`, then check
  the link and that one undo restores the selection text. Cover the cases
  that insert as is: no selection, selection already a URL, text that is not
  a URL, inside a code fence, and a `.txt` document.

Review gate: try the commands below. `just check` is clean.

Implementation context:
- Paste entry: `src/app/editor.rs` `execute_editing_command` (the
  `PASTE_COMMAND` branch reads the clipboard) and
  `src/app/editor/native_commands.rs` `editing`. The async provider path
  needs the command to await, so split paste out of the synchronous branch:
  resolve the provider on the foreground, await the provider call, then
  re-enter the editor.
- Model: `crates/knot-model/src/editor/editing.rs` `EditorState::paste`.
  Add the request construction and the revalidated apply there, as plain
  functions with tests.
- Language id at an offset: `BufferModel::editing_at` and
  `SyntaxState::language_at` (`crates/knot-model/src/syntax.rs`).
  `SyntaxLanguage` does not carry its registered id yet, so add it.
- Provider call: mirror `start_completion` and `completion_providers` in
  `src/app/extensions/host.rs` (`self.semantics.providers()`,
  `self.pool.call_provider` with a `ProviderCallRequest`). Here the lookup
  is `get`, by name, not `all`.
- Public contract: `crates/knot-host/src/js/knot.d.ts` next to
  `CompletionProvider`, and the `editor.completion` section of
  `docs/extension-api.md`. Update
  `docs/architecture/editor-and-languages.md` (native editing behavior) with
  the paste flow.
- Markdown: `extensions/@knot/language-markdown/src/extension.ts`, with
  tests in `extensions/@knot/language-markdown/test/`. Update the package
  description in `knot.jsonc`.
- Commit with `Release-note:` trailer: pasting a URL over selected text in
  Markdown makes a link.

## Checkpoint 2 — `Editor.preparePaste` and Vim

Deliverables:
- `Editor.preparePaste(text, range?)` handler and `knot.d.ts` entry.
- Vim visual `p`/`P` passes characterwise register text over a
  characterwise selection through it.

Validation:
- Product test for `preparePaste`: provider answer, no provider, and a
  range other than the selection.
- Vim test (`extensions/@knot/vim/test/registers.test.ts` or a case file):
  in a Markdown document, visual-select a word, then `p` from the clipboard
  and from a named register holding a URL. Both make a link and the unnamed
  register holds the replaced word. Linewise put is unchanged.

Review gate: try the Vim commands below. `just check` is clean.

Implementation context:
- Handlers: `src/app/extensions/host/handlers.rs` (mirror `read_selection`
  and the editor handlers around it), reusing checkpoint 1's lookup and call.
- Vim: `extensions/@knot/vim/src/visual.ts` `put`, where
  `text = value.text.repeat(count)` for the characterwise-over-characterwise
  case. The caret placement after it already uses the final `text`.
- Commit with `Release-note:` trailer: Vim's visual put of a URL over
  selected text in Markdown makes a link.

## Manual review

`post-init.js` registers `Try:` commands that open a Markdown scratch
document, select a word, and write a URL to the clipboard, so paste can be
tried by key, menu, and Vim. Checkpoint 2 adds a Vim variant.
