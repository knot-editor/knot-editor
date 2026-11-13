# Roadmap

Everything Knot has not built yet: known bugs, architectural work and technical
debt, and features. Completed work lives in the
[architecture](architecture.md) references and [decisions](decisions.md).

Every item has the same shape:

- **Title** with its stable `D###` identifier, or `B###` for bugs. IDs are
  never renumbered or reused; a new item takes the next unused ID.
- **Depends on** lists the unfinished items that must be complete, or designed
  together with this one, before it. Finished items are not listed.
- **Description** says what is missing today and when it becomes worth doing,
  then what to build.

Each section is ordered so that an item comes after the items it depends on,
wherever the sections allow it. Items with no dependencies are ordered by
priority. An item marked 🚧 has a plan under [plans/](plans/).

Knot has no users, so every current command, binding, setting, surface, API,
and stored format may be redesigned or removed, with no migration or
compatibility path. Descriptions name current behavior only as a starting
point.

## Working an item

Before implementing an item, draft its plan with the `/brainstorm-plan` skill:
it analyzes the item in its own session and writes the design in
`docs/plans/d###-<slug>.md`, however small the item is. Mark the item 🚧
with a link to the plan, and keep design detail out of this file. Remove the
item once its durable behavior lives in the architecture references and
decisions, and remove its plan with it. Reports under [reports/](reports/)
hold the analyses that support some items.

## Bugs

- **D183 — Deterministic indentation after Enter and closing tokens**
  - Depends on: none
  - Description: Enter and a typed closing token such as `}` or `else` leave a
    request for the indent query that waits for a syntax tree of that exact
    revision, and any later edit drops it. When the parse exceeds the 1 ms
    foreground budget and finishes in the background, a key typed first leaves
    the line at the wrong indentation, so the result depends on timing in
    large files. Enter between an auto-closed pair is unaffected. Fix it now:
    let these two commands parse in the foreground up to a larger budget,
    chosen from a benchmark of incremental reparses on a large file, and keep
    the deferred request only beyond it. Rebasing the request over later
    typing was rejected: the line would shift after the typed text and needs
    rules for when to give up.

- **D175 — Type-ahead after a held Enter**
  - Depends on: none
  - Description: Enter on a query-driven picker whose rows are not yet current
    waits for them, and keys typed meanwhile are swallowed. Fix it when dropped
    keys are noticed in daily use, or another command opens something after a
    picker: hold those keys and replay them through key routing once the opened
    view has focus.

## Architecture and technical debt

- **D182 — Interleave extension roots at awaits**
  - Depends on: none
  - Description: A package runs one root (command, listener, timer, or
    provider call) at a time to completion, and extension commands are serial
    across all packages. An awaiting command freezes its package's listeners
    and providers; a command that awaits a timer or an event of its own
    package deadlocks it until restart; and a language client cannot handle
    out-of-order responses or server requests. Do it now, before D067 and
    D117: carry each root's identity through its continuations instead of
    global state, let the scheduler keep several live roots per package, make
    the command queue per package, and audit the bundled packages for state
    assumed stable across an await. The
    [scheduling report](reports/extension-task-scheduling.md) has the analysis
    and proposed checkpoints.

- **D092 — Piece tree text buffer**
  - Depends on: none
  - Description: `TextBuffer` keeps its pieces in a flat `Vec` searched
    linearly, shifts a flat line-start index on every edit, and copies the
    piece list for each snapshot. Coalesced typing keeps the piece count low,
    so measured typing latency is acceptable. The change absorbs the former
    piece-table compaction (D013) and line-start index (D014) items. Do it
    before or together with the first bulk-edit feature (Replace D001,
    multi-cursor editing D142, substitution D150, formatting, or language-server
    edit lists D067), or when long editing runs show measured latency, memory,
    or stabilization problems. Follow the
    [text buffer analysis](reports/text-buffer-structure.md): benchmark the
    current buffer, store the pieces in a copy-on-write B-tree that summarizes
    bytes, line breaks, and UTF-16 length, resolve positions through ordered
    piece locators, and keep the piece semantics, edit log, and `TextBuffer`
    API unchanged.

- **D186 — Much faster Rust test suites** 🚧
  - Depends on: none
  - Description: Follow its [plan](plans/d186-faster-test-suites.md).
    Product-driven suites are CPU-bound. `knot-ext-vim` takes
    ~1.1 s wall time in parallel on 10 cores but ~4.5 s serially, and neither
    lower nor higher thread counts help; the workspace takes ~12 s. Every test
    boots a full product (V8 pool, gpui, stores), the 21 general Vim tests
    each boot their own, and every key settles the extension pool. Cutting
    per-case work by ~0.8 s CPU moved the parallel wall time by only ~0.1 s,
    so only less total CPU work helps. Do it now: profile where a product
    test's CPU goes (startup, settling, V8 turns), then share one started
    product across the tests of a binary where isolation allows, and make
    settling after each key cheaper.

- **D156 — Fewer host round trips per key**
  - Depends on: none
  - Description: Found by the headless package tests. A key bound to an
    extension command costs several foreground requests, each a turn on an
    extension worker and a response back: a Vim motion reads the selection, a
    line, and the tab size, writes the caret, and looks up the focused editor's
    window, and the selection notification and the status bar package read
    again. Headless Vim cases take about 4.7 ms each on Apple M1 Pro/macOS
    debug builds, about 40% of it waiting on those turns, and half of each
    turn is the debug-built host glue. Do it when package cases grow slow
    enough to matter again, or typing latency in a modal package is noticeable
    in the release app: give the command context the editor so the window
    lookup goes, let an editor handle read what a motion needs in one request,
    and measure whether building the host glue optimized in debug, as a crate
    of its own, is worth it.

