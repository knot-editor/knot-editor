# Workbench and lifecycle decisions

Panes, tabs, tools, terminals, window layouts, restoration, and the application
state database. The [decisions index](../decisions.md) lists every topic, and
the [documentation guide](../README.md#decisions) sets the entry rules.

## Workbench layout and view lifetime

A binary split tree with tabbed leaves and stable pane and tab identities keeps
layout transitions typed and targets exact. One protected closure flow defers
every presentation change until all decisions and saves succeed, because
closing as each prompt is answered would leave a partial closure on Cancel.
Empty-workbench replacement is shell policy rather than a layout invariant.
Application-wide drag coordination exists because macOS delivers a pressed drag
to the window it started in, so no single window can hit-test the destination.

Tools are tabs in dock groups of the same split tree rather than a separate
sidebar model, so they share lifecycle, observation, and command targeting with
documents and terminals, and one `CommandView` path resolves focus and targets
for every surface. The workbench keeps hidden tool definitions so native state
survives hiding without retaining closed documents.

Tool-layout memory stores only tools; document structure is never reproduced
in a fresh window. Only explicit user arrangements publish the default,
so nothing the user did not arrange, such as a restore or an extension's
automatic presentation, becomes what new windows inherit. Profiles and layered
preferences were left out; Reset Layout is the single escape.

A window renders a product session rather than owning the workbench, focus, and
key routing, so commands, focus, and keys run without a window and package
behavior tests need none. Focus is kept as Knot identities because gpui reports
focus only for an active window, while a session without one still needs a
focused pane. The type keeps the name `ProductShell` because "session" already
names project and terminal sessions.

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

Preview tabs follow VS Code. Previews, the
reopen stack, recent order, and maximize describe a moment of browsing rather
than an arrangement worth restoring, so none is recorded. Reopen Closed Tab
recreates tabs through the layout's per-kind step rather than its own, so a
reopened tab matches one restored after a restart.

New Terminal is on ctrl-` because Reopen Closed Tab takes ⌘⇧T, as in browsers
and VS Code; Knot has no terminal panel, so the key that shows one elsewhere
opens a terminal tab. Named keys and ctrl-` stay global in terminals on Windows
and Linux, because shells read ctrl only with printable characters.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Terminal

Use `alacritty_terminal` as a native surface separate from text buffers. Ghostty
VT offered too little behavioral advantage to offset separate PTY integration,
pre-1.0 FFI, Zig packaging, integration size, and poor debug parsing performance
(D063 revisits it). The Alacritty adapter met the required behavior without
copying Zed code; Zed remains an attributed reference for compatibility gaps.

Sessions are application-owned and outlive their presentation, so tab moves and
window transfers keep the shell. One attachment at a time keeps resize and
input authority unambiguous. Gesture ownership from press through release keeps
a modifier change mid-drag from leaving a reported button pressed. Link opening
is limited to HTTP(S) to bound link policy, and validating on release keeps
changing output from opening a stale destination. Physical application-keypad
support stays deferred rather than vendoring gpui to recover its missing key
identity.

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

A restored terminal reads its shell's directory from the process rather than
OSC 7, which `alacritty_terminal` does not parse and which needs shell
integration (D039); reading the shell rather than the foreground job keeps a
running editor or build from changing the answer. A deleted directory falls
back to the default rule rather than its nearest existing parent, a rare case
where the root is a fine landing place. Moving to libghostty (D063) replaces
the query with the emulator's OSC 7 state (D128).

Reference: [terminal sessions](../architecture/workbench-and-lifecycle.md#terminal-sessions-and-views).

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
and [extension guide](../extension-api.md#project-sessions).

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
move (D062), and released terminals stop as closing any window does. The
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
failed records nothing, so the next good launch restores the last good session.

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
