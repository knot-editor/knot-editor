# D164 — Command-line launcher

Source: [D164](../roadmap.md) (before D165).
Scope: `knot <file>...` forwards paths to the running instance or starts one,
plus a palette command that installs the `PATH` entry. Mirrors VS Code's `code`
CLI and its `Shell Command: Install … in PATH` behavior; no new inventions.

## Agreed design

- **Transport:** Unix socket in the user data-local dir (same `directories::ProjectDirs`
  pattern as `RecoveryStore::user_path`). First process to bind is primary; later
  invocations send newline-delimited file-URI strings and exit 0. Stale socket:
  connect-attempt first, rebind when nobody listens.
- **Path semantics:** the *client* canonicalizes argv paths against its own cwd
  to absolute `file:` URIs (reuse `OpenRequest::from_path` normalization at the
  boundary); the server re-validates each line with `OpenRequest::from_url` and
  drops invalid lines with a `[knot]` stderr note. Never send raw relative paths.
- **Placement:** unchanged by this item. Each forwarded path becomes one launch
  request through the existing queue (`dispatch_launch_request` → one window per
  request). D165 owns the reuse-vs-new-window rule; `--new-window` is parsed and
  carried now so D165 can honor it as an explicit override later.
- **CLI surface:** `knot [--new-window] [--version] [--help] [path ...]`.
  `--version`/`--help` never touch the socket. Usage errors exit 2 (as today).
  Out of scope: `--wait`, `--reuse-window`, `--goto`/`--diff`, stdin (`-`).
- **Install:** palette command `Shell Command: Install 'knot' command in PATH`
  (plus matching uninstall, as VS Code has), creating/removing a
  `/usr/local/bin/knot` symlink to `std::env::current_exe`. Privilege or I/O
  failure reports through the native warning prompt with the manual
  `sudo ln -s …` fallback spelled out; success shows only its effect, per the
  workbench lifecycle convention. Works the same for `Knot.app` and `cargo run`
  builds since it always points at the running executable.

## Explicit exclusions

- D165 placement rule (files in an open project reusing that window).
- Remote/SSH forwarding, `--wait`/git-editor integration, file-URL schemes beyond
  what `OpenRequest::from_url` already accepts.

## Checkpoints

### 1. CLI parsing: many paths, `--new-window`, `--help`
Deliverable: `parse_launch_arguments` takes `knot [--new-window] [path ...]`,
`--help` text, exit-2 usage errors; unit tests in `entry.rs`.
Review gate: run `knot a b`, `knot --help`, `knot --new-window x` against the
built binary and eyeball output/exit codes.
Implementation context: extend `src/app/entry.rs` (`parse_launch_arguments`,
`is_version_request`, `OpenRequest::from_path`); mirror the existing
`#[cfg(test)]` cases there. No `product/` changes in this checkpoint.

### 2. Single-instance forwarding
Deliverable: second `knot <files>` sends URIs to the running primary and exits;
with no primary it starts normally. Primary feeds received URIs into the same
`open_requests` channel as `on_open_urls`, so macOS file-open and CLI share one
path. Automated tests for client/server framing and stale-socket takeover.
Review gate: start one instance, run `knot file-a file-b` from another cwd in a
terminal, confirm one window per file opens in the first instance and the second
process exits 0.
Implementation context: new small module under `src/app/` (e.g.
`src/app/launch_socket.rs`, declared in `src/app/mod.rs`) owning bind/connect/
send; hook into `src/app/mod.rs::run` (primary check before `product::run`) and
`src/app/product/launch.rs::run` (listener task sending into `open_requests`);
socket path beside `RecoveryStore::user_path` / `StateDatabase::user_path`.

### 3. Install/uninstall `PATH` commands
Deliverable: palette `Shell Command: Install 'knot' command in PATH` and
`Shell Command: Uninstall 'knot' command from PATH`; failure → native warning
prompt with manual fallback; plus `Try:` entries in a personal-config
`post-init.js` for the manual review pass.
Review gate: run install from a dev build, `which knot`, launch a file from a
fresh shell, then uninstall and confirm removal.
Implementation context: register beside other product commands in
`src/app/product/commands.rs`; symlink `std::env::current_exe` →
`/usr/local/bin/knot`; warning prompt follows the lifecycle doc's native-prompt
convention.

### 4. Close-out
`just check` clean (warnings included), `cargo fmt`, commit per checkpoint.
Resolving this plan answers D164's "Reconsider" clause; D165 then refines
placement using the carried `--new-window` flag.

## Deferred decisions

- Whether `--new-window` should also skip session-restore grouping once D165
  lands (default: explicit flag always wins).
- `/usr/local/bin` fallback to `~/.local/bin` when privileged install fails
  non-interactively — proposed: no, keep the manual-fallback instruction only.
