# D161 — gpui-independent crates and vocabulary

Status: 🚧 checkpoint 3 awaiting review. Roadmap item [D161](../roadmap.md).
Order: **D161 → [D160](d160-gpui-component.md) → [D162](d162-thin-gpui-shell.md).**
This plan runs before the gpui port; D162 continues it after the port.

## Goal

The split between what needs gpui and what does not is enforced by the
compiler instead of by convention. Knot becomes a Cargo workspace whose text,
transport, extension-runtime, and model crates cannot see gpui. The model code
that is already gpui-free moves there, Knot-owned key, color, and string types
replace gpui's in non-view code, and the workbench layout holds identities
instead of view entities. Editing a bundled extension's TypeScript rebuilds
and retests only its group crate: the application library and its test
binary stay fresh, so `cargo test` after a Vim edit costs one small crate.
`cargo run` may rebuild more; it is not on the test loop. Users see no
behavior change.

## Background

- gpui plays three roles today: rendering and platform input (expected);
  the object model and runtime of all state (`Entity`, `Context`, `notify`,
  `observe`, 26 `impl Global`s, `cx.spawn`); and vocabulary types in non-UI
  code (`Keystroke` in the key router and keymaps, `Hsla`/`HighlightStyle` in
  the theme, `SharedString` in documents, `Entity<EditorView>`,
  `Entity<TerminalView>`, and `FocusHandle` in workbench tabs). This plan
  removes the third role and fences the code free of the second; D162 thins
  the second.
- 677 of 1,252 tests are `#[gpui::test]`. `src/core`, `src/host`, and
  `src/app/syntax` have none; `src/app/workbench` has 38 against 1, although
  layout needs no rendering.
- About 35 modules under `src/app` already use no gpui but share a crate with
  code that does, so nothing keeps them that way.
- `src/host` depends on `src/app` only in tests (`protocol.rs` and
  `engine/tests.rs` use `app::declared_host`). `src/app` uses
  `host::protocol` 317 times, and editor code imports its wire types.
- Cargo recompiles every dependent of a crate whose output changed, and a
  package's lib and bin share one dependency list. Any edge from the library
  (normal or dev) to compiled extension output therefore recompiles the
  library and its test binary on a TypeScript edit.
- gpui's own code will not vanish while Zed uses it; the realistic risks are
  release cadence and API churn, and a migration is the unlikely case. A
  narrower gpui surface reduces both the port's reach and a migration's.

## Design

- **Workspace layout.** Crates live under `crates/`; the root package stays
  the `knot` gpui application library, and `knot-app` builds the binary.

  ```text
  knot-core       text storage, anchored ranges, transactions
  knot-protocol   host::protocol: Knot-owned serde transport types (leaf)
  knot-host       V8, scheduler, module graph, declarations   → protocol
  knot-model      gpui-free editor state and policy           → core, protocol
  knot-bundled    BundledSource and the tsc build helper      (leaf)
  knot            gpui shell, views, sessions, extension bridge → all above
  knot-ext-*      compiled bundled packages and their tests   → bundled; dev → knot
  knot-app        the knot binary                             → knot, knot-ext-*
  knot-handler-macros  (unchanged)
  ```

  `knot-protocol` exists so `knot-model` can use transport types without
  depending on V8. Syntax and the grammars live in `knot-model`, since the
  buffer model owns syntax state and the grammar C code builds once. The gpui
  shell stays one crate: its parts depend on each other heavily.
- **The rule for `knot-model`.** It never depends on gpui, V8, or the host.
  Its functions take plain data and `&mut` access to the models they change,
  and return results or effects as data; they never take closures expecting a
  runtime context, subscribe, or spawn. Its tests are plain `#[test]`s, and
  they are the second consumer of every API: an API a plain test cannot call
  without imitating gpui has gpui's shape and is redesigned.
- **Vocabulary.** A Knot `Keystroke` and `Modifiers` own key notation,
  parsing, and matching; a Knot color and highlight style own theme values;
  `Arc<str>` replaces `SharedString` outside views. The shell converts at the
  view edge only (gpui `Keystroke` in, `Hsla`/`HighlightStyle` and
  `SharedString` out). No small-string or interning library: nothing measured
  needs one, and a Knot newtype can change its representation later.
