# D161 — gpui-independent crates and vocabulary

Status: 🚧 approved, not started. Roadmap item [D161](../roadmap.md).
Order: **D161 → [D160](d160-gpui-component.md) → [D162](d162-thin-gpui-shell.md).**
This plan runs before the gpui port; D162 continues it after the port.

## Goal

The split between what needs gpui and what does not is enforced by the
compiler instead of by convention. Knot becomes a Cargo workspace whose text,
transport, extension-runtime, and model crates cannot see gpui. The model code
that is already gpui-free moves there, Knot-owned key, color, and string types
replace gpui's in non-view code, and the workbench layout holds identities
instead of view entities. A TypeScript edit no longer rebuilds the Rust
application crate. Users see no behavior change.

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
- `build.rs` sits on the application crate with
  `rerun-if-changed=extensions/@knot` and runs `tsc` over all 12 bundled
  packages on every rerun.
- gpui's own code will not vanish while Zed uses it; the realistic risks are
  release cadence and API churn, and a migration is the unlikely case. A
  narrower gpui surface reduces both the port's reach and a migration's.

## Design

- **Workspace layout.** Crates live under `crates/`; the root package stays
  the `knot` gpui application and binary.

  ```text
  knot-core       text storage, anchored ranges, transactions
  knot-protocol   host::protocol: Knot-owned serde transport types (leaf)
  knot-host       V8, scheduler, module graph, declarations   → protocol
  knot-model      gpui-free editor state and policy           → core, protocol
  knot-bundled    build.rs and the compiled package table     (leaf)
  knot            gpui shell, views, sessions, extension bridge → all
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
- **Bundled packages.** `knot-bundled` owns `build.rs` and exposes the
  compiled sources. `build.rs` keeps a per-package fingerprint of its inputs
  in `OUT_DIR` and runs `tsc` only for packages whose inputs changed. Package
  admission (`bundled_package`, diagnostics) stays in the application.

## Exclusions

- `BufferModel`, `Editor`, documents, settings, languages, the extension
  bridge, and product commands stay gpui entities and globals here; making
  them plain is [D162](d162-thin-gpui-shell.md).
- A Knot runtime abstraction over spawn, notify, observe, or globals.
- Splitting the gpui shell into several crates, and a separate syntax crate.
- Any behavior, setting, or extension API change.

## Deferred decisions

- Which keymap-registry state moves with the matching logic, and which stays
  in the `ApplicationKeymaps` global. Decide in checkpoint 4 from the code.
- Whether terminal-emulator state (alacritty) has a gpui-free part worth
  moving. Decide in D162 if a terminal change needs it.

## Checkpoints

Before checkpoint 1, record a baseline in this plan: `cargo build --timings`
for a clean debug build, the incremental rebuild after touching one file in
`src/app/editor/` and one under `extensions/@knot/vim/src`, and the
`just test` wall time. Each checkpoint records the same numbers.

### 1. Workspace with knot-core, knot-protocol, and knot-host

- Deliverables: the three crates under `crates/`, the application depending
  on them, and the host's test-only edges to `app` removed. The handler
  macros emit paths to the new crates.
- Validation: `just test` and `just lint` across the workspace;
  `tests/handler_linking.rs` still links every declared handler and command;
  `cargo test -p knot-core` and `cargo test -p knot-host` build without gpui.
- **Implementation context:** move `src/core/` to `crates/knot-core/src/`,
  `src/host/protocol.rs` to `crates/knot-protocol/src/lib.rs`, and the rest of
  `src/host/` (with `src/host/js/knot.d.ts`) to `crates/knot-host/`.
  `src/lib.rs` re-exports `declarations` for the linking test; repoint it.
  The macros in `crates/knot-handler-macros/src/` emit
  `crate::host::declarations::` (9 sites) and
  `crate::app::commands::declarations::` (4); host paths become
  `::knot_host::declarations::`. Move the `app::declared_host` uses in
  `protocol.rs` tests (around line 930) and `src/host/engine/tests.rs` into
  application tests, or give them host-local fixtures. `build.rs`'s
  `rerun-if-changed=src/host/js/knot.d.ts` and every doc link to `src/core`,
  `src/host`, or `knot.d.ts` follow the move (`AGENTS.md` map and table,
  `docs/extension-api.md`, `docs/architecture/`). `justfile` and
  `.github/workflows/ci.yml` gain `--workspace` where missing; CI's
  `cargo test --lib host::declarations` (line 74) becomes `-p knot-host`.
- Review gate: the timing numbers, and the crate graph reads as designed.

### 2. knot-bundled with per-package compilation

- Deliverables: `build.rs` and the generated table in `knot-bundled`; only
  changed packages recompile.
- Validation: the bundled-package startup tests unchanged; a build test or
  script check that touching one package's source reruns `tsc` for that
  package alone, and that a lockfile or `tsconfig.base.json` change reruns
  all; `scripts/check-extensions.sh` unchanged.
- **Implementation context:** move `build.rs` to `crates/knot-bundled/` with
  paths relative to the workspace root; it exposes `PACKAGES` and
  `BundledSource` from the include in `src/app/extensions/bundled.rs`, which
  keeps `report()`. Fingerprint each package directory (paths, sizes, and
  mtimes are enough) plus the shared inputs into
  `OUT_DIR/extensions/<name>.fingerprint`, and skip `compile` when it
  matches and `dist` exists.
- Review gate: an edit to the Vim package rebuilds in the time of one `tsc`
  run plus a relink.

### 3. Knot-owned keys, colors, and strings

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
  and move with checkpoint 4.
- Review gate: the conversion edge is small and obviously placed.

### 4. knot-model with the gpui-free modules

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
  checkpoint 3 types. `completion.rs` (uses `zoom::rems`) and `editor/wrap.rs`
  (depends on `model`) wait for D162. Globals such as `ApplicationLanguages`
  and `ApplicationKeymaps` stay in `src/app` and wrap the moved plain state.
  `pub(crate)` items crossing the boundary become `pub`; keep the public
  surface to what the application uses.
- Review gate: the crate's contents and public surface read as one
  coherent layer, not a grab bag.

### 5. Workbench layout over identities

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

### 6. Documentation

- Deliverables: `docs/architecture.md` system map and ownership describe the
  crates and the `knot-model` rule; `AGENTS.md`'s repository map and routing
  table follow the new paths; `docs/development.md` covers workspace
  commands and the bundled-package rebuild; a decision in
  `docs/decisions/platform.md` with its index line (crate-enforced gpui
  boundary, `knot-protocol` below model and host, one bundled crate with
  per-package fingerprints, no runtime abstraction, measured build times);
  the D160 and D162 plans' order lines updated to say D161 is complete, with
  a link to that decision; this plan and D161's roadmap entry removed.
- Validation: the `review-docs` checks for the touched files.
- Review gate: D160 can start; its implementation contexts point at the
  new paths.
