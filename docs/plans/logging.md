# Application log

Status: 🚧 checkpoint 1 implemented, awaiting its review gate. No roadmap item; the D117 consumer is
recorded on [D117](../roadmap.md).

## Goal

A problem on a machine the developer cannot see, such as a configured
`font.family` that is not installed and silently falls back, leaves a durable
trace. Knot writes a log file that survives a Dock launch, and **Open Log**
shows it in the editor.

## Design

- **`tracing` is the one logging API.** Today there is no logging: about forty
  `eprintln!("[knot] …")` calls, and stderr is lost when Knot starts from the
  Dock or through `open -a`. `tracing` is only a transitive dependency of gpui
  today. It becomes a direct dependency of `knot`, `knot-host`, and
  `knot-model`, none of which gain a gpui dependency. Records from the `log`
  crate, which gpui and other dependencies use, reach the same subscriber
  through `tracing-subscriber`'s `tracing-log` feature, so a failure gpui reports (for example `failed to load
  font …`) lands in the file with no Knot code.
- **Targets name the subsystem.** `knot::<area>` (`knot::terminal`,
  `knot::fonts`, `knot::extensions`). Extension `console.*` becomes
  `knot::extension` at the matching level (`debug`, `info`, `warn`, `error`;
  `log` is `info`) with the package name, or the personal-config label, as a
  `package` field: a record's target is static, so it cannot carry the package.
- **`knot::config` marks user-fixable configuration problems.** A `warn!` whose
  cause is something the user can fix in settings or the install (a font
  family that is not installed) uses the target `knot::config`. D117 raises a
  notice at every such site; nothing here builds that bridge.
- **One file sink, a stderr sink when attached to a terminal.** The subscriber
  is installed at the top of `knot::run`, before the launch socket and argument
  errors, so early failures are recorded. Stderr output stays for
  `cargo run`. There is no in-memory ring buffer: viewing is the file.
- **The file lives in the platform log directory** (`~/Library/Logs/Knot` on
  macOS, the state directory elsewhere), as `knot.log`. Writing happens off the
  foreground through `tracing-appender`'s non-blocking writer; the guard is
  flushed in `host::quit_after_storage`, per the repository rule for durable
  background writers.
- **Rotation is by file count and size at startup.** At launch, when `knot.log`
  is at least 5 MB it becomes `knot.1.log`, each older file shifts up, and
  `knot.9.log` is deleted: at most 10 files and about 50 MB. Names keep the
  `.log` extension. A second Knot sharing the directory rotates only its own
  startup. A long `debug` session can grow a file past the cap until the next
  launch, which is accepted.
- **The level is `info` by default,** `KNOT_LOG` (an `EnvFilter` directive,
  such as `knot::fonts=debug,info`) overrides it. `info` records
  extension loads and failures; per-key or per-frame events are `trace` and
  stay off.
- **Open Log opens the file as a document** through the ordinary Open path,
  so it gets a tab, search, and the log file's own watching if the document
  layer watches files. **Reveal Log Folder** shows the directory in the
  platform file manager. Both are native commands.

## Exclusions

- A log viewer, filtering UI, or structured log browsing.
- An extension-facing logging API beyond the existing `console`.
- Telemetry, crash reporting, or uploading logs.
- A bridge from a log record to a D117 notice (see D117).
- Per-document or per-window files.

## Checkpoints

### 1. Backend, file sink, and Open Log 🚧

✅ Implemented: backend and rotation (`src/app/logging.rs`), the `eprintln!`
migration, the `knot::extension` console routing, **Knot: Open Log** and
**Knot: Reveal Log Folder**, tests, and documentation in
[development](../development.md#application-log). Remaining: the review gate.

- A `knot::logging` module installs the subscriber (file with rotation, stderr
  when attached to a terminal, `KNOT_LOG` filter, `tracing-log` bridge), and
  returns the flush guard. Rotation is a pure function over a directory so it
  tests without the subscriber.
- Every `eprintln!("[knot] …")` in `src` and `crates` becomes a `tracing`
  macro at an appropriate level and target, except the user-facing
  command-line output of `--help` and `--version` (stdout) and the argument
  error before logging starts. Extension `console.*` is routed as designed.
- **Open Log** and **Reveal Log Folder** commands.
- Documentation: a logging section in the relevant architecture reference
  (startup and ownership), `docs/settings.md` or `docs/development.md` for
  `KNOT_LOG` and the file location, and the extension guide's `console`
  paragraph.
- Validation: rotation (shift, cap on count, missing numbers, an unreadable
  directory), `KNOT_LOG` parsing, a file written through the real subscriber
  in a temp directory, extension `console.warn` landing in the file with its
  package target, Open Log opening a document whose text is the file.
- **Implementation context.** Add `crates/knot-…`-independent logging to
  `src/app/logging.rs` (declared in `src/app/mod.rs`); install it at the top
  of `run` in `src/app/mod.rs` (the first lines hold the `eprintln!` at
  `mod.rs:155`). The directory comes from `directories::ProjectDirs` as in
  `StateDatabase::user_path` (`src/app/record_store.rs:25`); the log
  directory is `ProjectDirs::state_dir()` falling back to
  `data_local_dir()/logs`, with macOS using `~/Library/Logs/Knot`. Flush the
  guard from `extensions::host::quit_after_storage`. The `eprintln!` sites are
  in `src/app/{mod,recovery,launch_socket}.rs`,
  `src/app/product/launch.rs`, `src/app/extensions/host.rs`, and
  `crates/knot-host/src/engine/{capsule,marshal}.rs`; extension `console` is in
  `crates/knot-host/src/js/` and its native handler. Mirror
  `src/app/product/show_keybindings.rs` and its `commands.rs` entry
  (`commands.rs:639`) for declaring native commands, and the Open path in
  `src/app/product/open.rs` for opening the file.
- **Review gate:** run the app launched from the Dock, trigger a warning, run
  Open Log, confirm the file, the rotation, and that stderr still shows
  output under `cargo run`.

### 2. Font diagnostic

- When `font.family` resolves for a scope (startup and each change in the
  settings store), check whether the platform loaded the primary family. When
  it did not, log
  `warn!(target: "knot::config", …)` that names the setting, the scope, the
  family, and the fallback used. The check runs on the foreground once per
  distinct family, not per frame.
- Validation: a test with an uninstalled family produces exactly one record
  with the setting, family, and scope; changing to an installed family logs
  nothing; the terminal scope is checked independently of the editor.
- **Implementation context.** gpui's `TextSystem::resolve_font` returns a
  fallback id silently and calls `log::warn!("failed to load font …")` itself;
  the check uses `window.text_system()` or the application's text system from
  the existing view code in `src/app/typography.rs` and
  `src/app/terminal/layout.rs`, and `knot_model::typography::FontSettings` for
  the resolved settings. Read `FontSettings::resolve`
  (`crates/knot-model/src/typography.rs`) for the scopes; the terminal scope
  helper is `terminal_scope()` in `src/app/terminal/view.rs`. Subscribe to
  setting changes the way `TerminalView`'s `_settings_subscription` does.
- **Review gate:** configure a nonexistent family in `pre-init.js`, confirm the
  warning in Open Log, then a real one and confirm silence. Use a `Try:`
  command from `post-init.js` per the repository rule.

### 3. D117 hook-up (deferred, not part of this plan)

D117's description names the `knot::config` target. This plan builds nothing for
it; the D117 plan raises a notice at each `knot::config` site.

## Deferred

- Whether the log directory belongs in the recovery listing or a diagnostics
  bundle for bug reports.
- Rotating while running, if long `debug` sessions prove to matter.
