# Product Roadmap and Deferred Work

The selected work below has an explicit implementation order. The deferred
register that follows is grouped by area, not by priority, but its items are
not independent: [dependency order](#dependency-order) lists the chains, and
each item's **Order** line names what must come before and after it.
Completed decisions remain in [decisions](decisions.md); proposed architecture and
checkpoint details live in active plans.

Each item has a stable `D###` identifier, a reason for deferral, a concrete
trigger for reconsideration, and, when it has one, its order relative to other
items. "After" means the earlier item must be complete or designed together
with this one; "before" is the same constraint seen from the other side. IDs
are never renumbered or reused. New items take the next unused ID. When an
item is promoted, its slice plan retains the ID and the item is removed from
this register. Items of the active [daily-driver](plans/daily-driver.md) and
[package manager](plans/package-manager.md) plans are referenced here by ID
and ordered in their plans.

## Selected work

The production extension APIs and JavaScript projects initiative (D046–D053,
D005) is complete: an ordinary bundled JavaScript extension manages named
multi-root projects, switches them with unsaved text and layouts, and rebinds
to the windows Knot restores at startup. Current behavior lives in the
[architecture](architecture.md) references and rationale in
[decisions](decisions.md).

Extension authoring (D056) is complete: bundled packages and examples are
TypeScript compiled at build time, split into modules, and tested under
`node:test` against an in-memory host.

The editor view data model is complete: views read lines from the model
instead of a private text copy, typing coalesces in the piece table, the
consumed edit log is discarded, and inserted newlines follow the buffer's line
ending. D002 and D031 can build on the view's line model.

The codebase health review is complete. The V8 engine and the editor view are
split by concern, terminal, extension, and command modules live in their own
directories, large test modules sit beside their code, and local filesystem I/O
no longer needs a tokio runtime.

Syntax highlighting (D002) is complete: native incremental tree-sitter
parsing with injections, languages registered through the bundled
language packages, explicit languages that survive restarts and project
sessions, and a layered paint pipeline shaped for later language servers. Its
follow-ups are D064–D067.

The [daily-driver editor](plans/daily-driver.md) initiative is selected: it
removes demo surfaces and adds workbench and editing commands, a richer
palette, themes, Find, navigation, file watching, and bundled explorer,
project-selector, and search extensions, keymap presets, and extension
entitlements. It promotes D007, D008, D010, D031, D034, D053, D060, D064, D068,
and D089 and adds D069–D088, D120–D123, D127, D134, and D143.

The editor API and Vim mode initiative is complete: key contexts, long-lived
editor handles with selection observation, the status bar, undo groups, view
controls, decoration styles, visual-line movement, and folding, proven by a
bundled JavaScript Vim mode. Vim features left for later are D113, D142, D144,
D146, D150, D151, D152, and D158. Language servers
(D067) follow it.

The [package manager](plans/package-manager.md) initiative is selected: extension
installation becomes an inspectable application-owned lifecycle with a lockfile,
a local archive format, installs from git repository links, version-constrained
dependency resolution, a manager surface, and updates with pinning and rollback.
Git links replace a package registry, so publishing is a push and discoverability
is a website; consent stays with entitlement grants. It promotes D033 and adds
D101–D106.

Extension isolate memory (D137) is complete: Developer: Show Extension Memory
reports each package's heap, and isolates release their garbage after startup
and long turns. Lowering the fixed per-package cost is D139.

## Dependency order

Each line is a chain: an item to the left of an arrow comes first. Items not
listed have no ordering constraint; each item's **Order** line is the
complete list for that item.

- **Editing extensibility** (see the
  [extensibility report](reports/extensibility.md); the
  completed editor API initiative built D058, D066, D077, D109–D112, D114,
  and D141):
  - D110 → D113, D147, D148; D109 → D113, D027, D142
  - D134 → D135, D136
  - D077 → D149
  - D073, D074, D075 → D130; D074 → D131; D073 → D132
  - D066 → D144 → D151
  - D058 → D150, D146, D151, D152; D031, D092 → D150
  - D092 → D142, D001, D067
- **Extension presentation and language tooling:**
  - D114 → D145 → D067 → D022
  - D064, D160 → D055
  - D160 → D162
  - D087 → D067, D023
  - D079 → D115 → format on save with D067
  - D145 before D021, which is reconsidered only if D145 falls short.
  - D114 → D027
  - D077 → D117 → D067
  - D071 → D118 → D067, D082
  - D053 → D119
- **Find:** D031 → D001, with or after D092; D031 → D150.
- **Package manager:** D087 → D101 → D102 → D103 → D104 → D106;
  D064, D071, D103 → D105 → D106; D106 → D107, D108; D103 → D116.
- **Settings:** D070 → D089 → D090; D008, D122 → D090; D122 → D089;
  D070 → D091, D045, D059; D089 → D124.
- **Projects and workspaces:** D070, D084 → D120; D122 → D123; D087 → D125,
  D126; D089 → D125.
- **Start surface:** D083, D084 → D061.
- **Launcher:** D164 → D165; D120 → D165, D166.
- **Website:** D168 → D167.
- **Terminal:** D063 with or before D041; D121 → D127 → D043; D127, D063 →
  D128.

## Deferred register

## Editing and workbench

- **D164 — Command-line launcher**
  - Deferred: Knot starts only from the app bundle or `cargo run`; there is
    no `knot <file>...` helper on the `PATH` like VS Code's `code`.
  - Reconsider: Daily use from a terminal. Then add a small `knot` helper
    that passes the paths to the running instance, or starts one, and an
    install command for the `PATH` entry.
  - Order: before D165.

- **D165 — Launcher files open in a new window unless in an open project**
  - Deferred: Files given on the command line (D164) need a placement rule.
  - Reconsider: With D164. Open them in a new window, except that a file
    inside the roots of an already-open project opens in that project's
    window.
  - Order: after D164; uses the projects of D120.

- **D001 — Replace and advanced Find modes**
  - Deferred: These modes depend on current-file Find (D031, in the
    [daily-driver plan](plans/daily-driver.md)).
  - Reconsider: Daily use identifies required replacement, regular-expression,
    case, or whole-word behavior.
  - Order: after D031; replace-all is a bulk edit, so with or after D092.

- **D157 — Indent width separate from tab width**
  - Priority: low.
  - Deferred: `editor.tabSize` sets both how wide a tab displays and how far
    indentation steps, so Vim's `:set tabstop` and `:set shiftwidth` (D058)
    write the same setting. Only mixed styles lose, such as GNU-style C that
    indents by 4 and shows tabs as 8, turning 8 columns of indentation into
    one tab.
  - Reconsider: Real files in such a style need editing. Then add an indent
    width setting defaulting to the tab size, and carry it through indent and
    outdent, reindent and indent queries, Enter's indentation, indentation
    detection, the conversion commands, the status bar's picker, and
    `DocumentInfo.indentation`.

- **D159 — Trim trailing whitespace on modified lines only**
  - Deferred: D079 trims every line on save. Trimming only the lines a change
    touched keeps saves from adding noise to the diff of a file that already
    had trailing whitespace, and needs a notion of modified lines that Knot
    lacks.
  - Reconsider: A VCS or diff layer exists, such as a change gutter or
    diff view. Then add `"modified"` to the values of
    `files.trimTrailingWhitespace` (`"off"`, `"all"`) and trim only the lines
    that differ from the baseline.
  - Order: after D079 and the diff layer.

- **D149 — Line ending facts and conversion**
  - Deferred: Newlines take the buffer's first line terminator (see
    [core and buffers](architecture/core-and-buffers.md)), derived from the
    text, and nothing reports or changes it. The status bar (D077) ships
    without a line-ending item.
  - Reconsider: A CRLF/LF workflow needs to see or convert a file's endings, or
    mixed endings cause trouble. Then decide whether the ending stays derived
    or becomes stored document state that can report mixed, add it to
    `DocumentInfo` as `documentChanged`, convert every terminator in one undo
    step, and add a status bar item (click: LF or CRLF). Keep it consistent
    with D079's final-newline insertion, which uses `line_ending()`.
  - Order: after D077.

