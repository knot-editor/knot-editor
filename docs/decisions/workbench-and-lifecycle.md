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

A window is a center of documents surrounded by left, right, and bottom docks,
each its own split tree of the same pane and tab types, rather than one
free-form tree whose groups are docks. Every surface then has a home, so
toggle commands can show, focus, or hide it wherever the user keeps it, and a
dock hides whole with its arrangement. Free-form docks gave no such home, and
every placement rule had to name kinds; each kind instead declares the areas it
allows. Tools are tabs rather than a separate sidebar model, so they share
lifecycle, observation, and command targeting with documents and terminals, and
one `CommandView` path resolves focus and targets for every surface. The
workbench keeps hidden tool definitions so native state survives hiding without
retaining closed documents.

Terminals in docks are ordinary flat tabs, not a terminal panel with an inner
tab strip, so splitting, moving, and focus need no second tab model. Floating
tool windows and a top dock were left out as more surface than daily use asks
for. A stripe button replaces the other tools of its pane instead of adding a
tab, as IntelliJ, VS Code, and Zed do, and the button's menu offers Show
Beside; switching among tools by a tab strip in the dock was rejected because
users found stacked tabs confusing.

Maximizing a dock pane while its dock is hidden is remembered and restored when
the dock shows again, as VS Code's `preserve` and Zed's zoom do, because losing
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

Preview tabs follow VS Code. Previews, the
reopen stack, recent order, and maximize describe a moment of browsing rather
than an arrangement worth restoring, so none is recorded. Reopen Closed Tab
recreates tabs through the layout's per-kind step rather than its own, so a
reopened tab matches one restored after a restart.

Toggle Terminal is on ctrl-` because Reopen Closed Tab takes ⌘⇧T, as in browsers
and VS Code, and ctrl-` is the key they use to show the terminal; it toggles the
dock terminals rather than opening a new one, which New Terminal does. Named keys
and ctrl-` stay global in terminals on Windows and Linux, because shells read
ctrl only with printable characters.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Terminal

Terminals are a native surface separate from text buffers, over libghostty-vt
in the `knot-terminal` crate with a `portable-pty` PTY. It is preferred over
`alacritty_terminal` since libghostty-vt 1.0 has a stable, semver C ABI: its
mode-aware key and mouse encoders cover the Kitty keyboard protocol,
modifyOtherKeys, and every mouse format, which Knot would otherwise encode
itself, and its callbacks expose OSC 7 and the protocols D041 needs, which
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