- **Workbench identities.** Workbench tabs carry document, terminal-session,
  and tool identities. The product session owns the map from identity to view
  entity and focus handle, and creates or drops views as the workbench
  reports tabs opened or closed. Layout transitions (split, move, resize,
  close, pin, preview, docks) become plain `knot-model` code.
- **Bundled packages.** The library never depends on compiled extension
  output. It receives packages as `&'static [BundledSource]`, through
  `knot::run` and a `test-support` entry point. `knot-bundled` is a stable
  leaf holding `BundledSource` and a `build` module whose per-package
  fingerprints in `OUT_DIR` rerun `tsc` only for changed packages. Packages
  are grouped into crates by domain, each with a `build.rs` calling that
  helper and a `PACKAGES` table:

  | Crate | Packages |
  | --- | --- |
  | `knot-ext-languages` | `language-{markdown,others,python,rust,toml,typescript}` |
  | `knot-ext-vim` | `vim` |
  | `knot-ext-workbench` | `projects`, `status-bar`, `go-to-line`, `themes` |

  One crate per package is too fine, and one for all too coarse as packages
  grow; a new package joins the group of its domain or starts one. Package
  admission (`bundled_package`, diagnostics) stays in the library.
- **Extension tests are integration tests.** A group crate's tests run the
  real editor (the library with `test-support`) and the real packages they
  need, never stubs. A group may load another group's packages and accept
  that an edit there reruns its tests. A test about an extension lives in
  its group crate; a core test that needs only some package uses an inline
  fixture or none. `knot-app` holds the tests that need every package.

## Exclusions

- `BufferModel`, `Editor`, documents, settings, languages, the extension
  bridge, and product commands stay gpui entities and globals here; making
  them plain is [D162](d162-thin-gpui-shell.md).
- A Knot runtime abstraction over spawn, notify, observe, or globals.
- Splitting the gpui shell into several crates, and a separate syntax crate.
- Any behavior, setting, or extension API change.

## Deferred decisions

- Which keymap-registry state moves with the matching logic, and which stays
  in the `ApplicationKeymaps` global. Decide in checkpoint 5 from the code.
- Whether terminal-emulator state (alacritty) has a gpui-free part worth
  moving. Decide in D162 if a terminal change needs it.

## Checkpoints

Before checkpoint 1, record a baseline in this plan: `cargo build --timings`
for a clean debug build, the incremental rebuild after touching one file in
`src/app/editor/` and one under `extensions/@knot/vim/src`, and the
`just test` wall time. Each checkpoint records the same numbers.

## Timings

Warm incremental builds (`cargo build`, debug, Apple silicon) after touching one
file, and `nextest` wall time with warm test binaries. A clean build could not
be measured: the sandbox cannot download the V8 prebuilt, so only numbers
against the existing target directory exist, and they are noisy.

| | `src/app/editor.rs` | vim `main.ts` | core `lib.rs` | tests |
| --- | --- | --- | --- | --- |
| Baseline (`e158c9e`) | 3.3 s | 4.0 s | 3.5 s | 23 s |
| 1. Workspace crates | 3.1 s | 3.8 s | 3.2 s | 34 s (includes test relinking) |
| 3. Group crates | 3 s | 3 s (`cargo test --no-run`); 2 s (`nextest run -p knot-ext-vim --no-run`) | 3 s | 10 s |

Checkpoint 1 changes nothing here as expected. Moving `build.rs` into
`knot-bundled` alone made a Vim edit 5.4–5.7 s, because the library still
depends on it; checkpoint 3 removed that edge. After it, a Vim edit rebuilds
`knot-ext-vim` and `knot-app` only; `scripts/check-bundled-incremental.sh`
asserts that the library stays fresh.

### 1. Workspace with knot-core, knot-protocol, and knot-host

✅ Complete and approved. The crates live under `crates/`; the root package
depends on them, and `knot-core`, `knot-protocol`, and `knot-host` have no gpui
in their dependency trees. Durable consequences for later checkpoints:

- `declarations.rs` lives in `knot-protocol`, not `knot-host`: `HostRequest`
  embeds `DeclaredRequest`, so the two cannot be separate crates. The handler
  macros emit `::knot_protocol::declarations::`; `HostSuccess` impls for
  protocol types must live in `knot-protocol` (orphan rule).