- **D139 — Per-package isolate floor** 🚧
  - Depends on: none
  - Description: Each package keeps its own isolate, which costs about 1.2 MiB
    resident while idle; a bare isolate with an empty context is about
    1.05 MiB of that, and the bootstrap a startup snapshot could skip is about
    0.15 MiB. At 40 packages that is about 48 MiB, which is not yet worth the
    work with today's handful of packages. Do it when Knot ships enough
    extensions that their idle cost shows in daily use, with 40 or more as the
    target. Follow its [plan](plans/d139-isolate-floor.md): measure the real
    set again, try V8 flags for small isolates, then a startup snapshot, and
    design sharing or unloading only if those fall short.

- **D129 — Refactor builtin language extensions**
  - Depends on: none
  - Description: Each bundled language package is named `@knot/language-<name>`
    because a package ID is exactly `@scope/name`. The grammars without a
    package of their own still share `@knot/language-others`. Do it before the
    next bundled language package, or when the remaining grammars get packages
    of their own: decide whether package IDs gain a nested form such as
    `@knot/language/rust`, which touches identity validation, discovery,
    storage paths, and the build, and split the remaining grammars.

- **D133 — TypeScript case modules for native behavior tests**
  - Depends on: none
  - Description: Language packages keep the cases that native code runs
    against their registered data, such as comment toggling, as JSONC files
    beside the package, which a generic Rust product test reads. TypeScript
    modules would type-check the cases and share them in code, but the Rust
    runner cannot read TypeScript without spawning Node at test time,
    evaluating them in `build.rs`, or a test-only V8 loading path. Do it when
    more native features need per-package cases (auto-closing, indentation,
    text objects) and untyped JSONC causes mistakes or repeated cases: choose
    one of those loading paths and replace every JSONC case file with
    TypeScript, so packages keep one format.

- **D138 — Extension startup report as an untitled document**
  - Depends on: none
  - Description: Developer: Show Extension Startup Report toggles a custom
    overlay that the window renders, and launch opens it when a package
    failed. Developer: Show Extension Memory already presents its report as a
    clean untitled document, which can be selected, copied, searched, and
    compared across runs without a bespoke surface. Do it when the overlay
    needs a change, or reading or copying a failure from it gets in the way:
    render the report as text into a clean untitled document as Show Extension
    Memory does, open it from launch when a package failed, and remove the
    overlay, its toggle, and its rendering.

- **D054 — Dedicated read-only generated text surface**
  - Depends on: none
  - Description: Contributed text frames embed the ordinary editor view over a
    private read-only buffer. This reuses selection, copy, scrolling, and
    rendering without a second renderer, but couples tool frames to the editor
    key context and command surface. Do it when a text frame needs behavior
    the editor surface cannot express, or tool-focused keymaps and editor
    commands conflict: extract a dedicated read-only presentation over the same
    buffer layer.

- **D063 — Migrate the terminal engine to libghostty**
  - Depends on: none
  - Description: The `alacritty_terminal` adapter meets current terminal
    behavior. Ghostty VT was rejected earlier for its separate PTY
    integration, pre-1.0 FFI, Zig packaging, integration size, and poor debug
    parsing performance. Do it when libghostty offers a stable embeddable API
    and packaging that fits the Rust build, or covers terminal behavior
    Alacritty lacks (such as D041 protocols), and with or before D041 if it
    covers those protocols. Replace the emulator behind the existing
    application-owned session without changing its PTY ownership or
    single-attachment contract.

## Features

### Workspace, projects, and files

- **D087 — Extension entitlements**
  - Depends on: none
  - Description: Packages declare entitlements in `knot.jsonc`, starting with
    process spawning and outbound HTTP. An installed package prompts once, on
    its first load, and again when an update adds an entitlement; the grant is
    stored by canonical package identity. Bundled packages declare entitlements
    through the same path but are granted by shipping with the binary, and
    personal config is implicitly granted. A missing or denied entitlement
    makes the API reject with a typed error instead of failing the package, and
    grants can be reviewed and revoked. The APIs are spawning a process (with
    arguments, working directory, and environment; streaming stdout and stderr,
    writing stdin, observing exit, and killing it; processes are cleaned up
    when the lifecycle tears down) and a `fetch`-shaped HTTP client. This
    provides consent and visibility, not a sandbox: runtime isolation is not a
    security boundary, and the decision record must say so. Decide how bundled
    consumers locate tools: bundle `rg` with Knot, or require it on PATH.

- **D089 — Project-local settings**
  - Depends on: none
  - Description: Settings read from a file in the repository, attached to
    folders rather than to projects, because a folder may belong to several
    projects. Add the settings store's `path` scope fact with its glob syntax
    and path normalization. Decide the file's location and format (declarative
    data or code), and whether settings for one particular root set live in
    the `@knot/projects` record. Workspace trust (D124) stays deferred, so
    project settings must not run code until it exists.

- **D084 — Project selector modal**
  - Depends on: none
  - Description: `@knot/projects` replaces its Projects tool with a picker
    modal: recent projects first, then switch, new, rename, and delete as
    picker actions or companion commands. Remove the Projects tool
    registration and its tree code.

- **D123 — Project inference extension** 🚧
  - Depends on: none
  - Description: A bundled extension that finds the version-control root above
    an opened document outside the window's roots and offers to open it as a
    project, as Emacs's project.el infers projects. Decide which markers count
    (`.git` first), whether a window without roots switches without asking, and
    how often the offer may repeat for the same folder. See its
    [plan](plans/d123-project-inference.md).

