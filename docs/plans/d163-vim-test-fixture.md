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
✅ Done. Fixture covers the 6 syntax-aware topics; `cargo test -p knot-ext-vim` 32/32. Bracket pairs ride along because native `%` pairs only registered brackets. Measured suite time ~3.6s → ~1.1s.
Implementation context:
- `crates/knot-ext-vim/Cargo.toml`
- `crates/knot-ext-vim/tests/vim.rs` (`ENABLE`, `start`, `packages()`)
- `extensions/@knot/vim/test/cases/operators.jsonc` (case #87)

### 2. In-place buffer reuse in `TestEditor::load`

Reuse the window, workbench, document, buffer, and editor across all 436 cases
instead of destroying and recreating them, without leaking state between cases.
`TestEditor::load` is the shared product driver, so this saving compounds for
every current and future extension suite built on it, not just Vim.

#### Why the naive reload fails

A fresh workbench implicitly resets state owned by five layers; a plain text
replacement via `BufferModel::replace` resets none of it, and by construction
leaks undo history, anchors, and dirty state. The earlier attempt panicked in
the `scrolling` topic (`line 13 out of range (3)`, see
[stale editor queries](../reports/stale-editor-queries.md)): the retained
viewport row survived the text replacement and Vim queried it.

| Layer | Stale state if only the text is replaced |
| --- | --- |
| `BufferModel` (`crates/knot-model/src/buffer.rs`) | Undo history (undo crosses cases), anchored ranges, contributions, marks, pending changes, fold-region/snapshot caches, syntax tree. `replace` advances the revision off the document's clean baseline, so every case starts dirty. No wholesale reset API exists; `replace_generated_text` asserts read-only and keeps history. |
| `EditorState` (`crates/knot-model/src/editor.rs`) | Scroll offset and viewport row (the panic), folds, wrap map, completion, auto-closed pairs, pending indent, decorations, lifecycle-owned key contexts/caret shapes/bars. |
| `Editor`/`EditorView` (`src/app/editor.rs`, `src/app/editor/view.rs`) | Syntax cache and caret-blink key — both generation/revision-keyed, so expected to self-invalidate; verify, do not add code speculatively. |
| Vim `Followed` (`extensions/@knot/vim/src/extension.ts`, `normal.ts`) | Keyed by `ViewTarget`, which a reused tab keeps: mode, pending keys, `want`/`caret`, visual and previous-visual marks, jump list, context mark, `scroll` option, `foldsOff`. `vim.reset` today clears only shared registers, marks, and last-find. |
| Document/driver (`crates/knot-model/src/documents.rs`, `src/app/languages.rs`, `src/app/documents.rs`) | Language/syntax attachment per case, per-document setting scopes, dirty baseline. |

#### Approach: reuse identity, reset content at the owning layer

Never reset via a committed text replacement.

1. **`BufferModel::reset_to_text`** (new, `crates/knot-model/src/buffer.rs`):
   swap text; clear history, anchored ranges, contributions and marks, pending
   changes, and derived caches; keep language, document identity, and access
   policy; reparse syntax. Keep the revision monotonic and re-baseline the
   document through a new `mark_clean` on the document collection (the same
   operation a save performs). Do not reset the revision to 0: a stale
   extension-side revision could then alias the new text.
2. **Editor-state swap** (`src/app/editor.rs`): `release` the old state's
   anchors, then install a fresh `EditorState::new` over the same buffer. This
   resets scroll, folds, wrap, and completion at once instead of field-by-field
   clearing. Lifecycle-owned presentation (key contexts, caret shape) drops
   with it and is re-applied by Vim in step 3. Order is: load, Vim reset,
   selection/layout, settle — with no settle in between, so no extension query
   observes the intermediate state.
3. **Extend `vim.reset`** (`extensions/@knot/vim/src/extension.ts`): also reset
   every followed editor — mode to normal via `setMode`, pending cleared,
   jump/context/previous marks disposed, `scroll` and `foldsOff` cleared. This
   amends the plan's "no production Vim runtime logic" exclusion above:
   `vim.reset` already exists solely for these tests (`palette: false`), so
   extending it stays within intent; the exclusion now covers only Vim behavior
   logic. Marks must be disposed before the buffer's anchored store is cleared
   in step 1 (disposing afterwards is a silent no-op leak); pin that ordering
   in the driver.
4. **`TestEditor::load` orchestration** (`src/app/product/driver/editor.rs`):
   keep `document`, `model`, `window`, and `editor`; reset the buffer, swap
   the editor state, apply the language through the existing `set_language`
   path (`src/app/languages.rs`) when it changes, then `show(selection)` as
   today. Language-change notification follows the buffer notify, so Vim's
   `languageMayHaveChanged` recheck runs against the new state.

#### Step 5 (approved): stale-row hardening first

Pull [D185](../roadmap.md#bugs) forward as its own commit before the rewire:
in-place reuse makes stale view rows reachable by construction, so
`editor_row_at` and its sibling `editor_*` handlers in
`src/app/extensions/host/handlers.rs` return their existing `AccessError`
instead of panicking, with the regression test the report specifies. Once this
checkpoint lands, update or remove the deferred D185 entry and its report as
part of close-out.

#### Execution order

Each step is committed separately once verified.

1. ✅ D185 stale-row hardening. Done in `knot-model` instead of the
   handlers: stale row and caret queries clamp to the current text, so no
   handler needs an error path.
2. ✅ `BufferModel::reset_to_text` (test-support; rebuilds the syntax state
   from scratch) + document re-baseline (`mark_clean` in the model collection
   and an app wrapper that republishes recovery and observation), with model
   and gpui tests: undo empty after reset, no anchored ranges survive, clean
   after re-baseline.
3. ✅ Editor-state swap (`Editor::reset_state`), with an editor test
   asserting scroll, folds, and selection are gone after a reset; the
   driver-level check lands with the rewire in step 5.
4. Extend `vim.reset` to per-editor state, keeping existing shared clearing.
5. Rewire `TestEditor::load`. Run topics individually with `scrolling` first
   (canary for the original panic), then the full suite. A leaked sixth owner
   of per-case state shows up as an order-dependent case failure; isolate it
   with `KNOT_CASES` filtering.

Review gate: Run `cargo nextest run -p knot-ext-vim`. All tests pass cleanly,
and total suite execution time drops by an additional ~500 ms against the
checkpoint-1 baseline (~1.1s).
Implementation context:
- `src/app/extensions/host/handlers.rs` (stale `editor_*` queries, step 5)
- `crates/knot-model/src/buffer.rs` (`BufferModel::reset_to_text`)
- `crates/knot-model/src/documents.rs`, `src/app/documents.rs` (re-baseline)
- `src/app/editor.rs` (editor-state swap)
- `extensions/@knot/vim/src/extension.ts` (`vim.reset`)
- `src/app/product/driver/editor.rs` (`TestEditor::load`)

### 3. Close-out
Deliverable:
- Update `docs/roadmap.md` for D163.
- Run `just check` and `cargo fmt`.

Review gate: `just check` clean (no warnings), all tests passing, clean git status.
