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
wording, which changes less as one types than VS Code's prompts. The current
position is shown as placeholder text, as in Zed.
`commands.unavailable()` exists so a handler can report that it does not apply,
the counterpart of `invalidArguments`.

References: [editor views](../architecture/editor-and-languages.md#editor-views),
[commands](../architecture/commands.md#extension-execution), and
[extension guide](../extension-api/commands-and-keys.md#register-a-command).

## Search Everywhere

Go to File, buffer switching, and the command palette are one tabbed picker
(All, Buffers, Files, Actions) in `@knot/go-to-file`, differing only in the tab
they start on, so each query is matched, ranked, and highlighted by one
implementation. Scope is chosen only by tab: IntelliJ-style prefixes (`>`, `@`)
were rejected as a second syntax to learn beside tabs, and `:N` stays because
it parameterizes a row rather than selecting a scope. Commands chosen from
Actions rank first through the `history` primitive, so the palette needs no
native command history or stored record. Recent files are not a separate tab: Buffers orders open documents by recent activity, and Open
Recent stays its own command because it lists files outside the window's roots.
Buffers has its own binding because a preset keymap cannot bind arguments, which
is why Go to Buffer is its own command.

`workbench.show-command-palette` stays native so a failed or disabled package
cannot leave the editor without a way to reach the extension report or Close.
A minimal native palette (titles only, no bindings, descriptions, or recents)
is the fallback; keeping the full native palette beside the package was
rejected as two implementations of one list. Search Everywhere has no default
key binding since cmd-p and the palette key are taken.

References: [semantic UI](../architecture/semantic-ui.md#extension-transient-interactions),
[commands](../architecture/commands.md).

## Bundled Vim mode

`@knot/vim` is a bundled JavaScript package on public APIs only, because it is
the acceptance test for the editor API; native modal editing and a Helix-style
model were rejected. A JavaScript state machine parses typed text, since native
key sequences cannot enumerate counts. Visual mode keeps its own anchor, and
ordinary text math, over line snapshots, stays pure. Indentation, joining, `%`,
and visual rows invoke native commands so they follow the language. The unnamed
register is the system clipboard, as `clipboard=unnamedplus` makes it, with a
remembered linewise flag. `ctrl-w` treats a window as a tab, as other editors'
Vim modes do, so `c` and `q` close the tab and cycling skips docks. `.`,
counted inserts, and macros replay input from an
[editor recording](../architecture/commands.md#recording) through the package's
own parser and ordinary command dispatch, so a replay targets the editor that
started it and a macro stops when focus leaves that editor, since following
focus waits for [D182](../roadmap.md); visual-block waits for multiple
selections.

Search and substitution run on the buffer's native regex `find` and `replace`
rather than a second matching engine in JavaScript, so no text crosses the
host boundary and `:s` is one revision-checked edit and one undo step. Patterns
are written in Vim's syntax and translated to the engine's by a pure function,
because Vim users type Vim patterns; a setting selects the engine's own syntax,
as Find's regex mode reads it. Atoms the engine cannot express (`\zs`, `\ze`,
lookaround) fail with an error instead of matching differently, and a custom
engine to cover them was rejected as too costly for their daily use. The
last pattern and direction are Vim's own, apart from Find's query. Earlier
entries of the `/`, `?` and `:` prompts and of Find's fields are a generic
input `history` option, with a separate list per package, since a Vim pattern
recalled into Find's literal search would be noise.

The Ex commands that need Knot's help (`:e!`, `:w <file>`, `:sav`, `:wa`) rest
on generic primitives any extension can use, `documents.reload`,
`documents.save`, `files.writeText`, and a native Save All, rather than
Vim-only native code. `:e!` reuses the undoable external-change reload, so Undo
takes a revert back, which Vim cannot do. `:qa!` quits as `:qa` does, since
quitting keeps unsaved text. Buffers are tabs, as windows are, so Vim buffer
numbers are not supported. Range commands compute their text over line
snapshots and apply it as one edit, so each is one undo step.

References: [extension guide](../extension-api/commands-and-keys.md#bind-keys-and-modal-contexts),
[text search](../architecture/core-and-buffers.md#text-search),
[settings](../settings.md#bundled-packages) and
[roadmap](../roadmap.md) (D142, D146, D151, D182, D201).

## Bundled explorer

The explorer is `@knot/explorer`, built only on public API, rather than native
code with an extension veneer: every gap it exposed (icon references, gestures,
reveal, speed search, inline editing, `files.copy`, `files.reveal`) was closed in
the API for all extensions, so the package proves the surface and the native
workspace tree and its private path are gone. Cut and Copy keep the taken item in
the extension rather than the system clipboard, so pasting text elsewhere never
meets a file and the clipboard holds only text. Multi-select,
compact folders, and Git decorations are roadmap items (D177, D178, D126).

References: [extension guide](../extension-api/ui.md#activation-selection-and-editing-in-trees)
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

References: [projects](../architecture/workbench-and-lifecycle.md#projects) and
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

References: [projects](../architecture/workbench-and-lifecycle.md#projects) and
[session replacement](../architecture/workbench-and-lifecycle.md#session-replacement).

## Project inference

`@knot/projects` infers a project from the nearest `.git` ancestor of an opened
file, inside the registry package rather than a separate one, because adopting a
project needs its switch path and a second package's root changes would race
its tracking. Only `.git` counts, file or directory; other version-control and
language markers wait for a real need. The filesystem root and the home
directory never count, through an exact `homeUri` host fact rather than a path
depth heuristic, so a dotfiles repository at the home directory cannot claim
every loose file under it.

Inference never asks. A window without roots that holds only the opened file
adopts the checkout automatically, because launch placement created that window
for the file and rooting it cannot surprise; everything else is a command. A
confirmation after opening was rejected: it appeared over a file already shown,
Cancel undid nothing, accepting moved the file, and it needed dismissal memory.
Asking before opening would put an extension callback on every open and launch
path, and a status-bar offer cannot be per window.

A checkout the registry already knows, the common case because every root set
is remembered, opens through the switch, which carries a rootless window's tabs
into the restored project or keeps a rooted window and its project untouched.
Setting the roots and letting tracking restore the project was rejected: the
window holding the file is not reusable, so the session opened in yet another
window. Adding the checkout as a root of a rooted window would silently change
that project's identity, and a plain switch from it would close its project's
windows.

References: [projects](../architecture/workbench-and-lifecycle.md#projects).