- **D085 — Cross-file search**
  - Depends on: D087
  - Description: A bundled extension that wraps ripgrep through process
    spawning and parses its `--json` output, passing the effective ignore
    rules. It provides a query input with case, regex, whole-word, and glob
    options; streamed results in a tree grouped by file (contributed tree
    groups and row actions already exist); opening a result at its range as a
    preview tab; cancelling a superseded search; and refresh. It promotes
    D007: decide here how results recover when edits make their targets stale.

- **D126 — Git status per root**
  - Depends on: D087
  - Description: No Git integration exists, though every compared editor shows
    at least the branch and file status, built in or through a near-universal
    plugin. Do it now that the explorer is in daily use: show each root's
    branch and file status in the explorer and the status bar from a bundled
    extension over process spawning or a native library, and decide how status
    refreshes with file watching.

- **D124 — Workspace trust**
  - Depends on: D089
  - Description: Project-local settings start as data that runs no code, and
    nothing else executes on behalf of a repository. Do it when project
    settings, tasks (D125), or language servers would run code or commands from
    an opened repository, and before D125 runs repository-defined commands:
    decide, as VS Code's workspace trust and Neovim's `exrc` trust database do,
    how a folder becomes trusted, what an untrusted folder disables, and where
    grants are stored, reusing D087's grant storage if it fits.

- **D090 — EditorConfig support**
  - Depends on: D089
  - Description: `.editorconfig` files need the `path` scope fact; file
    watching exists (`files.watch`). Do it once D089 is complete, or when daily
    use hits a project that relies on `.editorconfig`: a bundled package finds
    the files above each opened document, turns each section into a
    `path`-glob scope, and writes them from the root down to the nearest file,
    so last-write-wins reproduces EditorConfig's rule that the nearest file
    takes precedence.

- **D091 — Settings UI**
  - Depends on: none
  - Description: Settings are set from code. Definitions carry a type, default,
    and description so a UI can be built, but none exists. Do it when
    discovering or changing settings from code becomes the friction in daily
    use: build the UI as an ordinary extension over `settings.define`, `get`,
    `set`, and `observe`. It keeps its choices in extension storage and applies
    them again in `start()`, which beats `pre-init` and loses to `post-init`,
    so it must show when a later write overrides a choice. Also decide whether
    it lists undefined keys that hold values, to help spot typos.

- **D184 — Document-scoped settings survive hot exit**
  - Depends on: none
  - Description: Values scoped to a document end with it, and a window layout
    records only a document's explicit language. After quit and relaunch a
    restored document keeps its language but loses its own `editor.tabSize` and
    `editor.softWrap`. Do it when daily use resets a document's tab size or
    wrapping on relaunch: record a document tab's document-scoped setting
    values in the window layout and write them back on restore, as the explicit
    language is, so nothing is kept for documents that are not open.

- **D164 — Command-line launcher** 🚧
  - Depends on: none
  - Description: Knot starts only from the app bundle or `cargo run`; there is
    no `knot <file>...` helper on the `PATH` like VS Code's `code`. Do it for
    daily use from a terminal: add a small `knot` helper that passes the paths
    to the running instance, or starts one, and an install command for the
    `PATH` entry. See its [plan](plans/d164-command-line-launcher.md).

- **D165 — Launcher files open in a new window unless in an open project**
  - Depends on: D164
  - Description: Files given on the command line need a placement rule. Decide
    it together with D164: open them in a new window, except that a file inside
    the roots of an already-open project opens in that project's window.

- **D166 — Guided personal configuration**
  - Depends on: none
  - Description: A new user gets no hint that Knot is customized through
    personal config and extensions. Do it before Knot is offered to other
    users: on first launch, create an `AGENTS.md` in the user config folder
    that tells coding agents how to customize Knot, and present that folder as
    a special built-in project so it is one click away.

- **D061 — Start surface in the empty center**
  - Depends on: D084
  - Description: An empty center paints a blank surface that opens documents
    and takes drops, but lists nothing. When a start surface (recent projects
    and files, common commands) is selected, draw it in the empty center,
    keeping its focus and drop behavior. It lists the recent files and the
    projects of D084.

- **D169 — Default key for Toggle Explorer**
  - Depends on: none
  - Description: Toggle Explorer has no key; Toggle Left Dock (`secondary-b`)
    already shows the explorer, which lives in the left dock. IntelliJ binds
    `cmd-1` and VS Code `cmd-shift-e` to focus the explorer. Do it when daily
    use wants the explorer focused or toggled apart from its dock, or tool
    stripes make tool toggles common: choose a key against the presets.

- **D176 — Drag and drop in trees**
  - Depends on: none
  - Description: The explorer moves files through Cut and Paste or Rename;
    contributed trees take no drags or drops. Do it when daily use wants files
    moved by pointer: add native drag sources and drop targets to contributed
    trees, reported to the extension as semantic row IDs, and use them in the
    explorer.

- **D177 — Multi-select in trees**
  - Depends on: none
  - Description: Contributed trees select one row, so explorer actions apply to
    one item. Do it when daily use wants bulk delete, copy, or move: let trees
    select several rows, return them from the selected-item API, and apply
    explorer actions to all of them.

- **D178 — Compact folders in the explorer**
  - Depends on: none
  - Description: Each folder is its own row. VS Code and IntelliJ can join a
    chain of single-child folders (`a/b/c`) into one row. Do it when deep
    single-child chains, such as Java packages, make the explorer tedious:
    decide row identity, reveal, and actions on a joined row.

- **D174 — Show Difference for files changed on disk**
  - Depends on: none
  - Description: The Reload or Keep prompt for a document changed on disk
    offers no comparison, because Knot has no diff view. Do it when a diff view
    exists, such as with Git integration (D126), or choosing between Reload and
    Keep blind becomes friction in daily use: add Show Difference to the
    prompt, comparing the buffer with the file.

