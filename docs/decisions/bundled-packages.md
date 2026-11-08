# Bundled packages decisions

Product workflows built as bundled JavaScript packages on the public API. The
[decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Go to Line and the editor handle

Go to Line is the bundled `@knot/go-to-line` package rather than native code,
because a workflow that only reads lines and moves the caret should prove the
public API: it forced a single-selection editor handle, line lookups from the
line index, and live input, which a Vim mode, occurrence highlighting, and
language tooling need as well. The handle lives within its command's invocation
tree, as `CommandTarget` does, and resolves the captured presentation on every
call, so a handle cannot outlive its tab or act on a view nobody invoked it
in; modes use the long-lived
[editor handle](extension-host.md#editor-handles) instead. Columns are
computed in JavaScript from one line's snapshot, so a large file never crosses
the boundary whole. A centered reveal recenters only when the caret's line was
off screen, as VS Code's Go to Line does, so short hops never jolt the view.
Its message line copies Zed's wording, which changes less as one types than
VS Code's prompts, and a separator typed before its column reads as the line
alone, so the preview holds while `42:4` is typed. Showing the current
position as placeholder text, as Zed does, waits for D140.

The native Knot preset binds the package's command, so a preset or personal
binding replaces it like any default; while the package is not running, the key
reports the command unavailable. A handler signals that outcome with
`commands.unavailable()`, the counterpart of `invalidArguments`.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views),
[commands](../architecture/commands.md#extension-execution), and
[extension guide](../extension-api.md#register-a-command).

## Bundled Vim mode

`@knot/vim` is a bundled JavaScript package on public APIs only, because it is
the acceptance test for the editor API; native modal editing and a Helix-style
model were rejected. A JavaScript state machine parses typed text, since
native key sequences cannot enumerate counts. Visual mode keeps its own
anchor, and ordinary text math, over line snapshots, stays pure. Indentation,
joining, `%`, and visual rows invoke native commands so they follow the
language. The unnamed register is the system clipboard, as
`clipboard=unnamedplus` makes it, with a remembered linewise flag. `ctrl-w`
treats a window as a tab, as other editors' Vim modes do, so `c` and `q` close
the tab and cycling skips docks. Search, `.` repeat, and visual-block wait for
Find, recording, and multiple selections.

References: [extension guide](../extension-api.md#bind-keys-and-modal-contexts) and
[roadmap](../roadmap.md) (D113, D142, D150, D151).

## Bundled project registry

The bundled `@knot/projects` package owns project records: one application-global
storage record holding JS-generated stable IDs, names, roots, and the recent
selection. Native code owns workspaces and roots; the registry asks the
application to open or replace them, so project identity and policy stay in
JavaScript while the application keeps every private bypass closed.

A window belongs to a project when its roots equal the project's roots, ignoring
order, and the project's session is active. No two projects share a root set,
while one folder may belong to several projects. The registry persists no
per-window binding: the binding lives in memory and is rederived from root sets
at activation, so a restart can never resurrect a stale association.

Projects follow root edits. When a bound window's roots change to a set no other
project uses, the project adopts it and its other windows follow through the
public `replaceRoots`, so a project stays one root set across its windows without
a separate Add Folder to Project command. Clearing the roots, or choosing
another project's set, unbinds the window instead of merging projects.

The Projects tool starts hidden. Its visibility and dock selection are native
tool-layout state: a command-invoked extension show or hide publishes the
fresh-window default like any other explicit user arrangement.

References: [extension guide](../extension-api.md) and
[workbench lifecycle](../architecture/workbench-and-lifecycle.md#automatic-tool-layout-memory).

## Project sessions

A project's session is active or suspended, recorded per project in a
`session:<id>` record separate from the registry so live tracking writes stay
small. Requiring an active session for membership keeps root-derived
activation while protecting suspended text: opening a suspended project's
folder by hand never binds the window, so tracking can never overwrite the
record holding its captures. Only a switch carries captures; losing the last
window by closure suspends without them because closure already decided about
dirty documents. Startup waits for `launched`, so natively restored windows
rebind by root set first; it then marks the remaining active records suspended
and never releases anything. A record with an unknown schema is preserved rather
than overwritten and blocks orphan reconciliation, because a newer version's
record may reference any capture.

A project may span several windows and restoring it restores all of them:
switching closes every source window and creates every destination window with
its stored layout, so each window's record is self-contained and no source
window's leftover presentation leaks into another project. Active records
describe membership only: dirty active text is protected by automatic recovery
and hot exit, and automatic restores keep their document UUID, so an active
record's document references survive restart. Captures are released only after
the record that drops them is durable; orphans are reconciled at activation
rather than released implicitly.

Switching retains text and durably suspends the source before the native commit,
so a failed, cancelled, or interrupted switch never loses text: the source stays
intact and its suspended record is a valid suspension of a still-active project
that the next tracking write reactivates. An edit racing the commit retries with
fresh captures, up to three attempts. A partial restore commits and reports the
skipped entries together, because the source is already durably suspended.
Package commands and listeners are serial roots, so live tracking never records
the intermediate state of a switch.

References: [extension guide](../extension-api.md#project-sessions) and
[workbench lifecycle](../architecture/workbench-and-lifecycle.md#session-replacement).