- **D006 — File drag and drop**
  - Deferred: Open and Save As establish workspace-independent documents first.
  - Reconsider: Platform interaction becomes a selected product slice.

- **D132 — Editing data for the remaining bundled languages**
  - Deferred: D073 supplies comment and bracket data only for
    the languages with their own packages (Rust, TypeScript and JavaScript,
    Python, TOML, Markdown). The languages in `@knot/language-others` (C,
    C++, CSS, Go, Java, JSON, JSONC, tree-sitter query, shell script, YAML)
    register without it, so editing commands treat them as having no
    comments or brackets. HTML has its block comment, which Markdown's
    injected HTML blocks need, but no brackets.
  - Reconsider: Daily editing in one of those languages misses toggle
    comment or auto-closing, or the languages move into their own packages
    (D129). Then add the data with per-language behavior tests like
    D073's.
  - Order: after D073; with or after D129.

- **D135 — Reindent pasted text**
  - Deferred: D134 indents lines from the syntax tree on Enter, while
    typing a closer, and through Reindent Selected Lines and Reindent Buffer.
    Pasted lines keep the indentation they had where they were copied from.
  - Reconsider: Pasting code between different nesting depths needs a
    Reindent Selected Lines afterwards often enough to be friction. Then decide, as
    Zed's auto-indent on paste and VS Code's `editor.autoIndentOnPaste` do,
    whether to reindent every pasted line from the query or shift the block
    by the first line's change and keep its relative indentation, and how
    pasting inside strings and comments behaves.
  - Order: after D134; with D136, which may turn it off.

- **D136 — Auto-indent setting**
  - Deferred: D134's automatic indentation on Enter and while typing is
    always on for languages with an indent query.
  - Reconsider: A language's query misbehaves in daily use, or someone
    wants plain keep-indentation Enter. Then add a language-scoped
    `editor.autoIndent` setting (D070), as VS Code's `editor.autoIndent`
    does, choosing between keeping the indentation, brackets only, and the
    full query, and decide whether it also governs D135.
  - Order: after D134.

- **D130 — Syntax-scoped language configuration**
  - Deferred: D073's editing configuration applies to a whole language, or
    to an injected language's layer. Some constructs need other values
    inside one language: JSX children comment with `{/* */}` rather than
    `//`, and auto-closing may differ inside template strings or attributes.
  - Reconsider: Toggle comment or auto-closing inside JSX, or another
    language's sub-context, gets in the way in daily use. Then add a
    per-language query that names syntax scopes and per-scope overrides of
    the editing configuration, as Zed's `overrides.scm` does.
  - Order: after D073, D074, and D075.

- **D131 — Richer language editing data**
  - Deferred: D073 carries comment tokens, one block comment style, and
    bracket pairs. Comment continuation on Enter (`///`,
    `*` inside a block comment), VS Code's `onEnterRules`, per-language word
    characters, and a second block comment style (Pascal's `{ }` and
    `(* *)`) are not modelled.
  - Reconsider: Writing doc comments, word selection in a language with
    unusual identifiers (CSS `-`, Lisp), or a bundled language with two
    block comment styles needs them. Then extend the same registration
    rather than adding a parallel one.
  - Order: after D074; word characters before D080 if it needs them.