- **D006 — File drag and drop**
  - Depends on: none
  - Description: Open and Save As establish workspace-independent documents
    first. Do it when platform interaction becomes a selected product slice.

- **D009 — Binary buffers and encoding detection**
  - Depends on: none
  - Description: The first slice explicitly targets UTF-8 text. Do it when a
    concrete file workflow requires non-UTF-8 or binary inspection.

- **D171 — Binary file reads for extensions**
  - Depends on: none
  - Description: `files.readText` is text-only, because no consumer needs
    non-UTF-8 content, and a binary transport would be designed without one.
    Do it when a consumer needs non-UTF-8 file content: add a bytes read beside
    `readText`.

- **D011 — Remote filesystem providers**
  - Depends on: none
  - Description: The URI boundary is validated with local and memory providers.
    Do it when a specific remote workflow is selected.

- **D012 — Persistent undo and history trees**
  - Depends on: none
  - Description: Session-local linear undo is sufficient for dogfooding. Do it
    when users need edit history across restarts or branching history.
    Byte-offset view state already survives restarts and project switches.

- **D015 — URI deduplication across processes**
  - Depends on: none
  - Description: The first slice handles identity within one application
    process. Do it when multi-process editing or document handoff is
    introduced.

- **D016 — Sandboxed workspace roots**
  - Depends on: none
  - Description: Local roots are trusted and intentionally follow symlinks. Do
    it when Knot opens untrusted workspaces under a security boundary.

### Extension platform and language tooling

- **D117 — Non-modal notifications and progress**
  - Depends on: D182
  - Description: Extension messages use the native modal prompt, and native
    success and progress notices were dropped because their effect is visible.
    A long-running extension task, such as starting a language server or
    indexing, has no non-blocking way to report progress or a passing notice.
    Do it before D067, when D085 or another bundled extension runs work that
    takes seconds: add a transient notice and a progress handle with a title,
    optional percentage, and cancellation, shown through the status bar rather
    than a new surface.

- **D181 — Native operation failures in the status bar**
  - Depends on: D117
  - Description: A native operation whose failure leaves no other visible
    effect, such as Save, Open, Add Folder, or recovery, reports it through a
    native warning prompt, which interrupts and leaves no trace once
    dismissed. The status bar carries no operation messages. Do it with or
    after D117: report these failures through its transient notice, keep the
    last ones reachable from the status bar, and keep prompts only for failures
    that need a decision.

- **D145 — Virtual text and anchored floating panels**
  - Depends on: none
  - Description: Decoration styles exist, but extensions still cannot draw
    virtual text or inlay hints, or anchor a floating panel to a position. Do
    it before D067 needs hover, inlay hints, or diagnostic text: design them
    against a real language server, as inline virtual text and an anchored
    floating surface that reuses the semantic tree and text content kinds.

- **D115 — Lifecycle hooks and native command overrides**
  - Depends on: none
  - Description: Extensions observe document and window changes after they
    commit. They cannot run before save, wrap or replace a native command, or
    react to a language change in time to affect it. Do it when format on save,
    an LSP client (D067), or a workflow that needs to customize a native
    command is selected, and before format on save in D067: add awaited hooks
    at a few named points (before save first) and a way to wrap a native
    command, with a bounded wait so a slow hook cannot block saving. The save
    pipeline it hooks into exists.

- **D067 — Language servers and semantic tokens**
  - Depends on: D182, D087, D092, D145, D117, D115
  - Description: No language client exists. The syntax highlighting work
    prepared for one: LSP language IDs, `Send` text snapshots, a highlight
    vocabulary that semantic token types and modifiers map onto, and a
    semantic-token paint layer above syntax. Do it when daily editing needs
    diagnostics, navigation, hover, or semantic highlighting: decide whether
    the client is native or an extension (if an extension that spawns servers,
    it needs D087), define a whole-range semantic-token contribution that
    scales to file-sized token counts unlike per-item anchored contributions,
    and connect diagnostics to decorations and completion to D022. It needs
    D092 for server edit lists and D145 for hover, inlay hints, and diagnostic
    text, and format on save builds on D115.

- **D022 — Rich completion policy and presentation**
  - Depends on: D067
  - Description: The prototype validated provider aggregation and replaceable
    native presentation. Do it when completion becomes part of a dogfood slice;
    language-server completion follows D067.

- **D023 — Standardized AI integration**
  - Depends on: D087
  - Description: No concrete user workflow or provider boundary has been
    selected. Do it when an AI-assisted editing slice is defined; D087 gates
    its network access.

- **D125 — Tasks and run configurations**
  - Depends on: D087, D089, D124
  - Description: No investigation yet. VS Code and Zed read `tasks.json`, Emacs
    has `compile` and `project-compile`, and Neovim has `:make` and plugins. Do
    it when running builds, tests, or scripts from a terminal becomes friction
    in daily use: investigate what a task is (a command line, a working
    directory, environment, and output handling), where definitions live
    (project-local settings or a separate file), whether output goes to a
    terminal or a generated text view with clickable locations (D027), and
    whether it ships as a bundled extension over process spawning. It comes
    with or after workspace trust.

- **D027 — Interactive generated text views**
  - Depends on: none
  - Description: Contributed text frames cover read-only generated output, and
    no product path creates a generated document. The extension-facing use
    case is an interactive generated view in the style of magit: a text surface
    whose ranges carry semantic actions (stage, open, expand), that refreshes
    in place with stable positions, and that has no persistence identity. Do it
    when a concrete extension workflow such as Git status or search results
    needs interactive generated text beyond a read-only frame: define how an
    extension creates and refreshes the view, attaches per-range actions, and
    how such views behave in closure, recovery, and restoration. It builds on
    the editor handle's cursor and selection access and the decoration styles.

