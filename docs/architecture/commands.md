# Commands and keymaps

Part of the [architecture](../architecture.md). [Decisions](../decisions.md)
records rationale. Public API contracts live in [knot.d.ts](../../crates/knot-host/src/js/knot.d.ts).

## Dispatch

The application owns one foreground catalog for definitions, name ownership,
and admission. Keys, palette, menus, and scripts invoke the same semantic commands
and structured outcomes. The palette captures its origin before taking focus.

Each definition carries a title, an optional description, and whether the
palette lists it; native input commands such as cursor movement opt out, and
extensions pass the same metadata to `commands.register`. The palette shows
each command's title, description, and the binding shown for the origin's view
kind: the argument-less binding with the fewest keystrokes, then the shortest
text, then the latest write. It matches the command name without showing it.
Commands confirmed in the palette form a most-recently-used list of 50 names in
the application state database; recent commands list first with an empty query
and break equal scores otherwise.

Annotated native handlers declare metadata and typed async signatures; linked
adapters assemble catalog entries and generated name constants. Distinct routing
participants may share a name and metadata. Assembly rejects duplicate
participant entries or conflicting metadata. Dynamic extension ownership remains in the
application catalog. Declaration contracts live in
[commands/declarations.rs](../../src/app/commands/declarations.rs).

