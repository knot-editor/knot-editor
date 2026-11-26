# Workbench and view lifecycle

Part of the [architecture](../architecture.md). [Decisions](../decisions.md)
records rationale; deferred work lives in the [roadmap](../roadmap.md).

## Product application launch

The macOS product is packaged as `Knot.app` for native activation, file-open,
and reopen callbacks. `knot [path ...]` resolves each path against its working
directory to a `file:` URI; one invocation, like one macOS file-open event, is
one launch batch. Knot is single-instance: the running Knot binds a per-user
local socket before it starts (a Unix socket in the user's local data
directory, a named pipe named after the user on Windows), and a later `knot` that connects
sends its batch and exits; a stale socket file is replaced. With no Knot
running, a `knot` inside `Knot.app` started from a shell has LaunchServices
start the app with the paths (`open -a`), while any other binary, `cargo run`
included, becomes the running Knot in the foreground.

Batches join a launch queue while
[personal config and extensions](extension-host.md#package-admission-and-startup)
start. Successful startup first restores
[unsaved documents](documents-and-persistence.md#durable-unsaved-document-recovery),
then the [recorded windows](#native-window-hot-exit), then places each queued
batch. Recovered documents that no restored window presents go to the first
opened window, so text is never hidden. With nothing restored or requested one
empty window opens. The last batch's focus wins, else the previously active
restored window keeps focus, and the app activates.

Placement (`knot_model::launch::place`) treats restored windows like any open
window. A folder focuses the window that has it as a workspace root, else opens
a new window with it as the workspace. Each file goes to the most recently
active window whose roots contain it, counting windows the batch just opened,
so `knot . src/main.rs` opens `main.rs` in the folder's window. The files no
window holds open together in one new window. The batch focuses the last
window it created, else the window that received its first file.

The palette commands `Shell Command: Install 'knot' command in PATH` and
`Shell Command: Uninstall 'knot' command from PATH` point `knot` at the running
executable, so it stays valid while the app stays where it is. On macOS it is
the link `/usr/local/bin/knot`, created through the system password prompt and
refused while the app runs translocated; the Homebrew cask links `knot` itself.
On Linux it is a link in `$XDG_BIN_HOME`, else `~/.local/bin`; on Windows the
executable's folder in the user `Path`. Uninstall removes only a link to Knot,
or exactly that `Path` entry, and a failure names the manual step.
When a package failed to start, each launch window opens the extension startup
report once; windows opened later do not.
`ApplicationSnapshot.launched` and a one-time `launched` event then tell
extensions that startup windows exist; extensions activate before any window
does. A reopen after launch opens an empty window and restores nothing. The
[zoom level](#zoom) is read before the first window opens. Fatal
config failure discards the queue and opens a focused diagnostic window with
Copy and Quit; windows remain unavailable for that launch, and nothing
is recorded.

A native operation whose failure leaves no other visible effect, such as Save,
Open, Add Folder, recovery, or creating a transfer window, reports it through a
native warning prompt that needs no answer. Successful and in-progress
operations show only their effect. Extension calls receive the failure as their
result instead of a prompt. Anything else Knot tells the user without
interrupting goes through [notifications](#notifications).

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

A window paints the center and each shown tool window as cards on the
`window.background` color; the theme's [chrome](semantic-ui.md#themes-and-color-keys)
sets their corner radius, gap, window margin, and border, so a theme may be
Islands (rounded cards with gaps) or flat (square cards meeting at borders).
The gap between two cards is also the handle that resizes them, a minimum width
the chrome sets, and splits inside a card are thin lines. On macOS the title bar
is transparent and the window buttons sit in a toolbar row that drags the
window and zooms it on a double click; other platforms keep the native frame
and have no row. The status bar sits on the window background.

Focus is window state, by identity: the focused pane's active surface, the
palette, or the transient interaction, over the workbench's focused pane. An
attached native window focuses the matching gpui handle once the current update
ends, and reports a click that moves gpui focus back to the window. A blur
leaves the window's focus alone.

Native prompts go through the window, which asks its native window; a window
without a native window answers nothing, which callers treat as cancellation.
The native window stays an adapter for raw input: key-downs and typed text
reach the window through it (see [keymaps](commands.md#keymaps)), and tab drags
and tool-window resizing are pointer input. Every surface's state, the editor, trees,
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
picker, and text inputs scales its constants. Tool-window sizes stay window pixels, as
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
removes a package's items. An item's `placement` puts it in the status bar or
in the toolbar row at the window's top, which renders the same items, per-window
values, and click commands; hidden IDs skip both. The row shows on macOS
always and elsewhere only with toolbar items. The editor a bar describes is `WindowInfo.statusView`
([extension host](extension-host.md#application-metadata-queries)).

Right-clicking a tab header opens the tab context menu from the `tab` items of
the menu registry ([context menus](commands.md#context-menus)). The bundled
`@knot/chrome` package adds Close, Close Other Tabs, Close Tabs to the Right,
Close All, Split Right, and Split Down, and the `content` items: Cut, Copy,
and Paste for editors, and Copy and Paste for terminals.

The same package registers the built-in status items on the right:
the cursor position with the selection size, counted in characters up to
1 MiB and in kilobytes beyond, the document's indentation, and its language. It follows each window's `statusView` with an editor handle and
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

## Notifications

The application owns one `NotificationStore` (`crates/knot-model/src/notifications.rs`,
the `ApplicationNotifications` global in `src/app/notifications.rs`): the last
100 notices, each read or unread, and the rule for which notices a window's
popups show. A notice is raised natively through `notifications::raise` or by
an extension through `workbench.showNotice`; every change redraws the windows
and publishes `notificationsChanged` to observers
([extension host](extension-host.md#application-observation)).

Each window mirrors the store into its popup stack and paints it over the
workbench (`src/app/product/notifications.rs`). A notice with a target shows
only in that window, otherwise in all of them. Popups are not the history:
a popup that times out hides in its own window and leaves the notice unread,
while closing it, running one of its actions, or showing the panel marks the
notice read everywhere. Nothing is persisted, and notices raised before any
window exists, such as startup configuration warnings, wait in the store and
pop up when the first window opens. While the Notifications panel shows in a
window, that window paints no popups.

The bundled `@knot/notifications` package builds the panel on the public
extension API alone: a tool window listing the history, with a dot on its
stripe button (`setBadge`) that is red while an unread notice is an error and
blue while any other is unread. Popups can be turned off with `notifications.popups`,
for a package that presents notices itself.

A native operation that fails with no other visible effect (Save, Open,
Reopen, Add Folder, recovery, Restart Extension, tab relocation, shell
commands) raises a notice from Knot in the window where it ran
(`KnotWindow::raise_notice` in `src/app/product.rs`): `error` for a failure,
`warning` for something unavailable. A Save failure offers Save As…, and
Restart Extension also reports success, which nothing else shows. Native
prompts remain only where the user must choose.

Every `knot::config` warning also raises a sticky `warning` notice, with
actions that run Knot: Open Personal Configuration and Knot: Open Log: setting
warnings and a `font.family` that is not installed. The fatal configuration
error keeps its diagnostic window.

## Workbench and protected closure

The layout model (`knot_model::workbench`) owns a window's areas: a center of
documents and terminals surrounded by six dock slots, each showing at most one
tool window; the center and every tool window are the same binary split tree of
tabbed panes; stable pane/tab identities, ordered tabs, and the focused pane. A tab holds the
identity of what it shows (a document, a terminal session, a diff, or a tool) and a
presentation identity, which captured targets compare. The window's workbench
entity pairs the layout with the view behind each presentation: it creates a
view when a transition opens a tab and drops it once no tab or tool presents
it. A document tab presents one `EditorView` of an application-owned document,
fresh whenever it opens, shows another document, or moves to another window. A
terminal tab presents the sole attached view of an application-owned session,
with no document model. A diff tab presents a `DiffView`, described [below](#diff-view). Its identity
sits in the workbench like a terminal session's, and its split opens the same
comparison in a fresh view.
A fixed tool window's tab presents its tool's native frame, whose
definition and presentation survive hiding.
The slots are left top and bottom, right top and bottom, and bottom left and
right. A side's extent is the size of the window in its first slot, else its
second; when both show, the side splits by the share of the first. Each tool
window remembers its own size and share, keeping them when it moves to another
slot, and resizing a side sets the size of the window there. By default the
side slots span the window's height and the bottom slots sit under the center
between them; `workbench.bottomDockLayout` set to `full` runs them across the
window's width under the side slots. Painting, directional navigation, and
`Workbench::area_rects` share the arrangement. Hiding a tool window keeps its
tabs and views, so shells keep running, and focusing a pane of a hidden one
shows it.

A tool window is fixed or a container. A fixed one (Explorer, Notifications)
holds the single tree or text tab it registered with and shows only a title
row. A container (Terminal) holds tabs opened into it: every pane shows its tab
strip, even with one tab, and a `+` button that runs the container's `newTab`
command, which also runs when the window is shown empty. Closing or moving out
a container's last tab hides it, and a pane whose last tab goes away.
Split Right and Down open a new tab in a new pane beside the current one, Split
and Move moves the current tab there, and Unsplit merges the panes back. Each
tab kind declares where it may live (`TabKind::allows`), and placement,
relocation, directional moves, and restore check it: documents live in the
center, a fixed tool's tab in its own window, a diff only in the center, and a
terminal in the center or in the container it was opened into, never in another
tool window. Open as
Editor Tab and Return to Tool Window, or a drag onto the center or any strip of
the container, move a terminal between them.

Toggling a tool window follows one pattern: a hidden one shows and takes
focus, a shown but unfocused one takes focus, and a focused one hides,
returning focus to the center's last focused pane. Every registered tool has a
generated `<viewId>.toggle` command. Hide Active Tool Window (from the editor,
the most recently active visible window), Jump to Last Tool Window, Hide All
Tool Windows (run again, it restores the windows it hid), Maximize Tool Window,
the Stretch commands, Move to a slot, and the split commands are workbench
transitions. Esc in a non-terminal tool window focuses the center and leaves
the window shown; a terminal always receives Esc.

A center tab sits on the tab background with an accent line under the
active one; its close button shows on the active or hovered tab, a dirty
document's dot takes the button's place until hover, and a pinned tab shows a
pin icon. A tool window's header carries its bold title at the start of its
first pane, and its actions, options menu, and hide button at the end;
double-clicking it maximizes the window. The options menu offers Move to
(slots), Stretch, and Remove from Sidebar. Stripes on the left and right window
edges (`workbench.toolStripes`; a stripe without buttons is not painted,
except while a button is dragged) group buttons by slot (`Workbench::stripes`):
the left stripe shows left top, a separator, left bottom, and bottom left at
its lower end, and the right stripe mirrors it. A click shows a hidden tool
window in its slot, hiding the one there, or hides a shown one
(`Workbench::stripe_click`); an empty container opens its first tab. Dragging a
button to another group, or the button menu's Move to, moves the window to
that slot. A tool window removed from the sidebar has no button and hides; the
More tool windows button on the left stripe lists such windows, and picking one
shows it and restores its button. A tool window's badge paints a blue or red
dot on its button, which `ViewRegistration.setBadge` sets in every window.
Splitting a document creates another view. Empty panes collapse, keeping their siblings' ratios. The
center may be empty; it then paints a blank surface that takes keyboard focus
while no pane has it, opens new documents and terminals, and accepts dropped
tabs. When the focused pane goes, focus moves to another pane of its area, then
the center's last focused pane, else to the empty center; a tool window takes
focus only when the user or a command focuses it, so a fresh window or an
emptied center never leaves keys aimed at a tool.

Each split gives a ratio of its extent to its first child, addressed by its
boundary pane, the first pane of its second child. Grow and shrink move a tool
window's size, or a center pane's nearest split, by a step; equalize gives
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
from ratios and tool-window sizes over the painted workbench, so they work while a
pane is maximized; a move goes only to a neighbor whose area allows the tab,
and with none splits the pane on that side. Focus commands given
`{ withinArea: true }` stay in the focused pane's area, which keeps Vim's
`ctrl-w` keys among the center's editors; given `{ count: n }` they step `n`
panes, each from the pane the last step reached, and stop at the edge. Cycling and first and last pane
commands walk the focused pane's area in layout order, and the workbench
remembers the previously focused pane. Resizing can be limited to the width or
the height, which changes the nearest split along that axis, or, along a
side's axis, the size of its tool window. Maximize is window render state that lasts while its pane stays focused, in a
tool window as in the center; hiding the window keeps its maximized pane, and
showing it with that pane focused maximizes it again.

One application-level coordinator protects tab, pane, window, and quit
closure. The window collects its open views and the documents' state
and runs the prompts, saves, and closes; which views a close covers, which
dirty documents need a prompt, and whether a plan still holds at commit are
decided by `knot_model::workbench::closure`. A close covers a tab; Close Other
Tabs a pane's unpinned tabs but the origin; Close Tabs to the Right its unpinned tabs after the origin; Close Pane every tab of the pane, hiding a fixed tool window, and Close Other
Panes every tab of the window's other panes in the origin's area. Window and
quit closes need no origin tab. The
coordinator captures exact window, workbench, tab, surface, and presentation identities,
counts live document views across windows, and deduplicates dirty documents
losing their final view. A tab is one view of each document its editors show,
once however many editors a composite view holds on it, and a composite's tab
shows the unsaved-changes marker exactly when closing it would prompt. Native Save / Don't Save / Cancel prompts run serially;
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
terminal leaves an empty center, beside any tool windows, and keeps hidden tool
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

## View tree

A tab presents one root view, and a view that contains others lists them
through the `ViewTree` interface (`src/app/view_tree.rs`), so the snapshot,
focus, handles, and command captures walk the tree without naming a container.
A view emits `ViewTreeChanged` when its children change and `ViewFocusChanged`
when focus moves between them; both publish `presentationsChanged`. A view's
identity is its gpui entity, which extensions see as a `ViewNodeTarget`, while
the tab keeps its `ViewTarget` for tab operations. Moving a tab keeps its views,
and a view that leaves the tree invalidates its targets.
`Workbench::focused_leaf` is the leaf view focused in a tab, and a window's
`focusedView` and `statusView` are the leaves of the focused tab and of the
[status editor](extension-host.md#application-metadata-queries).

## Composite views

A `CompositeView` (`src/app/composite_view.rs`) is a center tab, `TabSurfaceId::Composite`, whose
layout a package describes as data and the user cannot rearrange. A package
reserves a kind and opens instances through `src/app/extensions/host/composites.rs`;
the lifecycle that registered a kind owns it and its instances, which close
when the instance, the kind, or the lifecycle ends. Instances are left out of
window layouts, hot exit, and Reopen Closed Tab.

The layout is a tree of groups (row, column, tabs) whose leaves are editors,
trees, and text frames. Each nested group is a view of its own in the
[view tree](#view-tree), so every view has an identity, and focus and command
captures name the leaf. A composite has no state of its own beyond which leaf
has focus, which reselecting the tab restores. Tree and text leaves are
instances of a view registered `embedded`, which no tool window hosts; the
composite binds each as a contributed view of its window and unbinds it when
the view leaves the layout or the composite is released. An editor on a
document is a view of that document, counted once per tab for closure.

Replacing the layout builds the new one against the old, taking the view of
each keyed member that shows the same buffer, document, or embedded view, so
its handle and state survive. The focused view keeps focus when it survives;
otherwise the layout's initial focus takes over. The host refuses an update
that would drop the final view of a document with unsaved changes before
changing anything.

A scroll link, also tab data, joins two keyed editors. The composite
subscribes to both, and when one scrolls it moves the other to the line
`knot_model::diff::map_line` gives through the package's map, then records both
positions so the echo of its own scroll stops there. Setting or replacing a link,
or replacing the layout, brings the second editor in line with the first.

A connector member is a strip, not a view, so it has no node in the view tree.
It names two keyed editors of the layout, resolved after each build, and paints
a band for each link from the line spans of both editors as of their last
paint, filled with the link's theme color key. Following the scroll costs the
package nothing, but a change of text needs the package to send the links again
with the layout.

The toolbar belongs to the tab's data. A click captures a command target for
the composite's own tab and its focused leaf, as a click on an editor
contribution does, and dispatches the command there whatever has focus.

## Diff view

A `DiffView` (`src/app/diff_view.rs`) is a center-only tab of two ordinary
`EditorView`s. The original is always a snapshot: a private read-only buffer
with a label and a language, outside document identity as text frames are. The
modified side is a snapshot or an application document, an editable view of the
document's shared buffer that counts as a view of that document for closure and
view counts, so closing the document's last view prompts as usual. Diff tabs are
left out of window layouts, hot exit, and Reopen Closed Tab: native code cannot
recreate a snapshot without the package that supplied it.

The view diffs its sides itself with `knot_model::diff::line_hunks`: when it
opens and 150 ms after an edit to either side, off the foreground, keeping the
result only if both sides' revisions still match. `hunk_decorations` turns each
hunk into `diff.*.background` line bands on both sides, with a gutter wedge on
a side that has no lines in the hunk.

Side by side, the editors have no filler rows. Scrolling one scrolls the other
through `knot_model::diff::map_line`, which maps lines outside hunks one to one
and lines inside a hunk proportionally, and a strip between them joins each
hunk's lines in the band colors. Inline shows only the modified editor, with
each hunk's original lines above it as [block rows](editor-and-languages.md#editor-views)
banded in `diff.removed.background`; it stops scroll syncing. `diff.layout` sets
how a new tab opens and `diff.toggle-layout`, also a toolbar button, switches
an open one.

Commands on a diff tab act on the editor of the side that last had focus; inline
that is the modified side, and the status bar follows that side. Both editors hold the `diff` key context, in which
Alt+F5 and Alt+Shift+F5 run `diff.next-hunk` and `diff.previous-hunk` ahead of
the version control package's bindings; they move that caret to the first line
of the next or previous hunk on that side, wrapping at the ends. A bar above the
sides holds buttons for those commands, the layout toggle, and, when the
modified side is a document, a lock that runs `diff.toggle-read-only`: it makes
that side's editor refuse edits through a per-view read-only flag on the editor,
so the document's other views stay editable.

Extensions open a diff with
[`workbench.openDiff`](../extension-api/application.md#show-the-differences),
and version control opens one from [its commands and gutter](version-control.md#showing-changes).

## Window layouts

One versioned `WindowLayout` value serves both restorers: native window hot exit
and extension sessions through `workbench.captureLayout`. It records logical
window bounds, windowed, maximized, or fullscreen state, and the display UUID;
the center's split tree and each tool window's slot, size, share, visibility,
and split tree, with each split's ratio; ordered tabs, which are pinned, and the selected tab per
pane; the focused pane; and each editor tab's view state. Preview, maximize, recent order, and the reopen stack
are not recorded, so restored tabs are regular. Diff tabs are not recorded or
kept for Reopen Closed Tab, since native code cannot recreate a snapshot. Every
other tab kind is restored by position and records only what recreates it: a document tab its
document identity, resource URI, and explicit language, a fixed tool window records no tabs, and a terminal tab its shell's working
directory and label, so a restored terminal starts a fresh shell there under
its name. The value never contains pane, tab, view,
or window handles.

Restoring plans a pruned arrangement in one place for both restorers. Document
tabs resolve to the window's entry with the same document identity, else the
same normalized URI. Unmatched documents, containers not registered, tabs whose kind the area
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

Cursor, selection, and scroll changes, focus, tool-window resizing, window bounds, and
other window changes restart a per-window trailing-edge debounce of about a
second. When it settles the window captures its layout: a layout that differs
from the last one it reported sends a `viewStateChanged` observation event, and
a changed layout or root set re-records the window for
[hot exit](#native-window-hot-exit).

## Session replacement

`workbench.replaceWindows` closes a set of windows and opens their replacements
in one foreground commit. It knows windows, documents, roots, layouts, and
retained captures, never projects; the
[project registry](#projects) builds its switch on
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

## Projects

The bundled `@knot/projects` package owns projects; the host knows only
[session replacement](#session-replacement). The package keeps one registry
record and one `session:<projectId>` record per project, holding each window's
entries and layout.

A session is `active` while the project has windows. A window belongs to a
project when its roots equal the project's roots, in any order, and the session
is active. A window showing a folder set no project has creates an unnamed
project, named after its first root's folder until renamed; the
`projects.rememberedLimit` setting bounds how many unnamed projects are kept,
sparing named and active ones and ones holding unsaved text. Adding or removing
a folder in a bound window updates the project and its other windows; clearing
the folders, or choosing another project's folders, takes the window out of the
project.

Selecting a project suspends the current one, retaining its unsaved and
untitled text and layouts in the suspended record, and restores the
destination's windows with their splits, docks, and view state; a project may
span several windows. While active, the record follows tabs and view state
live. Opening a suspended project's folder restores it: an otherwise empty
window is replaced by its windows, and a window with other content stays while
the project opens in a new one. Closing a project's last window suspends it
without captures, because closure already asked about dirty documents. Removing
a suspended project says how many unsaved documents its record discards. The
Projects tool's New Window action (`projects.new-window`) opens another window
on the current window's project, while File › New Blank Window opens one
outside any project.

At startup the application restores every window from the last session and only
then reports `launched`; the registry then rebinds the restored windows by
their roots and suspends active projects whose windows did not return. A
session record written by a newer version is kept unchanged, marked unreadable
in the Projects tool, and never switched to; while one exists, no retained
capture is released as an orphan.

The registry also infers projects from Git checkouts, using `files.stat` and
`homeUri`. A file opened alone in a window without folders, as `knot <file>`
does for a file no open folder holds, or saved there to a new file, makes the
nearest folder holding a `.git` entry that window's project, never the
filesystem root or the home folder. A new folder becomes the window's folder in
place; a suspended project is restored and the window's tabs move into it.
Open Containing Repository as Project (`projects.open-containing-repository`)
does the same on request; from a window with folders, the repository opens in
a new window and the current project stays open.

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

Fixed tool windows stay in their owning window. A drag
offers targets only where its tab's kind allows: a terminal can drop on the
center or any strip of its container, and panes of a disallowed area show no
target and refuse the release. Releasing a tool outside all Knot frames cancels
without creating a window. Dividers beside a tool window drag its size up to
the window edge, keeping only the minimum; other dividers drag the split's
ratio, leaving each side a minimum size.

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
a window near the pointer. Preview rendering, hit-test thresholds, divider
drag handling, and coordinate fallbacks live in the [relocation
code](../../src/app/product/tab_drag.rs).

## Automatic tool-layout memory

One application-owned `ToolLayoutStore` holds a single versioned tool-only
snapshot in the application state database: one SQLite file with a table per
UI-state record and one connection that serializes their writes, which also
backs [native window hot exit](#native-window-hot-exit). Quit flushes every
pending record. Unsaved-text recovery keeps its own database, because a
damaged recovery file is preserved and reported rather than treated as empty. It records, per tool window by stable namespaced ID, its slot, whether it
shows and has a stripe button, and its size and share of its side. Tabs,
documents, terminals, the center, and live handles never enter the record.

A fresh window gives each tool window its stored place as the tool registers,
as packages attach; a tool the record does not name takes its declared
slot. Already-open windows keep their live arrangements. Only explicit
user arrangements publish a new default: moves, resizes, and show/hide
changes, including a command-invoked extension show, hide, or focus. Divider
dragging coalesces through a trailing-edge debounce so one settled arrangement
reaches storage, and identical publishes skip storage. Restore, window
activation, listener callbacks, automatic presentation, and document structure
changes never publish. The cache updates synchronously so the next window
inherits immediately; writes run off the foreground and the quit path flushes a
pending write before exit. Reset Layout returns to product defaults, every
tool window to its declared slot. A missing, corrupt, or version-mismatched
record falls back to defaults, and unknown stored IDs are skipped without
creating blank frames.

Tool frames stay workbench-owned behind stable namespaced IDs and explicit
native content variants. Extension contributions attach through this contract
instead of receiving layout storage or persisting live handles. A contributed
view provides a semantic tree, read-only text, or, as a container, no content of its own; text content
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

A terminal tab lives in the center or in the container tool window it was
opened into. The Terminal container, New Terminal, New Terminal in Editor, and
`terminal.toggle` (ctrl-`) belong to the bundled `@knot/chrome` package;
the engine, restore, and sessions stay native. A terminal is labelled after its
shell with the lowest free ` (N)` suffix from 2 in its window, and reads the
same in the tool window and the center. While the program has set a non-empty
title (OSC 0 or 2) the tab shows it instead, without a suffix; Rename Session
sets a name that wins over any title, and layouts keep only such explicit
names, since a restored session starts a new shell. Terminal tab labels end
with an ellipsis at a maximum width, and the tooltip shows the full label. `terminals.open` takes the destination as `location`, `"editor"`
or a container's view ID. Window layouts restore a container's terminals in
it, and a layout naming a container that has not registered drops its tabs.

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
restart. The opener chooses it: `@knot/chrome`'s New Terminal passes the focused
terminal's directory, else the root containing the last focused document, else
the first root. Splitting a terminal pane uses the split terminal's current
directory, and a session opened without one starts in the first local root, else the home directory. A
terminal tab restored in place starts in its recorded directory, or the home
directory when that no longer exists; restoration installs a window's roots
before its layout.

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
`{ view: "terminal" }`, with the size times the zoom factor; cell height
follows `font.lineHeight`, which defaults to 1.0 in terminals and 1.2
elsewhere, with the baseline centered in the
cell. A change resizes the PTY grid. Fallback glyphs
stay within emulator cell allocations. Each cell is shaped and clipped alone,
except that with `font.ligatures` on, consecutive single-width ASCII
punctuation cells on one line that draw identically are shaped as one run
clipped to their cells, so ligatures such as `->` form; letter ligatures do
not. Glyphs paint at physical-pixel-snapped cell origins with fixed cell
advances; the pieces of one run share the run's clip, so a ligature glyph's
ink can overflow into its cluster's blank cells. On macOS the
application turns off font smoothing at startup, so glyphs rasterize at
their natural stem weight rather than thickened by luminance. Selection is the terminal's
own, tracked
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
sanitizing bracketed paste. OSC 52 does not access the clipboard; additional
protocols remain deferred in the roadmap.

A bell rung while the terminal's tab is not shown, because another tab of its
pane is active or its tool window is hidden, marks the tab with a `bell-ring`
icon in the `notification.info` color and, unless a contributed badge is set,
the hidden tool window's stripe button with the info dot. The workbench model
keeps the marked tabs and clears a mark when its tab is shown. A bell in a
shown terminal does nothing. A desktop notification (OSC 9 or 777) raises an
`info` [notice](#notifications) in the window holding the terminal, even while
it is focused: the title is the program's, or the tab label when it gives none,
and the message is the body. Its "Show Terminal" action runs `terminal.show`
with the session number, which reveals the tool window, shows the tab, and
focuses it. A session turns these engine events into gpui events, and the
application applies them in the window whose workbench holds the session.

A mouse press chooses application reporting or local selection through release,
even if modifiers change. Shift forces local interaction. Local selection is
libghostty's selection gesture: a click selects cell by cell, a double click a
word, a triple click a line, and a drag extends by that unit, with a cell
joining a drag once the pointer passes its middle. The view passes the press
its platform click count and the emulator owns the rest. A right press that is not reported opens the
terminal's `content` menu. Wheels choose their
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