- **D119 — Extension menu and context-menu items**
  - Depends on: none
  - Description: Extension commands reach the palette and keymaps but not the
    application menus or the context menus of the editor and tabs. Do it when a
    bundled or installed extension needs a command where users look for it by
    pointer: let a registration place a command in a named menu location,
    reusing the tree context menus. If more popups then take gpui focus, give
    "a native popup is open" its own Knot-side focus identity instead of
    extending the key path's popup exception.

- **D140 — Placeholder text in input fields**
  - Depends on: none
  - Description: `TextInput` renders placeholder text, which editor bars use,
    but `workbench.showInput` and the palette query have no way to set one, so
    those inputs can only explain themselves through their title or message
    line. Do it before the next bundled input that would benefit from a hint:
    add a `placeholder` option to `showInput`, and let Tab accept it into an
    empty field, as Zed's Go to Line does. Update `@knot/go-to-line` to show
    the current position as `line:column` there.

- **D180 — Modal surface for informational views**
  - Depends on: none
  - Description: Transient surfaces are only a picker and a text input, so
    `Knot: About` shows its version and commit as picker rows. A modal surface
    needs its own view, focus handling, and key routing, and the abort command
    must learn to dismiss it. Do it when a second informational view is needed
    (a changelog or a "what's new" panel), or the picker About proves
    confusing: add the modal surface, adjust abort, and move About onto it.

- **D019 — Quotas, backpressure, slow-consumer policy, and timeouts**
  - Depends on: none
  - Description: Correct policy depends on observed extension workloads. Do it
    when real extensions can affect responsiveness or resource use.

- **D020 — Public command schemas, aliases, and concurrent task graphs**
  - Depends on: none
  - Description: Ordinary commands and awaited composition cover validated
    cases. Native typed handler declarations are implemented; public schema
    introspection remains open. User macros and repetition moved to D113. Do it
    when a concrete automation workflow cannot be expressed cleanly.

- **D021 — General extension UI or WebView escape hatch**
  - Depends on: D145
  - Description: Surface-specific semantic UI remains sufficient. Do it only
    when a selected extension requires genuinely arbitrary UI, and only after
    D145 has shown its surfaces fall short.

- **D032 — Extension source maps**
  - Depends on: none
  - Description: Built JavaScript loads and reports generated source locations
    without mapping them back to authoring files. Do it when debugging built
    extensions requires errors and stack traces to identify the original source
    locations.

- **D065 — Runtime grammar loading**
  - Depends on: none
  - Description: A fixed tree-sitter grammar set is compiled into the binary;
    languages register by native grammar name, so an installed package cannot
    add a grammar Knot does not ship. Do it when a user needs a language
    outside the bundled set: load WASM grammars on the V8 that already hosts
    extensions, and let a registration ship its grammar. Tree-sitter's own wasm
    store is built on wasmtime, which would add a second engine, and native
    grammar libraries are ruled out, so this needs a Knot-owned store backend
    over V8's WASM API: it provides the grammar's lex functions and external
    scanner, which the native parser calls on every lex step, so start with a
    spike that measures per-token call overhead and isolate ownership on
    background parse threads.

- **D057 — Extension and product localization (i18n)**
  - Depends on: none
  - Description: Contributed view titles, tree labels, status text, native
    prompt text, and the bundled registry's own strings are literal English.
    Extensions and personal config can pass arbitrary strings, but there is no
    locale source, message catalog, or lookup for native-owned product text. Do
    it when shipping a second language becomes a product requirement: define
    locale discovery, a catalog format with fallback, which strings are
    extension-owned versus application-owned, contribution-time versus
    call-time resolution, and how native product text such as "Projects"
    reaches the same mechanism.

### Package manager

- **D101 — Package manager** 🚧
  - Depends on: D087
  - Description: Extension installation becomes an inspectable
    application-owned lifecycle: a lockfile, local archives, installs from git
    links, version-constrained dependencies, a manager surface, and updates
    with pinning and rollback. Git links replace a package registry, so
    publishing is a push and discoverability is a website; consent stays with
    entitlement grants. It promotes D033 and covers D101–D106; the
    [plan](plans/package-manager.md) holds the design and phases. Left for
    later, each when installs outgrow "trust this link": signing and verified
    publishers (D107), a hosted registry with in-editor search (D108), and
    private repositories, SSH or token credentials, and per-project package
    sets, which also need D089 (D116).

### Editing and languages

- **D088 — VS Code, Zed, IntelliJ, and Sublime keymap presets**
  - Depends on: none
  - Description: Map each editor's default bindings onto Knot's commands, per
    platform, once the command set is stable. Commands with no equivalent stay
    unbound. Add a test that every preset binding names a registered command,
    and a Select Keymap Preset picker. Decide whether first launch asks for a
    preset.

- **D132 — Editing data for the remaining bundled languages**
  - Depends on: D129
  - Description: Comment and bracket data exist only for the languages with
    their own packages (Rust, TypeScript and JavaScript, Python, TOML,
    Markdown). The languages in `@knot/language-others` (C, C++, CSS, Go, Java,
    JSON, JSONC, tree-sitter query, shell script, YAML) register without it, so
    editing commands treat them as having no comments or brackets. HTML has its
    block comment, which Markdown's injected HTML blocks need, but no brackets.
    Do it when daily editing in one of those languages misses toggle comment
    or auto-closing, or the languages move into their own packages (D129): add
    the data with per-language behavior tests.

