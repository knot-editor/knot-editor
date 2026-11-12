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

References: [editor views](../architecture/editor-and-languages.md#editor-views),
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

## Bundled explorer

The explorer is `@knot/explorer`, built only on public API, rather than native
code with an extension veneer: every gap it exposed (icon references, gestures,
reveal, speed search, inline editing, `files.copy`, `files.reveal`) was closed in
the API for all extensions, so the package proves the surface and the native
workspace tree and its private path are gone. Cut and Copy keep the taken item in
the extension rather than the system clipboard, so pasting text elsewhere never
meets a file and the clipboard holds only text. Multi-select, drag and drop,
compact folders, and Git decorations are roadmap items (D177, D176, D178, D126).

References: [extension guide](../extension-api.md#activation-selection-and-editing-in-trees)
and [semantic UI](../architecture/semantic-ui.md#extension-trees).

## Bundled project registry

`@knot/projects` owns project records while native code owns workspaces and
roots, so project identity and policy stay in JavaScript while the application
keeps every private bypass closed. A window belongs to a project by its root set
rather than a stored binding, which is rederived at activation, so a restart can
never resurrect a stale association. No two projects share a root set, so the
match is unambiguous, while one folder may belong to several projects.

Every folder set a window shows is remembered automatically as an unnamed
project, as in VS Code and Zed, because asking for a name made remembering opt-in
and most folders were never remembered. Names are therefore optional labels that
are not unique; the root set is the identity users can tell apart, so pickers
show roots beside the name. Unnamed projects are bounded by count, oldest first,
sparing named and active ones and ones holding retained text; an age limit was
deferred until daily use asks for it.

Projects follow root edits, so a project stays one root set across its windows
without a separate Add Folder to Project command. Clearing the roots, or choosing
another project's set, unbinds the window rather than merging projects.

References: [extension guide](../extension-api.md#project-sessions) and
[automatic tool-layout memory](../architecture/workbench-and-lifecycle.md#automatic-tool-layout-memory).

## Project sessions

Each project's session is its own record, separate from the registry, so live
tracking writes stay small. Requiring an active session for membership protects
suspended text: tracking never binds a window to a suspended project, so it can
never overwrite the record holding its captures. Opening a suspended project's
folder by hand instead restores the project through the switch, which already
owns captures, rather than through a second restore path. Losing
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
