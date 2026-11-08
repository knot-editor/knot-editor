# Workbench and lifecycle decisions

Panes, tabs, tools, terminals, window layouts, restoration, and the application
state database. The [decisions index](../decisions.md) lists every topic, and
the [documentation guide](../README.md#decisions) sets the entry rules.

## Workbench layout and view lifetime

Use a binary split tree with tabbed leaves and stable pane/tab identities.
Application-wide last-view knowledge protects shared documents. One protected
closure flow defers presentation changes until all decisions and saves succeed,
avoiding partial closure on cancellation. Empty-workbench replacement is shell
policy rather than a layout invariant.

Tab relocation preserves editor state and terminal sessions independently of
closure. Exact presentation captures reject replacement and transfer; preparing
a new destination window first preserves the source if creation fails.
Application-wide drag coordination handles macOS delivering pressed drags to the
originating window, including cross-window hit testing and preview placement.

Native tool frames retain presentation identity independently of tab attachments.
Dock groups use the existing split tree and typed tabs so tools share lifecycle,
observation, and command targeting with document and terminal presentations.
Keeping hidden definitions in the workbench preserves native state without
retaining closed documents or rebuilding obsolete document groups.
Extension-contributed frames render through the shared
[semantic UI](../architecture/semantic-ui.md) tree and text surfaces, and
`CommandView` supplies the focus handle and key-context kind for every tab
surface, so capture, keybinding routing, and close targets never branch per
surface. The native explorer stays a shell-owned tool until a bundled
JavaScript extension replaces it (D053).

Automatic tool-layout memory persists one versioned tool-only snapshot in
application-owned SQLite, separate from extension storage. Document splits,
terminal contents, and live handles are never serialized: tools in document
groups flatten to the normal document group of a fresh window, so document
structure is never reproduced. Fresh windows inherit the last user-customized
arrangement while open windows stay independent, and only explicit user
arrangements publish a new default; restore, activation, listener callbacks,
automatic presentation, and document structure changes never publish, so
nothing the user did not arrange becomes the default. Reset Layout returns to
product defaults with no profiles or layered preferences.

A window renders a product session rather than owning the workbench, focus,
and key routing itself, so commands, focus, and key handling run without a
window and package behavior tests need none. Focus is kept as Knot
identities and the window follows it, because gpui reports focus only for an
active window and a session without one still needs a focused pane. The
session type keeps the name `ProductShell`: "session" already names project
and terminal sessions, so the docs say product session where the type is meant.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md).

## Workbench navigation

Every common pane and tab operation has a command. Ordinary splits record a
ratio so sizes survive restarts and project switches, while docks keep their
absolute sizes because a sidebar should not grow with the window. A split is
addressed by its boundary pane, the first pane of its second child, which is
unique, so dividers and the layout need no separate split identity.

Recently used switching on ctrl-tab has no overlay, unlike VS Code's and
Zed's switchers: each press activates the next tab immediately and the order
commits when ctrl is released, so a walk counts as one activation. The order is
per pane, as both editors default to, and is not recorded. Directional
commands use pane rectangles laid out from ratios and dock sizes rather than
painted bounds, because a maximized window paints only one pane, and maximize
lasts only while its pane is focused, so every focus change ends it without
separate hooks.

Preview tabs follow VS Code: a browsed document replaces the pane's preview in
place, and editing, pinning, double-clicking, dragging, or opening it again
without `preview` keeps it. Previews, the reopen stack, and maximize are not
recorded, because each describes a moment of browsing rather than an
arrangement worth restoring. Reopen Closed Tab recreates tabs through the
layout's per-kind step, so a reopened tab is restored exactly as after a
restart; untitled documents are skipped because their text is gone.

New Terminal is on ctrl-` because Reopen Closed Tab takes ⌘⇧T, as in browsers
and VS Code; Knot has no terminal panel, so the key that shows one elsewhere
opens a terminal tab. Named keys and ctrl-` stay global in terminals on Windows
and Linux, because shells read only ctrl with printable characters.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Terminal

Use `alacritty_terminal` as a native surface separate from text buffers. Ghostty
VT offered insufficient behavioral advantage to offset separate PTY integration,
pre-1.0 FFI, Zig packaging, integration size, and poor debug parsing performance
(D063 revisits it). The Alacritty adapter met the required behavior without
copying Zed code; Zed remains an attributed reference for later concrete
compatibility gaps.

An application-owned session outlives its presentation. One attachment keeps
resize and input authority unambiguous. Ordered generation-scoped protocol events
preserve emulator replies while repaint wakeups coalesce. Gesture ownership from
press through release prevents modifier changes from leaving reported buttons
pressed. A session-local mouse toggle makes selection and Copy usable inside TUIs.

HTTP(S)-only opening bounds link policy. Explicit OSC 8 identity takes precedence
over detected text, and release-time validation prevents changing output from
opening a stale destination. Physical application-keypad support remains deferred
rather than vendoring gpui solely to recover its missing native key identity.