- **D131 — Richer language editing data**
  - Depends on: none
  - Description: Language editing configuration carries comment tokens, one
    block comment style, and bracket pairs. Comment continuation on Enter
    (`///`, `*` inside a block comment), VS Code's `onEnterRules`, per-language
    word characters, and a second block comment style (Pascal's `{ }` and
    `(* *)`) are not modelled. Do it when writing doc comments, word selection
    in a language with unusual identifiers (CSS `-`, Lisp), or a bundled
    language with two block comment styles needs them: extend the same
    registration rather than adding a parallel one.

- **D130 — Syntax-scoped language configuration**
  - Depends on: none
  - Description: Editing configuration applies to a whole language, or to an
    injected language's layer. Some constructs need other values inside one
    language: JSX children comment with `{/* */}` rather than `//`, and
    auto-closing may differ inside template strings or attributes. Do it when
    toggle comment or auto-closing inside JSX, or another language's
    sub-context, gets in the way in daily use: add a per-language query that
    names syntax scopes and per-scope overrides of the editing configuration,
    as Zed's `overrides.scm` does.

- **D136 — Auto-indent setting**
  - Depends on: none
  - Description: Automatic indentation on Enter and while typing is always on
    for languages with an indent query. Do it when a language's query
    misbehaves in daily use, or someone wants plain keep-indentation Enter: add
    a language-scoped `editor.autoIndent` setting, as VS Code's
    `editor.autoIndent` does, choosing between keeping the indentation,
    brackets only, and the full query, and decide whether it also governs D135.

- **D135 — Reindent pasted text**
  - Depends on: D136
  - Description: Lines are indented from the syntax tree on Enter, while typing
    a closer, and through Reindent Selected Lines and Reindent Buffer. Pasted
    lines keep the indentation they had where they were copied from. Do it when
    pasting code between different nesting depths needs a Reindent Selected
    Lines afterwards often enough to be friction: decide, as Zed's auto-indent
    on paste and VS Code's `editor.autoIndentOnPaste` do, whether to reindent
    every pasted line from the query or shift the block by the first line's
    change and keep its relative indentation, and how pasting inside strings
    and comments behaves. D136 may turn it off.

- **D157 — Indent width separate from tab width**
  - Depends on: none
  - Description: Low priority. `editor.tabSize` sets both how wide a tab
    displays and how far indentation steps, so Vim's `:set tabstop` and
    `:set shiftwidth` write the same setting. Only mixed styles lose, such as
    GNU-style C that indents by 4 and shows tabs as 8, turning 8 columns of
    indentation into one tab. Do it when real files in such a style need
    editing: add an indent width setting defaulting to the tab size, and carry
    it through indent and outdent, reindent and indent queries, Enter's
    indentation, indentation detection, the conversion commands, the status
    bar's picker, and `DocumentInfo.indentation`.

- **D159 — Trim trailing whitespace on modified lines only**
  - Depends on: D126
  - Description: Save trims every line. Trimming only the lines a change
    touched keeps saves from adding noise to the diff of a file that already
    had trailing whitespace, and needs a notion of modified lines that Knot
    lacks. Do it when a VCS or diff layer exists, such as a change gutter or
    diff view: add `"modified"` to the values of `files.trimTrailingWhitespace`
    (`"off"`, `"all"`) and trim only the lines that differ from the baseline.

- **D149 — Line ending facts and conversion**
  - Depends on: none
  - Description: Newlines take the buffer's first line terminator (see
    [core and buffers](architecture/core-and-buffers.md)), derived from the
    text, and nothing reports or changes it. The status bar ships without a
    line-ending item. Do it when a CRLF/LF workflow needs to see or convert a
    file's endings, or mixed endings cause trouble: decide whether the ending
    stays derived or becomes stored document state that can report mixed, add
    it to `DocumentInfo` as `documentChanged`, convert every terminator in one
    undo step, and add a status bar item (click: LF or CRLF). Keep it
    consistent with the final-newline insertion on save, which uses
    `line_ending()`.

- **D001 — Replace and regular-expression Find**
  - Depends on: D092
  - Description: Current-file Find matches literal text, with case and
    whole-word toggles. Replace and regular expressions are missing. Do it when
    daily use identifies required replacement or regular-expression behavior:
    add them to the `@knot/find` bar and `buffer.find`. Replace-all is a bulk
    edit, so it comes with or after D092.

- **D142 — Multiple and rectangular selections**
  - Depends on: D092
  - Description: The editor handle and views hold one selection. Vim's
    visual-block mode, multi-cursor editing, and rectangular selection need
    several, and each edit across them is a bulk edit. Do it when visual-block
    mode or multi-cursor editing is selected: let views and the editor handle
    hold an ordered selection set, with rectangular selection as one way to
    create it, and apply edits across the set as one batch. Decide then whether
    `ctrl-v` in Vim's normal mode stays native paste or starts visual-block
    mode.

- **D144 — Syntax-tree queries and text objects**
  - Depends on: none
  - Description: The `folds` queries serve folding only. Extensions cannot
    query the syntax tree, and there are no `textobjects` queries, so Vim's
    `if`, `ac`, and `ia`, structural selection expansion, and tree-based
    motions cannot be built. Do it when daily Vim use misses tree-sitter text
    objects, or structural selection is selected: add per-language
    `textobjects` queries beside `folds` and `indents`, and expose node queries
    through the extension API without exposing tree-sitter objects.

- **D113 — Command and input recording**
  - Depends on: none
  - Description: Nothing can observe the stream of invoked commands together
    with the text typed between them, so neither repeat nor macros can be
    built. It absorbs user macros and repetition from D020. The first Vim mode
    ships without `.`, `q`, the `".` register (the last inserted text), and
    counts on insert commands such as `3ihi<Esc>`, which all need the text
    typed in insert mode. Do it when daily Vim use misses them, or a user macro
    workflow is selected: record a view's invoked commands with arguments plus
    its typed text as edits, and replay the recording through ordinary dispatch.

