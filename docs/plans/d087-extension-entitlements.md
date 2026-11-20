# D087 — Extension entitlements

Status: 🚧 checkpoint 1 done; checkpoints 2 and 3 not started. Source: [roadmap D087](../roadmap.md), first
item of the [version control plan](vcs.md), whose `@knot/git` package is the
first consumer of process spawning.

## Outcome

A package declares in `knot.jsonc` the entitlements it needs, starting with
`process`. When Knot starts and an installed package has an entitlement the user
has not decided on yet, because the package is new or an update added the
entitlement, Knot asks before loading any package. A package the user allowed
loads and can spawn processes. A package the user denied does not load. The user
reviews and changes decisions later through a native command. Extensions get a
process API: run a command and collect its output, or spawn one and stream it.

This gives consent and visibility, not a sandbox. Runtime isolation is not a
security boundary: a package runs with the whole public API, files included,
and the decision record says so.

## Design

### Entitlements and declarations

- The manifest gains `entitlements: ("process")[]`, defaulting to empty. The set
  can grow: `network` arrives with `fetch` (D222).
- Entitlements are coarse. There are no per-program allow lists: anything that
  can run `git` can run `sh -c`, so a list would be theater.
- A gated call checks only that the calling package declared the entitlement.
  If it didn't, the call rejects with the new `EntitlementNotDeclared` code,
  which signals an author bug. Every loaded installed package holds all the
  entitlements it declared, because a package with a denied or undecided
  entitlement never loads. So there is no runtime "denied" state and no
  `EntitlementDenied` code.
- Bundled packages declare entitlements the same way and the same check applies
  to them, but shipping in the binary grants them. Personal config has no
  manifest and holds every entitlement.

### Grants

- Decisions are stored per canonical package identity and per entitlement as
  granted or denied, independent of version. An update that adds an entitlement
  therefore asks only about the new one.
- They live in one versioned record of the application state database
  (`StateDatabase` / `RecordStore`), flushed on quit like the other records.

### Startup consent

- The order becomes: personal-config pre-init → installed discovery → consent →
  installed packages → post-init. Pre-init runs first because a later pre-init
  API (`disableExtension(id)`, not part of this item) may remove a package
  entirely, and a disabled package must not be asked about. The shell
  environment (below) is resolved meanwhile.
- If any discovered, valid, installed package has an undecided declared
  entitlement, Knot opens a dedicated consent window, modeled on the
  fatal-config diagnostic window, before any window of the workbench opens. It
  lists each such package with its display name, identity, version, and
  undecided entitlements, with Allow and Deny for each package, and Continue.
  Startup waits for it. If nothing is undecided, no window opens.
- Allow stores a grant and Deny a denial for that package's undecided
  entitlements. Closing the window, or pressing Continue without deciding,
  stores nothing for those packages. They don't load this launch, and Knot asks
  again at the next one.
- A package with any denied or undecided entitlement doesn't load. Its
  startup-report entry says why: "not loaded: entitlement `process` denied", or
  "not decided". Its dependents fail as with any dependency failure. Denied
  packages don't make the startup report open on launch, since the user just
  chose that. Undecided ones don't either.
- Developer: Restart Extension… re-reads the manifest. If an entitlement is
  undecided, it asks in the current window with the native confirmation before
  activating: Allow grants and activates, Dismiss loads nothing and stores
  nothing. A denied package stays not loaded. Restart doesn't override a
  denial; Manage Entitlements does.

### Reviewing grants

- A native command, Extensions: Manage Entitlements…, opens the transient
  picker listing installed packages that declare entitlements, with their
  state. Choosing one toggles between allowed and denied.
- Granting a package that isn't loaded activates it through the restart
  machinery. Revoking performs a terminating unload, which kills its processes,
  and records "not loaded: entitlement denied" in the report. Dependents behave
  as they do after a restart today.
- This stays native: a JS API that edits grants would itself need a privileged
  entitlement.

### Process API

- `processes.run({ command, args?, cwd?, env?, stdin? })` resolves
  `{ exitCode, signal, stdout, stderr }` once the process exits. Most `git`
  calls use it.
- `processes.spawn({ command, args?, cwd?, env? })` returns a handle with
  `onStdout(listener)`, `onStderr(listener)`, `write(text)`, `closeStdin()`,
  `kill(signal?)`, and `exited: Promise<{ exitCode, signal }>`.
- Output is text: UTF-8 decoded with replacement, carrying split code points
  across chunks. There's no byte transport until a consumer needs one.
- Backpressure: a stream reads its next chunk only after the listener's
  notification is acknowledged, using the same per-subscription ordering as
  buffer listeners and `files.watch`. A flood stalls the child on its pipe
  instead of growing memory. Output that arrives before a listener is attached
  is held until one attaches.
- `command` resolves through `PATH` in the effective environment. `cwd` is a
  file URI and defaults to the user's home directory. `env` is merged over the
  inherited environment, and a `null` value unsets a variable. A missing
  command or `cwd` rejects with a typed code rather than a generic failure.
- Each child starts in its own process group. Lifecycle teardown (unload,
  restart, revoke, startup rollback, quit) kills the group, so helpers spawned
  by `git` die too. `kill` signals the group as well.
- Cancelling a command does not kill its processes: the lifecycle owns them,
  and a command that wants them gone on cancellation calls `kill` from its
  signal's abort listener.

