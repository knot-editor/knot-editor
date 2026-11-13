# D163 — Vim tests on a minimal language fixture

Source: [D163](../roadmap.md) (deferred register; reconsider: now).
Scope: Decouple `knot-ext-vim` tests from `knot-ext-languages`, replace full language package loading with a minimal inline fixture in test setup, and optimize test-case buffer reloading.

## Agreed design

- **Targeted inline fixture in `post-init.js`**: `crates/knot-ext-vim/tests/vim.rs` registers the minimal language configuration strictly where needed, so non-language tests pay zero language registration overhead:
  - Default `ENABLE` script used by `start(cx)` only sets `vim.enabled = true`, keeping startup at the absolute minimum (~60 ms) for all 21 general unit tests and the 4 non-language topic tests (`marks`, `objects`, `registers`, `scrolling`).
  - A `"rust"` fixture using the compiled-in tree-sitter `"rust"` grammar with minimal queries is registered only for the 6 topic tests whose cases test syntax-aware behavior (`ex`, `folding`, `insert`, `motions`, `operators`, `visual`):
    - Indent query: `[ (block) (declaration_list) ] @indent.begin`, `"}" @indent.branch` (supports `o`/`O`, `==`, `=ap`, `V=`)
    - Fold query: `(block) @fold` (supports `zc`)
    - Highlight query: `(string_literal) @string` (supports `%` skipping brackets in string literals)
  - `vim_follows_the_setting_and_a_language_scoped_value` registers its own dummy `"markdown"` language in its existing custom `post-init.js`.
- **Drop `knot-ext-languages` dev-dependency**: Remove `knot-ext-languages` from `crates/knot-ext-vim/Cargo.toml`. `packages()` in `tests/vim.rs` returns only `knot_ext_vim::PACKAGES`.
- **Single-language test cases**: Change case #87 in `extensions/@knot/vim/test/cases/operators.jsonc` from `typescript` to `rust` (or the fixture language). The Vim `==` operator delegates to native reindentation; language-specific indentation is tested in `knot-ext-languages`.
- **In-place buffer reload in `TestEditor::load`**: Update `TestEditor::load` in `src/app/product/driver/editor.rs` to replace buffer text and update selection on the existing `Buffer` instead of destroying and recreating the entire `Workbench` entity on every case.

## Explicit exclusions

- Changes to production Vim extension runtime logic (`extensions/@knot/vim/src/`).
- Adding new test fixture crates or build scripts.

## Checkpoints

### 1. Minimal language fixture and decoupling `knot-ext-languages`
Deliverable:
- Update case #87 in `extensions/@knot/vim/test/cases/operators.jsonc` to test in Rust.
- Keep default `ENABLE` strictly to `vim.enabled = true` so general tests pay zero fixture overhead.
- Register `"markdown"` only in `vim_follows_the_setting_and_a_language_scoped_value`'s own `post-init.js`.
- Provide `start_with_language_fixture` in `tests/vim.rs` registering the minimal `"rust"` fixture for the 6 language-dependent topic tests.
- Update `packages()` in `tests/vim.rs` to return only `knot_ext_vim::PACKAGES`.
- Remove `knot-ext-languages` from `crates/knot-ext-vim/Cargo.toml`.

Review gate: Run `cargo test -p knot-ext-vim`. All 32 tests pass; tests without languages start in ~60 ms without any language registrations, and tests with languages register only the minimal Rust fixture.
Implementation context:
- `crates/knot-ext-vim/Cargo.toml`
- `crates/knot-ext-vim/tests/vim.rs` (`ENABLE`, `start`, `packages()`)
- `extensions/@knot/vim/test/cases/operators.jsonc` (case #87)

### 2. In-place buffer reuse in `TestEditor::load`
Deliverable:
- In `src/app/product/driver/editor.rs`, update `TestEditor::load` so that when called on an existing test editor, it replaces the buffer text (`model.update(cx, |model, cx| ...)`), adjusts the language if needed, resets selection, and settles, avoiding full `Workbench` and document teardown/recreation on all 436 cases.

Review gate: Run `cargo nextest run -p knot-ext-vim`. All tests pass cleanly, and total suite execution time drops by an additional ~500 ms.
Implementation context:
- `src/app/product/driver/editor.rs` (`TestEditor::load`, `new_document`, `BufferModel`)

### 3. Close-out
Deliverable:
- Update `docs/roadmap.md` for D163.
- Run `just check` and `cargo fmt`.

Review gate: `just check` clean (no warnings), all tests passing, clean git status.