- **D147 — Key contexts on non-editor views**
  - Depends on: none
  - Description: Extensions set key contexts on editor views only. Terminals,
    the explorer, and contributed tree and text views resolve bindings by view
    kind alone. Do it when a consumer needs a modal tree, terminal, or
    contributed view, such as Vim keys in the explorer: extend the same
    lifecycle-owned contexts and fact-count resolution to that view.

- **D148 — Richer key context matching**
  - Depends on: none
  - Description: A binding names at most one context, with no negation or
    `when`-style expressions. Do it when one context per binding forces an
    extension to duplicate many bindings or keep redundant contexts in sync:
    allow several contexts, or negated ones, per binding, still resolved by
    counting matching facts rather than by an expression language.

- **D059 — Professional typography: ligatures and configurable font features**
  - Depends on: none
  - Description: Surfaces use shared fixed typography defaults. Ligatures and
    font features need shaping options per surface, and the terminal assumes
    fallback glyphs stay within emulator cell allocations, so enablement rules
    differ between the editor and the PTY grid. Do it when daily readability or
    a font choice requires ligatures, stylistic sets, variable-font settings,
    or configurable terminal typography, together with D045: define font
    selection, fallback, and feature toggles as `font.*` settings scoped per
    surface, such as `{ view: "terminal" }`, and terminal cell metrics for
    fonts whose shaped advance differs from the default.

### Vim mode

The bundled Vim mode is built on generic editor primitives, so a Helix or
Kakoune mode could use them too. The
[extensibility report](reports/extensibility.md) maps Vim's needs onto them.

- **D146 — Broad Ex command set**
  - Depends on: D113
  - Description: The first Vim mode implements a core Ex set: `:w`, `:q`,
    `:q!`, `:wq`, `:x`, `:e`, `:<line>`, and `:set` for a few options.
    Substitution and `:noh` are D150. `:g` and `:v`, `:normal`, `:sort`, `:d`,
    `:m`, `:t` and other range commands, `:reg` and `:marks`, `:map` and its
    variants, and user-defined Ex commands are missing. So are `:w <file>` and
    `:sav <file>`, which need a public save to a given URI without the Save
    dialog, and `:e!`, which needs a revert of a document to its file's text;
    neither primitive exists. Do it when daily Vim use reaches for one of them:
    add it inside the Vim package over existing primitives, or add the missing
    generic primitive first. `:normal` and `:g` with a normal-mode command also
    need replaying keys through the mode, which D113's recording provides, and
    `:g` and `:v` come with D150's pattern matching.

- **D150 — Vim search and substitution**
  - Depends on: D092
  - Description: The first Vim mode ships without search, so that Vim search
    reuses current-file Find's match layer instead of a second engine, and
    without substitution, a whole-buffer edit. Missing: `/`, `?`, `n`, `N`, `*`,
    `#` with match highlights, `:noh`, and `:s` and `:%s` with ranges and flags.
    Do it when daily Vim use misses search: build search inside the Vim package
    on Find's matching and highlights, decide how Vim's regex syntax maps onto
    the engine Find uses, and add substitution as one batch in one undo step.
    Substitution is a bulk edit, so it comes after D092.

- **D151 — Smaller Vim commands**
  - Depends on: D144, D061
  - Description: The first Vim mode covers the commands of its scope. Commands
    outside it that daily editing may reach for are missing: `r<cr>` replacing
    characters with a line break; `J` leaving out the space before a `)`; `gJ`
    joining without spaces; a repeated `i(` or `a(` in visual mode growing the
    selection to the enclosing pair; sentence motions and objects (`( )`, `is`,
    `as`); tag objects (`it`, `at`); `gp`, `gP`, and `]p`; `_` and `|`; `gi`;
    and `ga`; and the `ctrl-w` keys that need workbench commands: `n` an empty
    pane (with the empty-pane handling of D061), counts on `ctrl-w h j k l`
    (they need native counts, as every child command keeps the pane its parent
    started in), and `x` `r` `R` swapping and rotating panes. Do it when daily
    Vim use reaches for one of them: add it inside the Vim package over
    existing primitives, with cases beside the others. Tag objects wait for
    D144's syntax-tree queries.

- **D152 — Persistent Vim global marks**
  - Depends on: none
  - Description: The Vim mode's global marks `A`–`Z` hold a document and a
    native mark, so they last while that document is open and end when it
    closes. Vim's viminfo and Neovim's shada keep them across restarts and for
    files no longer open, with the registers. Do it when daily Vim use reaches
    for a global mark set in an earlier project session or in a closed file:
    keep each mark's URI and offset in the package's storage beside the
    registers, recreate the native mark when its document opens, open the file
    for a mark whose document is closed, and clamp an offset past a changed
    file's end, as view state does.

### Workbench and windows

- **D037 — Whole-pane dragging and arbitrary split-tree reshaping**
  - Depends on: none
  - Description: Tabs move individually into existing panes or new edge splits;
    moving groups of tabs and restructuring whole split branches is a separate
    interaction and layout problem. Do it when daily use shows that moving tabs
    one at a time or creating edge splits is too cumbersome for a common layout
    change.

- **D170 — Overlay mode for docked tools**
  - Depends on: none
  - Description: A tool is always docked, resizing the center. IntelliJ's
    "undock" mode slides a tool over the center without resizing it, which
    suits a wide tool opened briefly. Do it when daily use shows tools opened
    briefly and dismissed, where the center reflowing is disruptive: add the
    mode per tool as a placement, with focus loss dismissing it.

