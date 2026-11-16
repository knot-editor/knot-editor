# Application log

Status: 🚧 checkpoints 1 and 2 done; checkpoint 3 is deferred to D117. No roadmap item; the D117 consumer is
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

### 1. Backend, file sink, and Open Log ✅

Approved. The subscriber, rotation, `eprintln!` migration, `knot::extension`
console routing, and the **Knot: Open Log** and **Knot: Reveal Log Folder**
commands are in `src/app/logging.rs` and `src/app/product/application_log.rs`;
behavior is documented in [development](../development.md#application-log).
The Dock-launch and rotation-by-size paths were not exercised by hand beyond
the `try.log` records reaching the file.

### 2. Font diagnostic ✅

Approved. `report_missing_family` in `src/app/typography.rs` logs one
`warn!(target: "knot::config", …)` per scope (`editor`, `terminal`) and family
that the platform replaces with its fallback; behavior is documented in
[development](../development.md#application-log). The test platform resolves
every font, so the automated test covers the once-per-scope logic, and the
platform lookup was verified by hand. Failed extension callbacks also log
their package, position, and stack (`knot-host` `engine/capsule.rs`).

### 3. D117 hook-up (deferred, not part of this plan)

D117's description names the `knot::config` target. This plan builds nothing for
it; the D117 plan raises a notice at each `knot::config` site.

## Deferred

- Whether the log directory belongs in the recovery listing or a diagnostics
  bundle for bug reports.
- Rotating while running, if long `debug` sessions prove to matter.
