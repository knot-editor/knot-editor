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

### 1. Parsing and launch batches ✅
Done: `LaunchBatch` and `LaunchCommand` in `crates/knot-model/src/entry.rs`;
`parse_launch_arguments` accepts many paths, `--help`, `--version`, and `--`, and
rejects unknown flags (exit 2). The launch gate in `src/app/product.rs` and
`on_open_urls` carry batches. Until checkpoint 4 each batch item opens its own window.

### 2. Single-instance forwarding ✅
Done: `src/app/launch_socket.rs` owns the socket name, forward, bind (replacing a
stale file), and the listener thread that feeds the batch channel. `knot::run`
forwards before creating the product, otherwise binds. The gpui `Application` is still
created before the forward decision; checkpoint 3 restructures the entry point if it
needs to. The Windows named-pipe path is untested; framing and stale-socket tests are
Unix-only.

### 3. macOS cold start from the bundle ✅
Done: `src/app/launcher.rs` runs `open -a <bundle> --env KNOT_STARTED_BY_LAUNCHER=1 --args <absolute paths>`
when no primary answers and `current_exe` is inside a `.app`; the env var keeps the
app it starts from calling `open` again. `knot::run` takes a platform factory, so no
gpui application exists before the forward or cold-start decision.

### 4. Placement ✅
Done: `knot_model::launch::place` decides, as data, where a batch's folders and files
open; `src/app/product/launch.rs` resolves each path's kind, runs the placement for the
CLI, `on_open_urls`, and batches queued during startup (after session restore), and
opens each window's resources one after another. `ApplicationWindows` tracks activation
order. A window whose folder is still loading has no roots yet, so a batch arriving in
that instant can open the folder again.

### 5. Install command on macOS 🔍
Implemented in `src/app/shell_command.rs` and `src/app/product/commands.rs`; the link
targets the running executable, so `/Applications` and `~/Applications` both work.
Awaiting the review gate.
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