- Each crate has a `test-support` feature that exposes test-only helpers; the
  application enables it as a dev-dependency. Tests that call a facade
  operation live in the application (`src/app/extensions/host/engine_tests.rs`,
  `declaration_tests.rs`), whose binary links the handlers. The 33 declaration-free
  engine tests stay in `knot-host`, giving it 57 tests that build without gpui.
- Every blocking wait in test helpers and tests is bounded by
  `scheduler::TEST_WAIT` (30 s) and panics on expiry (verified for
  `receive_request`); `.config/nextest.toml` also terminates tests blocked past
  three 60 s periods. Plain `cargo test` has only the 30 s bounds.
- Timings are in [Timings](#timings); a clean build could not be measured.

### 2. Core tests without the bundled packages, and a product test driver

The extension tests must run outside the library, but they reach deep
internals today (`ProductShell` fields, workbench panes, editor viewport,
window sessions, extension storage, recovery). Exporting those would keep the
coupling this plan removes, so the tests are ported to a narrow driver that
exposes only plain data.

**Design.** `knot::test_support` (feature `test-support`, also compiled
under `cfg(test)`) holds:

- `TestProduct`: starts the real product (globals, recovery, state database,
  launch gate) with the given packages, an optional `post-init`, and
  installed packages. It drives through keys, text, commands, Open, roots,
  windows, prompts, package restart, quit, and relaunch over the same state;
  it observes through `snapshot()`, a serde description of windows, panes,
  tabs, documents (URI, dirty, text), and roots, plus contributed tree rows,
  status-bar items, a package's stored records, and recovery snapshots.
- `TestEditor`: the case harness's editor session (marked text, keys,
  caret, caret shape, key contexts, viewport rows, folds, scoped settings).

Only these types and their data structs are public; the library's own items
stay `pub(crate)`. Ported tests import only `crate::test_support`, and a
`pub use` of a crate-private type fails to compile, so the boundary holds
before checkpoint 3 moves them.

#### 2a. Inject the packages and switch incidental tests ✅

`knot::run(packages)` hands the compiled-in packages to the extension host,
which passes them to `discover_all`; nothing in the library imports
`knot_bundled::PACKAGES`, and `src/main.rs` passes it until checkpoint 3.
Tests start the bundled set only by passing it. The startup, keybinding,
window-restore, recovery, and settings tests that started it incidentally
now run on inline fixtures or none, and the `discover_all` tests inject a
fixture bundled package.

#### 2b. TestEditor, a minimal TestProduct, and the editor-level extension tests ✅

`knot::test_support` exposes `TestProduct`, `TestEditor`, plain result
structs, `wait_until`, `cases` (case files, path-parameterized), and
`editing` (one action in a bare editor). The driver code lives in
`src/app/product/driver/` so it can use the product's private items, and
`setup.rs` there also holds the globals and windows the library's own tests
start from. The Vim, language (comments, indents, brackets, folds,
highlights), Go to Line, and status bar tests, and the every-package load
and memory tests, use only `crate::test_support`. The library builds with
`--features test-support` outside `cfg(test)`, which widened 27 test-only
gates (`cargo clippy -p knot --lib --features test-support` is clean).

- Comment cases now run through `editor.toggle-line-comment` and
  `editor.toggle-block-comment` instead of the language's editing
  configuration. That exposed a product bug: a Markdown HTML block is
  injected HTML, which declared no comment tokens, so a commented Markdown
  line could not be uncommented. HTML now declares `<!--`/`-->` and has its
  own cases in `language-others`.
- `TestEditor::press` waits until every package is quiet. A command waiting
  on a picker or prompt keeps its package busy, so Go to Line and Vim's `:q`
  send those keys with `press_unsettled` and wait for what they show.

#### 2c. TestProduct for the projects tests ✅

`TestProduct::launch` starts the whole application, keeps its launch window,
and keeps package storage, recovery, and the window record in files under
its own directory. `TestWindow` drives a window through native keys,
commands, roots, opened files, and splits, and reports its documents,
editors, and layout. `TestTree` reads and acts on a contributed tree.
`relaunch_restoring` quits and launches again in the same process over the
same files. `projects.rs` and `project_sessions.rs` import only
`crate::test_support`, and the test count is unchanged.