A new terminal opened from a focused terminal, or by splitting one, starts in
that terminal's current directory, as iTerm, Ghostty, and Zed's split do:
someone working in a terminal wants the next one where they already are.
Otherwise a new terminal starts in the root of the window's most recently
focused document, as Zed's default does, rather than always in the first root,
which lands in the wrong folder of a multi-root window, or asking with a
picker, as VS Code does, which adds a step to every terminal. Remembering the
last focused document lets a terminal opened from a tool still follow the file
being edited. A window without roots uses the home directory rather
than the active document's folder, as most editors do: such a window is usually
a scratch window, and the result should not depend on which file happens to be
open. Daily use may revisit this. The process's own working directory is never
used, because it is `/` when Knot is launched from Finder.

A restored terminal starts in the directory its shell was last in, read from
the shell process when the layout is captured (`proc_pidinfo` on macOS,
`/proc/<pid>/cwd` on Linux). Querying the process needs no shell setup, unlike
OSC 7, which `alacritty_terminal` does not parse and which needs shell
integration (D039); it reads the shell rather than the foreground job, so a
running editor or build does not change the answer. Where the query is
unavailable, as on Windows, the start directory is recorded, so a chosen
directory survives restarts. A deleted directory falls back to the default rule
rather than its nearest existing parent, a rare case where the root is a fine
landing place. A `cd` starts no capture, so a crash can restore an older
directory; quit and project switches always capture fresh. Moving to
libghostty (D063) replaces the query with the emulator's OSC 7 state (D128).

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md).

## Window layouts and restoration by position

One versioned JSON layout serves native window hot exit and extension sessions,
so both restorers share capture, pruning, and installation, and extensions store
it verbatim without a parallel schema. It holds no handles: every tab kind
records only what recreates it in the same position (a document's identity and
URI, a tool's stable ID, nothing for a terminal), and a future kind joins by
adding one recreation step. A tab is pruned only when its kind cannot be
recreated, and pruning lives in one place, so a stored layout can never produce
an invalid workbench. View state is byte offsets that clamp and snap to
character boundaries, which survive changed files without remembering line
models.

The layout is captured on demand and announced through a debounced
`viewStateChanged` event rather than pushed with every change, so cursor
movement costs one capture per settled second and listeners write only changed
records. Restoration never publishes the tool-only default, which keeps restored
windows from overwriting the arrangement new windows inherit.

References: [workbench lifecycle](../architecture/workbench-and-lifecycle.md#window-layouts)
and [extension guide](../extension-api.md#project-sessions).

## Session replacement contract

`workbench.replaceWindows` is a generic typed operation over windows, documents,
and retained captures, and knows nothing about projects, so an installed package
can build the same workflow. It runs as a protected operation so it cannot
interleave with closure prompts or another replacement. Staging every read
before one foreground commit means observers see only the committed result and
need no operation tag, and staged documents never enter the document collection
early. The commit revalidates the exact suspended set and each final view's
retained revision in Rust, from revisions recorded at retain, so a racing edit or
tab rejects as `SessionConflict` with nothing changed rather than losing text.

A live document always wins over staged content, which keeps one source of truth
per file: documents open elsewhere and documents restored by hot exit are
presented rather than duplicated, and a retained restore's preserved untitled
copy never arises from a session. Restored dirty documents are durably protected
before resolution so callers can release captures immediately.

Switching closes released windows rather than reusing them: windows are always
created with their layout and geometry, which keeps one creation path for
startup and switching, avoids moving existing windows (D062), and makes a
released window's terminals stop as closing any window does. Terminals come
back in place as fresh shells, and the rejected alternative, clearing a
released window that held a terminal, left a window outside every session.

References: [workbench lifecycle](../architecture/workbench-and-lifecycle.md#session-replacement)
and [documents](../architecture/documents-and-persistence.md#retained-captures).

## Native window hot exit and launch precedence

Startup restoration is native hot exit extended to windows: the application,
not the project extension, records every product window and restores them before
launch requests open, so a broken or slow extension never loses the user's
windows, matching how unsaved text is protected. Projects rebind to restored
windows by root set after `launched` instead of reopening them, so there is one
restorer at startup and no duplicate windows. The record is marked consumed
durably before restoration commits, trading the arrangement for a guaranteed
clean next launch if restoration itself crashes.

Precedence is recovered documents, then recorded windows, then each queued
request in its own window. Documents resolve live by identity first, so restored
recovery text is presented in place; requests deduplicate against everything
restored; the last request window takes focus because it is what the user just
asked for, else the previously active window. Recovered documents no window
presents open in the first window, so text is never hidden. A reopen after
launch is a request for a window, not a restore. Missing roots are dropped
rather than failing every window. A launch whose config failed records nothing,
so the next successful launch still restores the last good session.

References: [workbench lifecycle](../architecture/workbench-and-lifecycle.md#native-window-hot-exit)
and [documents](../architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Application state database

UI state that the application keeps across restarts, such as the tool-only
layout, the recorded windows, palette command history, and the zoom level,
lives in one SQLite file with a table per record, rather than a file per
feature. One shared connection serializes their writes, so concurrent debounced
writes never meet SQLite's busy error and quit flushes them together.
Unsaved-text recovery stays in its own database: a damaged recovery file must
be preserved and reported, while damaged UI state reads as absent; recovery
commits every capture with FULL synchronization; and a UI-state failure must
never reach the store that protects text. Extension records stay per package.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#automatic-tool-layout-memory).
