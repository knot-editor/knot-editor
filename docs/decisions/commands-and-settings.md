# Commands and settings decisions

The command namespace, key routing, keymaps, and the settings store. The
[decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Commands and keymaps

One semantic namespace and dispatcher serves keys, menus, palette, and scripts.
Captured presentation authority preserves intent across focus changes and awaits
without granting ambient access to whichever window is active later. Bounded
awaited composition supports reuse without a general task graph; ordinary
functions remain preferable when a caller needs a semantic return value.

Knot resolves keymap precedence itself because gpui's context depth cannot
express defaults beneath a last-write-wins stack. It matches keys itself too:
gpui matches the contexts of the last rendered frame, so bindings had been
registered by key and view kind alone, and a sequence bound in one package's
context then held its first key in every editor and resolved to nothing where
the context was off; an installed package's `z b` swallowed Vim's `zb`. Matching
outside gpui also keeps key handling free of windows, so windows without one and
package cases need none.

Writers are not ranked by owner. Configuration is code that runs in order, as in
Emacs and Neovim, so the latest write wins and personal overrides belong in
`post-init`. VS Code and Zed rank the user's keymap above extensions only
because it is a declarative file with no place in a load order. View-specific
bindings still beat global ones, as Emacs mode maps beat the global map. The
accepted cost is that a package restarted or binding late beats earlier personal
writes until Knot restarts; this is rare, and settings behave the same way.

Default bindings are a preset beneath every write, like Zed's base keymap, so
switching presets never overrides extension or personal bindings. Presets follow
the theme rule: the Knot preset is native data, because typing and saving must
work before packages start and when they fail, and other presets are
registrations with no `extends`. The selection resolves at startup completion
and on change rather than on registration, which keeps package load order out of
the first keystroke. Extensions branch on the platform in code rather than
registrations carrying per-platform fields. On Windows and Linux terminals
follow VS Code and Zed in leaving plain ctrl chords to the shell; on macOS editors
take the Cocoa Emacs keys every native text field accepts.

Modal editing keeps its grammar in JavaScript over three generic primitives: key
contexts, context-scoped bindings, and text bindings. Counts, operators, and
pending state cannot be native sequences, because a pending sequence times out
and counts cannot be enumerated as bindings. A context is one more scope fact,
so context bindings follow the settings rule rather than gpui's context depth.
Text bindings follow VS Code's `type` override scoped to a context, as an Evil
state map has its own default binding, and receive text after keyboard layout
and IME, so symbols are right on any layout.

Keys after a key-invoked extension command wait for it and replay in order,
for every command without opt-in, rather than adding a synchronous host API:
native painting and raw input never call JavaScript, and a busy isolate would
block the foreground. On Apple M1 Pro/macOS release builds under the gpui test
dispatcher, a key bound to a command awaiting `setSelection` reached the
committed caret at p50 169–176 µs, p99 245–277 µs, and a no-op command at p50
21 µs, so most of the path is the foreground request and the turn's resumption;
a real window adds its wakeup latency within the frame margin. Text typed right
after the key landed before the commit in 199–200 of 200 trials without ordering
and in none with it. Releasing the hold on focus loss and after one second keeps
a command awaiting a picker from swallowing its typing and a stuck extension
from keeping the keyboard.

Overlapping bindings are not conflicts, because the latest write winning is the
rule rather than an error; Show Keybindings lists every slot instead. The
palette lists commands by title because names are an addressing scheme. Low-level
commands opt out of it with one flag rather than the palette gaining context
rules like VS Code's `when` clauses, since a command that does not apply to its
target already reports so. Recently used commands rank first, as in VS Code, and
persist because rebuilding the list each run would lose the ranking that
makes the palette fast.

Reference: [commands](../architecture/commands.md).

## Application settings

Settings work like Emacs variables: one in-memory store that all code writes,
where a later write replaces an earlier one whoever made it; ranking writers by
owner was rejected for the same reason as for keymaps. Nothing is persisted
natively, because config and packages rebuild the store on every launch; a
settings UI is an extension that keeps its choice in its own storage and sets it
again at start.

A scope is a set of facts, and the match with the most facts wins, with ties to
the latest write, so a scoped value beats an unscoped one however late that is
written, as an Emacs buffer-local value beats `setq-default`. Ranking by fact
count keeps one rule that future facts, such as a path for project settings
(D089), extend without new precedence.

Values scoped to a document end with it, because they are state of a document
that exists only while it does, and keeping them would leak one entry per file
ever opened. The buffer carries its document's identity as a stamp rather than
views looking it up, because tab size is read for every painted line.

Definitions follow `defcustom`. Values may precede their definition because
`pre-init` runs before packages start, and undefined keys stay inert, as in
Emacs. Teardown keeps values, which belong to nobody, so Restart Extension needs
no rewrite. A setting that names a contribution, such as a theme, holds only the
name and falls back when it is unregistered, so values never depend on package
load order.

Detection overrides are lifecycle-owned claims (`languages.associate`), not a
setting. A map-valued `files.associations` was built and reverted: a map is one
value, so one writer's `set` replaced every other writer's patterns, teardown
could not remove a package's entries, and a merging write would still leave
entries without owners.

References: [extension host](../architecture/extension-host.md#settings),
[editor views](../architecture/editor-and-languages.md#editor-views), and
[languages](../architecture/editor-and-languages.md#syntax-highlighting-and-languages).
