# D073 — Language editing configuration

Status: 🚧 checkpoints 1–3 approved; checkpoint 4 implemented, awaiting the
final review. Item of the
[daily-driver plan](daily-driver.md); D074 (editing commands) and D075
(bracket matching and auto-closing) consume it.

## Goal

A language registration carries the data editing commands need: comment
tokens and bracket pairs with auto-closing flags. A pure native function
decides which comment token toggling adds or strips. The bundled Rust,
TypeScript and JavaScript (with their JSX variants), Python, TOML, and
Markdown packages supply the data, and tests check concrete results per
language. Personal config can replace one value by spreading the registered
definition. No command behaves differently yet; D074 and D075 read the data.

Indentation is not part of this item. Zed, Helix, and Neovim indent from
tree-sitter `indents` queries, which D066 adds, so regex patterns or bracket
heuristics written now would be replaced.

## Design

- **Part of the language, not settings.** Comment tokens and bracket pairs
  describe the language like its grammar does, so they are optional fields of
  `LanguageDefinition`, registered with it and replaced with it under the
  existing latest-registration-wins rule. Toggles such as whether
  auto-closing is enabled remain language-scoped settings, owned by D075 and
  D079.
- **`languages.get(id)`** returns the effective full definition, queries and
  editing data included, so a derived language spreads it and changes a key,
  as `themes.get` allows for themes. `list` stays the light summary.
- **Fields**, all optional, following Zed's language config:

  ```ts
  comments?: { line?: string[]; block?: [open: string, close: string] };
  brackets?: { open: string; close: string;
               autoClose?: boolean;  // default true
               newline?: boolean;    // default open !== close
               notIn?: ("string" | "comment")[] }[];
  autoCloseBefore?: string;
  ```

  - `comments.line` lists every line comment token. The first is what
    commenting inserts; uncommenting strips the longest one that matches, so
    Rust lists `//`, `///`, and `//!` and toggling a doc comment never leaves
    `/ foo`. Tokens omit the trailing space: commenting inserts the token and
    one space, and uncommenting removes the token and one following space.
    A language with only a block comment (Markdown) falls back to wrapping
    each line; one with neither makes the commands no-ops.
  - One block comment style per language, as in Zed.
  - `brackets` is one list for matching, auto-closing, and surrounding. A
    pair whose `open === close` (quotes) never takes part in matching.
    `notIn` names syntax families at the cursor where auto-closing is
    suppressed. `newline` means Enter between the pair opens an indented
    middle line. Auto-closing happens only when the next character is
    whitespace, the end of the line, or one of `autoCloseBefore`.
- **Comment evaluation.** Given a set of lines, the evaluator decides
  whether toggling comments or uncomments them: comment at the smallest
  indentation of non-blank lines, uncomment when every non-blank line starts
  with a known token, strip the longest match. Block-only languages wrap and
  unwrap each line.
- **Native ownership.** The definition crosses as one JSON argument decoded
  with serde, as theme definitions do. The registry exposes the resolved
  configuration of a language ID so native editing code reads it without
  calling JavaScript; injected layers resolve their own language's
  configuration. The evaluator is a plain function over text lines and the
  configuration, with no gpui or buffer dependency.
- **Bundled scope.** Rust, `javascript`, `javascriptreact`, `typescript`,
  `typescriptreact`, Python, TOML, and Markdown get the data.
  `markdown_inline` exists only for injection and gets none. The languages in
  `@knot/language-others` are deferred as D132.

## Exclusions

- Editing commands: comment toggling and auto-closing are D074 and D075.
- Indentation and auto-indent: D066's `indents` queries. Until then D074's
  Enter keeps the current line's indentation.
- Data for the languages in `@knot/language-others`: deferred as D132.
- Syntax-scoped overrides such as JSX's `{/* */}`: deferred as D130.
- On-Enter comment continuation, word characters, and a second block comment
  style: deferred as D131.

## Deferred decisions

- Whether `notIn` checks the syntax tree or highlight captures at the
  cursor: decided in D075.

## Checkpoints

### 1. Contract and native storage — ✅ approved

Completed: the editing fields and `languages.get` are in
`src/host/js/knot.d.ts`; `src/app/languages/editing.rs` holds the registered
`EditingDefinition` and the resolved `EditingConfiguration`, which fills
bracket defaults, so `get` returns editing fields as registered and
JavaScript validation checks shapes only. Comment and bracket tokens must be
nonempty without surrounding whitespace. Open for D074: a syntax layer holds
no language ID, so resolving an injected layer's configuration needs a
layer-to-ID mapping.

### 2. Comment evaluator — ✅ approved

Completed: `EditingConfiguration::toggle_comments(lines)` in
`src/app/languages/editing.rs` returns byte-range replacements per line, so
commenting, uncommenting, wrapping, and unwrapping share one shape.
Indentation was removed from D073 in favor of D066's `indents` queries.

### 3. Bundled language data with per-package tests — ✅ approved

Completed: Rust, the four ECMAScript languages (one shared set), Python,
TOML, and Markdown register comments, brackets, and `autoCloseBefore` in
their `src/languages.ts`, with the reason for each departure from Zed's
configs beside the data: no `<` pair in Rust or ECMAScript and no `'` in
Rust, quotes suppressed in strings and comments, and Markdown quotes that
only surround. Each package keeps comment toggling cases in
`test/comments.jsonc`, which `src/app/product/tests/language_editing.rs` runs
through the native toggle on the registered configuration; it fails when a
language with comment tokens has no cases. Bracket behavior cases come with
D075 as a sibling case file; TypeScript case modules are D133.
`testLanguagePackage` checks token shapes.

### 4. Documentation — ✅ implemented, final review pending

- ✅ Deliverables: the language registration section of the extension guide
  shows editing data and deriving a language with `languages.get`; the
  semantic UI reference notes that the registry owns editing configuration
  and the comment evaluator for native editing code; a decision records data
  in the registration, Zed's merged bracket list, and indentation left to
  D066's queries. D073 is marked ✅ in the daily-driver plan; this plan is
  removed after the final review.
- **Implementation context:** `docs/extension-api.md` (Register a
  language), `docs/architecture/semantic-ui.md` (Syntax highlighting and
  languages), `docs/decisions.md` (a new Language editing configuration
  section after Syntax highlighting), `docs/plans/daily-driver.md`.
- Review gate: final review of D073.