- **D059 — Professional typography: ligatures and configurable font features**
  - Deferred: Surfaces use shared fixed typography defaults. Ligatures and
    font features need shaping options per surface, and the terminal assumes
    fallback glyphs stay within emulator cell allocations, so enablement rules
    differ between the editor and the PTY grid.
  - Reconsider: Daily readability or a font choice requires ligatures,
    stylistic sets, variable-font settings, or configurable terminal
    typography. Then define font selection, fallback, and feature toggles as
    `font.*` settings (D070) scoped per surface, such as `{ view: "terminal" }`
    (with D045), and terminal cell metrics for fonts whose shaped advance
    differs from the default.
  - Order: after D070; with D045.

- **D061 — Empty normal-group start panel**
  - Deferred: A workbench whose normal document group would be empty receives
    an untitled document, so docks never fill the window, including after a
    project switch to a session with no documents. A dedicated empty panel needs
    its own surface, focus, and command-routing rules.
  - Reconsider: The placeholder untitled document gets in the way in daily use,
    or a start surface (recent projects and files, common commands) is
    selected. Then replace the placeholder with a native empty-group panel that
    keeps the normal group's position and accepts opened documents.
  - Order: after D083 and D084, whose recent files and projects a start
    surface lists.

- **D037 — Whole-pane dragging and arbitrary split-tree reshaping**
  - Deferred: D004 moves individual tabs into existing panes or new edge splits;
    moving groups of tabs and restructuring whole split branches is a separate
    interaction and layout problem.
  - Reconsider: Daily use shows that moving tabs one at a time or creating edge
    splits is too cumbersome for a common layout change.

## Documents, storage, and history

- **D009 — Binary buffers and encoding detection**
  - Deferred: The first slice explicitly targets UTF-8 text.
  - Reconsider: A concrete file workflow requires non-UTF-8 or binary
    inspection.

- **D011 — Remote filesystem providers**
  - Deferred: The URI boundary is validated with local and memory providers.
  - Reconsider: A specific remote workflow is selected.

- **D012 — Persistent undo and history trees**
  - Deferred: Session-local linear undo is sufficient for dogfooding.
  - Reconsider: Users need edit history across restarts or branching history.
    Byte-offset view state already survives restarts and project switches.
  - Order: after D111, which defines the undo steps a history records.

- **D092 — Piece tree text buffer**
  - Deferred: `TextBuffer` keeps its pieces in a flat `Vec` searched linearly,
    shifts a flat line-start index on every edit, and copies the piece list
    for each snapshot. Coalesced typing keeps the piece count low, so
    measured typing latency is acceptable. Absorbs the former piece-table
    compaction (D013) and line-start index (D014) items.
  - Reconsider: Before or together with the first bulk-edit feature, such as
    Replace (D001), multi-cursor editing, formatting, or language-server edit
    lists (D067), or when long sessions show measured latency, memory, or
    stabilization problems. Then follow the
    [text buffer analysis](reports/text-buffer-structure.md): benchmark the
    current buffer, store the pieces in a copy-on-write B-tree that summarizes
    bytes, line breaks, and UTF-16 length, resolve positions through ordered
    piece locators, and keep the piece semantics, edit log, and `TextBuffer`
    API unchanged.
  - Order: before or with D001, D142, and D067.

- **D015 — URI deduplication across processes**
  - Deferred: The first slice handles identity within one application process.
  - Reconsider: Multi-process editing or document handoff is introduced.

- **D090 — EditorConfig support**
  - Deferred: `.editorconfig` files need file watching (D008), reading
    arbitrary files through the public API (D122), and the `path` scope fact
    (D089).
  - Reconsider: D008 and D089 are complete, or daily use hits a project that
    relies on `.editorconfig`. Then a bundled package finds the files above
    each opened document, turns each section into a `path`-glob scope, and
    writes them from the root down to the nearest file, so last-write-wins
    reproduces EditorConfig's rule that the nearest file takes precedence.
  - Order: after D008, D089, and D122.

- **D091 — Settings UI**
  - Deferred: Settings (D070) are set from code. Definitions carry a type,
    default, and description so a UI can be built later, but none exists.
  - Reconsider: Discovering or changing settings from code becomes the
    friction in daily use. Then build the UI as an ordinary extension over
    `settings.define`, `get`, `set`, and `observe`. It keeps its choices in
    extension storage and applies them again in `start()`, which beats
    `pre-init` and loses to `post-init`, so it must show when a later write
    overrides a choice. Also decide whether it lists undefined keys that hold
    values, to help spot typos.
  - Order: after D070.

- **D124 — Workspace trust**
  - Deferred: Project-local settings (D089) start as data that runs no code,
    and nothing else executes on behalf of a repository.
  - Reconsider: Project settings, tasks (D125), or language servers would run
    code or commands from an opened repository. Then decide, as VS Code's
    workspace trust and Neovim's `exrc` trust database do, how a folder becomes
    trusted, what an untrusted folder disables, and where grants are stored,
    reusing D087's grant storage if it fits.
  - Order: after D089; before D125 runs repository-defined commands.

- **D016 — Sandboxed workspace roots**
  - Deferred: Local roots are trusted and intentionally follow symlinks.
  - Reconsider: Knot opens untrusted workspaces under a security boundary.

## Extensions and automation

- **D166 — Guided personal configuration**
  - Deferred: A new user gets no hint that Knot is customized through
    personal config and extensions.
  - Reconsider: Before Knot is offered to other users. Then, on first launch,
    create an `AGENTS.md` in the user config folder that tells coding agents
    how to customize Knot, and present that folder as a special built-in
    project so it is one click away.
  - Order: after D120.

- **D129 — Refactor builtin language extensions**
  - Deferred: Each bundled language package is named `@knot/language-<name>`
    because a package ID is exactly `@scope/name`. The grammars without a
    package of their own still share `@knot/language-others`.
  - Reconsider: Before the next bundled language package, or when the
    remaining grammars in `@knot/language-others` get packages of their own.
    Then decide whether package IDs gain a nested form such as
    `@knot/language/rust`, which touches identity validation, discovery,
    storage paths, and the build, and split the remaining grammars.

