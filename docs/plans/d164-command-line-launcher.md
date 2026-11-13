# D164 — Command-line launcher

Source: [D164](../roadmap.md), which now also covers the placement rule that was
D165.

Outcome: `knot <path>...` from a terminal opens files and folders in the running
Knot, or starts Knot when none runs, and places each path in the window that
already holds its project. A palette command puts `knot` on the `PATH` once per
machine. This works on macOS, Windows, and Linux, and `cargo run` keeps working as
it does today.

Related docs: [product application launch](../architecture/workbench-and-lifecycle.md#product-application-launch),
[documents and persistence](../architecture/documents-and-persistence.md),
`packaging/homebrew/knot@nightly.rb.in`.

## Agreed design

### CLI surface

- `knot [--version] [--help] [path ...]`. `--version` and `--help` print, exit 0,
  and never touch the socket. Usage errors (unknown flag) print to stderr and exit 2.
- Every path is resolved against the client's cwd to an absolute, lexically
  normalized `file:` URI through `OpenRequest::from_path`. Raw relative paths are
  never sent. A path that does not exist is a new file, as today.
- One invocation is one **launch batch**. A macOS file-open event (`on_open_urls`)
  is also one batch, so Finder and the CLI share every rule below.
- Out of scope: `--new-window`, `--wait`, `--reuse-window`, `--goto`, `--diff`,
  stdin (`-`).

### Single instance, no daemon

The running editor is the only long-lived process. It owns the socket, which
goes away when it quits.

1. **An editor is already running:** the client connects, sends the batch, and
   exits 0. The same on every platform.
2. **Nothing runs and the executable is inside a `.app` bundle** (the brew link or
   the installed symlink): the client runs
   `open -a <bundle> --args <absolute paths>` and exits 0. LaunchServices starts
   Knot as a normal app, not a child of the shell, as VS Code's `code` does.
3. **Nothing runs and the executable is loose** (`cargo run`, Linux, Windows):
   the process becomes the primary and runs in the foreground exactly as today:
   logs, Ctrl-C, terminal attached.

No fork, no self-respawn. A `cargo run` started while another Knot runs forwards
to it; one socket serves every build.

**Transport:** the `interprocess` crate's local sockets. That's a Unix domain socket
in the user data-local dir on macOS and Linux (`ProjectDirs` as
`StateDatabase::user_path`), and on Windows a named pipe whose name includes the
user name, since pipe names are machine-global. Framing: one message per batch,
one URI per line, ended by connection close. The primary re-validates each line
with `OpenRequest::from_url` and drops invalid lines with a `[knot]` stderr note.
Stale socket: if connect fails, remove the file and bind. Two cold starts at the
same instant are not handled.

The primary binds before creating the product so that a launch during startup
still finds it. A plain listener thread feeds received batches into the same
channel as `on_open_urls`. No tokio runtime is needed. Batches that arrive before
startup completes wait in the launch gate, like today's queued requests.

### Placement (formerly D165)

Placement runs per batch, after the launch gate opens. Windows restored by session
restore are open windows for these rules.

1. **Folders first:** focus the window that already has the folder as a workspace
   root. Otherwise open a new window with the folder as its workspace, through the
   existing `OpenResource::Directory` path (`start_open_operation` →
   `open_workspace`). `knot .` is the main use case.
2. **Each file** goes to a window whose workspace roots contain it, including
   windows step 1 just opened. So `knot . src/main.rs` puts `main.rs` in the new
   folder window. If several windows match, it goes to the most recently focused one.
3. **The files left over** go together into **one** new window.
4. **Focus:** the last window the batch created, otherwise the window that received
   the first file. The app activates.

Example: `knot ~/proj/a.rs ~/notes.md` with `~/proj` open → `a.rs` opens in the
`proj` window, `notes.md` in a new window, which gets focus.

What a placement means is a pure `knot-model` function over plain data: each
window's roots and focus order, and each batch item's resolved kind. It returns
targets as data. The application resolves file versus directory through the
filesystem provider first, then runs the returned opens.

### `PATH` install

Palette commands `Shell Command: Install 'knot' command in PATH` and
`Shell Command: Uninstall 'knot' command from PATH`, on every platform. Success
shows only its effect. Failure reports through the native warning prompt with the
manual step spelled out, per the
[lifecycle convention](../architecture/workbench-and-lifecycle.md#product-application-launch).

- **macOS:** symlink `/usr/local/bin/knot` → the running executable, created through
  `osascript -e 'do shell script "…" with administrator privileges'`, which shows the
  system password prompt and creates `/usr/local/bin` if missing. The command refuses
  when `current_exe` is under App Translocation. Brew users never need the command:
  the cask's `binary` stanza already links `knot` and relinks it on upgrade.
- **Windows:** add the folder containing `knot.exe` to the user `Path`
  (`HKCU\Environment`) and broadcast `WM_SETTINGCHANGE`. No elevation; new terminals
  pick it up. Uninstall removes exactly that entry.
- **Linux:** symlink `~/.local/bin/knot` → the running executable. No elevation.

Every target is the install location of the running executable, so the link stays
valid across upgrades as long as the app stays where it is.

## Explicit exclusions

- Remote/SSH forwarding, `--wait` and git-editor integration, URL schemes beyond
  what `OpenRequest::from_url` accepts.
- The console window that the console-subsystem `knot.exe` shows when started
  from Explorer ([D188](../roadmap.md)), and Windows and Linux installers
  ([D189, D190](../roadmap.md)).
- A separate socket per build; detaching loose binaries.

## Checkpoints

### 1. Parsing and launch batches
Deliverable: `parse_launch_arguments` accepts many paths and `--help`, rejects
unknown flags with exit 2, and returns a batch. `product::run`, the launch gate, and
`on_open_urls` carry batches. Until checkpoint 4, each batch item still opens its own
window, so behavior is unchanged except that many paths are accepted.
Validation: unit tests in `crates/knot-model/src/entry.rs` beside the existing cases.
Update the `src/app/product/tests/observation.rs` caller and the startup product
tests that dispatch launch requests.
Review gate: `cargo run -- a b`, `cargo run -- --help`, `cargo run -- --bogus`; check
the output, the windows, and exit codes.
Implementation context: `crates/knot-model/src/entry.rs` (`OpenRequest::from_path`,
`parse_launch_arguments`, `is_version_request`, possibly a `LaunchBatch` type);
`src/app/mod.rs::run`; `src/app/product/launch.rs::run` (`open_requests` channel,
first-request dispatch, `dispatch_launch_request`); `LaunchGate` in
`src/app/product.rs` (`Pending(Vec<Option<OpenRequest>>)` becomes batches, `None`
stays a reopen); the gate drain in `finish_product_startup`.

### 2. Single-instance forwarding
Deliverable: a new `src/app/launch_socket.rs` (declared in `src/app/mod.rs`) owns
the socket path, bind, connect, send, and the listener thread. `knot::run` tries
to forward before creating the product, and otherwise binds and passes the listener
into `product::run`, whose listener thread feeds the batch channel. Loose binaries
cold-start in the foreground.
Validation: tests for framing round trip, invalid lines dropped, and stale-socket
takeover (bind, drop without unlinking, rebind), using a tempdir path.
Review gate: start `cargo run`. From another terminal and cwd run
`target/debug/knot file-a file-b` and `target/debug/knot .`. The second process exits
0 and the first instance opens the paths. Quit, then run it again and confirm it
starts normally over the stale socket.
Implementation context: add `interprocess` to the root `Cargo.toml`; socket path
beside `StateDatabase::user_path` in `src/app/record_store.rs`; the entry order in
`src/app/mod.rs::run` (version/help → parse → forward or bind →
`product::run`). Note that `crates/knot-app/src/main.rs` creates the gpui platform
before `knot::run`; that's fine here, but checkpoint 3 needs the forward and cold-start
decision before platform creation, so restructure the entry point now if it's
simpler (for example, `knot::run` taking a platform factory).

### 3. macOS cold start from the bundle
Deliverable: when no primary answers and `current_exe` is inside a `.app`, the client
runs `open -a <bundle> --args <absolute paths>` and exits 0. This happens before any
gpui platform exists.
Validation: unit tests for bundle detection from an executable path and for the
`open` argument list.
Review gate: with Knot not running, `knot ~/some/dir` and `knot a.txt` through the
brew link (or a symlink to a locally built `Knot.app`). The shell returns at once,
Knot starts as a normal app, and the paths open. Then the same commands with Knot
running.
Implementation context: `src/app/launch_socket.rs` (or a sibling `launcher.rs`);
`crates/knot-app/src/main.rs` entry order; `packaging/macos/Info.plist`; the
`just` bundle recipe in `justfile`.

### 4. Placement
Deliverable: the placement function in `knot-model` and its wiring into batch
dispatch for both the CLI and `on_open_urls`, including batches drained after session
restore. Windows track focus order if they don't already.
Validation: plain tests for each rule (folder reuse versus new window, file into a
matching root, file into a window the same batch created, several matches → most
recently focused, leftovers grouped into one window, focus choice). Product tests
through the driver (`TestWindow::roots`, `TestApplication::windows`) for a forwarded
batch, a restored window receiving a file, and `knot .` opening a workspace.
Review gate: with a project window open, `knot <file in project> ~/elsewhere.md`,
`knot <project dir>`, `knot . src/x.rs` from a new directory. Then quit with the
project open, relaunch with `knot <file in project>`, and confirm it lands in the
restored window.
Implementation context: a new `crates/knot-model/src/launch.rs` (or beside
`entry.rs`); workspace roots from `crates/knot-model/src/workspace.rs`; window registry
`ApplicationWindows` and `windows(cx)` in `src/app/product.rs` (opening order only today;
activation is observed at `src/app/product.rs:863`); open path
`start_open_operation` / `open_workspace` in `src/app/product/open.rs`;
`open_product_window` in `src/app/product/launch.rs`.

### 5. Install command on macOS
Deliverable: install and uninstall palette commands as designed. On other
platforms they come in checkpoint 6.
Validation: tests for the built shell command and for translocation refusal.
Uninstall removes only a link that points at a Knot executable.
Review gate: from a locally built `Knot.app`, run install, enter the password, run
`which knot` in a new shell, `knot .`, then uninstall and confirm removal. Cancel the
password prompt and confirm the warning gives the manual `sudo ln -s …`. `Try:` entries
in personal-config `post-init.js` run both commands.
Implementation context: register beside other product commands in
`src/app/product/commands.rs`; warning prompt as Save and Open failures do
(`src/app/product/open.rs`).

### 6. Install command on Windows and Linux
Deliverable: the same commands on Windows (user `Path` entry) and Linux
(`~/.local/bin` symlink).
Validation: pure tests for adding and removing a `Path` entry (idempotent, keeps the
other entries); a Linux symlink test in a tempdir.
Review gate: on each platform, install, open a new terminal, `knot .`, uninstall.
Implementation context: same module as checkpoint 5, split by `cfg`; on Windows a
registry crate (`winreg`) and `windows-sys` for the broadcast.

### 7. Close-out
Update [product application launch](../architecture/workbench-and-lifecycle.md#product-application-launch)
with forwarding, batches, and placement. Record the single-instance and
cold-start choice (`open -a` versus fork or detached respawn) in
`docs/decisions/platform.md` and its index line. Mention in `docs/development.md` that
`cargo run` forwards to a running Knot. Remove D164 from the roadmap and delete this
plan. `just check` clean, `cargo fmt`, commit per checkpoint.

## Deferred decisions

- `--new-window` as an explicit override of placement: add it when daily use
  wants one.
- Whether the Windows installer (D189) should replace the install command with
  an "Add to PATH" checkbox, as VS Code does.