- The driver always sets the storage root it launches with, and the host's
  in-memory test storage is also used under `test-support`. Without that,
  extension tests built outside `cfg(test)` would read the developer's real
  package records.
- Packages stop before a relaunch closes the windows, as quit stops them.
  A root change made "while the package is not running" is now
  `stop_package`, the change, then `reload_packages`, instead of one update
  that relied on the ordering of events.

### 3. Extension group crates and the knot-app binary

✅ Implemented; awaiting review. Notes for later checkpoints:

- `BundledSource` is `Copy`; `knot-app` exposes `packages()`, the three
  groups joined, for `main` and its test.
- `knot-ext-vim`'s tests also load `knot-ext-languages`: Vim's indentation,
  contexts, and `o`/`O` cases depend on language configuration.
- The group crates' `build.rs` files run in parallel, so the helper's
  `npm ci` holds a file lock (`scripts/extensions/.npm-ci.lock`).
- The root package lists every workspace crate in `default-members`, so a plain
  `cargo test` still covers all of them and `cargo run -p knot-app` runs the app.
- Moving the tests outside the library exposed a `cfg(test)` gate on the
  session-ID registry that now follows `test-support`.

- Deliverables: `knot-ext-{languages,vim,workbench}` with their packages and
  tests; `crates/knot-app` owning `main.rs` and the `knot` binary; the root
  `knot` package is library-only, with no normal or dev edge to compiled
  packages; `knot-bundled` reduced to `BundledSource` and the build helper.
- Validation:
  - `just test` passes with the same test count.
  - `scripts/check-bundled-incremental.sh` also asserts that after a Vim
    `main.ts` edit, `cargo test --no-run` rebuilds `knot-ext-vim` alone (the
    `knot` library and its test binary stay Fresh), and that a
    `tsconfig.base.json` change rebuilds every group crate.
  - `just run` starts the app with every package.
- **Implementation context:**
  - Move the compile and fingerprint logic of `crates/knot-bundled/build.rs`
    into a `pub mod build` of `knot-bundled`, taking package names; each
    group crate lists it as a build-dependency, calls it from its `build.rs`,
    and includes the generated table as `knot-bundled/src/lib.rs` does now.
  - Root `Cargo.toml`: drop `[[bin]]`; keep `knot-bundled` only for the type;
    add workspace `default-members` so `cargo run` at the root runs
    `knot-app`; `justfile`'s `run` recipe follows.
  - `crates/knot-app/src/main.rs` joins the groups' `PACKAGES` and calls
    `knot::run`.
  - Move tests into each crate's `tests/`, dev-depending on `knot` and gpui
    with `test-support`: `src/app/product/tests/vim.rs` → `knot-ext-vim`;
    `language_{editing,indents,brackets,folds}.rs` and
    `extensions/host/highlight_tests.rs::bundled_languages_highlight_their_fixtures`
    → `knot-ext-languages`; `go_to_line.rs`, `status_bar_package.rs`,
    `projects.rs`, and `project_sessions.rs` → `knot-ext-workbench`;
    `host.rs::bundled_packages_load_with_product_discovery` and
    `measure_bundled_package_memory` → `knot-app`, the only crate that sees
    every package.
  - `scripts/check-extensions.sh`, the TypeScript package test runner, and
    the `extensions/@knot` layout are unchanged.