- **D065 — Runtime grammar loading**
  - Deferred: D002 compiles a fixed tree-sitter grammar set into the binary;
    languages register by native grammar name, so an installed package cannot
    add a grammar Knot does not ship.
  - Reconsider: A user needs a language outside the bundled set. Then load
    WASM grammars on the V8 that already hosts extensions, and let a
    registration ship its grammar. Tree-sitter's own wasm store is built on
    wasmtime, which would add a second engine, and native grammar libraries
    are ruled out, so this needs a Knot-owned store backend over V8's WASM
    API: it provides the grammar's lex functions and external scanner, which
    the native parser calls on every lex step, so start with a spike that
    measures per-token call overhead and isolate ownership on background
    parse threads.

- **D067 — Language servers and semantic tokens**
  - Deferred: No language client exists. D002 prepares for one: LSP language
    IDs, `Send` text snapshots, a highlight vocabulary that semantic token
    types and modifiers map onto, and a semantic-token paint layer above
    syntax.
  - Reconsider: Daily editing needs diagnostics, navigation, hover, or
    semantic highlighting. Then decide whether the client is native or an
    extension, define a whole-range semantic-token contribution that scales
    to file-sized token counts unlike per-item anchored contributions, and
    connect diagnostics to decorations and completion to D022.
  - Order: after D087 if the client is an extension that spawns servers,
    D092 for server edit lists, and D145 for hover, inlay hints, and
    diagnostic text; before D022's language-server completion. Format on
    save builds on D115.

- **D133 — TypeScript case modules for native behavior tests**
  - Deferred: Language packages keep the cases that native code runs
    against their registered data, such as comment toggling, as JSONC files
    beside the package, which a generic Rust product test reads. TypeScript
    modules would type-check the cases and share them in code, but the Rust
    runner cannot read TypeScript without spawning Node at test time,
    evaluating them in `build.rs`, or a test-only V8 loading path.
  - Reconsider: More native features need per-package cases (auto-closing,
    indentation, text objects) and untyped JSONC causes mistakes or repeated
    cases. Then choose one of those loading paths and replace every JSONC
    case file with TypeScript, so packages keep one format.

- **D138 — Extension startup report as an untitled document**
  - Deferred: Developer: Show Extension Startup Report toggles a custom
    overlay that the product shell renders, and launch opens it when a
    package failed. Developer: Show Extension Memory already presents its
    report as a clean untitled document, which can be selected, copied,
    searched, and compared across runs without a bespoke surface.
  - Reconsider: The overlay needs a change, or reading or copying a failure
    from it gets in the way. Then render the report as text into a clean
    untitled document as Show Extension Memory does, open it from launch
    when a package failed, and remove the overlay, its toggle, and its
    rendering.

- **D139 — Per-package isolate floor**
  - Deferred: Each package keeps its own isolate, which costs about 1.2 MiB
    resident while idle; a bare isolate with an empty context is about
    1.05 MiB of that, and the bootstrap a startup snapshot could skip is
    about 0.15 MiB. At 40 packages that is about 48 MiB. With today's
    handful of packages it is not yet worth the work.
  - Reconsider: Knot ships enough extensions that their idle cost shows in
    daily use, with 40 or more as the target. Then follow its
    [plan](plans/d139-isolate-floor.md): measure the real set again, try V8
    flags for small isolates, then a startup snapshot, and design sharing or
    unloading only if those fall short.

- **D156 — Fewer host round trips per key**
  - Deferred: Found by the headless package tests. A key bound to an extension command costs
    several foreground requests, each a turn on an extension worker and a
    response back: a Vim motion reads the selection, a line, and the tab
    size, writes the caret, and looks up the focused editor's window, and
    the selection notification and the status bar package read again.
    Headless Vim cases take about 4.7 ms each on Apple M1 Pro/macOS debug
    builds, about 40% of it waiting on those turns, and half of each turn
    is the debug-built host glue.
  - Reconsider: Package cases grow slow enough to matter again, or typing
    latency in a modal package is noticeable in the release app. Then give
    the command context the editor so the window lookup goes, let an editor
    handle read what a motion needs in one request, and measure whether
    building the host glue optimized in debug, as a crate of its own, is
    worth it.

- **D032 — Extension source maps**
  - Deferred: D018 can load built JavaScript and report generated source
    locations without mapping them back to authoring files.
  - Reconsider: Debugging built extensions requires errors and stack traces to
    identify the original source locations.

- **D019 — Quotas, backpressure, slow-consumer policy, and timeouts**
  - Deferred: Correct policy depends on observed extension workloads.
  - Reconsider: Real extensions can affect responsiveness or resource use.

- **D020 — Public command schemas, aliases, and concurrent task graphs**
  - Deferred: Ordinary commands and awaited composition cover validated cases.
    Native typed handler declarations are implemented; public schema
    introspection remains deferred. User macros and repetition moved to D113.
  - Reconsider: A concrete automation workflow cannot be expressed cleanly.

- **D021 — General extension UI or WebView escape hatch**
  - Deferred: Surface-specific semantic UI remains sufficient.
  - Reconsider: A selected extension requires genuinely arbitrary UI.
  - Order: after D145; reconsider only if its surfaces fall short.

- **D125 — Tasks and run configurations**
  - Deferred: No investigation yet. VS Code and Zed read `tasks.json`, Emacs
    has `compile` and `project-compile`, and Neovim has `:make` and plugins.
  - Reconsider: Running builds, tests, or scripts from a terminal becomes
    friction in daily use. Then investigate what a task is (a command line, a
    working directory, environment, and output handling), where definitions
    live (project-local settings or a separate file), whether output goes to a
    terminal or a generated text view with clickable locations (D027), and
    whether it ships as a bundled extension over process spawning (D087).
  - Order: after D087 and D089; with or after D124.

