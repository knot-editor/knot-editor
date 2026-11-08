# Workbench and lifecycle decisions

Panes, tabs, tools, terminals, window layouts, restoration, and the application
state database. The [decisions index](../decisions.md) lists every topic, and
the [documentation guide](../README.md#decisions) sets the entry rules.

## Workbench layout and view lifetime

A binary split tree with tabbed leaves and stable identities. One protected
closure flow defers changes until every decision and save succeeds, so
cancellation never leaves partial closure. Relocation preserves editor and
terminal state; cross-window drags are coordinated application-wide because
macOS delivers them to the originating window.

Tools are tabs in dock groups of the same tree, sharing lifecycle and command
targeting, with hidden definitions retained by the workbench. One tool-only
layout persists in SQLite; fresh windows inherit it, and only explicit user
arrangements publish a new default. Document structure is never reproduced.

A window renders a product session rather than owning the workbench, so focus
and keys run without a window; focus is kept as Knot identities because gpui
reports it only for active windows.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md).

## Workbench navigation

Every pane and tab operation has a command. Splits record ratios while docks
keep absolute sizes. Ctrl-tab switching has no overlay and commits on release.
Directional commands use laid-out rectangles rather than painted bounds.
Preview tabs follow VS Code. Previews, the reopen stack, and maximize describe
a moment, so none is recorded; Reopen Closed Tab reuses the layout's per-kind
recreation.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Terminal

Use `alacritty_terminal` as a native surface separate from buffers. Ghostty VT
was rejected for its separate PTY integration, pre-1.0 FFI, Zig packaging, and
slow debug parsing (D063 revisits it). Sessions outlive presentations with one
attachment for unambiguous resize and input authority. Link opening is
HTTP(S)-only.

A new terminal starts in the focused terminal's directory, else the root of
the most recently focused document, else home; never the process directory,
which is `/` from Finder. A restored terminal reads its shell's directory from
the process, which needs no shell integration (D039, D128).

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md).

## Window layouts and restoration by position

One versioned JSON layout serves hot exit and extension sessions, so both share
capture, pruning, and installation. It holds no handles, only what recreates
each tab, and view state is byte offsets that clamp. Layouts are captured on
demand and announced by a debounced event rather than pushed on every change.

References: [workbench lifecycle](../architecture/workbench-and-lifecycle.md#window-layouts)
and [extension guide](../extension-api.md#project-sessions).

## Session replacement contract

`workbench.replaceWindows` is generic over windows, documents, and captures, so
any package can build the same workflow. It stages every read before one
protected foreground commit that revalidates revisions, so a racing edit rejects
with nothing changed. A live document always wins over staged content. Released
windows are closed and new ones created with their layout, keeping one creation
path and avoiding moving windows (D062).

References: [workbench lifecycle](../architecture/workbench-and-lifecycle.md#session-replacement)
and [documents](../architecture/documents-and-persistence.md#retained-captures).

## Native window hot exit and launch precedence

The application, not the project extension, records and restores windows
before launch requests, so a broken extension never loses them; projects then
rebind by root set. The record is consumed before restoring, trading the
arrangement for a guaranteed clean launch after a crash. Precedence is
recovered documents, recorded windows, then requests, deduplicated against
what was restored.

References: [workbench lifecycle](../architecture/workbench-and-lifecycle.md#native-window-hot-exit)
and [documents](../architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Application state database

Application UI state lives in one SQLite file with a table per record, so one
connection serializes writes and quit flushes them together. Recovery stays
separate: damaged recovery must be preserved and reported while damaged UI
state reads as absent, and a UI-state failure must never reach the text store.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#automatic-tool-layout-memory).