### Inherited environment

- When Knot starts from Finder, its environment lacks the user's shell setup
  (Homebrew's `PATH`, for one). On macOS and Linux, startup resolves the
  environment once by running the user's `$SHELL` as a login interactive shell
  that prints its environment, with a timeout. It falls back to Knot's own
  environment on failure or timeout and logs why. Windows inherits Knot's
  environment.
- The resolved environment is the base for processes extensions spawn. It
  doesn't change terminals, which already start a login shell.

## Exclusions

- `fetch` and the `network` entitlement: D222.
- No sandbox, no per-program or per-path allow lists, and no gating of file
  reads or mutations (see the [file access decisions](../decisions/documents-and-persistence.md#extension-file-reads)).
- No PTY for spawned processes, no byte streams, no detached processes that
  outlive their lifecycle.
- No `disableExtension` pre-init API. Only the ordering that lets it exist later.
- Bundling `rg`, or requiring it on `PATH`, belongs to D088.
- Workspace trust (D124) may reuse the grant record later. Nothing here
  anticipates it.

## Deferred decisions

- Whether the process primitives live in a new leaf crate that owns children
  and their reader threads without gpui, as `knot-terminal` does, or in an
  application module. Decide in checkpoint 2 from how much of it is testable
  without gpui; a leaf crate is the default.
- The exact layout of the consent window. It is decided at checkpoint 1's
  review.

## Checkpoints

### 1. Declarations, grants, and startup consent ✅ approved

Manifest `entitlements`, the grant record (`knot-model`
`extensions/entitlements.rs`, persisted by `src/app/extensions/entitlements.rs`),
the startup consent window (`src/app/extensions/consent.rs`), the
`LoadResult::NotLoaded` outcome, and the restart confirmation are in place.
The consent window stays open after Continue or close until the workbench
opens, so quitting with the last window cannot fire in between. A package held
back by an entitlement does not open the startup report on launch; its
dependents fail normally. The author guide covers `entitlements` in
[packages](../extension-api/README.md#package-and-check-your-code).

### 2. Process API

Deliverables:

- The inherited-environment resolution at startup, available as a global.
- Process primitives: spawn in a new process group, piped stdio, chunked UTF-8
  reading with acknowledgement-driven backpressure, stdin writes and close,
  exit status, and group kill.
- Host operations behind the `process` gate, `EntitlementNotDeclared`, and the
  typed failures for a missing command or `cwd`. Lifecycle cleanup kills every
  process of the lifecycle.
- `processes.run` and `processes.spawn` in `knot.d.ts`, the bootstrap, the
  extension guide, and an example.

Validation: primitive tests (output that splits a code point, a large output
under backpressure, stdin round trip, exit codes and signals, group kill that
reaches a grandchild). Host tests for the gate (undeclared rejects, bundled
declared succeeds, personal config succeeds) and for teardown killing processes
on unload, restart, and quit. A TypeScript extension test through the real
editor (`scripts/extensions/testing`) that runs and streams a real program.
`just check`.

Review gate: a `Try:` command that runs `git status` from a package and shows
the output, launched from Finder, so `PATH` resolution is checked.

**Implementation context:**

- Pattern for children and threads without gpui: `crates/knot-terminal/src/pty.rs`
  (process lifecycle, kill, reader threads).
- Login-shell lookup: `src/app/terminal/session.rs` (`$SHELL` resolution).
  Startup hook: `start_product_startup`, so the environment is resolved before
  installed packages load.
- Handlers: annotate in `src/app/extensions/host/handlers.rs`. Mirror the
  `watchFiles`/`unwatchFiles` subscription (`ProductExtensionHost::watch_files`)
  for stream listeners and their ordered, acknowledged notifications, and the
  lifecycle-owned registry it uses for teardown in `remove_lifecycle`.
- Error codes: `HostErrorCode` in `crates/knot-protocol/src/` and
  `crates/knot-host/src/js/knot.d.ts`, plus descriptor checks in
  `src/app/extensions/host/declaration_tests.rs`.
- Entitlement lookup: the caller's `ExtensionKey` maps to its package identity
  through `package_keys` and `storage_owners` in `host.rs`. Keep the declared
  entitlements next to them when a package loads.

### 3. Manage Entitlements and documentation

Deliverables:

- Extensions: Manage Entitlements… with grant (activating a not-loaded package)
  and revoke (terminating unload, report entry updated).
- The decision record in `docs/decisions/extension-host.md` and its index:
  entitlements as consent and not a sandbox, coarse entitlements, asking at
  startup after pre-init, denial meaning not loaded, native grant management,
  text-only streams, and the resolved shell environment, each with its rejected
  alternatives. The startup order and entitlements go in
  `docs/architecture/extension-host.md`, and process ownership in
  `docs/architecture.md` if it changes boundaries. Remove "ungated until D087"
  wording in the file-access decisions.

Validation: application tests for revoking a running package (its processes
die and its report entry changes), and for granting a denied package (it
activates and its dependents are unaffected). `just check`.

Review gate: revoking and granting in the real app through `Try:` commands.

**Implementation context:**

- Picker and command: mirror `src/app/product/extension_restart.rs` (native
  command over the transient picker), registered in
  `src/app/product/commands.rs`.
- Activation and unload: `ProductExtensionHost::restart_package` and
  `unload_package_for_test`'s terminating-unload path in `host.rs`.
