# Bundled packages decisions

Product workflows built as bundled JavaScript packages on the public API. The
[decisions index](../decisions.md) lists every topic, and the [documentation
guide](../README.md#decisions) sets the entry rules.

## Go to Line and the editor handle

Go to Line is a bundled package, so a workflow that reads lines and moves the
caret proves the public API; it forced the editor handle, line lookups, and
live input. The handle lives within its command's invocation and resolves the
captured presentation on every call, so it cannot act on a view nobody invoked
it in. Columns are computed in JavaScript from one line, so large files never
cross whole. `commands.unavailable()` lets a handler report that it does not
apply.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views)
and [commands](../architecture/commands.md#extension-execution).

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

`@knot/projects` owns project records while native code owns workspaces and
roots, so project policy stays in JavaScript without private bypasses. A window
belongs to a project when its roots match and the session is active; the
binding is rederived at activation, so a restart cannot resurrect a stale one.
Projects follow root edits rather than offering a separate add-folder command.

References: [extension guide](../extension-api.md) and
[workbench lifecycle](../architecture/workbench-and-lifecycle.md#automatic-tool-layout-memory).

## Project sessions

A project's session is active or suspended. Membership requires an active
session, so opening a suspended project's folder by hand never overwrites its
captures. Switching retains text and durably suspends the source before the
native commit, so a failed or interrupted switch never loses text. Captures are
released only after the record dropping them is durable; orphans are reconciled
at activation. A record with an unknown schema is preserved.

References: [extension guide](../extension-api.md#project-sessions) and
[workbench lifecycle](../architecture/workbench-and-lifecycle.md#session-replacement).