- Review gate: the [timings](#timings) row for this checkpoint shows a Vim
  edit followed by `cargo test`, and by `cargo nextest run -p knot-ext-vim`,
  costing one small crate build and link.

### 4. Knot-owned keys, colors, and strings

- Deliverables: Knot `Keystroke`/`Modifiers`, color, and highlight-style
  types; the key router, keymaps, presets, key hold, theme resolution, and
  documents use them; conversions exist only where views meet gpui.
- Validation: the keymap, key-router, theme, and keybinding product tests
  unchanged in behavior; new plain tests for key notation parsing,
  round-tripping, and gpui conversion; `rg 'gpui::(Keystroke|Hsla|HighlightStyle|SharedString)'`
  shows only view and shell modules.
- **Implementation context:** `Keystroke` is used in
  `src/app/commands/{key_router,keymaps,key_hold}.rs`, `keymaps/presets.rs`,
  `src/app/product/{key_routing,key_hold}.rs`, `text_input.rs`, `picker.rs`,
  `tree_view.rs`, `workspace_tree.rs`, `transient.rs`, and
  `src/app/terminal/input.rs`. Parsing and matching must keep gpui's notation
  and semantics (`Keystroke::parse`, `key_char`, the matching note at
  `keymaps.rs:503`), because presets and user keymaps rely on them. The theme
  types are in `src/app/theme.rs` (`ColorDefault`, `Theme::color` at line
  448, the highlight styles); painting code in `src/app/editor/paint.rs`,
  `terminal/`, and the views converts. `SharedString` appears 41 times
  outside tests, mostly `documents.rs`. Types are created in `src/app` here
  and move with checkpoint 5.
- Review gate: the conversion edge is small and obviously placed.

### 5. knot-model with the gpui-free modules

- Deliverables: `knot-model` holding the modules that need no gpui, with no
  gpui, V8, or host dependency in its manifest, and their tests as plain
  tests there.
- Validation: `cargo test -p knot-model` builds without gpui or V8; the full
  suite unchanged.
- **Implementation context:** candidates, whose intra-app imports were
  checked: `history.rs`, `syntax.rs` and `syntax/` (imports
  `languages::editing` and `theme`), `indentation.rs`,
  `languages/editing.rs`, `filesystem/`, `resource.rs`, `open.rs`,
  `workspace.rs`, `editor/text.rs`, `commands/catalog.rs`,
  `commands/key_router.rs`, the keymap matching from `commands/keymaps.rs`,
  `extensions/package.rs` and `extensions/load.rs` (whose `bundled::report`
  call moves to the caller), theme resolution from `theme.rs`, and the
  checkpoint 4 types. `completion.rs` (uses `zoom::rems`) and `editor/wrap.rs`
  (depends on `model`) wait for D162. Globals such as `ApplicationLanguages`
  and `ApplicationKeymaps` stay in `src/app` and wrap the moved plain state.
  `pub(crate)` items crossing the boundary become `pub`; keep the public
  surface to what the application uses.
- Review gate: the crate's contents and public surface read as one
  coherent layer, not a grab bag.

### 6. Workbench layout over identities

- Deliverables: the workbench layout model in `knot-model` with tabs holding
  identities; the product session owns view entities and focus handles and
  reacts to the tabs the workbench opens and closes; workbench tests are
  plain tests.
- Validation: the workbench tests moved to plain `#[test]`s; the product
  session, closure, relocation, tab-drag, tools, and window-restore tests
  unchanged in behavior; a test that closing a tab drops its view.
- **Implementation context:** `WorkbenchTabPayload` and `ExtensionView`
  (`src/app/workbench.rs:237-300`) hold `Entity<EditorView>`,
  `Entity<TerminalView>`, `Entity<TreeView>`, `Entity<TextView>`, and
  `FocusHandle`; `src/app/workbench/{tools,close,resize,relocate,arrange}.rs`
  use `Context<Self>` mostly for `notify` and test setup. Callers that read a
  tab's view (`src/app/product/{render,key_routing,tab_drag,relocate,tools,sessions}.rs`)
  go through the session's view map. Snapshots for persistence
  (`WorkbenchTabSnapshot`, `window_layout.rs`, `tool_layout.rs`) already
  carry identities and should not change format.
- Review gate: a personal-config `post-init.js` with `try.*` commands that
  split, move, pin, close, and dock tabs; everything behaves as before.

### 7. Documentation

- Deliverables: `docs/architecture.md` system map and ownership describe the
  crates and the `knot-model` rule; `AGENTS.md`'s repository map and routing
  table follow the new paths, including `knot-ext-*` and `knot-app`;
  `docs/development.md` covers workspace commands and the bundled-package
  rebuild; a decision in
  `docs/decisions/platform.md` with its index line (crate-enforced gpui
  boundary, `knot-protocol` below model and host, bundled packages in
  domain group crates injected into a library-only `knot`, the `knot-app`
  binary, per-package fingerprints, no runtime abstraction, measured build times);
  the D160 and D162 plans' order lines updated to say D161 is complete, with
  a link to that decision; this plan and D161's roadmap entry removed.
- Validation: the `review-docs` checks for the touched files.
- Review gate: D160 can start; its implementation contexts point at the
  new paths.