An invocation captures the [product
session](workbench-and-lifecycle.md#product-sessions), its focus by identity
(layer, pane, and tab), workbench, tab, surface, and exact presentation, with
no window or focus handle. A window with no focused pane, such as one with an
empty center, captures no tab: commands that need one report unavailable, and
New, Open, and terminal commands open in the center. Routing offers it to the
captured focused view, then workbench, then application. Commands, native editing and movement
included, run against a session without a window.
Participants synchronously claim/decline before starting typed async work; all
declining produces unavailable. Raw text, IME, pointer, scroll, and terminal bytes
remain view input protocols. Find commands currently report unavailable.

Focus changes cannot redirect an admitted command. Pending work revalidates the
captured session and presentation before foreground access: the session must
be open and still show the captured workbench. Same-pane reorder
preserves authority; replacement or transfer invalidates it even under unchanged
document identity. No foreground borrow crosses an await.

Owned command contexts run short foreground closures in the current dispatch
turn, after the active window update releases its borrow, preserving order with
subsequent key events and raw text input. Only suspended futures move to gpui's
executor. This also keeps extension participant selection synchronous, including
inline children. Dialog and protected-close handlers await semantic completion.

## Keymaps

One application-owned registry retains a preset slot and lifecycle-owned slots,
then derives a merged effective map. The selected preset's bindings are defaults
beneath every extension and personal-config binding. Among those, the binding
whose scope requires the most facts of the focused view wins (a view kind and
one of its key contexts, then a view kind, then global), and the latest write
breaks remaining ties.
Removing a lifecycle's ownership reveals earlier writes. An unavailable command
does not select another binding. Null unbinds leave raw input available.

The Knot preset is native data for each platform: `secondary-` entries become
cmd on macOS and ctrl on Windows and Linux, and per-platform tables add
movement keys and, on macOS, the Cocoa Emacs editing keys in editors. On
Windows and Linux every global binding whose first keystroke is ctrl with a
printable character other than backtick is unbound in terminals, so the shell
receives readline and job-control keys, and terminals copy and paste with
ctrl-shift-c and ctrl-shift-v. Named keys such as tab and pageup, and ctrl-`,
stay global because shells do not read them, so tab navigation and Toggle
Terminal work from a terminal. On every platform Shift+F10, which shows a
tree's context menu, is unbound in terminals, whose programs read function
keys.
Extensions register further presets by name, lifecycle-owned like themes. The
`keymap.preset` setting selects one when startup completes and whenever the
setting changes; an unregistered name selects Knot, and installing a preset
replaces the whole slot. A preset binding for an undeclared view kind is
skipped.

Knot matches keys itself. Every key-down in a product window passes one
interceptor, ahead of gpui's key handling and text input, which hands it to
the session; a session without a window takes keys directly. While the
workbench has focus, the session's key router
(`crates/knot-model/src/commands/key_router.rs`) matches the key with the focused view's
kind and live key contexts. A key that begins a binding resolving to a command
there waits for the rest of its sequence; a completed binding enters command
dispatch. Any other key goes to the surface with Knot-side focus, whose state
handles its own keys, such as a tree's arrows, the palette's Enter, or a
terminal's bytes, and a key that surface ignores becomes typed text, as if
unbound. While a gpui-component popup that registered its focus handle
holds focus, such as a tree's context menu, a key the keymap leaves goes on
to gpui's dispatch instead, where the popup's own bindings run, so global and
extension bindings still win over it. A sequence that stops matching runs its longest bound prefix, or
delivers its first key natively, and routes the rest again, and a pending
sequence flushes the same way after one second or ends when focus moves. Keys
typed in the palette or a transient interaction skip the router and go to
them. Menus and palette commands bypass keymaps. gpui holds bindings only for
menu key equivalents, the config-error window's Quit, and the bindings
gpui-component installs at startup, which every keymap rebuild restores.

### Editor bars

While a field of an editor bar has Knot-side focus, the router resolves keys
against the native view kind `bar` and the focused bar's ID as its only key
context, instead of `editor` and the editor's own contexts. Editor-kind
bindings, such as movement and editing keys, therefore never reach the field;
global bindings still apply. Commands a bar key runs capture the bar's editor
tab, so `editor` and `buffer` are the bar's. Keys the keymap leaves go to the
field's text input. Tab and Shift+Tab are native form keys with no command or
binding: they move focus to the next or previous field across the editor's
bars, top to bottom, wrapping, and never insert text.

### Key contexts

Extensions set named key contexts on editor views through the editor handle.
The view keeps each context with the lifecycle that set it, so two packages'
contexts of one name are independent and teardown clears its own; contexts are
presentation state, never recorded in layouts. The router resolves
keys against the view's live contexts rather than a rendered frame's, which a
context set by the previous key may not have reached, and a sequence bound in
a context holds no key where that context is off. Menus and the palette ignore contexts, and a key that some
context unbinds shows no menu key.

An editor also takes a caret shape (`bar`, `block`, or `underline`) from an
extension, kept per lifecycle like key contexts: the latest set wins, clearing
reveals the previous one, and teardown clears its own. A block or underline
covers the character under the caret, with a half-line width at the end of a
line; the shape is presentation state and never recorded.

A text binding routes committed text that an editor would insert to a command
with `{ text }` as its arguments, scoped and resolved like key bindings but in
its own slots. Committed text, after keyboard layout and IME, reaches the
session before the editor inserts it: a window's editor offers it from its
text input handler through a router the product shell installs, and a
session without a window types into the editor itself. Where the editor's
text binding applies, the session starts the key hold at once and dispatches
the command once the caller is done with the editor, since capturing a
command target reads it. Where a text binding applies, composition never
touches the buffer and the IME's commit routes as text.

### Ordered delivery

A key that dispatches an extension command holds its session's later input
until that command finishes, with or without a window: the session queues
later key-downs before matching them, and committed text for the held
editor, in one queue. Release replays the queue in order, a key through the
window's keystroke dispatch when the session has one and otherwise through
the session's own key path, so each key sees the state its predecessor
committed, as native commands already do by finishing in the dispatch turn.
Release replays one input per deferred step, letting a replayed Enter or
Backspace commit before the text typed after it. A replayed key or text that
starts another extension command holds the rest of the queue. The hold also
releases once a picker, input, or the palette takes focus, so keys typed ahead
of a command's input reach it, on the first key after focus leaves the held
view otherwise, and after one second, so a stuck extension never keeps the
keyboard. Composition preedit is not queued, since the IME already shows it.

Menus show each command's shown global single-keystroke binding through a
binding to the menu action in a context no element declares, so it labels the
menu without dispatching. The menus are set again whenever the keymap changes.
On macOS the application outlives its last window, so Quit, New, New Blank
Window, and Open… use a separate menu action that an application-level handler
also takes: with no window open, Quit quits and the others open a window and
run in it, while every other menu item is disabled because no handler takes
its action. Other platforms quit when the last window closes (see [native
window hot exit](workbench-and-lifecycle.md#native-window-hot-exit)).
macOS performs a menu's key equivalent only when the window leaves the key
unhandled, so a key that some view unbinds shows no menu key. Show Keybindings
lists every slot with its owner and the owner that overrides it, using the same
winner rule as resolution.

Synchronous JavaScript keybinding calls form an ordered atomic batch per V8 turn.
Changed effective maps rebuild bindings across windows; host-response ordering is
specified in [transport](extension-host.md#handler-declarations-and-transport).

## Extension execution

The foreground bridge owns one serial invocation tree, admitted presentation
authority, buffer capture, cancellation, and completion channels. The pooled
runtime schedules JavaScript. Nested invocation first selects a participant in
the parent's captured session; session-handled outcomes return to the bridge
without dispatching twice. Same-lifecycle children run inline under the parent's
scheduler root; cross-lifecycle children await foreground completion. Each parent
has at most one unfinished child, and ancestry cycles are unavailable.

View-scoped extension handlers bind to live instances of a contributed view
registration. The focused instance claims before global extension handlers reach
the application participant. Disposal, lifecycle teardown, and session end
remove bindings; late results validate instance and lifecycle.

Contextual buffer/selection access validates the live invocation, caller lifecycle,
target authority, and exact presentation. Roots and children inherit the same
capture. Activation and unrelated callbacks cannot borrow command focus. A valid
nontext target has a null buffer; contributed-view selection support is
limited to admitted contributed-view handlers. A document-editor target also
receives an editor handle whose selection and view-state operations resolve the
captured presentation on every call, so they reject once the tab closes, moves,
or the invocation tree ends. `workbench.editor(target)` instead binds a
long-lived handle to the exact document tab a `ViewTarget` names: the host
keeps the tab's identity (session, pane, tab, surface, presentation) per
lifecycle, and each call resolves it, rejecting `InvalidTarget` once the tab
closes, moves to another pane or window, or is replaced, and never following a
replacement. A handle is the same editor interface under another authority,
carried to the native operations as one integer that cannot collide with
invocation IDs. After each committed application change the host drops the
handles whose tab is gone and queues one `onDidClose` notification, as a
payload-free root like setting changes; disposal and lifecycle teardown drop
handles without notifying. Explicit buffer access remains independent
of presentation lifetime; clipboard access is application-scoped.

An editor can open an undo group on its buffer, owned by the calling lifecycle.
The host records each open group with its owner, the opening editor, whose
selection at the group's end it hands to the history, and, when opened through
a handle, that handle. Disposal, lifecycle teardown, and the handle's release or
stale tab end it; a group opened through a command's editor is bounded by
disposal, teardown, or buffer closure, since the command's end must not close
a mode the next command finishes.

Outcomes report execution status rather than semantic values. Resource APIs return
committed data directly; ordinary functions suit composition that needs return
values without command routing. Cancellation and exactly-once response settlement
belong to [host lifecycle](extension-host.md#disposal-cancellation-and-teardown).
