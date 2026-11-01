# D073 — Language editing configuration

Status: 🚧 checkpoint 2 implemented, awaiting review. Item of the
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

### 2. Comment evaluator — ✅ implemented, ⏳ review gate

- ✅ `EditingConfiguration::toggle_comments(lines)` in
  `src/app/languages/editing.rs`, tested in
  `src/app/languages/editing/tests.rs`. It returns byte-range replacements
  per line, so commenting, uncommenting, wrapping, and unwrapping share one
  shape; blank lines are untouched.
- ✅ Indentation removed from D073 (contract fields, `InvalidPattern`, the
  `regex` dependency, and its evaluators) in favor of D066.
- Review gate: the comment rules match what D074 needs.

### 3. Bundled language data with per-language behavior tests

- Deliverables: comments, brackets, and `autoCloseBefore` for Rust, the four
  ECMAScript languages, Python, TOML, and Markdown.
- Validation:
  - A product test starts the bundled language packages, asserts the
    startup report has no failures, and runs table-driven comment cases
    through the evaluator on each language's registered configuration. Every
    language's table is a full suite, not a sample: each comment token in
    both directions, mixed and indented selections, and each language's
    bracket pairs and flags. For example, Rust toggling on a `///` line
    strips `///`; TypeScript commenting a block indented at mixed levels puts
    `//` at the shallowest column; Markdown commenting wraps in `<!-- -->`.
  - Shared package tests check token shapes: nonempty tokens, and every
    language in scope declares comments.
- **Implementation context:**
  - `extensions/@knot/language-{rust,typescript,python,toml,markdown}/src/languages.ts`:
    add the fields; share the ECMAScript sets as constants within the
    TypeScript package.
  - `scripts/extensions/testing/languages.ts`: extend
    `testLanguagePackage` with the shape checks, applied only to packages
    in scope.
  - New `src/app/product/tests/language_editing.rs`, registered beside
    `src/app/product/tests/languages.rs`; reuse the product test harness
    that starts bundled packages, as `startup.rs` reads `startup_report`.
- Review gate: review the case tables per language; add cases where review
  finds a gap.

### 4. Documentation

- Deliverables: the language registration section of the extension guide
  shows editing data and deriving a language with `languages.get`; the
  semantic UI reference notes that the registry owns editing configuration
  and the comment evaluator for native editing code; a decision records data
  in the registration, Zed's merged bracket list, and indentation left to
  D066's queries. Remove this plan and mark D073 ✅ in the daily-driver plan.
- **Implementation context:** `docs/extension-api.md` (Register a
  language), `docs/architecture/semantic-ui.md` (Syntax highlighting and
  languages), `docs/decisions.md` (a new Language editing configuration
  section after Syntax highlighting), `docs/plans/daily-driver.md`.
- Review gate: final review of D073.
