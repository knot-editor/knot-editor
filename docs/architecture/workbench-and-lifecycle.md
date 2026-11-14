# Workbench and view lifecycle

Part of the [architecture](../architecture.md). [Decisions](../decisions.md)
records rationale; deferred work lives in the [roadmap](../roadmap.md).

## Product application launch

The macOS product is packaged as `Knot.app` for native activation, file-open,
and reopen callbacks. CLI and platform requests join a launch queue while
[personal config and extensions](extension-host.md#package-admission-and-startup)
start. Successful startup first restores
[unsaved documents](documents-and-persistence.md#durable-unsaved-document-recovery),
then the [recorded windows](#native-window-hot-exit), then opens each
queued request in its own window. Recovered documents that no restored window
presents go to the first opened window, so text is never hidden. With nothing
restored or requested one empty window opens. The last request window is
focused, else the previously active restored window, and the app activates.
When a package failed to start, each launch window opens the extension startup
report once; windows opened later do not.
`ApplicationSnapshot.launched` and a one-time `launched` event then tell
extensions that startup windows exist; extensions activate before any window
does. A reopen after launch opens an empty window and restores nothing. The
[zoom level](#zoom) is read before the first window opens. Fatal
config failure discards the queue and opens a focused diagnostic window with
Copy and Quit; windows remain unavailable for that launch, and nothing
is recorded.

The status bar carries no operation messages. A native operation whose failure
leaves no other visible effect, such as Save, Open, Add Folder, recovery, or creating a
transfer window, reports it through a native warning prompt that needs no
answer. Successful and in-progress operations show only their effect.
Extension calls receive the failure as their result instead of a prompt.

## Windows

A window (`KnotWindow` in `src/app/product.rs`) is what a native window
renders: it owns the workbench, Knot-side focus, the transient interaction and
palette, the key hold, and the key router. It runs with or without a native
window; only tests run it without one. A native window is the platform object
that renders a window, holds its bounds, activation, displays, and close
requests, owns the gpui focus handles, and forwards raw input to the window.
The application keeps a registry of open windows in `src/app/product.rs` that
records each window's workbench and, once attached, its native window, so
command targets and other windows can ask without reading a window that may be
updating. A window opens when it is created and ends when its native window
closes. Launch, restoration, tab transfer to a new window, and
[session replacement](#session-replacement) build the window first and then open
a native window for it. Extensions see a window's ID as `WindowTarget.window`.

Focus is window state, by identity: the focused pane's active surface, the
palette, or the transient interaction, over the workbench's focused pane. An
attached native window focuses the matching gpui handle once the current update
ends, and reports a click that moves gpui focus back to the window. A blur
leaves the window's focus alone.

Native prompts go through the window, which asks its native window; a window
without a native window answers nothing, which callers treat as cancellation.
The native window stays an adapter for raw input: key-downs and typed text
reach the window through it (see [keymaps](commands.md#keymaps)), and tab drags
and dock resizing are pointer input. Every surface's state, the editor, trees,
terminals, the palette, pickers, and text fields, runs without a native window,
so a window without one takes keys and text and gives each to the surface with
Knot-side focus, and keys after an extension command wait for it there too.
Extensions see a window without a native window as active once it is activated,
while no native window is active, which only tests do.

## Zoom

The application owns one zoom level in `src/app/zoom.rs`, from −4 to +6 steps
of 1.2, which Zoom In, Zoom Out, and Reset Zoom change for every window at once
and which persists as the `zoom` record of the state database. Each product
window sets its rem size from it, so chrome styled in rems scales, and surfaces
that measure in pixels multiply by the same factor: editors and terminals size
their text as `font.size` times the factor, and pixel chrome such as tabs, the
picker, and text inputs scales its constants. Dock sizes stay window pixels, as
on a window resize.

## Status bar

Each window renders a fixed-height bar below the workbench from the
application-owned status registry in `src/app/status_bar.rs`; extensions fill it
through `workbench.registerStatusItem` and own nothing else about it. The window
repaints it when the registry or any setting changes; an update that repeats
an item's value changes nothing, so packages may resend a value per key. A click captures the
window's command target and dispatches the item's command as a key or a tree
action does, so it runs in the clicked window. `statusBar.visible`, its Toggle Status
Bar command, and `statusBar.hiddenItems` are settings, and lifecycle teardown
removes a package's items. The editor a bar describes is `WindowInfo.statusTarget`
([extension host](extension-host.md#application-metadata-queries)).

The bundled `@knot/status-bar` package registers the built-in items on the right:
the cursor position with the selection size, counted in characters up to
1 MiB and in kilobytes beyond, the document's indentation, and its language. It follows each window's `statusTarget` with an editor handle and
sets the three items as that window's own values, so two windows show their own
editors; a window with no target has no values, and the items' empty shared
text hides them. Position updates from selection notifications, and indentation
and language from `documentChanged`, which every settings write publishes. Each
item's click runs a command: Go to Line, Change Indentation, and Set Language
Mode. Change Indentation offers VS Code's actions. Change Tab Display Size asks
for a size from 1 to 8 and writes the document's own `editor.tabSize`, which
works with focus anywhere. The others run the detect, convert, and reindent
commands on the focused editor, as the other two clicks do. Its "Indent Using"
entries are absent, since Convert Indentation also makes the style the
document's own. The package's tests run against the fake host and a product test
drives it through the real host.

## Workbench and protected closure

The layout model (`knot_model::workbench`) owns a window's areas: a center of
documents and terminals surrounded by left, right, and bottom docks of tools,
each a binary split tree of panes; stable pane/tab identities, ordered tabs,
and the focused pane. A tab holds the
identity of what it shows (a document, a terminal session, or a tool) and a
presentation identity, which captured targets compare. The window's workbench
entity pairs the layout with the view behind each presentation: it creates a
view when a transition opens a tab and drops it once no tab or tool presents
it. A document tab presents one `EditorView` of an application-owned document,
fresh whenever it opens, shows another document, or moves to another window. A
terminal tab presents the sole attached view of an application-owned session,
with no document model. A tool tab presents its tool's native frame, whose
definition and presentation survive hiding.
By default the side docks span the window's height and the bottom dock sits
under the center between them; `workbench.bottomDockLayout` set to `full` runs
the bottom dock across the window's width under the side docks. Painting,
directional navigation, and `Workbench::area_rects` share the arrangement. Each dock has an absolute window-local size, a visible
flag, and its own last focused pane; hiding a dock keeps its arrangement, and
focusing a pane of a hidden dock shows it. Toggling a dock or a tool follows
one pattern: a hidden one shows and takes focus, a shown but unfocused one
takes focus, and a focused one hides, returning focus to the center's last
focused pane. A tool's home is the dock it is in or last left, so moving it
to another dock rehomes it. Move Tab to Left, Right, or Bottom Dock and Move
Tab to Center move a tab to the end of that area's last focused pane, and
are unavailable when the tab is already there. Each tab kind declares the areas it
may occupy (`TabKind::allows`): documents the center, tools the docks, and
terminals either. Placement, relocation, directional moves, and restore check the
declaration, so a tool never joins the center and a document never enters a
dock. Thin stripes on the left and right window edges
(`workbench.toolStripes`) show a button for every tool by home dock, bottom-dock
tools at the left stripe's lower end, plus the terminal button in the terminal
home's group (`Workbench::stripes`). A click toggles the tool, or the dock
terminals. Like a stripe elsewhere, a click shows a hidden tool in place of the
other tools in its pane, which hide and keep their places; terminals stay, and
a tool already showing only takes focus, so tabs the user grouped stay grouped
(`Workbench::show_tool_replacing`). Programmatic shows and dock toggles add
tabs. The button's right-click menu shows the tool in a pane of its own beside
the dock's last focused pane (`Workbench::show_tool_beside`), hides it, or
moves it to another dock. Dragging a button to another stripe rehomes it,
moving a shown tool and leaving a hidden one hidden; the terminal button moves
every dock terminal to the target dock and makes it the terminal home.
Splitting a document creates another view; splitting a terminal creates
another session, in a dock as in the center. Empty panes collapse, keeping their siblings' ratios. The
center may be empty; it then paints a blank surface that takes keyboard focus
while no pane has it, opens new documents and terminals, and accepts dropped
tabs. When the focused pane goes, focus moves to another pane of its area, then
the center's last focused pane, else to the empty center; a dock takes focus
only when the user or a command focuses it, so a fresh window or an emptied
center never leaves keys aimed at a tool.

Each split gives a ratio of its extent to its first child, addressed by its
boundary pane, the first pane of its second child. Grow and shrink move a dock
pane's dock size, or a center pane's nearest split, by a step; equalize gives
every pane of a same-direction chain in the focused pane's area an even share. Each pane keeps its
tabs in recently activated order; a recent-tab walk (`workbench::RecentWalk`) shows tabs without
reordering them and commits one activation when ctrl is released. Pinned tabs
form a prefix of their pane, and every insertion, move, and restore keeps a tab
in its pinned or unpinned section. A document opened as a preview replaces the
pane's current preview in place and becomes regular when edited, pinned,
double-clicked, moved, kept open by command, or opened again without
`preview`; the
`workbench.previewTabs` setting turns previews into regular tabs. Directional
focus and moves find the neighbor across areas from pane rectangles laid out
from ratios and dock sizes over the painted workbench, so they work while a
pane is maximized; a move goes only to a neighbor whose area allows the tab,
and with none splits the pane on that side. Focus commands given
`{ withinArea: true }` stay in the focused pane's area, which keeps Vim's
`ctrl-w` keys among the center's editors. Cycling and first and last pane
commands walk the focused pane's area in layout order, and the workbench
remembers the previously focused pane. Resizing can be limited to the width or
the height, which changes the nearest split along that axis, or, along a
dock's axis, its size. Maximize is window render state that lasts while its pane stays focused, in a
dock as in the center; hiding a dock keeps its maximized pane, and showing the
dock with that pane focused maximizes it again.

One application-level coordinator protects tab, pane, window, and quit
closure. The window collects its open views and the documents' state
and runs the prompts, saves, and closes; which views a close covers, which
dirty documents need a prompt, and whether a plan still holds at commit are
decided by `knot_model::workbench::closure`. A close covers a tab; Close Other
Tabs a pane's unpinned tabs but the origin; Close Pane every tab of the pane, hiding tools in a dock, and Close Other
Panes every tab of the window's other panes in the origin's area. Window and
quit closes need no origin tab. The
coordinator captures exact window, workbench, tab, surface, and presentation identities,
counts live document views across windows, and deduplicates dirty documents
losing their final view. Native Save / Don't Save / Cancel prompts run serially;
Save reuses the ordinary persistence path. Quit is a hot exit: it first commits
every dirty document to [durable
recovery](documents-and-persistence.md#durable-unsaved-document-recovery) and
skips the prompts, keeping them only when that commit fails. An extension
closing one tab may ask to discard instead, as Vim's `:q!` does: its dirty
documents are approved as discarded at the revisions the close saw, without a
prompt, so recovery keeps their text until the close commits and a racing edit
keeps the tab open.

Presentation mutation waits until all decisions succeed, then revalidates the
capture, view counts, and approved content revisions. Cancellation, persistence
failure, racing edits, stale targets, and concurrent closure leave presentations
intact. An already accepted save remains committed if closure later cancels.
Terminal sessions stop only after the scope transition succeeds. Native close
requests are vetoed until authorized; Quit uses the same flow.

Workbench transitions remain independent of dialogs and document removal. The weak
window registry counts views in open windows without retaining windows or
workbenches. Final application document removal explicitly closes its model,
invalidating transport access even if a stale view retains the entity. Nonfinal
view closure leaves the shared document open. Closing the final document or
terminal leaves an empty center, beside any docks, and keeps hidden tool
definitions; no placeholder document replaces it. Closing a tool hides its
attachment without entering document confirmation. Close Tab with no focused
tab, as in an empty center, closes the window through window closure. A window
with no tabs closes without a prompt.

Each window keeps its last 20 closed document and terminal tabs (`workbench::RecentlyClosed`) as layout tabs
with their pane and position. Reopen Closed Tab recreates the latest through
the [layout](#window-layouts) per-kind step, in its pane when that still exists
and otherwise in the center; untitled documents and tools are not kept.

Native commands and extension APIs adapt the coordinator's semantic result.
Extension closure additionally revalidates caller lifecycle/cancellation before
save and close admission. Completion follows the foreground transition without
awaiting observation listeners.

## Window layouts

One versioned `WindowLayout` value serves both restorers: native window hot exit
and extension sessions through `workbench.captureLayout`. It records logical
window bounds, windowed, maximized, or fullscreen state, and the display UUID;
the center's split tree and each dock's size, visibility, and split tree, with
each split's ratio; ordered tabs, which are pinned, and the selected tab per
pane; the focused pane; and each editor tab's view state. Preview, maximize, recent order, and the reopen stack
are not recorded, so restored tabs are regular. Every tab kind is
restored by position and records only what recreates it: a document tab its
document identity, resource URI, and explicit language, a tool tab its stable
tool ID, and a terminal tab its shell's working directory, so a restored
terminal starts a fresh shell there. The value never contains pane, tab, view,
or window handles.

Restoring plans a pruned arrangement in one place for both restorers. Document
tabs resolve to the window's entry with the same document identity, else the
same normalized URI. Unmatched documents, unregistered or repeated tools, tabs whose kind the area
does not allow, and repeated documents within a pane are dropped, then empty
panes and one-child splits collapse; an out-of-range selection or focus falls
back to the first tab or the center.
Each surviving tab is then recreated by its kind's step (a fresh editor view of
the shared model, the registered tool frame, a new terminal session in its
recorded directory, or the window's default when that no longer exists), so a
split of one document restores as independent views of one model. A recorded
language becomes the document's explicit language unless the live document
already has one. Tools the
layout does not name are hidden and keep their remembered placement; entries
it does not place open in the center. Restoration never publishes the
[tool-only default](#automatic-tool-layout-memory). An unsupported version or
invalid shape is ignored and the entries are arranged as without a layout.

Restored windows are always created, so geometry is applied at creation and
moving an existing window stays deferred. Saved bounds are kept when enough of
their title bar lies on a connected display; otherwise the window is sized to
fit and centered on its saved display if connected, else on the primary one.

Cursor, selection, and scroll changes, focus, dock resizing, window bounds, and
other window changes restart a per-window trailing-edge debounce of about a
second. When it settles the window captures its layout: a layout that differs
from the last one it reported sends a `viewStateChanged` observation event, and
a changed layout or root set re-records the window for
[hot exit](#native-window-hot-exit).

## Session replacement

`workbench.replaceWindows` closes a set of windows and opens their replacements
in one foreground commit. It knows windows, documents, roots, layouts, and
retained captures, never projects; the
[project registry](../extension-api.md#project-sessions) builds its switch on
it, and native startup reuses it with no released windows. Admission claims the
protected-closure slot, so it never interleaves with closure prompts, quit, or
another replacement, and captures every released window. Staging validates
destination roots, reads files, and loads retained captures off the foreground
without creating documents, so observers see nothing until the commit.

The commit first revalidates: the caller is live, every released window is
live, the documents presented in the released windows equal the caller's
suspended set exactly, and each document whose final view is removed, when dirty
or a non-empty untitled document, names a capture retained at its current
revision. A mismatch rejects with `SessionConflict` and nothing changes. The
commit then creates every destination window with its geometry, so a failed
creation leaves the sources intact, and closes every released window as any
window closes, stopping its terminals. Each destination installs its roots, then
its [layout](#window-layouts) or, without one, presents its entries in the
center, which stays empty without entries. Documents left with no view are
removed through the ordinary path, which retires their automatic recovery. Each
window's result reports whether its layout was applied or ignored.

Each entry presents a live document matching its identity or resource before
considering staged content, so a replacement never duplicates an open document;
otherwise its capture is restored or its file opened. A missing clean file is
reported and skipped, and a failed entry is skipped with its error code while the
rest commits. Restored dirty documents are drained to durable automatic recovery
before the operation resolves, so the caller can release the captures they came
from.

## Native window hot exit

The application records every window's roots and layout in one record of the
application state database, beside the tool-only snapshot, with
the same debounce and flush. Recording starts, with a first record of the windows just opened, once
startup has opened them, so diagnostic windows and a launch whose config failed never write.
Settled window changes and window closure re-record every window in
creation order; closing a window removes it, so quitting with no windows
restores none. Quit records the windows before closing them and stops
recording, and the quit path flushes the record before exit. Except on macOS,
where the application outlives its last window, closing the last window is a
Quit, so it restores like one; a window closed without protected closure,
such as one with no panes, quits through the same storage drain.

Startup marks the record consumed durably before restoring it through the
replacement commit, so a crash during restoration loses only the arrangement and
the next launch opens the default window. Documents resolve live by identity
first, so restored recovery documents are presented in place, then by opening
their file; untitled documents nobody recovered are pruned, and roots that no
longer exist are dropped. Unsaved text keeps its own recovery path.

## Tab relocation

`Workbench` owns typed reorder, insertion, and edge-split transitions. Windows
coordinate transfers across workbenches on the foreground. Captures include
pane, tab, surface, and exact presentation; destinations retain a live
insertion/split anchor. Both endpoints and document/window ownership are checked
before mutation. Capturing or abandoning a move changes neither ownership nor
focus.

Same-pane reorder preserves tab identity. A moved tab stays in its pinned or
unpinned section and stops being a preview. Moving between panes preserves the
payload, collapses empty source panes, and allocates destination-local tab/pane
identities. Editors retain their view entity and scroll state; pending caret
reveals still apply. Terminal cross-window moves hand off the sole attachment to
a destination view of the same session; same-window moves retain their view.
Replacement or pane/window transfer invalidates old exact command targets.

Native tools stay in their owning window and in the docks. A drag
offers targets only in areas its tab's kind allows: dropping near a workbench
left/right/bottom edge moves a dock-capable tab to the end of that dock, while
other kinds keep the pane-local edge split there, and panes or an empty center
of a disallowed area show no target and refuse the release. Releasing a tool
outside all Knot frames cancels without creating a window. Dock-adjacent dividers drag to resize the
window-local dock size up to the window edge, keeping only the dock minimum;
other dividers drag the split's ratio, leaving each side a minimum size.

Transfers do not enter protected closure or remove documents or windows. An
emptied source keeps an empty center. New-window transfer creates and validates
an empty destination window and its native window before removing the source;
failure preserves the source and removes an unused new destination. The destination activates and
focuses the moved tab. On macOS a new terminal window is laid out before showing.

A transient application-wide drag coordinator holds weak endpoints and painted
window-local geometry. macOS delivers pressed drags to the originating window,
so events map to screen coordinates for hit testing against the foremost Knot
window and for preview positioning. Transfers run after input dispatch so both
windows are available for revalidation. Source closure or Escape cancels drag
state; destination closure removes its geometry. No coordinator owns window
lifetime.

Tab bars accept insertion, pane bodies accept append or edge splits, and an
empty center accepts a document or terminal as its first pane. Invalid
space inside Knot frames rejects a drop; release outside all Knot frames creates
a window near the pointer. Preview rendering, hit-test thresholds, workbench-edge dock targets, divider
drag handling, and coordinate fallbacks live in the [relocation
code](../../src/app/product/tab_drag.rs).

## Automatic tool-layout memory

One application-owned `ToolLayoutStore` holds a single versioned tool-only
snapshot in the application state database: one SQLite file with a table per
UI-state record and one connection that serializes their writes, which also
backs [native window hot exit](#native-window-hot-exit). Quit flushes every
pending record. Unsaved-text recovery keeps its own database, because a
damaged recovery file is preserved and reported rather than treated as empty. It records each dock's size, visibility, and split tree of stable namespaced
tool IDs with each pane's selection, and the dock each hidden tool returns to,
so each tool's home reaches fresh windows, and the terminal home dock. Documents,
terminals, the center, and live handles never enter the record.

A fresh window takes the stored dock sizes and gives each tool its stored place
as the tool registers, as packages attach. While a dock still holds
exactly the record's arrangement of the tools registered so far, the record's
tree is installed over it, so tools attaching one by one in any order rebuild
the stored arrangement and selection; a dock the user has changed meanwhile is
joined at its end instead. Already-open windows keep their live arrangements. Only explicit
user arrangements publish a new default: moves, dock resizes, and show/hide
changes, including a command-invoked extension show, hide, or focus. Divider
dragging coalesces through a trailing-edge debounce so one settled arrangement
reaches storage, and identical publishes skip storage. Restore, window
activation, listener callbacks, automatic presentation, and document structure
changes never publish. The cache updates synchronously so the next window
inherits immediately; writes run off the foreground and the quit path flushes a
pending write before exit. Reset Layout returns to product defaults, every
tool to its declared dock. A missing,
corrupt, or version-mismatched record falls back to defaults, and unknown stored
IDs are skipped without creating blank frames.

Tool frames stay workbench-owned behind stable namespaced IDs and explicit
native content variants. Extension contributions attach through this contract
instead of receiving layout storage or persisting live handles. A contributed
frame currently provides either a semantic tree or read-only text; text content
owns a private buffer rendered by the editor and never enters the document
collection or persistence.

## Workspace roots

Root and directory request validation belongs to
[documents and workspaces](documents-and-persistence.md). The file explorer is
the bundled `@knot/explorer` package, an ordinary extension tool; see
[extension trees](semantic-ui.md#extension-trees) for the surface it uses.

Native Open retains its captured target through the picker. Platforms unable to
select files and folders together first ask which kind to open; cancellation
leaves the workbench intact. Opening a folder replaces every root. Add Folder to
Workspace appends one through the window's transient folder prompt. Each edit
replaces the whole set computed from live foreground state.

## Terminal sessions and views

A terminal tab lives in the center or in any dock, as an ordinary flat tab. The
terminal home is the dock new terminals open in when none lives in a dock; it
starts at the bottom, and moving a terminal tab into a dock makes that dock the
home, which the tool-layout record carries to fresh windows. The workbench
tracks dock terminals by recent focus. Toggle Terminal (ctrl-`) shows and
focuses the most recently used dock terminal, hides its dock when it is already
focused, and with no dock terminal opens one in the home dock; center terminals
are ignored. New Terminal opens beside the focused terminal wherever it is,
otherwise beside the most recently used dock terminal, else in the home dock,
and shows the dock; New Terminal in Center opens in the last focused center
pane. `terminals.open` takes the same choice as `location`. Window layouts
restore dock terminals in their docks, while resetting the layout or
rebuilding a dock from the tool record moves the dock's terminals to the center.

The application registry owns stable `TerminalSession`s. A session owns its PTY,
libghostty terminal, process status, authoritative size, session preferences, and
event delivery. The [knot-terminal](../../crates/knot-terminal/src/lib.rs) PTY
reads and writes on its own threads; the reader feeds output to the terminal
under a mutex, and the foreground holds that lock only to update the render
state it paints from. Repaint wakeups coalesce; protocol requests, replies, and child exit retain order
in a process-generation-scoped queue. Restart replaces the process and terminal and
rejects old events, carrying the prior grid's lines into the new session
above a `--- terminal restarted ---` divider with new output continuing
below. Shutdown and child reaping run off the foreground.

A new session's working directory is chosen when it is created and kept across
restart. New Terminal, splitting a terminal pane, and `terminals.open` without
a directory start in the focused (or split) terminal's current directory.
Otherwise the window picks the innermost root containing the window's
most recently focused document, otherwise the first root, otherwise the home
directory, skipping roots that are not existing local directories. A terminal
tab restored in place starts in its recorded directory, or by this rule when
that no longer exists; restoration installs a window's roots before its layout,
so restored shells see them. `terminals.open` also accepts an explicit
directory. The rule lives in
[terminal_directory.rs](../../crates/knot-model/src/terminal_directory.rs).

A session reports its shell's current working directory: the last local
directory the shell reported through OSC 7, 9, or 1337 since it started
([`local_directory`](../../crates/knot-terminal/src/directory.rs)), otherwise
what querying the shell process returns on macOS and Linux
([terminal/process.rs](../../src/app/terminal/process.rs)), otherwise the
directory it started in. A directory on another host counts as unreported. A
captured layout records it, so quit and project switches see the latest `cd`,
while a crash restores the directory of the last debounced capture.

`TerminalView` owns focus, layout, scroll interaction, drawing, and input. An
attachment lease gives one view input/resize authority. Releasing it does not stop
the session; natural exit retains the final grid, and startup failure remains
visible and restartable. Protected closure removes affected registry entries and
stops their sessions after document decisions succeed.

Measured view geometry supplies drawing, baseline, cell hit testing, viewport
size, and PTY pixel dimensions at the destination window's scale. Cells are
measured for `font.size` and the `font.*` family settings resolved under
`{ view: "terminal" }`, with the size times the zoom factor, so a change
resizes the PTY grid. Fallback glyphs
stay within emulator cell allocations. Selection is the terminal's own, tracked
across scrolling; application mouse reports use viewport cells. Geometry is remeasured after resize
or attachment handoff. Typography and bootstrap metrics live in
[terminal layout](../../src/app/terminal/layout.rs) and
[session](../../src/app/terminal/session.rs).

Keyboard input follows command/keymap precedence. Keys the keymap leaves to
the terminal become key presses that libghostty encodes in the protocol the
program requested, legacy, the Kitty keyboard protocol, or modifyOtherKeys;
Option acts as Alt, Option-Left and Option-Right send the shell word
motions as in Ghostty, and Command and Super chords never reach the program
([keyboard input](../../src/app/terminal/input.rs),
[encoders](../../crates/knot-terminal/src/input.rs), and the
[keyboard fixture](../../scripts/terminal-keys-fixture.py)).
Focus reports require both exact view focus and native window activation. Detach
reports focus loss before handoff; stopped or detached views cannot write input.
Keyboard/paste reveals the live grid, while focus reports and emulator replies
preserve scrollback. Paste uses the PTY channel, normalizing plain input and
sanitizing bracketed paste. OSC 52, title, and bell events do not access product
features or the clipboard; additional protocols remain deferred in the roadmap.

A mouse press chooses application reporting or local selection through release,
even if modifiers change. Shift forces local interaction. Wheels choose their
current route independently. The session-local reporting toggle survives restart
and handoff without changing application-requested modes. Disabling it balances
active reports before local selection takes over. Focus, geometry, detach, restart,
and mode changes invalidate gestures without sending old-process releases to a
replacement. Gesture ownership lives in
[terminal/mouse.rs](../../src/app/terminal/mouse.rs); libghostty's mouse
encoder chooses the reported events, wire format, and coordinate limits from
the terminal's modes. A release past a format's limit is reported at the last
reported position. The [mouse fixture](../../scripts/terminal-mouse-fixture.py)
supports native acceptance.

Link lookup uses the painted frame's cells, soft wraps, and OSC 8 destinations;
cells with the same OSC 8 URI form one link. Explicit destinations take precedence; unsupported explicit schemes suppress text
detection. Only complete HTTP(S) destinations are actionable. Modifier-click owns
the gesture ahead of mouse reporting; release revalidates the displayed
cells, process, destination, and geometry. Output or lifecycle changes invalidate armed
links. Link recognition lives in [terminal/links.rs](../../src/app/terminal/links.rs).

## Observing product state

Snapshots derive associations and focus from live windows/workbenches rather than
replicating ownership. Committed layout, relocation, focus, activation, and closure
transitions signal the [application observer](extension-host.md#application-observation).
Diagnostic windows are excluded; terminals and tools remain explicit
non-document surfaces with no buffer/document association. Tool focus participates
in captured command targeting.