- **D126 — Git status per root**
  - Deferred: No Git integration exists. Every compared editor shows at least
    the branch and file status, built in or through a near-universal plugin.
  - Reconsider: The explorer (D034) is in daily use. Then show each root's
    branch and file status in the explorer and the status bar (D077) from a
    bundled extension over process spawning (D087) or a native library, and
    decide how status refreshes with file watching (D008).
  - Order: after D087, D034, and D077.

- **D107 — Package signing and verified publishers**
  - Deferred: The package manager (D101–D106) installs unsigned archives and git
    repositories and treats an explicit install confirmation plus entitlement
    grants (D087) as consent. A git install is only as trustworthy as the
    repository it came from: the pinned commit proves the content did not change
    after install, but not who wrote it. Archive hashes are published beside the
    file by the same party that serves it.
  - Reconsider: Installs are common enough that "trust this link" is not a
    sufficient story. Then define a signature over the installed content hash and
    commit, publisher keys and their distribution, how a source vouches for a
    publisher, and how revoked or rotated keys surface, without making signatures
    a condition for local installs.
  - Order: after the package manager plan (D101–D106).

- **D108 — Hosted package registry and publishing**
  - Deferred: The package manager (D101–D106) treats a git repository as the
    publishing mechanism, so it owns no artifact server, upload or publication
    flow, accounts, archive storage, download metrics, or in-editor catalog.
    Discoverability and voting live on a separate website that links
    repositories.
  - Reconsider: Git installs prove too risky or awkward (mutable history,
    missing build output in repositories, no version metadata to resolve
    against), or in-editor search and popularity ranking become worth running a
    server. Then define an index schema, publication and ownership, archive
    hosting and hashes, version metadata for resolution, and how it coexists
    with git sources under one lockfile.
  - Order: after the package manager plan (D101–D106).

- **D116 — Additional package sources**
  - Deferred: The package manager (D101–D106) installs from local archives and
    git repositories over public HTTPS, with one version per package identity.
  - Reconsider: A concrete workflow needs private repositories, SSH or token
    credentials, a local filesystem path as a long-lived source, or a
    per-project package set (which depends on D089). Then define credential
    storage, source precedence, and pinning per source rather than adding
    ad-hoc special cases to the lockfile.
  - Order: after D103, whose git sources it extends; a per-project package
    set also after D089.

- **D055 — Extensible icon and status-glyph system (icon packs)**
  - Deferred: Contributed tree items use a small fixed `TreeIcon` set, status
    badges are colored dots in a theme color key, and actions are label-only.
    Themes (D064) carry only colors and syntax styles. There is no way
    for a theme or an extension to replace or extend item, action, or status
    glyphs, and no icon-pack mechanism. Shipping a demo's verbs as tokens in the
    shared API would bake one feature's vocabulary into the boundary.
  - Reconsider: A theme or extension needs to supply or override glyphs (for
    example a file-type or source-control icon pack). Then define an
    application-owned icon registry with named tokens and lifecycle-owned
    packs registered like themes (`themes.register`), selected through a
    setting as `theme.*` selects themes, with a deterministic native
    fallback, and migrate the fixed `TreeIcon` set and label-only actions
    onto it.
  - 🚧 Planned: [D055 plan](plans/d055-icon-packs.md).
  - Order: after D160.

- **D022 — Rich completion policy and presentation**
  - Deferred: The prototype validated provider aggregation and replaceable
    native presentation.
  - Reconsider: Completion becomes part of a dogfood slice.
  - Order: after D064; language-server completion after D067.

- **D023 — Standardized AI integration**
  - Deferred: No concrete user workflow or provider boundary has been selected.
  - Reconsider: An AI-assisted editing slice is defined.
  - Order: after D087, which gates network access.

- **D117 — Non-modal notifications and progress**
  - Deferred: Extension messages use the native modal prompt, and native
    success and progress notices were dropped because their effect is
    visible. A long-running extension task, such as starting a language
    server or indexing, has no non-blocking way to report progress or a
    passing notice.
  - Reconsider: D067, D085, or another bundled extension runs work that
    takes seconds. Then add a transient notice and a progress handle with
    a title, optional percentage, and cancellation, shown through the D077
    status bar rather than a new surface.
  - Order: after D077; before D067.

- **D118 — Query-driven picker**
  - Deferred: `workbench.showPicker` takes a complete item list and filters
    it locally, which deliberately avoids streaming providers.
  - Reconsider: A picker's items depend on the query or are too many to
    send up front, such as workspace symbols, live grep, or Go to File on a
    large tree. Then add a provider that receives each query with
    cancellation and returns items, keeping the static list as the common
    case. A preview of the selected item waits for D109's reveal.
  - Order: after D071, which settles the shared picker; before D067's
    workspace symbols and D082 on large trees.

- **D140 — Placeholder text in input fields**
  - Deferred: The shared `TextInput` behind `workbench.showInput` and the
    palette query shows nothing while empty, so an input can only explain
    itself through its title or message line.
  - Reconsider: Before the next bundled input that would benefit from a
    hint. Then render placeholder text in `TextInput` and add a
    `placeholder` option to `showInput`, and let Tab accept it into an empty
    field, as Zed's Go to Line does. Update `@knot/go-to-line` to show the
    current position as `line:column` there.

- **D119 — Extension menu and context-menu items**
  - Deferred: Extension commands reach the palette and keymaps but not the
    application menus or the context menus of the editor and tabs.
  - Reconsider: A bundled or installed extension needs a command where users
    look for it by pointer. Then let a registration place a command in a
    named menu location, reusing the tree context menus D053 adds.
  - Order: after D053.

