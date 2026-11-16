# D045 — General terminal settings

Status: 🚧 approved, not started. Implements [D045](../roadmap.md) (Next up #1).

## Goal

Give terminals three user-visible defaults under new `terminal.*` keys, so the
session-local mouse toggle is no longer the only terminal preference:

- `terminal.mouseReportingEnabled` (boolean, default `true`): whether new
  sessions report mouse input to the running program.
- `terminal.cursorStyle` (string, default `"block"`): the cursor shape a new
  session starts with. Values `"block"`, `"bar"`, `"underline"`, `"hollow"`.
- `terminal.scrollbackLines` (integer, default `10000`): how many rows of
  history a new session keeps.

## Chosen design (agreed)

- **Mouse is a boolean, not an enum.** It mirrors the existing per-session flag
  exactly. The setting is only the default for new sessions; the
  `terminal.toggle-mouse-reporting` command keeps working as a per-session
  override that survives view handoff and restart, as today.
- **Cursor is a default; the program wins.** A program escape sequence that
  changes the shape still takes effect (standard terminal behavior). A forced
  override that ignores programs was rejected: it would surprise anyone whose
  program sets its own cursor (e.g. bar for insert mode).
- **Scrollback applies to new sessions and restarts only.** Existing sessions
  keep their buffer; there is no live trim/grow. `0` keeps no history. The
  value is clamped to a sane range at the read site.
- **All three keys are global** (no view scope). Unlike `font.*`, there is no
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

## Checkpoint 1 — Keys, mouse default, scrollback

Deliverables:

- The three key constants and `native_definitions` entries with descriptions;
  typed readers in `src/app/settings.rs` (`terminal_mouse_reporting`,
  `terminal_scrollback_lines` with clamping, cursor reader in checkpoint 2 or
  here).
- `TerminalSession::new_with_options` reads the mouse default and the
  scrollback length at construction instead of `true` and `SCROLLBACK_LINES`.
  Restart picks up the current scrollback value through the same path.
- Rows in `docs/settings.md` for the three keys (type, default, scope,
  one-line behavior each).

Implementation context: seams are `crates/knot-model/src/settings.rs`
(constants + `native_definitions`, mirroring `EDITOR_TAB_SIZE` /
`EDITOR_LINE_NUMBERS`), `src/app/settings.rs` (readers mirroring
`editor_tab_size` / `editor_line_numbers`, including the unknown-string
fallback), `src/app/terminal/session.rs` (`mouse_reporting_enabled` init and
the `Terminal::new(self.size, SCROLLBACK_LINES)` call in `start`), and
`docs/settings.md` (table rows mirroring the `font.*` entries). Session
creation needs an `App` to resolve settings; follow how `font_size` in
`src/app/terminal/view.rs` reads through `terminal_scope()`.

Validation: model tests for defaults, clamping, and unknown-string fallback;
session tests that a `false` mouse default starts a session with reporting
off while the toggle still flips it per session, and that a small scrollback
value bounds a new session's history. `just check`, `cargo fmt`.

Review gate: keys, defaults, and toggle interplay in the real app (open a
terminal, flip the toggle, open a second terminal to see the default hold).
Later work (cursor paint) builds on the readers without changing them.

## Checkpoint 2 — Cursor-style default

Deliverables:

- The program's shape still wins; when it has not set one, the view paints
  the `terminal.cursorStyle` shape. Mapping covers all four engine styles
  (`Block`, `Bar`, `Underline`, `BlockHollow` from
  `crates/knot-terminal/src/render.rs`).
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