- **D062 — Positioning existing windows**
  - Depends on: none
  - Description: gpui sets a window's bounds only at creation and can
    afterwards only resize it, including on its separate platform crates.
    Session restoration and project switching always create their windows, so
    they apply saved geometry at creation and no product path needs to move an
    existing window. Do it when a workflow needs to move or re-frame an open
    window, such as window arrangement commands, an extension API, or reusing
    windows during restoration: add a small native per-platform window-frame
    operation (`NSWindow` frame on macOS, `SetWindowPos` on Windows, X11
    configure requests) beside gpui rather than vendoring it. Wayland
    compositors own placement, so positioning there stays best-effort.

### Terminal

- **D041 — Additional terminal protocols and product events**
  - Depends on: D063
  - Description: Legacy keyboard/mouse protocols and HTTP(S) links are covered;
    Kitty/modifyOtherKeys, graphics, OSC 52 clipboard access, titles, and bells
    have no selected product contract. Do it when a daily shell/TUI workflow
    demonstrates a specific missing protocol or event, with or after D063 if
    libghostty covers the protocol: define its compatibility and product
    behavior together.

- **D128 — Terminal working directory from libghostty**
  - Depends on: D063
  - Description: A terminal's working directory is read by querying the shell
    process (`proc_pidinfo` on macOS, `/proc/<pid>/cwd` on Linux), because
    `alacritty_terminal` does not parse OSC 7. Windows records only the start
    directory. Do it when D063 moves the terminal engine to libghostty: read the
    directory from the emulator's OSC 7 state instead of the process, which may
    also cover Windows and remote shells.

- **D045 — General terminal settings**
  - Depends on: D059
  - Description: Shared typography and the session-local mouse toggle are the
    only terminal preferences; no terminal keys are defined in the settings
    store. Do it with D059 when daily use needs configurable fonts or terminal
    preferences: define them as settings, with fonts as `font.*` keys scoped
    `{ view: "terminal" }` so the editor keeps its own values.

- **D043 — Persistent terminal sessions**
  - Depends on: none
  - Description: Live local sessions survive view reconstruction, but not
    application exit. Restored terminal tabs start fresh shells in their last
    working directory. Do it when those tabs need their processes or scrollback
    to continue; persistent processes and grid restoration need a separate
    lifecycle contract.

- **D044 — Simultaneous terminal views**
  - Depends on: none
  - Description: A sole attachment keeps input ownership and PTY resize
    authority unambiguous while supporting cross-window transfer. Do it when
    daily use requires viewing one live session in multiple panes: define
    independent viewport and shared resize/input policies.

- **D155 — Word and line selection in terminals**
  - Depends on: none
  - Description: A terminal's local selection is always character by
    character: double- and triple-click start a plain selection, unlike editors
    and most terminal emulators. Do it when selecting a path, URL, or line from
    terminal output by dragging gets in the way: start the emulator's semantic
    selection on a double click and its line selection on a triple click while
    mouse reporting is off, so a drag afterwards extends by words or lines, with
    the emulator's default word separators.

- **D042 — Additional terminal link destinations**
  - Depends on: none
  - Description: Only explicit and detected HTTP(S) destinations are supported.
    Arbitrary URI handlers and filesystem-path links require an opening policy.
    Do it when a concrete workflow needs file navigation or another URI scheme.

- **D039 — Shell integration**
  - Depends on: none
  - Description: Ordinary terminal input and output cover current shell
    workflows. Do it when daily use requires working-directory tracking,
    command boundaries, or exit status: select supported shells and integration
    setup.

- **D038 — Remote PTYs**
  - Depends on: none
  - Description: Local interactive sessions cover current terminal workflows.
    Do it when daily use requires remote shells: select transport,
    authentication, and disconnect behavior for a concrete workflow.

- **D040 — Physical application-keypad input**
  - Depends on: none
  - Description: gpui merges numeric-keypad keys with ordinary key identities,
    so Knot retains numeric input but cannot emit distinct application-keypad
    sequences. The choice was deferral over vendoring gpui for a platform patch.
    Do it when gpui exposes keypad identity, or a concrete TUI workflow
    requires a framework patch and its platform maintenance cost.

### Platforms, accessibility, and website

- **D030 — Accessibility bridge**
  - Depends on: none
  - Description: gpui has a public AccessKit bridge (`Element::a11y_role`,
    `write_a11y_info`), and gpui-component's widgets publish roles through it,
    but the editor, trees, terminal, palette, and pickers publish nothing,
    though their semantic state is already retained. Do it when accessibility
    becomes a product priority, or a user needs a screen reader: give each
    native surface its roles and the editor its text and caret, starting from
    the semantic tree state.

- **D035 — macOS distribution**
  - Depends on: none
  - Description: Local `Knot.app` builds and the unsigned nightly cask cover
    dogfooding without release credentials. Do it when Knot is ready to run on
    other Macs: choose a release artifact, then add Developer ID signing,
    hardened runtime validation for V8, notarization, and an install/update
    path.

- **D168 — Website screenshots and documentation** 🚧
  - Depends on: none
  - Description: The landing page has no screenshot, and the user-facing docs
    (extension guide, settings reference) are readable only on GitHub. The
    [website plan](plans/website.md) covers screenshots on the landing page and
    a generated documentation section covering getting started, configuration,
    the extension guide, and a feature tour.

- **D167 — Website blog and newsletter**
  - Depends on: D168, D035
  - Description: The website plan covers screenshots and documentation only. A
    blog and a newsletter need a publishing workflow, a feed, and a subscriber
    service, and have nothing to announce before the first release. Do it when
    Knot approaches a first release (D035): choose a static blog under
    `website/` with an RSS feed, and a newsletter provider that keeps
    subscriber data out of the repository.
