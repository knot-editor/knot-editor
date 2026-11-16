# D045 — General terminal settings

Status: 🚧 in progress, checkpoint 2 at its review gate (checkpoint 1 ✅). Implements [D045](../roadmap.md) (Next up #1).

## Goal

Give terminals user-visible defaults under new `terminal.*` keys, so the
session-local mouse toggle is no longer the only terminal preference:

- `terminal.mouseReportingEnabled` (boolean, default `true`): whether new
  sessions report mouse input to the running program.
- `terminal.cursorStyle` (string, default `"block"`): the cursor shape a new
  session starts with. Values `"block"`, `"bar"`, `"underline"`, `"hollow"`.

`terminal.scrollbackLines` is deferred: the pinned `libghostty-vt` 1.0.1
accepts `SCROLLBACK_MAX_LINES`/`MAX_BYTES` and reports them back, but enforces
neither on write nor on resize (100 written lines are all kept with a limit
of 5, 0, or 50), and it offers no trim API. A scrollback setting would be a
silent no-op, so it waits for engine support (see Blocked follow-up).

## Chosen design (agreed)

- **Mouse is a boolean, not an enum.** It mirrors the existing per-session flag
  exactly. The setting is only the default for new sessions; the
  `terminal.toggle-mouse-reporting` command keeps working as a per-session
  override that survives view handoff and restart, as today.
- **Cursor is a default; the program wins.** A program escape sequence that
  changes the shape still takes effect (standard terminal behavior). A forced
  override that ignores programs was rejected: it would surprise anyone whose
  program sets its own cursor (e.g. bar for insert mode).
- **All keys are global** (no view scope). Unlike `font.*`, there is no
  per-view use case; readers take the terminal scope only where the existing
  plumbing already does.
- Unknown `terminal.cursorStyle` strings fall back to the default, mirroring
  `editor_line_numbers` in `src/app/settings.rs`.

## Boundaries

- Native keys are defined once in `crates/knot-model/src/settings.rs`
  (`native_definitions`), read in `src/app/settings.rs`, and documented in
  `docs/settings.md`. This slice adds no scoped facts and no extension API.
- `knot-terminal` stays paint-and-VT only. If the engine accepts a default
  cursor style at construction, pass it in; otherwise apply the default at the
  view's paint site. No new engine state for user preferences.
- Session restart keeps its current shape (fresh `Terminal`, carried text
  replayed under a divider); it just reads the current scrollback value.

## Exclusions

- Titles, bells, and desktop notifications stay ignored (D041).
- `terminals.open` still starts the login shell; no command/arguments option
  (D192).
- No live resize of scrollback on existing sessions, no per-view overrides, no
  cursor-blink setting, no migration (Knot has no users; defaults just apply).

## Deferred decisions

- None. Key names, values, defaults, and precedence are settled above; the only
  open implementation question (engine default-style seam vs paint-site
  fallback) is answered by inspection in checkpoint 2.

## Checkpoint 1 — Mouse default ✅

Delivered:

- The `terminal.mouseReportingEnabled` and `terminal.cursorStyle` key
  constants and `native_definitions` entries with descriptions (the cursor
  reader and paint land in checkpoint 2).
- A typed `terminal_mouse_reporting` reader in `src/app/settings.rs`.
- `TerminalSession::new_with_options` reads the mouse default at construction
  instead of `true`; the toggle still flips it per session.
- A `docs/settings.md` Terminal section with the mouse row, and both keys
  declared in `knot.d.ts` (required by
  `every_native_key_is_declared_for_extensions`).

Implementation context: seams are `crates/knot-model/src/settings.rs`
(constants + `native_definitions`, mirroring `EDITOR_TAB_SIZE`),
`src/app/settings.rs` (reader mirroring `editor_tab_size`),
`src/app/terminal/session.rs` (`mouse_reporting_enabled` init), and
`docs/settings.md` (table row mirroring the `font.*` entries). Session
creation needs an `App` to resolve settings; `&mut Context<_>` coerces to
`&App` (as in `src/app/editor.rs`).

Validation: model test for the new defaults; session test that a `false`
mouse default starts a session with reporting off while the toggle still
flips it per session. `just check`, `cargo fmt`.

Review gate: keys, defaults, and toggle interplay in the real app (open a
terminal, flip the toggle, open a second terminal to see the default hold).

## Blocked follow-up — Scrollback length

`terminal.scrollbackLines` (integer, default `10000`, clamped, new sessions
and restarts only) is designed but not shipped: probes showed the pinned
engine accepts the limit and reports it back yet keeps all 100 written lines
with a limit of 5, 0, or 50, before and after resize, for both the lines and
bytes options, and it exposes no trim API. Ship it when a `libghostty-vt`
build enforces the limit: re-add the key, the clamped reader, the
`Terminal::new` argument, the settings-table row, and a session test that a
small value bounds a new session's history.

## Checkpoint 2 — Cursor-style default

Deliverables:

- The program's shape still wins; when it has not set one, the view paints
  the `terminal.cursorStyle` shape. Mapping covers all four engine styles
  (`Block`, `Bar`, `Underline`, `BlockHollow` from
  `crates/knot-terminal/src/render.rs`). The engine also exposes
  `GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_STYLE`, so prefer passing the default
  at construction if the program's escape still overrides it.
- The cursor row in `docs/settings.md`, and the typed reader in
  `src/app/settings.rs` (mirroring `editor_line_numbers`: unknown strings
  fall back to `"block"`).
- The `terminal.toggle-mouse-reporting` palette entry and docs stay as they
  are; no command changes in this slice.

Implementation context: first inspect whether libghostty exposes a default
cursor style at construction (the `Terminal::new` path in
`crates/knot-terminal/src/terminal.rs` and the `set_*` methods beside it). If
it does, pass the setting through at `start` in
`src/app/terminal/session.rs`. If it does not, apply it at the paint site in
`src/app/terminal/view.rs` where `snapshot.cursor` is drawn (beside the
`cursor.style` match), using the setting only as the initial shape. Views
already repaint on any settings change (`observe_global::<ApplicationSettings>`
in `view.rs`), so a cursor change applies live with no extra subscription.
Reader mirrors `editor_line_numbers`: unknown strings fall back to `"block"`.

Validation: view/paint tests that the default shape renders when the program
sets none, and that a program-set shape still renders (whichever seam the
inspection picks must show both). `just check`, `cargo fmt`.

Review gate: manual pass in the real app via `Try:` commands registered from a
personal-config `post-init.js` (one command per cursor value plus one that
runs a program which sets its own cursor, e.g. vim insert mode), confirming
the default shows and the program still overrides it. Remove the plan and the
roadmap 🚧 once the durable behavior is covered here and the settings table
owns the keys.

Landed ✅ (awaiting the gate pass):

- `Terminal::set_default_cursor_style` in `crates/knot-terminal/src/terminal.rs`
  passes the setting through `GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_STYLE` at
  construction; the program's escape still overrides it.
- `terminal_cursor_style` reader in `src/app/settings.rs` (unknown text means
  `"block"`, mirroring `editor_line_numbers`); `TerminalSession::start` applies
  it, so new sessions and restarts pick it up.
- The `terminal.cursorStyle` row in `docs/settings.md`; `knot.d.ts` already
  declared the key in checkpoint 1.
- Engine test (default renders, `\x1b[4 q` wins) and session test (setting
  flows into a new session, program wins, unknown falls back). `just check`
  passes; `Try:` commands are registered in the personal `post-init.js`.
