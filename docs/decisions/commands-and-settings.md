# Commands and settings decisions

The command namespace, key routing, keymaps, and the settings store. The
[decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Commands and keymaps

Use one semantic namespace and dispatcher across keys, menus, palette, and
scripts. Captured presentation authority preserves intent across focus changes
and awaits without granting ambient access to whichever window is active later.
Explicit buffer access remains useful independently of a presentation.

Bounded awaited command composition supports reuse without a general task graph:
inline same-extension children avoid self-queue deadlock, while ancestry and
one-child rules constrain composition. Outcomes describe execution; ordinary
functions remain preferable when semantic return values are needed.

Resolve keymap precedence in Knot because gpui context depth cannot express
defaults beneath a last-write-wins stack. Retain owner slots so teardown reveals
earlier bindings. Atomic per-turn JavaScript batches prevent partial keymaps
and make startup readiness follow foreground state.

Match keys in Knot too, rather than registering every binding with gpui.
gpui can only match the contexts of the last rendered frame, so bindings were
registered by key and view kind alone, and a sequence bound in one package's
context then held its first key in every editor and resolved to nothing where
the context was off: an installed package's `z b` swallowed Vim's `zb`. Knot's
router holds a key only where some binding beginning with it resolves to a
command, and delivers the keys of a sequence that stops matching as if
unbound, as gpui did. Matching outside gpui also keeps key handling free of
windows, so window-independent sessions and package cases run without one.
gpui keeps bindings only to label menus.

Keybindings and languages do not rank writers by owner. Configuration is code
that runs in order, as in Emacs and Neovim: packages start after `pre-init`
and `post-init` runs after them, so the latest write wins and personal
overrides belong in `post-init`. VS Code and Zed rank the user's keymap above
extensions only because it is a declarative file with no place in a load
order. View-specific bindings still beat global ones, as Emacs mode maps beat
the global map and more specific setting scopes win. Languages need no base:
nothing switches bundled languages at runtime. The accepted cost is that a
package restarted or binding late beats earlier personal writes until Knot
restarts; this is rare, and settings behave the same way.

Default bindings are a preset in one slot beneath every write, like Zed's base
keymap, selected by the `keymap.preset` setting, so switching presets never
overrides extension or personal bindings. Presets follow the theme rule: the
Knot preset is native data, because typing and saving must work before packages
start and when they fail, and other presets are lifecycle-owned registrations
by name with no `extends`, so a derived or user-written preset spreads
`keymaps.getPreset`. The selection resolves at startup completion and on
setting changes, not on registration, which keeps package load order out of the
first keystroke. Extensions learn the OS from a `knot.platform` constant and
branch in code rather than registrations carrying per-platform fields. Knot's
own tables write shared keys with `secondary-`, expanded per platform so tests
check all three tables on any host. On Windows and Linux terminals follow VS
Code and Zed: plain ctrl chords belong to the shell, derived from the table
rather than listed, and copy and paste move to ctrl-shift-c and ctrl-shift-v.
On macOS editors take the Cocoa text system's Emacs keys, which every native
text field accepts.

Modal editing keeps its grammar in JavaScript and gets three generic
primitives: lifecycle-owned key contexts on editor views, bindings scoped to
a context, and text bindings that route typed text to a command. Counts,
operators, and pending state cannot be native sequences, because a pending
sequence times out after one second, as gpui's did, and counts cannot be
enumerated as bindings. A context is one more scope fact, so context bindings
join the settings rule of most facts, then latest write, rather than gpui's
context depth. Knot resolves contexts against the view's live state, because
gpui matches the last rendered frame and a context set by the previous key may
not be painted yet. Text bindings follow VS Code's `type` override scoped to a
context, as an Evil state map has its own default binding, and receive text
after keyboard layout and IME, so symbols are right on any layout. A text
command gets exactly `{ text }` rather than merged fixed arguments, and preedit
is not shown where text routes to a command, since nothing would ever insert it.

Keys after a key-invoked extension command wait for it and replay in order,
for every such command without opt-in, rather than adding a synchronous host
API: native painting and raw input never call JavaScript, and a busy isolate
would block the foreground. On Apple M1 Pro/macOS release builds under the
gpui test dispatcher, a key bound to a command awaiting `setSelection`
reached the committed caret at p50 169–176 µs, p99 245–277 µs, and a no-op
command at p50 21 µs, so most of the path is the foreground request and the
turn's resumption; the dispatcher polls instead of running the platform loop,
so a real window adds its wakeup latency within the frame margin. Text typed
right after the key landed before the commit in 199–200 of 200 trials without
ordering and in none with it, at p50 about 194 µs. The hold releases when
focus leaves the view, so a command awaiting a picker never swallows its
typing, and after one second, so a stuck extension never keeps the keyboard;
Restart Extension remains the recovery.

Overlapping bindings are not reported as conflicts, because the latest write
winning is the rule rather than an error. Show Keybindings instead lists every
slot with its owner and what overrides it. Menus show the effective key
through bindings in a context nothing declares: gpui 0.2.2 reads a menu's key
equivalent from the bindings of its action, and the window handles a key
before the menu does, so a key never runs twice.

The palette lists commands by title and description rather than by name,
because names are an addressing scheme, while still matching the name for
users who know it. Low-level input commands opt out of the palette instead of
the palette gaining context rules like VS Code's `when` clauses: hiding a few
commands that only make sense from a key needs one flag, and a command that
does not apply to its target already reports so. Recently used commands rank
first, as in VS Code, and the list survives restarts because rebuilding it
each session would lose the ranking that makes the palette fast.

Reference: [commands](../architecture/commands.md).

## Application settings

Settings work like Emacs variables: one in-memory store that personal config,
packages, and native code all write, where a later write replaces an earlier
one whoever made it. This is the keymap rule above applied to values, and
ranking writers by owner was rejected for the same reason. Nothing is
persisted natively, because config and packages rebuild the store on every
launch; a UI that changes a setting is an extension that keeps its choice in
its own storage and sets it again in `start()`.

A scope is a set of facts that must all match a read's context: a document's
language and a view kind, with a file path to come for project settings
(D089). The match with the most facts wins and ties go to the latest write, so
a scoped value beats an unscoped one however late that is written, as an
Emacs buffer-local value beats `setq-default`. Ranking by fact count keeps one
rule that a `path` fact extends without new precedence.

A `document` fact, the document's UUID, scopes a value to one document, such
as its detected or chosen indentation. Values otherwise belong to nobody, but
these are state of a document that exists only while it does, so releasing the
document drops them rather than leaking one entry per file ever opened. They
are not persisted: a restore detects again, as other view state does. The
buffer carries its document's identity as a stamp rather than views looking
it up, because tab size is read for every painted line; the stamp is not
resource identity, which stays with the document.

Definitions follow `defcustom`: a key gets a type, a default, and a
description that a future settings UI lists. Values may precede their
definition because `pre-init` runs before packages start; definition
validates them, and keys never defined stay inert, so typos are silent as in
Emacs. Teardown removes a lifecycle's definitions but not the values, which
belong to nobody, so Restart Extension needs no rewrite. A write of the wrong
type is a host error rather than a `TypeError`, because only the native store
knows the definition. A setting that names a contribution, such as a theme,
holds only the name and falls back when it is unregistered, so values never
depend on package load order.

Detection overrides are claims, not a setting. A map-valued
`files.associations` was built and reverted: a map is one value, so one
writer's `set` replaced every other writer's patterns and teardown could not
remove a package's entries, and a merging write would still leave entries
without owners. `languages.associate` makes each claim lifecycle-owned in the
same order as registrations, so claims compose and disposal reveals the
previous one.

References: [extension host](../architecture/extension-host.md#settings),
[editor views](../architecture/workbench-and-lifecycle.md#editor-views), and
[semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages).
