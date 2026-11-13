# D063 — Migrate the terminal engine to libghostty

Status: 🚧 planned. Source: [roadmap D063](../roadmap.md). Covers
[D128](../roadmap.md) (working directory from OSC 7). Current behavior:
[terminal sessions and views](../architecture/workbench-and-lifecycle.md#terminal-sessions-and-views);
current rationale: [terminal decision](../decisions/workbench-and-lifecycle.md#terminal).

## Outcome

Terminals run on libghostty-vt 1.0 instead of `alacritty_terminal`, and
existing behavior is unchanged: the same sessions, attachment, restart,
selection, links, mouse reporting, focus reports, paste, theme colors, and
typography. Keys and the mouse are encoded by libghostty, so apps that request
the Kitty keyboard protocol or modifyOtherKeys get it. A restored terminal and
a terminal opened from a terminal follow the shell's OSC 7 directory, which
also works on Windows and over remote shells that report it.

No contributor, CI job, or release build needs Zig.

## Why now

libghostty-vt 1.0 (November 2026) removes the reasons D063 was deferred: it
has a stable, semver C ABI, which CI checks for compatibility. Building it
ReleaseFast in Zig also removes the debug-build parsing cost. It covers the
protocols D041 and D128 wait for: a render-state API with dirty rows,
mode-aware key and mouse encoders (legacy, Kitty keyboard, modifyOtherKeys,
SGR, URxvt, SGR-pixels), and callbacks for PTY replies, title, bell, OSC 7,
OSC 52, notifications, and semantic prompts. Docs:
[lib.ghostty.org/vt/1.0.1](https://lib.ghostty.org/vt/1.0.1/),
[packaging](https://github.com/ghostty-org/ghostty/blob/main/PACKAGING-LIBGHOSTTY.md).

## Design

### Prebuilt libraries, no Zig in ordinary builds

- A manually dispatched workflow, `.github/workflows/libghostty.yml`, installs
  the Zig version that libghostty pins (0.17.x for 1.0). It downloads the
  official source and dependency tarballs for a pinned libghostty-vt version
  and checks their minisign signatures. It builds `-Doptimize=ReleaseFast
  -Dcpu=baseline` for every target we build (see 1a for the runners). Then it publishes one archive per target (static library, headers,
  license) with its sha256 to a GitHub release in this repository, named
  `libghostty-vt-<version>`. Kitty graphics are compiled out until D041 picks
  them up.
- Targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`,
  `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
  `x86_64-pc-windows-msvc`.
- `crates/knot-terminal/build.rs` holds the release URL and the sha256 for each
  target. It downloads the archive once, caches it under the Cargo target
  directory, verifies it, and links it statically on every platform,
  Windows included. `KNOT_LIBGHOSTTY_DIR`
  points at a locally built install tree instead, for offline builds or to
  try a new version.
- The bindgen output from `ghostty/vt.h` is checked in, so building needs
  neither bindgen nor libclang. A `just` recipe regenerates it from the
  downloaded headers.
- Upgrading libghostty means rerunning the workflow and updating the version,
  checksums, and bindings in one commit. The stable ABI makes this rare.

### Ownership

- New gpui-free crate `crates/knot-terminal`:
  - the raw FFI and a small safe wrapper over the parts we use: terminal,
    render state, key and mouse encoders, grid refs, selection, and callbacks;
  - the PTY, from `portable-pty` (Unix PTYs, Windows ConPTY), and its reader
    thread;
  - the engine, which feeds PTY output to the terminal and turns callbacks into
    ordered events;
  - plain tests that feed bytes and read the screen back.
- The libghostty terminal is not thread-safe; it lives behind a mutex, as
  Alacritty's `Term` does now. The reader thread calls `vt_write`; the
  callbacks that fire during it (PTY replies, OSC 7, bell, title) push into
  the session's existing process-generation-scoped event queue, and repaint
  wakeups coalesce as they do today. The foreground holds the lock only for
  `render_state_begin_update` and paints from the render state afterwards.
- `TerminalSession` (`src/app/terminal/session.rs`) keeps ownership of the
  session, generations, restart, the attachment lease, status, and the event
  delivery contract. `TerminalView` keeps geometry, drawing, gestures, and
  focus. The PTY ownership and the single-attachment contract do not change.
- Product events stay as today: no title, bell, notification, or graphics
  behavior, and OSC 52 read and write are refused. D041 owns them.

### Encoding

libghostty's key and mouse encoders, configured from the terminal's modes,
replace Knot's wire-format code in `input.rs` and `mouse.rs`. Knot keeps the
decisions about which key reaches the terminal (command and keymap precedence)
and who owns a mouse gesture (reporting or local selection, the reporting
toggle, Shift, balanced releases). It translates gpui events into libghostty
key and mouse events.

### Working directory (D128)

The session records the OSC 7/9/1337 directory from the `PWD_CHANGED`
callback. It accepts local `file://` URIs and paths only. A directory from
another host, such as an SSH session, is not a local path and is ignored.
`current_directory` prefers that directory, falls back to querying the shell
process (`process.rs`) when the shell never reported one, and then to the
start directory. Shell integration that emits OSC 7 stays D039; zsh on macOS
and fish emit it by default only in some setups, so the fallback stays.

## Exclusions

- D041 product events and protocols that need a product contract: OSC 52,
  titles, bells, notifications, Kitty graphics, semantic prompts.
- Terminal preferences (D045), including scrollback length; keep the current
  10,000 lines.
- Using the system libghostty through pkg-config. Revisit when distributions
  and Homebrew package it.
- Search inside terminals.

## Checkpoints

Mouse selection and links are off between 3a and 3b; nobody uses the product
yet, and the gap keeps each step reviewable.

### 1a. Prebuilt library workflow and release ✅

[`libghostty.yml`](../../.github/workflows/libghostty.yml) published the
[`libghostty-vt-1.0.1`](https://github.com/knot-editor/knot-editor/releases/tag/libghostty-vt-1.0.1)
release with Zig 0.17.1 (1.0.1 requires 0.17.x). Darwin targets build on
macOS, because libghostty's archive step runs Apple's tools, and MSVC on
Windows; Linux cross-compiles both architectures. Darwin targets macOS 13.
The static archive needs no extra link libraries. The Windows archive also
carries `ghostty-vt.dll` and its import library for 1b's linking decision.
`-Dcpu=baseline` could be raised later for a faster build.

### 1b. Linked binding ✅

[`crates/knot-terminal`](../../crates/knot-terminal) links the release
statically on every platform, Windows included (the static `.lib` links under
MSVC; the archive's DLL and import library go unused and the next release can
drop them). `src/ffi.rs` is checked in and regenerated by
`just libghostty-bindings`, which parses the headers as C23 so enums are
`c_int` on every target. The smoke test passed on Linux, macOS, and Windows in
CI run 43517959459. Builds and upgrades:
[development](../development.md#libghostty-vt).

### 2a. Screen wrapper ✅

`Terminal` ([`terminal.rs`](../../crates/knot-terminal/src/terminal.rs)) and
`RenderState` ([`render.rs`](../../crates/knot-terminal/src/render.rs)) are
the surface; `RenderState::update` holds the terminal's lock only for the
begin phase. Rows and cells are read through lending `next()` calls that
reuse one cells handle and text buffer per frame; dirty state accumulates
until `RenderState::clean`. Inverse and faint stay with the painter.
[`tests/screen.rs`](../../crates/knot-terminal/tests/screen.rs) replaced the
1b smoke test.

### 2b. Modes, replies, and grid queries ✅

`Terminal::set_event_handler`
([`events.rs`](../../crates/knot-terminal/src/events.rs)) delivers replies,
title, bell, and working-directory changes in stream order. Size reports
come from the last resize. The selection, text, and hyperlink queries are in
[`grid.rs`](../../crates/knot-terminal/src/grid.rs). The selection covers only
plain drag selection; the terminal tracks it, and frames carry each row's
selected columns. Tests are in
[`tests/queries.rs`](../../crates/knot-terminal/tests/queries.rs).

Consequences for later checkpoints:
- OSC 52 has no callback, so libghostty ignores it as Alacritty does. A read
  callback would also turn on Kitty paste events (mode 5522).
- libghostty exposes OSC 8 URIs but not `id`s. In 3b, links are grouped by
  URI, so links with the same URI and different `id`s become one.
- `Terminal::text` returns logical lines, so restart replay needs no wrap
  flags.
- DA1 answers `CSI ? 62 ; 22 c` instead of Alacritty's `CSI ? 6 c`. In 3a,
  `session_tests.rs` takes libghostty's bytes.
- libghostty answers the Kitty keyboard query. Until checkpoint 4, programs
  that turn the protocol on still receive legacy keys.

### 2c. PTY engine ✅

`Pty` ([`pty.rs`](../../crates/knot-terminal/src/pty.rs)) runs the program
on portable-pty with reader, writer, and waiter threads. Tests are in
[`tests/pty.rs`](../../crates/knot-terminal/tests/pty.rs); only macOS ran
locally, so CI first compiles the Linux and Windows paths.

Consequences for later checkpoints:
- The engine owns the terminal's event handler and writes replies to the
  PTY itself, in stream order. In 3a, the session queue carries only
  `PtyEvent`s (title, bell, directory, exit), and generation checks no
  longer guard replies.
- The exit is reported after the output drains, or after 200 ms, since a
  background job or ConPTY can keep output open. Draining is always on.
- The default shell matches Alacritty's. An invalid working directory
  falls back to home.
- Writes go through a duplicated descriptor, because portable-pty's writer
  sends a newline and EOF when dropped.
- After shutdown, the reader keeps a duplicated master open until the
  program exits, so a program that ignores SIGHUP gets no EOF.

### 3a. Session and drawing on libghostty

Deliverables:
- `TerminalSession` runs on the engine: events, process generations,
  coalesced wakeups, restart with the old screen replayed above the divider,
  focus reports, paste, and the process query for the directory.
- `TerminalView` paints from the render state: cells, styles, colors, cursor
  shape and visibility, with D059 typography and punctuation-run ligatures
  unchanged. `colors.rs` configures the palette instead of answering color
  queries.
- Keyboard and mouse reporting keep Knot's own wire formats, reading modes
  from the engine. Mouse selection and links are off.

Validation: ported session, view, and product terminal tests for lifecycle,
restart, attachment, focus, paste, and drawing; `just check`. Manual: `try.*`
commands that open terminals running a TUI (vim or htop), a long output, a
restart, and a color-query script.

Review gate: daily shell and TUI use, minus selection and links.

**Implementation context:** `src/app/terminal/{session,view,colors,layout,input,mouse}.rs`,
`session_tests.rs`, `view/tests.rs`, `src/app/product/tests/terminal.rs`.
Tests that build `Term` with `VoidListener` and `Processor` become
`knot-terminal` terminals fed bytes. `alacritty_terminal` may remain only
where selection and links still reference it.

### 3b. Selection, copy, and links

Deliverables: local mouse selection and copy over tracked grid refs, the
scroll interaction during selection, OSC 8 links and HTTP(S) text detection
over displayed cells with release-time revalidation, and the
gesture-ownership rules between reporting and selection.

Validation: ported `links.rs` and `mouse_tests.rs` selection cases, and the
product test for plain drag copy. Manual: `try.*` commands with OSC 8 output,
URLs across soft wraps, and `scripts/terminal-mouse-fixture.py`.

Review gate: selection and links in daily use.

**Implementation context:** `src/app/terminal/{links,mouse,view}.rs`. Link
detection keeps its algorithm and reads cells and hyperlinks from the engine.

### 3c. Remove Alacritty and update the docs

Deliverables: `alacritty_terminal` leaves the workspace and the remaining
tests are ported. The
[terminal sessions](../architecture/workbench-and-lifecycle.md#terminal-sessions-and-views)
reference, the crate list in `docs/architecture.md`, the repository map in
`AGENTS.md`, and the [terminal decision](../decisions/workbench-and-lifecycle.md#terminal)
with its index line describe libghostty.

Validation: `just check`; `cargo tree` shows no `alacritty_terminal`.

Review gate: the documentation.

**Implementation context:** `Cargo.toml` (workspace and root dependencies),
`src/app/terminal/mod.rs` module docs, `docs/decisions.md`.

### 4. libghostty encoders

Deliverables:
- `input.rs` translates gpui keystrokes into libghostty key events (action,
  key, mods, consumed mods, UTF-8, unshifted codepoint, composing) and encodes
  them with an encoder set from the terminal. macOS Option-as-Alt keeps the
  current behavior.
- `mouse.rs` keeps gesture ownership and encodes through the mouse encoder
  with the session's pixel geometry; the reporting toggle and balanced
  releases are unchanged.
- Knot's own escape-sequence tables are removed.

Validation: the existing input and mouse tests assert the same bytes for
legacy modes; new tests cover Kitty keyboard flags and modifyOtherKeys
requested by the app. Manual: `try.*` commands running an app that enables
the Kitty keyboard protocol, plus the mouse fixture.

Review gate: typing, shortcuts, and mouse reporting in shells and TUIs.

**Implementation context:** `src/app/terminal/input.rs` and `mouse.rs` with
`mouse_tests.rs`; the key and mouse encoder wrappers go in `knot-terminal`.
Named keys that stay global on Windows and Linux (the terminal decision) are
routing, not encoding, and stay in `view.rs`.

### 5. Working directory from OSC 7 (D128)

Deliverables:
- The session records the reported directory per process generation.
  `current_directory` prefers it and falls back to the process query, then
  the start directory.
- Remove D128 from the roadmap, update D041's description now that
  libghostty covers its protocols, and mark D063 complete.

Validation: engine tests for `file://` parsing, remote-host rejection, and
percent-encoding; a session test where a shell's `printf '\e]7;…'` changes the
directory reported to layout capture. Manual: a `try.*` command that opens a
terminal from a terminal after `cd` in a shell that emits OSC 7.

Review gate: restoration and the new-terminal directory in daily use.

**Implementation context:** `TerminalSession::current_directory` in
`src/app/terminal/session.rs`, `src/app/terminal/process.rs`, and the rule in
`crates/knot-model/src/terminal_directory.rs` (unchanged). Update the
restored-terminal paragraph of the terminal decision and the working
directory paragraph of the terminal sessions reference.
