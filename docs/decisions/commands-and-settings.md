# Commands and settings decisions

The command namespace, key routing, keymaps, and the settings store. The
[decisions index](../decisions.md) lists every topic, and the [documentation
guide](../README.md#decisions) sets the entry rules.

## Commands and keymaps

One semantic command namespace serves keys, menus, palette, and scripts.
Captured presentation authority preserves intent across awaits without ambient
access to whichever window is active later. Composition is bounded awaiting,
not a task graph.

Knot resolves bindings and matches keys itself, because gpui matches the
contexts of the last rendered frame, which let one package's sequence swallow
keys in every editor. This also makes key handling headless, so package cases
run without a window.

Configuration is code that runs in order, as in Emacs: the latest write wins,
and view-specific bindings beat global ones. Ranking writers by owner, as VS
Code does for its declarative file, was rejected. Native defaults are a preset
beneath every write, so switching presets never overrides extension or personal
bindings.

Modal editing keeps its grammar in JavaScript over three primitives: key
contexts, context-scoped bindings, and text bindings. Keys after a key-invoked
extension command wait and replay in order rather than adding a synchronous
API; measured at p50 about 170 µs, and it fixed text landing before the
command's commit. The hold releases on focus loss or after one second.

Overlaps are not conflicts; Show Keybindings lists every slot. The palette
lists by title, low-level commands opt out with one flag instead of `when`
clauses, and recently used commands persist across restarts.

Reference: [commands](../architecture/commands.md).

## Application settings

Settings work like Emacs variables: one in-memory store all code writes, latest
write wins, rebuilt from code on every launch. A scope is a set of facts; the
match with most facts wins and ties go to the latest write, one rule that
future facts extend. Document-scoped values end with their document.
Definitions follow `defcustom`, values may precede them, and undefined keys
stay inert.

Detection overrides are lifecycle-owned claims (`languages.associate`), not a
map setting: a map-valued `files.associations` was reverted because one writer
replaced the others' entries and teardown could not remove them.

References: [extension host](../architecture/extension-host.md#settings) and
[editor views](../architecture/workbench-and-lifecycle.md#editor-views).