- **D057 — Extension and product localization (i18n)**
  - Deferred: Contributed view titles, tree labels, status text, native prompt
    text, and the bundled registry's own strings are literal English. Extensions
    and personal config can pass arbitrary strings, but there is no locale
    source, message catalog, or lookup for native-owned product text.
  - Reconsider: Shipping a second language becomes a product requirement. Then
    define locale discovery, a catalog format with fallback, which strings are
    extension-owned versus application-owned, contribution-time versus call-time
    resolution, and how native product text such as "Projects" reaches the same
    mechanism.

## Editor extensibility

These items remain after the editor API initiative, which built the
primitives its Vim mode (D058) needs. D067 and D027 drive the
presentation and hook items. Keep each primitive generic, so a Helix or
Kakoune mode could use it too. The
[extensibility report](reports/extensibility.md) maps Vim's needs onto them.

- **D113 — Command and input recording**
  - Deferred: Nothing can observe the stream of invoked commands together
    with the text typed between them, so neither repeat nor macros can be
    built. Absorbs user macros and repetition from D020. The first Vim mode
    (D058) ships without `.`, `q`, the `".` register (the last inserted
    text), and counts on insert commands such as `3ihi<Esc>`, which all need
    the text typed in insert mode.
  - Reconsider: Daily Vim use misses `.`, `q`, `".`, or insert counts, or a
    user macro workflow is selected. Then record a view's invoked commands
    with arguments plus its typed text as edits, and replay the recording
    through ordinary dispatch.
  - Order: after D109 and D110.

- **D142 — Multiple and rectangular selections**
  - Deferred: The editor handle (D109) and views hold one selection. Vim's
    visual-block mode, multi-cursor editing, and rectangular selection need
    several, and each edit across them is a bulk edit.
  - Reconsider: Visual-block mode or multi-cursor editing is selected. Then
    let views and the editor handle hold an ordered selection set, with
    rectangular selection as one way to create it, and apply edits across
    the set as one batch. Decide then whether `ctrl-v` in Vim's normal mode
    stays native paste or starts visual-block mode.
  - Order: after D109 and D092.

- **D144 — Syntax-tree queries and text objects**
  - Deferred: the `folds` queries serve folding only. Extensions cannot
    query the syntax tree, and there are no `textobjects` queries, so Vim's
    `if`, `ac`, and `ia`, structural selection expansion, and tree-based
    motions cannot be built.
  - Reconsider: Daily Vim use misses tree-sitter text objects, or structural
    selection is selected. Then add per-language `textobjects` queries beside
    `folds` and `indents`, and expose node queries through the extension API
    without exposing tree-sitter objects.
  - Order: after D066; before D151's tag objects.

- **D146 — Broad Ex command set**
  - Deferred: The first Vim mode (D058) implements a core Ex set: `:w`,
    `:q`, `:q!`, `:wq`, `:x`, `:e`, `:<line>`, and `:set` for a few options.
    Substitution and `:noh` are D150. `:g` and `:v`, `:normal`, `:sort`, `:d`, `:m`,
    `:t` and other range commands, `:reg` and `:marks`, `:map` and its
    variants, and user-defined Ex commands are missing. So are `:w <file>`
    and `:sav <file>`, which need a public save to a given URI without the
    Save dialog, and `:e!`, which needs a revert of a document to its
    file's text; neither primitive exists.
  - Reconsider: Daily Vim use reaches for one of them. Then add it inside
    the Vim package over existing primitives, or add the missing generic
    primitive first; `:normal` and `:g` with a
    normal-mode command also need replaying keys through the mode, which
    D113's recording provides.
  - Order: after D058; `:normal` after D113; `:g` and `:v` with D150's
    pattern matching.

- **D150 — Vim search and substitution**
  - Deferred: The first Vim mode (D058) ships without search, so that Vim
    search reuses current-file Find's (D031) match layer instead of a
    second engine, and without substitution, a whole-buffer edit.
    Missing: `/`, `?`, `n`, `N`, `*`, `#` with match highlights, `:noh`,
    and `:s` and `:%s` with ranges and flags.
  - Reconsider: D031 lands, or daily Vim use misses search. Then build
    search inside the Vim package on Find's matching and highlights, decide
    how Vim's regex syntax maps onto the engine Find uses, and add
    substitution as one batch in one undo step.
  - Order: after D058 and D031; substitution after D092, as bulk edits are.

- **D151 — Smaller Vim commands**
  - Deferred: The first Vim mode (D058) covers the commands of its scope.
    Commands outside it that daily editing may reach for are missing:
    `r<cr>` replacing characters with a line break; `J` leaving out the
    space before a `)`; `gJ` joining without spaces; a repeated `i(` or
    `a(` in visual mode growing the selection to the enclosing pair; sentence motions and
    objects (`( )`, `is`, `as`); tag objects (`it`, `at`); `gp`, `gP`, and
    `]p`; `_` and `|`; `gi`; and `ga`; and the `ctrl-w` keys that need workbench commands: `n` an empty pane (with
    D061's empty-pane handling), counts on `ctrl-w h j k l` (they need native
    counts, as every child command keeps the pane its parent started in), `x` `r`
    `R` swapping and rotating panes.
  - Reconsider: Daily Vim use reaches for one of them. Then add it inside
    the Vim package over existing primitives, with cases beside the others;
    tag objects may wait for D144's syntax-tree queries.
  - Order: after D058; tag objects with or after D144; `n` with D061.

