# Bundled packages decisions

Product workflows built as bundled JavaScript packages on the public API. The
[decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Go to Line and the editor handle

Go to Line is the bundled `@knot/go-to-line` package rather than native code,
because a workflow that only reads lines and moves the caret should prove the
public API: it forced the editor handle, line lookups from the line index, and
live input, which a Vim mode, occurrence highlighting, and language tooling need
as well. Its handle lives within the command's invocation and resolves the
captured presentation on every call, so it cannot outlive its tab or act on a
view nobody invoked it in; modes use the long-lived
[editor handle](extension-host.md#editor-handles) instead. Columns are computed
in JavaScript from one line's snapshot, so a large file never crosses the
boundary whole.

The reveal recenters only when the target was off screen, as VS Code's Go to
Line does, so short hops never jolt the view. The message line copies Zed's
wording, which changes less as one types than VS Code's prompts. Showing the
current position as placeholder text, as Zed does, waits for D140.
`commands.unavailable()` exists so a handler can report that it does not apply,
the counterpart of `invalidArguments`.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views),
[commands](../architecture/commands.md#extension-execution), and
[extension guide](../extension-api.md#register-a-command).

## Bundled Vim mode

`@knot/vim` is a bundled JavaScript package on public APIs only, because it is
the acceptance test for the editor API; native modal editing and a Helix-style
model were rejected. A JavaScript state machine parses typed text, since native
key sequences cannot enumerate counts. Visual mode keeps its own anchor, and
ordinary text math, over line snapshots, stays pure. Indentation, joining, `%`,
and visual rows invoke native commands so they follow the language. The unnamed
register is the system clipboard, as `clipboard=unnamedplus` makes it, with a
remembered linewise flag. `ctrl-w` treats a window as a tab, as other editors'
Vim modes do, so `c` and `q` close the tab and cycling skips docks. Search, `.`
repeat, and visual-block wait for Find, recording, and multiple selections.

References: [extension guide](../extension-api.md#bind-keys-and-modal-contexts) and
[roadmap](../roadmap.md) (D113, D142, D150, D151).

## Bundled project registry

`@knot/projects` owns project records while native code owns workspaces and
roots, so project identity and policy stay in JavaScript while the application
keeps every private bypass closed. A window belongs to a project by its root set
rather than a stored binding, which is rederived at activation, so a restart can
never resurrect a stale association. No two projects share a root set, so the
match is unambiguous, while one folder may belong to several projects.

Projects follow root edits, so a project stays one root set across its windows
without a separate Add Folder to Project command. Clearing the roots, or choosing
another project's set, unbinds the window rather than merging projects.

References: [extension guide](../extension-api.md#project-sessions) and
[automatic tool-layout memory](../architecture/workbench-and-lifecycle.md#automatic-tool-layout-memory).

## Project sessions

Each project's session is its own record, separate from the registry, so live
tracking writes stay small. Requiring an active session for membership protects
suspended text: opening a suspended project's folder by hand never binds the
window, so tracking can never overwrite the record holding its captures. Losing
the last window by closure suspends without captures because closure already
decided about dirty documents. A record with an unknown schema is preserved and
blocks orphan reconciliation, because a newer version's record may reference any
capture.

Switching closes every source window and creates every destination window, so
each window's record is self-contained and no leftover presentation leaks into
another project. Active records describe membership only, because automatic
recovery and hot exit already protect live dirty text. Captures are released
only after the record dropping them is durable, and orphans are reconciled at
activation rather than released implicitly.

Switching retains text and durably suspends the source before the native commit,
so a failed, cancelled, or interrupted switch never loses text: the suspended
record is a valid suspension of a still-active project that the next tracking
write reactivates. A racing edit retries with fresh captures, up to three times.
A partial restore commits and reports the skipped entries rather than rolling
back, because the source is already durably suspended.

References: [extension guide](../extension-api.md#project-sessions) and
[session replacement](../architecture/workbench-and-lifecycle.md#session-replacement).
