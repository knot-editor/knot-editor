# Architecture Decisions

Validated choices, why they were made, and the evidence that decided them.
[Architecture](architecture.md) and its subsystem references own current
behavior; API contracts live in code. [Roadmap](roadmap.md) and
[active plans](plans/) own deferred and proposed work. The
[documentation guide](README.md#decisions) sets the entry rules and size budget.

## Platform and UI

Use gpui through public APIs so the editor and shell share one framework. Its
macOS text path passed styled, culled, IME, bidirectional, CJK, emoji,
long-line, and large-file fixtures; RTL alignment and logical-index mapping are
Knot's. Skia had lower p50 but worse p99 in primitive benchmarks, and gpui met
the bar, so iced and floem were not evaluated. macOS is the dogfooding
platform; Windows 11 and Linux (Wayland) passed bounded local smoke checks.

Reference: [workbench and lifecycle](architecture/workbench-and-lifecycle.md).

## Text buffer and stable positions

A hand-built stable-ID piece table gives direct control over edit events and
stable positions; a single-threaded UTF-8 core matches foreground ownership.
On a 1M-line, 29 MiB file, edits cost p50 678 µs with the line index, mostly
linear suffix shifts, which is acceptable until real workloads justify a tree
(D092).

Views read lines from the model rather than a private copy, which made every
keystroke O(document) per view, so the line index is eager and also records
UTF-16 line starts for platform text input. Typing extends the trailing add
piece and the consumed edit log is discarded, so long sessions do not degrade.
Inserted newlines follow the first line terminator, avoiding a whole-document
scan for a rare mixed-ending case.

Reference: [core and buffers](architecture/core-and-buffers.md).

## Syntax highlighting

Parse natively with tree-sitter over compiled-in grammars, while extensions
contribute language definitions (LSP ID, grammar, detection, queries) as data.
The tree must follow every commit and painting never calls JavaScript, so
parsing is a native mechanic like shaping. Upstream grammar crates over
`tree-sitter` were chosen over arborium, which lacks incremental parsing and
range queries. SQL is left out because its crate pins `cc` for the whole build.

Neovim's capture names and LSP IDs let adapted upstream queries and future
semantic tokens share one vocabulary, resolved by longest prefix as Zed, Helix,
and Neovim do.

The foreground reparses within 1 ms and continues in the background from a
cheap snapshot (p50 19 µs on a 29 MiB buffer). Ordinary files reparse in well
under a millisecond, while huge files and broken syntax (p99 up to 444 ms) need
the background path. Injections are nested layers. Bundled grammars added about
15 MB to the binary.

Each language is its own small package so its data changes independently;
packages cannot import each other, so languages sharing queries share a package.
A document's language is buffer state; an unregistered explicit ID is kept and
paints plain, so restoration needs no ordering against package startup.

References: [core and buffers](architecture/core-and-buffers.md#syntax-state)
and [semantic UI](architecture/semantic-ui.md#syntax-highlighting-and-languages).

## Language editing configuration

Comment tokens and bracket pairs describe a language, so they live on the
language registration rather than in settings; enabling a feature stays a
setting. One bracket list with per-pair flags serves matching, auto-closing,
and surrounding, as in Zed, instead of VS Code's three lists. Indentation comes
from tree-sitter queries, not regexes. Bundled data drops pairs that text
matching would mislead, such as `<` in Rust.

Packages keep behavior cases as JSONC beside their data, run natively by a
generic Rust test, because a package test against the fake host could only
restate the data.

References: [semantic UI](architecture/semantic-ui.md#syntax-highlighting-and-languages)
and [extension guide](extension-api.md#register-a-language).

## Indentation

Use nvim-treesitter's indent captures and semantics, evaluated natively, so its
queries adapt with light edits. Knot departs where upstream mishandles ordinary
code, such as string contents, block comments, unclosed brackets inside parse
errors, and Python dedent after `return`.

Enter commits first and indents from the reparsed tree, as Zed and Neovim do,
because Helix's pre-edit tree cannot see the typed text. The correction joins
the edit's undo step and is dropped if another commit lands first. Typing only
outdents, and only when the match reaches the caret.

References: [semantic UI](architecture/semantic-ui.md#syntax-highlighting-and-languages)
and [editor views](architecture/workbench-and-lifecycle.md#editor-views).

## Brackets

Matching, auto-closing, and wrapping are native over the registered pairs,
because the closer must land in the opener's commit and undo step, which an
asynchronous extension command cannot join. Data stays programmable through the
registration, and settings turn behavior off.

Matching pairs tokens within one syntax node rather than scanning text, so
strings and comments never match and no per-language query is needed.
Behavior follows VS Code's defaults; overtyping and pair deletion apply only to
closers the view inserted, so fixing unbalanced code can still add a `)`.

References: [editor views](architecture/workbench-and-lifecycle.md#editor-views)
and [semantic UI](architecture/semantic-ui.md#syntax-highlighting-and-languages).

## Editor gutter

The gutter is a fixed order of native lanes, and extensions share one marker
lane, because nothing needs a second lane and each would need its own width and
input rules. The marker lane is always reserved so text never shifts when a
diagnostic appears. Documents show the gutter and text frames opt in through
their registration, since one setting default cannot express that split.

References: [editor views](architecture/workbench-and-lifecycle.md#editor-views)
and [extension guide](extension-api.md#read-only-text-frames).

## Editing commands

Editing commands follow VS Code's behavior and default bindings, the
conventions most users already know; macOS keeps Cocoa's text keys. Each
command is one model batch recording the selection before and after. Move
Lines deletes and reinserts the neighbouring line so anchored contributions
survive.

Indentation style is detected once on load and written as a document-scoped
`editor.insertSpaces`, so it is one ordinary setting that the status bar and
extensions change. A mixed file falls back to the default rather than guessing.

Words split at VS Code's separators, not at UAX #29 boundaries alone, which
kept `some.thing` as one word.

References: [editor views](architecture/workbench-and-lifecycle.md#editor-views)
and [semantic UI](architecture/semantic-ui.md#syntax-highlighting-and-languages).

## Zoom, text size, and soft wrap

Zoom is one application level, as in VS Code, so everything including chrome
scales consistently. It is a state-database record, not a setting, because
settings are rebuilt from code at launch. `font.size` is the per-scope setting.

Wrapping measures monospace cells with `unicode-width` rather than shaping,
which would cost a shaping pass per line on every resize. Views rewrap only
changed lines from a per-revision log on the model, since the extension change
queue cannot be consumed by one view. Toggle Soft Wrap writes a
document-scoped setting so every view agrees.

Row and logical-line movement both exist, as Emacs pairs `next-line` with
`next-logical-line`, so a keymap chooses per key.

References: [editor views](architecture/workbench-and-lifecycle.md#editor-views)
and [zoom](architecture/workbench-and-lifecycle.md#zoom).

## Go to Line and the editor handle

Go to Line is a bundled package, so a workflow that reads lines and moves the
caret proves the public API; it forced the editor handle, line lookups, and
live input. The handle lives within its command's invocation and resolves the
captured presentation on every call, so it cannot act on a view nobody invoked
it in. Columns are computed in JavaScript from one line, so large files never
cross whole. `commands.unavailable()` lets a handler report that it does not
apply.

References: [editor views](architecture/workbench-and-lifecycle.md#editor-views)
and [commands](architecture/commands.md#extension-execution).

## Themes

Themes are contributions registered like languages, latest registration wins,
with no `extends`: a derived theme spreads `themes.get`. Native color keys and
their defaults live in Rust, because native surfaces paint before packages
start, so the defaults are also the fallback. Theme maps stay open, so a key
nobody defines is inert. Contributions name color keys rather than severity
tokens.

Selection is settings, resolved at startup completion and on change, keeping
package load order out of the first frame. Bundled themes share one package
because they are small palettes and Select Theme needs one owner.

References: [semantic UI](architecture/semantic-ui.md#themes-and-color-keys)
and [extension guide](extension-api.md#register-a-theme-and-define-colors).

## Decoration styles

A decoration names a color key and exactly one required style, so no
contributor relies on a default; two styles are two overlapping contributions.
Themes change colors, never styles. Backgrounds paint as quads behind the text
rather than text-run backgrounds, which gpui paints over the selection.

Reference: [semantic UI](architecture/semantic-ui.md).

## Anchored ranges and contributions

Keep stable geometry in core and feature policy in the application, so
independent features share one edit stream. With 10,000 annotations on a
million-line buffer, token stabilization cost p50 about 12 µs per edit against
70 µs for offset remapping, with higher p99, which supports inline
stabilization at that scale.

Reference: [core and buffers](architecture/core-and-buffers.md).

## Reversible edits

Undo uses ordinary mutations so it shares forward editing's anchor repair.
History belongs to the shared model so every view sees one history. Content
identity is tracked apart from monotonic revisions, so undo can return to clean
without weakening conflict detection. Persistent and branching history wait for
a demonstrated need.

Reference: [core and buffers](architecture/core-and-buffers.md).

## Extension runtime

Run JavaScript on direct V8 behind Knot-owned protocols. Persistent isolates
keep author state, and a bounded worker pool fixes thread cost as extensions
grow. Forced interruption is fatal to the lifecycle, giving cleanup a definite
boundary, and teardown releases foreground resources even when JavaScript
cannot run.

Immutable external UTF-16 snapshots serve V8 without changing the UTF-8 core:
10 MiB marshaled in 36 ms copied against 10.6 ms external and 0.3 ms cached,
while a whole-document boundary table exceeded the isolate heap.

Idle isolates rarely collect garbage, so the worker sends a low-memory
notification after startup and long turns, which cut a test package from
14 MiB to 1.6 MiB resident. The fixed idle cost of about 1.2 MiB per package
stays until enough packages justify more (D139).

Reference: [extension host](architecture/extension-host.md).

## Editor handles

A long-lived editor handle binds to the exact tab presentation, so a mode never
acts on a replacement tab; the host re-resolves it on every call and drops stale
handles on refresh, with no second ownership catalog.

Selection notifications carry no payload and coalesce to one pending turn per
handle, so a slow listener cannot grow a queue and pulling current state is
always correct. A caret change reached a listener in p50 305 µs.

Undo groups are opened and closed rather than scoped to a callback, because a
Vim-style change spans extension commands and native typing. Groups, caret
styles, and key contexts are lifecycle-owned, so a stuck extension cannot leave
them behind. Marks are native zero-width anchored ranges, because remapping
from asynchronous events would lag typing.

Reference: [extension host](architecture/extension-host.md#editor-handle-notifications).

## Handler declarations

Annotated async handlers keep argument, result, and error types at their
definition; generated adapters avoid parallel registration tables, and explicit
error codes survive renames. Short foreground requests run in the dispatch turn
to preserve key and text ordering. Values crossing from Rust into JavaScript
keep one hand-written decoder per type, shared with the declaration reader.

References: [extension host](architecture/extension-host.md) and
[commands](architecture/commands.md).

## Installed extension directories

Installed packages are inspectable directories of built JavaScript and a JSONC
manifest. Dependencies order startup but grant no module access across
isolates. An explicit API version is required before evaluation, and a rejected
package keeps its identity so it cannot hide duplicates.

Reference: [extension host](architecture/extension-host.md).

## Bundled extension packages

Bundled packages compile into the binary, so they work identically in
development and release and cannot drift from it. They use the reserved `@knot`
scope, which no installed directory can claim, and the same manifest and
dependency rules; a failing bundled package fails only its dependents.

Reference: [extension host](architecture/extension-host.md#package-admission-and-startup).

## Extension authoring and tests

Bundled packages are TypeScript compiled per file in `build.rs` without a
bundler, so the runtime loads ordinary modules with readable stack traces.
Node as a build requirement was preferred over committed output or a restricted
syntax. Policy is tested under `node:test` against a typed in-memory `knot`
fake, which is not a second host: product tests remain the authority across the
native boundary. Package behavior cases run in a windowless session with the
same key router, native commands, and ordered key delivery as a window. A
window's drawing had been most of a case's cost: on Apple M1 Pro debug builds,
364 Vim cases took 94 s in a window and 1.7 s headless. A fake host that stubs
native commands was rejected, because the cases exist to exercise them.

References: [extension guide](extension-api.md#write-and-test-in-typescript)
and [development](development.md).

## Bundled project registry

`@knot/projects` owns project records while native code owns workspaces and
roots, so project policy stays in JavaScript without private bypasses. A window
belongs to a project when its roots match and the session is active; the
binding is rederived at activation, so a restart cannot resurrect a stale one.
Projects follow root edits rather than offering a separate add-folder command.

References: [extension guide](extension-api.md) and
[workbench lifecycle](architecture/workbench-and-lifecycle.md#automatic-tool-layout-memory).

## Bundled Vim mode

`@knot/vim` is a bundled JavaScript package on public APIs only, because it is
the acceptance test for the editor API; native modal editing and a Helix-style
model were rejected. A JavaScript state machine parses typed text, since
native key sequences cannot enumerate counts. Visual mode keeps its own
anchor, and ordinary text math, over line snapshots, stays pure. Indentation,
joining, `%`, and visual rows invoke native commands so they follow the
language. The unnamed register is the system clipboard, as
`clipboard=unnamedplus` makes it, with a remembered linewise flag. `ctrl-w`
treats a window as a tab, as other editors' Vim modes do, so `c` and `q` close
the tab and cycling skips docks. Search, `.` repeat, and visual-block wait for
Find, recording, and multiple selections.

References: [extension guide](extension-api.md#bind-keys-and-modal-contexts) and
[roadmap](roadmap.md) (D113, D142, D150, D151).

## Project sessions

A project's session is active or suspended. Membership requires an active
session, so opening a suspended project's folder by hand never overwrites its
captures. Switching retains text and durably suspends the source before the
native commit, so a failed or interrupted switch never loses text. Captures are
released only after the record dropping them is durable; orphans are reconciled
at activation. A record with an unknown schema is preserved.

References: [extension guide](extension-api.md#project-sessions) and
[workbench lifecycle](architecture/workbench-and-lifecycle.md#session-replacement).

## Session replacement contract

`workbench.replaceWindows` is generic over windows, documents, and captures, so
any package can build the same workflow. It stages every read before one
protected foreground commit that revalidates revisions, so a racing edit rejects
with nothing changed. A live document always wins over staged content. Released
windows are closed and new ones created with their layout, keeping one creation
path and avoiding moving windows (D062).

References: [workbench lifecycle](architecture/workbench-and-lifecycle.md#session-replacement)
and [documents](architecture/documents-and-persistence.md#retained-captures).

## Window layouts and restoration by position

One versioned JSON layout serves hot exit and extension sessions, so both share
capture, pruning, and installation. It holds no handles, only what recreates
each tab, and view state is byte offsets that clamp. Layouts are captured on
demand and announced by a debounced event rather than pushed on every change.

References: [workbench lifecycle](architecture/workbench-and-lifecycle.md#window-layouts)
and [extension guide](extension-api.md#project-sessions).

## Native window hot exit and launch precedence

The application, not the project extension, records and restores windows
before launch requests, so a broken extension never loses them; projects then
rebind by root set. The record is consumed before restoring, trading the
arrangement for a guaranteed clean launch after a crash. Precedence is
recovered documents, recorded windows, then requests, deduplicated against
what was restored.

References: [workbench lifecycle](architecture/workbench-and-lifecycle.md#native-window-hot-exit)
and [documents](architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Extension restart

Restart Extension is the minimal escape hatch for a stuck turn without timeout
or quota policy (D019). It reuses terminating unload and shared cleanup, then
loads one fresh lifecycle under the same storage. Personal config is excluded;
restarting Knot recovers it.

Reference: [extension host](architecture/extension-host.md#restarting-a-package).

## Personal configuration

One manifest-free config directory holds optional pre-init and post-init
modules sharing state, preferring an existing XDG location. Config failure is
fatal to startup, because partial setup is unpredictable, and an error window
shows the diagnostics.

References: [extension host](architecture/extension-host.md) and
[workbench lifecycle](architecture/workbench-and-lifecycle.md).

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

Reference: [commands](architecture/commands.md).

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

References: [extension host](architecture/extension-host.md#settings) and
[editor views](architecture/workbench-and-lifecycle.md#editor-views).

## Views and extension UI

Share text and contributions through buffer models and keep interaction per
view. Extensions supply semantic data, never rendering callbacks; a widget tree
or WebView waits for an unmet use case. Vertical movement measures display
cells rather than shaped x, so editor state runs headless and tests check real
behavior, at the cost of slight drift beside proportional fallback glyphs.

References: [semantic UI](architecture/semantic-ui.md) and
[editor views](architecture/workbench-and-lifecycle.md#editor-views).

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

Reference: [workbench and lifecycle](architecture/workbench-and-lifecycle.md).

## Workbench navigation

Every pane and tab operation has a command. Splits record ratios while docks
keep absolute sizes. Ctrl-tab switching has no overlay and commits on release.
Directional commands use laid-out rectangles rather than painted bounds.
Preview tabs follow VS Code. Previews, the reopen stack, and maximize describe
a moment, so none is recorded; Reopen Closed Tab reuses the layout's per-kind
recreation.

Reference: [workbench and lifecycle](architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

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

Reference: [workbench and lifecycle](architecture/workbench-and-lifecycle.md).

## Extension tree content and actions

One registration declares a frame's content kind, provider or pushed text, and
actions. Groups are tree headers rather than a second list model. Actions are
command names with arguments over the captured-window route, and status names a
color key. Text frames push read-only text into a private buffer rendered by
the editor surface, avoiding a second renderer (D054).

Reference: [semantic UI](architecture/semantic-ui.md).

## Extension transient interactions

Pickers, inputs, and prompts resolve with typed values, so dismissal is an
ordinary result. Pickers receive complete lists and filter locally, avoiding a
streaming scheduler (D118); one native picker serves everything. Matching uses
`nucleo-matcher` (MPL-2.0) because fzf-quality ranking is not worth redoing.
Highlights and input changes are pulled by the caller's turn rather than
pushed, because a pushed notification would queue behind the awaiting command.
One interaction owns a window at a time. Confirmations reuse the native gpui
prompt, and one shared `TextInput` uses gpui's native input path.

Reference: [semantic UI](architecture/semantic-ui.md).

## Product window chrome and failure notices

A product window shows only product surfaces. The prototype toolbar duplicated
commands, and its status string was the only place some failures appeared.
Native failures with no visible effect now use the native warning prompt;
extensions receive failures as results and decide what to show. A failed
package startup opens the startup report.

Reference: [workbench and lifecycle](architecture/workbench-and-lifecycle.md#product-application-launch).

## Status bar

The bar is native and its items are extension registrations, so the shell owns
layout while packages, including bundled `@knot/status-bar`, own content and
personal config replaces an item by ID. Values are per window, because items
describe one window's editor, and `update` replaces the whole value. Hiding is
the `statusBar.hiddenItems` setting, not unregistration. Indentation choices
are ordinary document-scoped settings writes.

## Generated text surfaces

Generated output that benefits from text interaction uses ordinary buffers and
views; controllers keep semantic identities rather than parsing text. Documents
have no generated state: no product path needed it, and removing it dropped
special cases from recovery, closure, and Save (D027).

Reference: [semantic UI](architecture/semantic-ui.md).

## Capability aggregation

Aggregation is feature-specific and view-owned, with deterministic merging,
provider-local failure, and stable item identities.

Reference: [semantic UI](architecture/semantic-ui.md).

## URI workspaces and filesystem providers

Normalized URIs and an async byte-oriented provider boundary keep generic code
free of platform paths. Documents are independent of workspace containment.
Local roots are trusted, not symlink sandboxes. No-clobber creation,
conditional replacement, and atomic installation protect content; identity
changes only after successful I/O.

Reference: [documents and persistence](architecture/documents-and-persistence.md).

## Multi-root workspaces

A window holds an ordered, deduplicated root list, replaced as a whole.
Nested roots are allowed and browsed independently. Every root is a top-level
row so root actions always have a target. Removing a root never closes
documents.

Reference: [documents and persistence](architecture/documents-and-persistence.md#workspaces-and-resource-trees).

## Application document and root identity

Document UUIDs separate logical identity from mutable URIs and handles. Root
identities with monotonic generations keep stale directory work from becoming
current again.

Reference: [documents and persistence](architecture/documents-and-persistence.md).

## Application observation

Atomic registration, initial snapshot, and watermark prevent gaps. Metadata is
compared at committed transitions rather than kept in a second catalog, and
listeners run on existing FIFO roots without replay or acknowledgement.

Reference: [extension host](architecture/extension-host.md).

## Semantic resource operations

Native commands and extension APIs share product operations that return
committed semantic data, because a status string or later focus query cannot
identify what was opened. Cancellation prevents later mutation but does not
roll back an accepted save.

References: [documents](architecture/documents-and-persistence.md) and
[protected closure](architecture/workbench-and-lifecycle.md#workbench-and-protected-closure).

## Folder selection

`workspaces.pickFolder` returns a normalized, checked directory without
committing roots, so a project workflow decides what to do with it. It uses
the window's transient interaction slot.

## Extension record storage

One SQLite database per canonical package identity, plus one for config, so
ownership survives reloads and path changes. Synchronous calls on the
background executor with FULL synchronization avoid a storage service; one
JSON record is the atomic unit. Quit drains admitted work before `cx.quit()`,
because gpui's shutdown wait is too short.

Reference: [extension host](architecture/extension-host.md#extension-record-storage).

## Application state database

Application UI state lives in one SQLite file with a table per record, so one
connection serializes writes and quit flushes them together. Recovery stays
separate: damaged recovery must be preserved and reported while damaged UI
state reads as absent, and a UI-state failure must never reach the text store.

Reference: [workbench and lifecycle](architecture/workbench-and-lifecycle.md#automatic-tool-layout-memory).

## Unsaved-document recovery store

Unsaved text is protected in an application-owned database independent of
extension health. Automatic rows protect live dirty documents; retained rows
are immutable checkpoints that automatic captures never overwrite. Captures
debounce about a second, at least every five seconds while typing, and commit
with FULL synchronization off the foreground, which is cheap next to losing
text. A missing or damaged database is preserved and reported.

Reference: [documents and persistence](architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Unsaved-document restore and hot exit

A recovered document loads its current resource and applies the text as one
edit, so dirty state comes from content comparison and no history is
persisted. A changed file keeps its capture-time version, so Save reaches the
conflict prompt. A live document is never replaced; conflicts open an untitled
copy. Startup restores before launch requests so they deduplicate
deterministically.

Quit is a hot exit, as in VS Code and Zed: once dirty documents are durable,
quit skips save prompts, and a store failure falls back to the prompts. Closing
a tab or window still prompts.

Reference: [documents and persistence](architecture/documents-and-persistence.md#durable-unsaved-document-recovery).

## Retained recovery captures

Extensions retain text through application-stamped owners and IDs, so they
cannot name another owner or overwrite a capture. Owning by identity keeps
captures addressable across reloads and restarts. Each retain is a new
immutable capture that resolves after its durable commit, which orders records
without a barrier. Orphans are preferred to premature deletion; only release or
explicit discard removes a capture.

Reference: [documents and persistence](architecture/documents-and-persistence.md#retained-captures).