- **D158 — Leaving a dock tool from Vim's `ctrl-w`**
  - Deferred: Vim's `ctrl-w h j k l` (D058) uses directional pane focus,
    which reaches a dock's tool pane such as the explorer. Vim follows
    editors only, so `ctrl-w` does nothing there, and Knot's own chords
    or a click leave. Pane cycling (`w W t b`) already skips docks.
  - Reconsider: Landing in a dock by `ctrl-w` gets in the way. Then choose
    between binding the `ctrl-w` window keys in tool panes' key contexts,
    and skipping docks for `ctrl-w` direction keys while Vim is active.
  - Order: after D058.

- **D152 — Persistent Vim global marks**
  - Deferred: The Vim mode's (D058) global marks `A`–`Z` hold a document
    and a native mark, so they last while that document is open and end
    when it closes. Vim's viminfo and Neovim's shada keep them across
    restarts and for files no longer open, with the registers.
  - Reconsider: Daily Vim use reaches for a global mark set in an earlier
    session or in a closed file. Then keep each mark's URI and offset in
    the package's storage beside the registers, recreate the native mark
    when its document opens, open the file for a mark whose document is
    closed, and clamp an offset past a changed file's end, as view state
    does.
  - Order: after D058.

- **D145 — Virtual text and anchored floating panels**
  - Deferred: D114 adds decoration styles. Extensions still cannot draw
    virtual text or inlay hints, or anchor a floating panel to a position.
  - Reconsider: D067 needs hover, inlay hints, or diagnostic text. Then
    design them against a real language server, as inline virtual text and
    an anchored floating surface that reuses the semantic tree and text
    content kinds.
  - Order: after D114; before D067; D021 only if it falls short.

- **D115 — Lifecycle hooks and native command overrides**
  - Deferred: Extensions observe document and window changes after they
    commit. They cannot run before save, wrap or replace a native command,
    or react to a language change in time to affect it.
  - Reconsider: Format on save, an LSP client (D067), or a workflow that
    needs to customize a native command is selected. Then add awaited hooks
    at a few named points (before save first) and a way to wrap a native
    command, with a bounded wait so a slow hook cannot block saving.
  - Order: after D079, which defines the save pipeline; before format on
    save in D067.

- **D147 — Key contexts on non-editor views**
  - Deferred: D110 lets extensions set key contexts on editor views only.
    Terminals, the explorer, and contributed tree and text views resolve
    bindings by view kind alone.
  - Reconsider: A consumer needs a modal tree, terminal, or contributed
    view, such as Vim keys in the explorer. Then extend the same
    lifecycle-owned contexts and fact-count resolution to that view.
  - Order: after D110.

- **D148 — Richer key context matching**
  - Deferred: A D110 binding names at most one context, with no negation or
    `when`-style expressions.
  - Reconsider: One context per binding forces an extension to duplicate
    many bindings or keep redundant contexts in sync. Then allow several
    contexts, or negated ones, per binding, still resolved by counting
    matching facts rather than by an expression language.
  - Order: after D110.

## Terminals and generated surfaces

- **D040 — Physical application-keypad input**
  - Deferred: gpui merges numeric-keypad keys with ordinary key identities.
    D026 retains numeric input but cannot emit distinct application-keypad
    sequences. The user chose deferral over vendoring gpui for a platform patch.
  - Reconsider: gpui exposes keypad identity, or a concrete TUI workflow requires
    a framework patch and its platform maintenance cost.

- **D041 — Additional terminal protocols and product events**
  - Deferred: D026 covers legacy keyboard/mouse protocols and HTTP(S) links;
    Kitty/modifyOtherKeys, graphics, OSC 52 clipboard access, titles, and bells
    have no selected product contract.
  - Reconsider: A daily shell/TUI workflow demonstrates a specific missing
    protocol or event; define its compatibility and product behavior together.
  - Order: with or after D063 if libghostty covers the protocol.

- **D042 — Additional terminal link destinations**
  - Deferred: D026 supports explicit and detected HTTP(S) destinations only.
    Arbitrary URI handlers and filesystem-path links require an opening policy.
  - Reconsider: A concrete workflow needs file navigation or another URI scheme.

- **D043 — Persistent terminal sessions**
  - Deferred: Live local sessions survive view reconstruction, but not application
    exit. Persistent processes/grid restoration need a separate lifecycle contract.
  - Reconsider: Restored terminal tabs, which start fresh shells in their last
    working directory (D127), need their processes or scrollback to continue.

- **D155 — Word and line selection in terminals**
  - Deferred: A terminal's local selection is always character by
    character: double- and triple-click start a plain selection, unlike
    editors and most terminal emulators.
  - Reconsider: Selecting a path, URL, or line from terminal output by
    dragging gets in the way. Then start the emulator's semantic selection
    on a double click and its line selection on a triple click while mouse
    reporting is off, so a drag afterwards extends by words or lines, with
    the emulator's default word separators.

- **D044 — Simultaneous terminal views**
  - Deferred: A sole attachment keeps input ownership and PTY resize authority
    unambiguous while supporting cross-window transfer.
  - Reconsider: Daily use requires viewing one live session in multiple panes;
    define independent viewport and shared resize/input policies.

- **D045 — General terminal settings**
  - Deferred: Shared typography and the session-local mouse toggle cover D026;
    no terminal keys are defined in the settings store (D070).
  - Reconsider: Daily use needs configurable fonts or terminal preferences;
    define them as settings, with fonts as `font.*` keys scoped
    `{ view: "terminal" }` so the editor keeps its own values.
  - Order: after D070; with D059.

- **D063 — Migrate the terminal engine to libghostty**
  - Deferred: The `alacritty_terminal` adapter meets current terminal
    behavior. Ghostty VT was rejected earlier for its separate PTY
    integration, pre-1.0 FFI, Zig packaging, integration size, and poor debug
    parsing performance.
  - Reconsider: libghostty offers a stable embeddable API and packaging that
    fits the Rust build, or it covers terminal behavior Alacritty lacks (such
    as D041 protocols). Then replace the emulator behind the existing
    application-owned session without changing its PTY ownership or
    single-attachment contract.
  - Order: with or before D041 if libghostty covers its protocols; before
    D128.

