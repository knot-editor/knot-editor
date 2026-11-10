# Workbench and view lifecycle

Part of the [architecture](../architecture.md). [Decisions](../decisions.md)
records rationale; deferred work lives in the [roadmap](../roadmap.md).

## Product application launch

The macOS product is packaged as `Knot.app` for native activation, file-open,
and reopen callbacks. CLI and platform requests join a launch queue while
[personal config and extensions](extension-host.md#package-admission-and-startup)
start. Successful startup first restores
[unsaved documents](documents-and-persistence.md#durable-unsaved-document-recovery),
then the [recorded product windows](#native-window-hot-exit), then opens each
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
Copy and Quit; product windows remain unavailable for that launch, and nothing
is recorded.

A product window has no status line. A native operation whose failure leaves no
other visible effect, such as Save, Open, Add Folder, recovery, or creating a
transfer window, reports it through a native warning prompt that needs no
answer. Successful and in-progress operations show only their effect.
Extension calls receive the failure as their result instead of a prompt.

## Product sessions

A product session (`ProductShell`) is what a window shows: it owns the
workbench, Knot-side focus, the transient interaction and palette, the key
hold, and the key router, and runs without a window. Every product window
renders one session. The application keeps a registry of open sessions in
`src/app/product.rs` that records each session's workbench and, once
attached, its window, so command targets and other sessions can ask without
reading a session that may be updating. A session opens when it is created
and ends when its window closes. Launch, restoration, tab transfer to a new
window, and [session replacement](#session-replacement) build the session
first and then open a window for it. Extensions see a session's ID as
`WindowTarget.window`. "Session" alone in the rest of this reference may also
mean a project's or a terminal's session; the context says which.

Focus is session state, by identity: the focused pane's active surface, the
palette, or the transient interaction, over the workbench's focused pane. An
attached window focuses the matching gpui handle once the current update
ends, and reports a click that moves gpui focus back to the session. A blur
leaves the session's focus alone.

Native prompts go through the session, which asks its window; a session
without a window answers nothing, which callers treat as cancellation.
The window stays an adapter for raw input: key-downs and typed text reach the
session through it (see [keymaps](commands.md#keymaps)), and tab drags and
dock resizing are pointer input. Every surface's state, the editor, trees,
terminals, the palette, pickers, and text fields, runs without a window, so a
session without one takes keys and text and gives each to the surface with
Knot-side focus, and keys after an extension command wait for it there too.
Extensions see a session without a window as active once it is activated,
while no window is active, which only tests do.

## Zoom

The application owns one zoom level in `src/app/zoom.rs`, from −4 to +6 steps
of 1.2, which Zoom In, Zoom Out, and Reset Zoom change for every window at once
and which persists as the `zoom` record of the state database. Each product
window sets its rem size from it, so chrome styled in rems scales, and surfaces
that measure in pixels multiply by the same factor: editors and terminals size
their text as `font.size` times the factor, and pixel chrome such as tabs, the
picker, and text inputs scales its constants. Dock sizes stay window pixels, as
on a window resize.

## Editor views

An editor is two entities in `src/app/editor/` over plain state in
`knot-model` (`crates/knot-model/src/editor.rs`). `EditorState` holds one
presentation's state: cursor and selection, scroll and the viewport in rows,
wrapping, folds, IME preedit, key contexts, the caret shape, and its
completion controller, and it runs the native editing and movement commands
without a window or runtime. Each operation takes the buffer model it reads
or edits; the editor follows the settings its owner resolves for it and
queues clipboard writes and document setting changes as effects, and paste
receives the clipboard's text. The `Editor` entity wraps the state: before
each operation it resolves the settings for the editor's scope, then runs the
effects, notifies the buffer when its text changed, and notifies itself; it
also follows buffer changes and keeps the in-flight completion requests. The
`EditorView` renders it: it owns focus, painting, pointer gestures,
the platform input handler, shaped hit testing, and horizontal scrolling, and
each layout tells the editor its viewport height in rows and its wrap width in
cells. A document tab's presentation owns the view, which owns its editor; commands, editor
handles, and completion reach the `Editor`. Multiple editors share a model
while retaining independent presentation state. Completion ownership is
detailed in [semantic UI](semantic-ui.md#completion-ownership-and-aggregation).

A view keeps no copy of the text. It reads the lines it paints, measures, or
edits from the model, and converts positions, including the platform's
document-wide UTF-16 offsets, through the buffer's line index, so editing cost
does not grow with document size. A test guards that editing, navigation, IME,
scrolling, and painting never read the whole document.

Movement and deletion respect grapheme boundaries, and vertical movement
keeps the display cell it started from, counted with `unicode-width` and
expanded tabs, across shorter lines. Words are Unicode word segments split at VS
Code's default word separators, so each run of separators such as `.` or `::`
is its own unit; word movement stops at the edges of words and separator runs,
and double-click selects one. Dragging after a double- or triple-click keeps
the clicked word or line selected and extends by that unit in either
direction. Text,
selection, caret, hit testing, and IME bounds share one shaping projection and
horizontal offset. The caret never sits inside a `\r\n` terminator, and
inserted newlines from typing, paste, and IME take the buffer's line ending.
Editing/navigation reveal the caret; wheel scrolling may move away from it, up
to the widest line shaped since the text last changed. Clipboard operations use
source text; expanded tabs are only a rendering projection. Tab stops follow
`editor.tabSize`, which the view resolves while laying out for its scope: its
document's effective language and the `editor` view kind, or a contributed
text frame's view ID. Views observe the settings store and repaint on any
change. Text size is `font.size` in the same scope times the zoom factor, and
line height follows it at 20/14, rounded to whole pixels; a size change keeps
the top line in place.

With `editor.softWrap` resolved true for its scope, a view lays each line out
as rows that fit the text area, measured in monospace cells rather than shaped
(`crates/knot-model/src/editor/wrap.rs`). Every geometry path counts rows: scrolling and
clamping, painting, the caret, selections, hit testing, IME bounds, the
completion popup, and the gutter, which shows a line's number and marker on
its first row. Each row is shaped as its own slice of the line, and nothing
scrolls sideways while wrapping. The view rewraps only the lines the model
reports changed since it last followed it
([core and buffers](core-and-buffers.md#application-buffer-ownership)), and
every line when the width, text size, zoom, or tab size changes, keeping the
top row's text at the top. A caret at an offset where a row breaks shows at
the start of the next row. Toggle Soft Wrap writes the opposite of the
resolved value scoped to the view's document, so every view of the document
follows and the value ends with it; a view without a document reports the
command unavailable.

Up, Down, Page Up and Down, Home, smart Home, and End move and select by row,
keeping the caret's display cell across rows; End on a row that does not end its line
stops before its last character, whose end shows on the next row. Each has a
`-logical` twin, such as `editor.move-up-logical`, that moves by buffer lines
and, for vertical movement, lands on the target line's first row. Without
wrapping the two agree. The macOS preset binds Cocoa's ctrl-A and ctrl-E to
the twins, as they reach a paragraph's edges; other keys use the row
commands. A view's own edit
refreshes its derived state synchronously before resolving the resulting caret.
Other views update through their persistent positions.

Extension commands captured in a document editor reach it through an editor
handle ([commands](commands.md#extension-execution)) that reads and moves its
selection and saves and restores its view state. Moving the selection ends
the typing undo group as other caret moves do; a centered reveal recenters
the caret's row only when it lies outside the viewport and otherwise behaves
like ordinary movement, an always-centering reveal centers it regardless, and
top and bottom reveals always align the caret's line with that edge: a top
reveal shows a line's first row and a bottom reveal its last. The handle
counts rows: it reports the first row wholly visible in the last layout and
how many are, which may lag a scroll by a frame, scrolls by rows without
moving the caret or asking the next paint to reveal it, and maps a document row
to its buffer line and start column and a line and byte column to its row. Without wrapping,
rows and lines coincide. Go to Line is the bundled `@knot/go-to-line`
package built on that handle, bound to ctrl-G in the Knot preset.

Editing commands such as Enter, indent and outdent, comment toggling, and the
line commands compute every edit first and commit them as one batch that
records the view's selection before and after, so each command is one undo
step that restores the selection it started from. Commands that act on lines
take every line the selection touches, except a last line it reaches only at
column 0. The indentation unit is a tab or `editor.tabSize` spaces, resolved
from `editor.insertSpaces` in the view's scope, which includes its document:
a document entering the collection from a resource writes the style its
leading lines show as its own document-scoped value, and any later write to
that scope replaces it. Detect Indentation from Content detects again, dropping
the document's own values when the text shows no clear style. The two Convert
Indentation commands rewrite every line's leading whitespace at the same widths
as one undo step and write the same value; they never change how deep a line is
indented. Reindent Buffer recomputes every line from the language's indent query,
at `editor.tabSize` per level, and Reindent Selected Lines only the selection.
Backspace in leading spaces deletes back to
the previous tab stop. Comment
toggling reads the editing data of the language at the selection start,
including an injected one such as a Markdown code fence's
([semantic UI](semantic-ui.md#syntax-highlighting-and-languages)).

Enter commits the line break with the indentation before the caret, then the
view keeps a request to indent the new line from the language's indent query
([semantic UI](semantic-ui.md#syntax-highlighting-and-languages)). The request
applies once the syntax tree is a parse of the text at the same revision,
right after the commit when the foreground parse finishes within its budget,
otherwise when a background parse does, and any later commit drops it. The
correction joins Enter's undo step through the model's amend of the last
history entry. Between a bracket pair whose `newline` flag is set, Enter opens
a middle line, indented from the query or one unit deeper, with the closer
below at the opening line's indentation; on a line holding only whitespace,
Enter clears it. Typing that completes a closing or branching capture at the
start of a line's content, such as `}` or Python's `else:`, outdents the line
the same way within the typing group, never indenting it further; composition
and paste never reindent. Reindent Selected Lines applies the query to the
selected lines as one step, and Reindent Buffer to every line.

Brackets follow VS Code's language-defined behavior, read from the bracket
pairs of the language at the caret
([semantic UI](semantic-ui.md#syntax-highlighting-and-languages)). With the
caret next to a bracket, the view paints it and its partner in
`editor.bracketMatch`, caching the match per syntax generation and caret so
painting walks no tree; the bracket before the caret wins. Jump to Matching
Bracket moves before the partner, or before the closer of the innermost pair
around the caret. Typing an opener over a selection wraps it, as one undo
step, and keeps the inner text selected. Typing an opener without a
selection inserts its closer in the same typing commit when the pair
auto-closes, the next character is whitespace, the line end, or in
`autoCloseBefore`, the caret is not in a string or comment the pair lists in
`notIn`, and, for quotes, no word character precedes it. The view tracks
each pair it closed as an anchored range while the caret stays between
opener and closer: typing the tracked closer steps over it and Backspace
between the empty pair deletes both. Leaving the pair, selecting, or text
that no longer holds the pair drops the tracking, after which closers are
ordinary text. Composition never closes or wraps. `editor.autoClosingBrackets`
and `editor.autoSurround`, resolved for the view's scope, turn the two off.

The gutter left of the text is an ordered row of native lanes: extension
markers, line numbers, then folds. The view lays the lanes out before each paint, so
text bounds, hit testing, and painting read one layout. The marker lane has a
fixed width that stays reserved, so text never shifts when a marker appears;
the number lane fits the digits of the line count and is absent when
`editor.lineNumbers`, resolved for the view's scope, is `"off"`. Relative
numbers count from the caret's line, which shows its absolute number, and the
caret line's number paints in `editor.lineNumber.active`. As VS Code's default
does, the caret's line paints `editor.lineHighlight` across the text area
while nothing is selected, focused or not. Each lane handles its own pointer:
a number click selects its line with the line break, shift-click extends from
the selection's anchor, and dragging extends by lines; a marker click runs
the command of the marker painted there and never selects; a fold-lane click
toggles the fold or region starting on that line, Alt-click also its nested
ones, and never selects. The fold lane draws chevrons on foldable lines while
the pointer is over the gutter, or always when `editor.showFoldingControls` is
`"always"`, and always one on a collapsed fold's line; `editor.folding`, resolved
for the view's scope, turns the lane off. Documents always
have the gutter, while a contributed text frame has none unless its
registration asks for one.

A view folds line regions, which stay per view. A collapsed fold keeps its
first line visible, with an `editor.foldPlaceholder` `…` after it, and gives
its other lines no rows: the wrap map treats hidden lines as zero-row lines, so
the viewport, scrolling, row queries, and painting count only visible rows.
Each fold is an anchored byte range from the first line's start to the last
line's content end, so edits keep it over its text, and one left with fewer
than two lines is dropped. Folds may nest, and the hidden lines are their
union. The caret never rests in hidden lines: vertical and horizontal moves
step over a fold, folding moves a caret inside it to the first line, selecting
a fold's first line by pointer selects the whole fold, line commands take a
collapsed fold's lines with its first line, and anything that puts the caret
in hidden lines (Go to Line, an extension's `setSelection`, undo) expands the
folds hiding it; clicking a fold's placeholder expands it. The Fold, Unfold, Toggle Fold, Fold All, and Unfold All
commands act on the [regions](semantic-ui.md#syntax-highlighting-and-languages)
the language offers; Fold collapses the innermost uncollapsed region that
starts on the caret's line or encloses it. Extensions fold exact line ranges
through the [editor handle](../extension-api.md).

Restorable view state is the anchor and caret as byte offsets, the start of the
top visible row as a byte offset, and horizontal scroll in pixels. Applying it
clamps each offset to the current text and snaps back to a character boundary,
so a changed or shorter file never yields an invalid position, and keeps the
restored scroll instead of revealing the caret; a state restored before the
view first wraps its lines keeps its row once they wrap. It also records the
collapsed folds as byte ranges, from the first line's start to the last line's
content end; restoring replaces the view's folds with them, clamped to the text
and dropped once they span fewer than two lines. The editor handle's view state
carries no folds and leaves them as they are. Zoom is application state and
wrapping follows settings, so neither is recorded.

## Status bar

Each product window renders a fixed-height bar below the workbench from the
application-owned status registry in `src/app/status_bar.rs`; extensions fill it
through `workbench.registerStatusItem` and own nothing else about it. The shell
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

The layout model (`knot_model::workbench`) owns a session's areas: a center of
documents and terminals surrounded by left, right, and bottom docks of tools,
each a binary split tree of panes; stable pane/tab identities, ordered tabs,
and the focused pane. A tab holds the
identity of what it shows (a document, a terminal session, or a tool) and a
presentation identity, which captured targets compare. The session's workbench
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
moving a shown tool and leaving a hidden one hidden, and changing only the
terminal home for the terminal button.
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
dock's axis, its size. Maximize is shell render state that lasts while its pane stays focused, in a
dock as in the center; hiding a dock keeps its maximized pane, and showing the
dock with that pane focused maximizes it again.

One application-level coordinator protects tab, pane, window, and quit
closure. The product session collects its open views and the documents' state
and runs the prompts, saves, and closes; which views a close covers, which
dirty documents need a prompt, and whether a plan still holds at commit are
decided by `knot_model::workbench::closure`. A close covers a tab; Close Other
Tabs a pane's unpinned tabs but the origin; Close Pane every tab of the pane, hiding tools in a dock, and Close Other
Panes every tab of the session's other panes in the origin's area. Window and
quit closes need no origin tab. The
coordinator captures exact session, workbench, tab, surface, and presentation identities,
counts live document views across sessions, and deduplicates dirty documents
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
session registry counts views in open sessions without retaining sessions or
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
shell changes restart a per-window trailing-edge debounce of about a second.
When it settles the window captures its layout: a layout that differs from the
last one it reported sends a `viewStateChanged` observation event, and a
changed layout or root set re-records the native window session.

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
considering staged content, so a session never duplicates an open document;
otherwise its capture is restored or its file opened. A missing clean file is
reported and skipped, and a failed entry is skipped with its error code while the
rest commits. Restored dirty documents are drained to durable automatic recovery
before the operation resolves, so the caller can release the captures they came
from.

## Native window hot exit

The application records every product window's roots and layout in one
record of the application state database, beside the tool-only snapshot, with
the same debounce and flush. Recording starts once startup has opened its
windows, so diagnostic windows and a launch whose config failed never write.
Settled window changes and window closure re-record every product window in
creation order; closing a window removes it, so quitting with no windows
restores none. Quit records the windows before closing them and stops
recording, and the quit path flushes the record before exit.

Startup marks the record consumed durably before restoring it through the
session commit, so a crash during restoration loses only the arrangement and
the next launch opens the default window. Documents resolve live by identity
first, so restored recovery documents are presented in place, then by opening
their file; untitled documents nobody recovered are pruned, and roots that no
longer exist are dropped. Unsaved text keeps its own recovery path.

## Tab relocation

`Workbench` owns typed reorder, insertion, and edge-split transitions. Product
sessions coordinate transfers across workbenches on the foreground. Captures
include pane, tab, surface, and exact presentation; destinations retain a live
insertion/split anchor. Both endpoints and document/session ownership are checked
before mutation. Capturing or abandoning a move changes neither ownership nor
focus.

Same-pane reorder preserves tab identity. A moved tab stays in its pinned or
unpinned section and stops being a preview. Moving between panes preserves the
payload, collapses empty source panes, and allocates destination-local tab/pane
identities. Editors retain their view entity and scroll state; pending caret
reveals still apply. Terminal cross-window moves hand off the sole attachment to
a destination view of the same session; same-window moves retain their view.
Replacement or pane/window transfer invalidates old exact command targets.

Native tools stay in their owning product window and in the docks. A drag
offers targets only in areas its tab's kind allows: dropping near a workbench
left/right/bottom edge moves a dock-capable tab to the end of that dock, while
other kinds keep the pane-local edge split there, and panes or an empty center
of a disallowed area show no target and refuse the release. Releasing a tool
outside all Knot frames cancels without creating a window. Dock-adjacent dividers drag to resize the
window-local dock size within usable limits; other dividers drag the split's
ratio, leaving each side a minimum size.

Transfers do not enter protected closure or remove documents/sessions. An emptied
source keeps an empty center. New-window transfer creates and
validates an empty destination session and its window before removing the source; failure preserves the
source and removes an unused new destination. The destination activates and
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
as the tool registers: the explorer when a folder opens or a customized record
shows it, contributed tools as their packages attach. While a dock still holds
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

## Native resource tree

`WorkspaceTree` owns cached native filesystem presentation state. It emits
normalized resource requests and workspace generations to the shell, independently
of the extension `TreeView` provider protocol. The explorer is native tool
content in the workbench; the shell owns its resource event subscription.
Every root is a top-level row in root order, labelled by its last path segment
plus the parent segment when names collide. Directory listings are cached by URI
and shared between overlapping roots, while expansion and selection are keyed by
root and URI so a nested root browses independently. Changing workspace roots
keeps retained roots' expansion, selection, and listings, re-requests directories
that were loading under the stale snapshot, and never changes placement or
visibility. Hiding preserves selection, expansion, and the live native view.
Reopening returns to the pane and tab position it left while that pane is
still in its dock, else to its dock. A single click opens a file as a preview
tab, and a double click or Enter as a regular one. Files opened from a dock go
to the center, creating its first pane when it is empty. Root and directory request
validation belongs to [documents and workspaces](documents-and-persistence.md).

Native Open retains its captured target through the picker. Platforms unable to
select files and folders together first ask which kind to open; cancellation
leaves the workbench intact. Opening a folder replaces every root. Add Folder to
Workspace appends one through the window's transient folder prompt, and Remove
Folder and Move Folder Up/Down act on the explorer's selected root (the root row
or the root above the selection). Each edit replaces the whole set computed from
live foreground state.

## Terminal sessions and views

A terminal tab lives in the center or in any dock, as an ordinary flat tab. The
terminal home is the dock new terminals open in when none lives in a dock; it
starts on the right, and moving a terminal tab into a dock makes that dock the
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
Alacritty grid, process status, authoritative size, session preferences, and event
delivery. Alacritty reads, parses, and writes on its background event loop.
Repaint wakeups coalesce; protocol requests, replies, and child exit retain order
in a process-generation-scoped queue. Restart replaces the process/grid and
rejects old events. Shutdown and child reaping run off the foreground.

A new session's working directory is chosen when it is created and kept across
restart. New Terminal, splitting a terminal pane, and `terminals.open` without
a directory start in the focused (or split) terminal's current directory.
Otherwise the window's shell picks the innermost root containing the window's
most recently focused document, otherwise the first root, otherwise the home
directory, skipping roots that are not existing local directories. A terminal
tab restored in place starts in its recorded directory, or by this rule when
that no longer exists; restoration installs a window's roots before its layout,
so restored shells see them. `terminals.open` also accepts an explicit
directory. The rule lives in
[terminal/directory.rs](../../src/app/terminal/directory.rs).

A session reports its shell's current working directory by querying the shell
process, on macOS and Linux, and otherwise the directory it started in; the
query lives in [terminal/process.rs](../../src/app/terminal/process.rs). A
captured layout records it, so quit and project switches see the latest `cd`,
while a crash restores the directory of the last debounced capture.

`TerminalView` owns focus, layout, scroll interaction, drawing, and input. An
attachment lease gives one view input/resize authority. Releasing it does not stop
the session; natural exit retains the final grid, and startup failure remains
visible and restartable. Protected closure removes affected registry entries and
stops their sessions after document decisions succeed.

Measured view geometry supplies drawing, baseline, cell hit testing, viewport
size, and PTY pixel dimensions at the destination window's scale. Cells are
measured for `font.size` resolved under `{ view: "terminal" }` times the zoom
factor, so a change resizes the PTY grid. Fallback glyphs
stay within emulator cell allocations. Selection uses scrollback grid coordinates;
application mouse reports use viewport cells. Geometry is remeasured after resize
or attachment handoff. Typography, bootstrap metrics, and encoding rules live in
[terminal layout](../../src/app/terminal/layout.rs),
[session](../../src/app/terminal/session.rs), and
[keyboard input](../../src/app/terminal/input.rs).

Keyboard input follows command/keymap precedence and current emulator modes.
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
replacement. Wire formats and coordinate limits live in
[terminal/mouse.rs](../../src/app/terminal/mouse.rs); the
[mouse fixture](../../scripts/terminal-mouse-fixture.py) supports native acceptance.

Link lookup uses displayed emulator cells, soft wraps, and OSC 8 identity.
Explicit destinations take precedence; unsupported explicit schemes suppress text
detection. Only complete HTTP(S) destinations are actionable. Modifier-click owns
the gesture ahead of mouse reporting; release revalidates grid, scrollback,
process, destination, and geometry. Output or lifecycle changes invalidate armed
links. Link recognition lives in [terminal/links.rs](../../src/app/terminal/links.rs).

## Observing product state

Snapshots derive associations and focus from live shells/workbenches rather than
replicating ownership. Committed layout, relocation, focus, activation, and closure
transitions signal the [application observer](extension-host.md#application-observation).
Diagnostic windows are excluded; terminals and tools remain explicit
non-document surfaces with no buffer/document association. Tool focus participates
in captured command targeting; native explorer Copy copies the selected URI.
