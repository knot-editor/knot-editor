# Workbench and lifecycle decisions

Panes, tabs, tools, terminals, window layouts, restoration, and the application
state database. The [decisions index](../decisions.md) lists every topic, and
the [documentation guide](../README.md#decisions) sets the entry rules.

## Workbench layout and view lifetime

A binary split tree with tabbed leaves and stable pane and tab identities keeps
layout transitions typed and targets exact. One protected closure flow defers
every presentation change until all decisions and saves succeed, because
closing as each prompt is answered would leave a partial closure on Cancel.
Empty-workbench replacement is window policy rather than a layout invariant.
Application-wide drag coordination exists because macOS delivers a pressed drag
to the window it started in, so no single window can hit-test the destination.

A window is a center of documents surrounded by six dock slots, each showing
at most one tool window, and a tool window is the same split tree of tabbed
panes as the center, as IntelliJ's. Every surface then has a home, so toggle
commands can show, focus, or hide it wherever the user keeps it, and a tool
window hides whole with its tabs. Docks as split trees of tools gave a side
room for several tools at once and no place for a tool to own its tabs; each tab
kind instead declares where it may live. Tools are tabs rather than a separate
sidebar model, so they share lifecycle, observation, and command targeting with
documents and terminals, and one `CommandView` path resolves focus and targets
for every surface. The workbench keeps hidden tool definitions so native state
survives hiding without retaining closed documents. The model follows IntelliJ
first; a comparison with VS Code and Zed is deferred until it has been used.

Terminals live in a container tool window that `@knot/chrome` registers, rather
than as flat tabs in any dock: a container owns its tabs, strip, and `+`, so a
later agent window needs no second tab model, and splitting, moving, and focus
stay the center's. The package owns the new-tab command and starting directory
so that policy is JavaScript. Floating, windowed, and sliding tool windows and
a top dock were left out as more surface than daily use asks for. A stripe
button shows its window in its slot and hides the one there, keeping that
window's tabs, as IntelliJ does; switching among tools by a tab strip in the
dock was rejected because users found stacked tabs confusing, and Show Beside
went with the dock.

Maximizing a tool window's pane while the window is hidden is remembered and
restored when it shows again, as VS Code's `preserve` and Zed's zoom do, because losing
a deliberate maximize to a toggle surprises more than keeping it.

Tool-layout memory stores only tools; document structure is never reproduced
in a fresh window. Only explicit user arrangements publish the default,
so nothing the user did not arrange, such as a restore or an extension's
automatic presentation, becomes what new windows inherit. Profiles and layered
preferences were left out; Reset Layout is the single escape.

A native window renders a window rather than owning the workbench, focus, and
key routing, so commands, focus, and keys run without a native window and
package behavior tests need none. Focus is kept as Knot identities because gpui
reports focus only for an active native window, while a window without one still
needs a focused pane. The type is `KnotWindow`, not `Window`, because gpui
already exports `Window`.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Workbench navigation

Ordinary splits record ratios so sizes survive restarts and project switches,
while docks keep absolute sizes because a sidebar should not grow with the
window. Addressing a split by its boundary pane, which is unique, avoids a
separate split identity in dividers and the layout.

Recent-tab switching has no overlay, unlike VS Code's and Zed's switchers;
each press activates the next tab, and committing on release keeps a walk to
one activation. Directional
commands use laid-out pane rectangles rather than painted bounds, because a
maximized window paints only one pane; tying maximize to focus ends it on every
focus change without separate hooks.

Preview tabs follow VS Code, and documents and composite views share one
preview slot per pane so browsing diffs and files never piles up tabs. Previews, the
reopen stack, recent order, and maximize describe a moment of browsing rather
than an arrangement worth restoring, so none is recorded. Reopen Closed Tab
recreates tabs through the layout's per-kind step rather than its own, so a
reopened tab matches one restored after a restart.

Toggle Terminal is on ctrl-` because Reopen Closed Tab takes ⌘⇧T, as in browsers
and VS Code, and ctrl-` is the key they use to show the terminal; it toggles the
dock terminals rather than opening a new one, which New Terminal does. Named keys
and ctrl-` stay global in terminals on Windows and Linux, because shells read
ctrl only with printable characters. Go to File's ctrl-p stays global too, at
the cost of the shell's previous-history key, which the up arrow also
provides: a file finder that silently fails from one surface is more confusing,
and VS Code makes the same choice.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Terminal

Terminals are a native surface separate from text buffers, over libghostty-vt
in the `knot-terminal` crate with a `portable-pty` PTY. It is preferred over
`alacritty_terminal` since libghostty-vt 1.0 has a stable, semver C ABI: its
mode-aware key and mouse encoders cover the Kitty keyboard protocol,
modifyOtherKeys, and every mouse format, which Knot would otherwise encode
itself, and its callbacks expose OSC 7, titles, bells, and desktop notifications, which
Alacritty does not parse. Before 1.0, the pre-1.0 FFI, the Zig toolchain, and
slow debug-build parsing outweighed that advantage.

The library is linked statically from archives this repository's release
workflow builds ReleaseFast, so no contributor or CI job needs Zig and parsing
runs optimized even in debug builds; manual review rendered 200,000 lines of
output in 0.35 s. Building from source in `build.rs` was rejected for
requiring Zig everywhere, and the system library through pkg-config because no
distribution packages it yet.

Windows are application-owned and outlive their presentation, so tab moves and
native window transfers keep the window. One attachment at a time keeps resize and
input authority unambiguous. Gesture ownership from press through release keeps
a modifier change mid-drag from leaving a reported button pressed. Link opening
is limited to HTTP(S) to bound link policy, and validating on release keeps
changing output from opening a stale destination. Physical application-keypad
support stays deferred rather than vendoring gpui to recover its missing key
identity.

A name set with Rename Session wins over a program's title, because the name
is a deliberate choice while shells and programs rewrite their title at every
prompt; a title that replaced it would undo the rename on the next command.
Titles are not persisted, since a restored session starts a new shell.

A terminal opened from a terminal starts in that terminal's directory, as
iTerm, Ghostty, and Zed's split do: someone working in a terminal wants the
next one where they already are. Otherwise it follows the most recently
focused document's root, as Zed's default does, rather than always the first
root, which lands in the wrong folder of a multi-root window, or a picker, as
VS Code does, which adds a step to every terminal. Remembering the last focused
document lets a terminal opened from a tool still follow the file being edited.
A window without roots uses home rather than the active document's folder,
because such a window is usually a scratch window and the result should not
depend on which file happens to be open; daily use may revisit this. The
process directory is never used, because it is `/` when launched from Finder.

A terminal's directory is the one its shell reports through OSC 7, because
that works on Windows, which has no process query, and follows the shell
without asking the operating system. Querying the shell process remains the
fallback, since shells emit OSC 7 only with some setups until shell
integration (D039); it reads the shell rather than the foreground job, so a
running editor or build does not change the answer. A directory reported from
another host is ignored, since a local terminal cannot start there. A deleted
directory falls back to the default rule rather than its nearest existing
parent, a rare case where the root is a fine landing place.

Terminal user preferences are global new-session defaults. `terminal.mouseReportingEnabled`
is a boolean mirroring the session flag, and the toggle command keeps working
as a per-session override, because a mode enum would add states no program
protocol needs. `terminal.cursorStyle` sets the engine default while a program
escape still wins, because a forced override would fight programs that set
their own shape, such as a bar in insert mode. Neither key takes a view scope,
since no per-terminal use case needs one.

`terminal.scrollbackLines` stays undefined until a libghostty-vt build enforces
it: probes showed the pinned 1.0.1 accepts `SCROLLBACK_MAX_LINES`/`MAX_BYTES`
and reports them back yet keeps all 100 written lines with a limit of 5, 0, or
50, before and after resize, and exposes no trim API, so the setting would be a
silent no-op. Ship it then as a clamped global for new sessions and restarts:
the key, the reader, the `Terminal::new` argument, the settings-table row, and
a session test bounding a small history.

Reference: [terminal sessions](../architecture/workbench-and-lifecycle.md#terminal-sessions-and-views).

## Composite views and the diff package

The workbench stays tabs of one view each, and a view lays out its own
contents. A package describes a composite view as data, and every child view has
its own identity in a tree extensions see, so Vim, the status bar, and editor
handles work in any editor wherever it sits and nothing is privileged because
it is built in. Rejected: a native diff with per-side targets, and diff-specific
`sides` on tabs, which give a three-way merge or a mixed view nothing; nested
workbenches with draggable tabs inside a view, which would let the user
rearrange what the package lays out.

The diff is the `@knot/diff` package on those parts, not a native view that
rediffs itself. That reverses the rejection of a package that diffs and pushes
hunks to the view, which cited latency and state: `diff()` reads both buffers
natively, so no text crosses to the package, and it returns each side's
revision, so the package debounces and drops stale results. What remains is
pushing decorations, the scroll map, and block rows after each rediff.

A side's editability follows its content, as in IntelliJ: an open document is
editable and a snapshot is read-only. A lock on the modified side, which needed
a per-view read-only flag on every editor beside its buffer's, is gone.
Composite views are left out of window layouts and Reopen Closed Tab: native
code cannot recreate buffers the package supplied.

Side by side follows IntelliJ: no filler rows, a strip joining each hunk to its
counterpart, and scrolling mapped through the hunks. Rejected: VS Code's filler
rows, which need virtual rows in both editors just to keep the sides level.
Inline shows the modified document with each hunk's original lines as read-only
block rows in the wrap map, the counterpart of the zero-row lines folds already
use, so the text stays one editable editor.

Reference: [composite views](../architecture/workbench-and-lifecycle.md#composite-views)
and the [diff package](../architecture/version-control.md#the-diff-package).

## Window layouts and restoration by position

One versioned layout serves native window hot exit and extension sessions, so
both restorers share capture, pruning, and installation, and extensions store it
verbatim without a parallel schema. It holds no handles and each tab kind
records only what recreates it, so a new kind joins with one recreation step.
Pruning lives in one place, so a stored layout can never produce an invalid
workbench. View state is byte offsets that clamp, which survive changed files
without remembering line models.

The layout is captured on demand and announced through a debounced event rather
than pushed with every change, so cursor movement costs one capture per settled
second and listeners write only changed records.

References: [window layouts](../architecture/workbench-and-lifecycle.md#window-layouts)
and [projects](../architecture/workbench-and-lifecycle.md#projects).

## Session replacement contract

`workbench.replaceWindows` knows windows, documents, and retained captures but
not projects, so an installed package can build the same workflow as
`@knot/projects`. It runs as a protected operation so it cannot interleave with
closure prompts or another replacement. Staging every read before one foreground
commit means observers see only the committed result and need no operation tag.
Revalidating revisions in Rust at the commit turns a racing edit into a
rejection with nothing changed rather than lost text.

A live document always wins over staged content, which keeps one source of
truth per file. Draining restored dirty documents to recovery before resolving
lets callers release captures immediately.

Switching closes released windows and creates new ones rather than reusing
them: one creation path serves startup and switching, existing windows never
move, and released terminals stop as closing any window does. The
rejected alternative, clearing a released window that held a terminal, left a
window outside every session.

References: [session replacement](../architecture/workbench-and-lifecycle.md#session-replacement)
and [documents](../architecture/documents-and-persistence.md#retained-captures).

## Native window hot exit and launch precedence

The application, not the project extension, records and restores windows, so a
broken or slow extension never loses them, matching how unsaved text is
protected. Projects rebind restored windows by root set instead of reopening
them, so startup has one restorer and no duplicate windows. Marking the record
consumed before restoring trades the arrangement for a guaranteed clean next
launch if restoration itself crashes.

Recovered documents restore first, so restored windows present them in place
and requests deduplicate against everything restored. The last request window
takes focus because it is what the user just asked for. A launch whose config
failed records nothing, so the next good launch restores the last good record.

References: [native window hot exit](../architecture/workbench-and-lifecycle.md#native-window-hot-exit)
and [product launch](../architecture/workbench-and-lifecycle.md#product-application-launch).

## Application state database

UI state kept across restarts lives in one SQLite file with a table per record,
rather than a file per feature, so one connection serializes the writes:
concurrent debounced writes never meet SQLite's busy error, and quit flushes
them together. Unsaved-text recovery stays in its own database because its
failure rules differ: a damaged recovery file must be preserved and reported,
while damaged UI state reads as absent, and a UI-state failure must never reach
the store that protects text.

Reference: [automatic tool-layout memory](../architecture/workbench-and-lifecycle.md#automatic-tool-layout-memory).