- **D128 — Terminal working directory from libghostty**
  - Deferred: D127 reads a terminal's working directory by querying the
    shell process (`proc_pidinfo` on macOS, `/proc/<pid>/cwd` on Linux),
    because `alacritty_terminal` does not parse OSC 7. Windows records only
    the start directory.
  - Reconsider: When D063 moves the terminal engine to libghostty. Then read
    the directory from the emulator's OSC 7 state instead of the process,
    which may also cover Windows and remote shells.
  - Order: after D127 and D063.

- **D038 — Remote PTYs**
  - Deferred: Local interactive sessions cover current terminal workflows.
  - Reconsider: Daily use requires remote shells; select transport,
    authentication, and disconnect behavior for a concrete workflow.

- **D039 — Shell integration**
  - Deferred: Ordinary terminal input and output cover current shell workflows.
  - Reconsider: Daily use requires working-directory tracking, command
    boundaries, or exit status; select supported shells and integration setup.

- **D027 — Interactive generated text views**
  - Deferred: Contributed text frames cover read-only generated output, and no
    product path creates a generated document. The extension-facing use case is
    an interactive generated view in the style of magit: a text surface whose
    ranges carry semantic actions (stage, open, expand), that refreshes in place
    with stable positions, and that has no persistence identity.
  - Reconsider: A concrete extension workflow such as Git status or search
    results needs interactive generated text beyond a read-only frame. Then
    define how an extension creates and refreshes the view, attaches per-range
    actions, and how such views behave in closure, recovery, and restoration.
  - Order: after D109, whose cursor and selection access per-range actions
    need, and D114, whose decoration styles style its ranges.

- **D054 — Dedicated read-only generated text surface**
  - Deferred: Contributed text frames embed the ordinary editor view over a
    private read-only buffer. This reuses selection, copy, scrolling, and
    rendering without a second renderer, but couples tool frames to the editor
    key context and command surface.
  - Reconsider: A text frame needs behavior the editor surface cannot express,
    or tool-focused keymaps and editor commands conflict, at which point extract
    a dedicated read-only presentation over the same buffer layer.

## Platforms and accessibility

- **D160 — Current gpui and gpui-component**
  - Deferred: Knot is on `gpui` 0.2.2, Zed's last crates.io release, and
    hand-builds every widget.
  - 🚧 Planned: [D160 plan](plans/d160-gpui-component.md): port to the
    `gpui-pre` snapshots, then adopt gpui-component for leaf widgets.
  - Order: before D162, D055, and D030.

- **D162 — Plain models and a thin gpui shell**
  - Deferred: the buffer model, editor state, application stores, extension
    handlers, and product operations mix behavior with gpui runtime use, so
    their tests need a gpui application and a framework change would revisit
    behavior.
  - 🚧 Planned: [D162 plan](plans/d162-thin-gpui-shell.md): move that
    behavior into the model crate and keep a shell that resolves entities,
    calls the model, runs effects, and notifies.
  - Order: after D160.

- **D163 — Vim tests on a minimal language fixture**
  - Deferred: `knot-ext-vim`'s tests load the real language packages,
    because Vim's indentation, key-context, and `o`/`O` cases need language
    configuration, so an edit to a language package reruns the Vim tests.
  - Reconsider: now. Give the Vim tests an inline fixture language
    (the simplest that supplies the configuration they use) instead of
    `knot-ext-languages`, and drop that dev-dependency.

- **D168 — Website screenshots and documentation**
  - Deferred: The landing page has no screenshot, and the user-facing docs
    (extension guide, settings reference) are readable only on GitHub.
  - 🚧 Planned: [website plan](plans/website.md): screenshots on the landing
    page and a generated documentation section covering getting started,
    configuration, the extension guide, and a feature tour.
  - Order: independent; D167 follows it.

- **D167 — Website blog and newsletter**
  - Deferred: The [website plan](plans/website.md) covers screenshots and
    documentation only. A blog and a newsletter need a publishing workflow, a
    feed, and a subscriber service, and have nothing to announce before the
    first release.
  - Reconsider: Knot approaches a first release (D035). Then choose a static
    blog under `website/` with an RSS feed, and a newsletter provider that
    keeps subscriber data out of the repository.

- **D062 — Positioning existing windows**
  - Deferred: gpui sets a window's bounds only at creation and can
    afterwards only resize it, including on its separate platform crates. Session restoration and project switching always
    create their windows, so they apply saved geometry at creation and no
    product path needs to move an existing window.
  - Reconsider: A workflow needs to move or re-frame an open window, such as
    window arrangement commands, an extension API, or reusing windows during
    restoration. Then add a small native per-platform window-frame operation
    (`NSWindow` frame on macOS, `SetWindowPos` on Windows, X11 configure
    requests) beside gpui rather than vendoring it. Wayland compositors own
    placement, so positioning there stays best-effort.

- **D035 — macOS distribution**
  - Deferred: Local `Knot.app` builds cover dogfooding without release
    credentials or a distribution channel.
  - Reconsider: Knot is ready to run on other Macs; choose a release artifact,
    then add Developer ID signing, hardened runtime validation for V8,
    notarization, and an install/update path.

- **D030 — Accessibility bridge**
  - Deferred: gpui has a public AccessKit bridge (`Element::a11y_role`,
    `write_a11y_info`), and gpui-component's widgets publish roles through
    it, but the editor, trees, terminal, palette, and pickers publish
    nothing, though their semantic state is already retained.
  - Reconsider: Accessibility becomes a product priority, or a user needs a
    screen reader; then give each native surface its roles and the editor
    its text and caret, starting from the semantic tree state.
