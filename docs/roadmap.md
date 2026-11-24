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

The next unused ID is **D229**, and for bugs **B002**; take it for a new item and bump this line.

Each section is ordered so that an item comes after the items it depends on,
wherever the sections allow it. Items with no dependencies are ordered by
priority. [Next up](#next-up) orders the work to do next across all sections;
an item there is worth doing in that order, whatever its own trigger says. An
item marked 🚧 has a plan under [plans/](plans/).

Knot has no users, so every current command, binding, setting, surface, API,
and stored format may be redesigned or removed, with no migration or
compatibility path. Descriptions name current behavior only as a starting
point.

## Next up

The [version control plan](plans/vcs.md) comes first, in its milestone order:

- D229 — Editor targets for diff tabs.
- D220 — Anchored floating panels.
- D221 — Hunk popup.
- D214 — VCS changes view.
- D215 — VCS commit.
- D216 — Git operations.

Then:

- D225 — Gutter mark tooltips.
- D174 — Show Difference for files changed on disk.
- D022 — Editor autocompletion.
- D129 — Refactor builtin language extensions.
- D138 — Extension startup report as an untitled document.
- D054 — Dedicated read-only generated text surface.
- D085 — Cross-file search.

Remove an item from this list when it is done or dropped.

## Working an item

Before implementing an item, draft its plan with the `/brainstorm-plan` skill:
it analyzes the item in its own session and writes the design in
`docs/plans/d###-<slug>.md`, however small the item is. Mark the item 🚧
with a link to the plan, and keep design detail out of this file. Remove the
item once its durable behavior lives in the architecture references and
decisions, and remove its plan with it. Reports under [reports/](reports/)
hold the analyses that support some items.

## Bugs

- **B001 — Explorer tests flake under load**
  - Depends on: none
  - Description: `explorer::a_document_saved_under_a_new_name_is_selected_in_the_explorer`
    (once on CI, 1 in 120 locally) and
    `explorer::rename_selects_the_stem_and_open_documents_follow` (2 in 120)
    time out waiting for tree state when the machine is loaded, for instance
    10 `yes > /dev/null` processes beside 120 runs of the test binary, 30 at a
    time. Reading `followMoved`, the watcher refresh, and the tree view's
    `reveal`, `invalidate`, and `apply_response` found no race. Log the tree's
    rows, loading state, and pending reveal when the wait times out, stress the
    test until it fails, and fix the cause.

## Architecture and technical debt

- **D129 — Refactor builtin language extensions**
  - Depends on: none
  - Description: Each bundled language package is named `@knot/language-<name>`
    because a package ID is exactly `@scope/name`. The grammars without a
    package of their own still share `@knot/language-others`. Do it before the
    next bundled language package, or when the remaining grammars get packages
    of their own: decide whether package IDs gain a nested form such as
    `@knot/language/rust`, which touches identity validation, discovery,
    storage paths, and the build, and split the remaining grammars.

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

## Features

### Workspace, projects, and files

- **D222 — HTTP client for extensions**
  - Depends on: none
  - Description: Extensions have no network access. Do it when a package needs
    one: a `fetch`-shaped HTTP client gated by a `network` entitlement through
    the existing entitlement declarations and startup consent.

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

- **D204 — Opening a project restores the sidebar**
  - Depends on: none
  - Description: Opening a project from the Projects tool leaves that tool
    expanded in the sidebar, hiding the explorer or whatever was showing.
    Do it when the Projects tool is next touched, or before D084 replaces
    it: after opening a project, collapse the Projects tool and show the
    explorer, or restore the sidebar state from before the tool was opened.

- **D085 — Cross-file search**
  - Depends on: none
  - Description: A bundled extension that wraps ripgrep through process
    spawning and parses its `--json` output, passing the effective ignore
    rules. It provides a query input with case, regex, whole-word, and glob
    options; streamed results in a tree grouped by file (contributed tree
    groups and row actions already exist); opening a result at its range as a
    preview tab; cancelling a superseded search; and refresh. Decide here how
    results recover when edits make their targets stale.

- **D124 — Workspace trust**
  - Depends on: D089
  - Description: Project-local settings start as data that runs no code, and
    nothing else executes on behalf of a repository. Do it when project
    settings, tasks (D125), or language servers would run code or commands from
    an opened repository, and before D125 runs repository-defined commands:
    decide, as VS Code's workspace trust and Neovim's `exrc` trust database do,
    how a folder becomes trusted, what an untrusted folder disables, and where
    grants are stored, reusing the entitlement grant storage if it fits.

- **D090 — EditorConfig support**
  - Depends on: D089
  - Description: `.editorconfig` files need the `path` scope fact; file
    watching exists (`files.watch`). Do it once D089 is complete, or when daily
    use hits a project that relies on `.editorconfig`: a bundled package finds
    the files above each opened document, turns each section into a
    `path`-glob scope, and writes them from the root down to the nearest file,
    so last-write-wins reproduces EditorConfig's rule that the nearest file
    takes precedence.

- **D184 — Document-scoped settings survive hot exit**
  - Depends on: none
  - Description: Values scoped to a document end with it, and a window layout
    records only a document's explicit language. After quit and relaunch a
    restored document keeps its language but loses its own `editor.tabSize` and
    `editor.softWrap`. Do it when daily use resets a document's tab size or
    wrapping on relaunch: record a document tab's document-scoped setting
    values in the window layout and write them back on restore, as the explicit
    language is, so nothing is kept for documents that are not open.

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
    projects of D084. Vim's `ctrl-w n` waits for it.

- **D205 — Drag tree rows to other apps**
  - Depends on: none
  - Description: Tree rows drag only within Knot and the editor. Finder, mail clients, and
    other apps take nothing from them. Do it when daily use wants to hand an
    explorer file to another app: start a native OS drag carrying the row's
    `resource` as a file when the pointer leaves the window, and decide
    whether that drag copies or moves.

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
    offers no comparison. Add Show Difference to the prompt,
    comparing the buffer with the file in the diff view.

- **D006 — File drag and drop**
  - Depends on: none
  - Description: Open and Save As establish workspace-independent documents
    first. Do it when platform interaction becomes a selected product slice.

### Extension platform and language tooling

- **D226 — Optional package dependencies**
  - Depends on: none
  - Description: A package that imports another's contract must list it in
    `requires`, so the dependency loads first and a missing one blocks the
    consumer. Some consumers only enhance themselves when a provider exists,
    such as Vim's file completion or a feature that uses `@knot/vcs` when
    present. Add an optional dependency kind whose import still resolves, with
    a client that rejects while no provider is served.

- **D225 — Gutter mark tooltips**
  - Depends on: none
  - Description: Gutter marks show a color but not why they are there, so a
    diagnostic's message or a change's summary needs a click. Do it after the
    [version control plan](plans/vcs.md), before language-server diagnostics
    (D067) rely on it: let a gutter contribution carry a hover message that
    the gutter shows on hover.

- **D200 — Progress reporting**
  - Depends on: none
  - Description: A long-running task has no non-blocking way to show progress.
    Scanning the workspace file index takes about ten seconds on a very large
    project, and language-server indexing (D067) will too. Add a progress handle
    with a title, optional percentage, and cancellation, presented alongside
    notifications, with the file index scan as its first consumer.
    Deliver cancellation through the handle to the task that holds it.

- **D145 — Virtual text and inlay hints**
  - Depends on: none
  - Description: Decoration styles exist, but extensions still cannot draw
    virtual text or inlay hints. Do it before D067 needs inlay hints or
    diagnostic text, or blame (D217) needs annotations: design them against a
    real language server, as inline virtual text.

- **D220 — Anchored floating panels** 🚧
  - Depends on: none
  - Description: Extensions cannot anchor a floating panel to a position in
    an editor. The hunk popup (D221) is the first consumer, and language-server
    hover (D067) the next: add an anchored floating surface that reuses the
    semantic tree and text content kinds, with focus, dismissal, and
    positioning that follows the anchor. Part of the
    [version control plan](plans/vcs.md).

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

- **D022 — Editor autocompletion**
  - Depends on: none
  - Description: Completion providers register, aggregate, and show in a native
    list, but completion only opens on request, and no bundled package provides
    candidates. Open completion while typing, with triggers and a delay, show
    each candidate's kind and detail, and ship a bundled provider of the words
    in open documents. Language-server completion arrives with D067 over the
    same providers. Design the provider contract so that a later AI provider of
    inline suggestions, in the style of GitHub Copilot, fits it: slow and
    streamed results, cancellation as the user types, and a multi-line
    suggestion shown as virtual text (D145) rather than in the list.
    Completion providers serve the Knot-defined `completion` contract;
    measure the provider call's latency while typing, and add the
    current line's text to the request if reading the buffer costs too much.

- **D067 — Language servers and semantic tokens**
  - Depends on: D145, D220, D200, D115, D022
  - Description: No language client exists. The syntax highlighting work
    prepared for one: LSP language IDs, `Send` text snapshots, a highlight
    vocabulary that semantic token types and modifiers map onto, and a
    semantic-token paint layer above syntax. Do it when daily editing needs
    diagnostics, navigation, hover, or semantic highlighting: decide whether
    the client is native or an extension (if an extension that spawns servers,
    it needs the `process` entitlement), define a whole-range semantic-token contribution that
    scales to file-sized token counts unlike per-item anchored contributions,
    and connect diagnostics to decorations and completion to D022. It needs
    D220 for hover and D145 for inlay hints and diagnostic text, and format on
    save builds on D115.

- **D125 — Tasks and run configurations**
  - Depends on: D089, D124
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
    with or before D067, whose language servers are the first workload that
    can affect responsiveness or resource use.

- **D224 — Improve entitlement management window**
  - Depends on: none
  - Description: Managing entitlements is a transient picker with one row per
    installed package: a state word, no per-entitlement detail, an immediate
    toggle with no confirmation, and no view of bundled grants. Revisit it
    when more entitlement kinds (D222) make the decision heavier or real users
    hit it: per-entitlement states, confirmations, and a persistent surface.

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

### Version control

The [version control plan](plans/vcs.md) holds the agreed architecture and the
milestones these items belong to.

- **D221 — Hunk popup** 🚧
  - Depends on: D220
  - Description: A gutter marker shows that lines changed, not how. Clicking
    it opens a popup anchored to the hunk that shows the baseline's lines
    against the current ones, with revert, next and previous hunk, and opening
    the file's diff; backends may add their own actions, such as staging the
    hunk in D216.

- **D214 — VCS changes view** 🚧
  - Depends on: none
  - Description: There is no list of changed files. Add a changes view that
    shows the provider's groups of changed files (for Git: staged, unstaged,
    untracked) with the actions the provider defines, and opens a file's
    changes in the diff view.

- **D215 — VCS commit** 🚧
  - Depends on: D214
  - Description: Committing needs a terminal. Add the commit UI, a message
    editor with options the provider supplies (for Git: amend, sign-off), and
    commit through the provider; build the UI primitives it needs as extension
    API.

- **D216 — Git operations** 🚧
  - Depends on: D215
  - Description: Staging, branches, push and pull, stash, and other
    Git-specific workflows stay outside the shared provider contract. Build
    them in `@knot/git` as the plan's last milestone.

- **D217 — Blame**
  - Depends on: D145
  - Description: Nothing shows who last changed a line and in which commit.
    Do it after the [version control plan](plans/vcs.md): show blame through
    the provider, as end-of-line annotations on the current line or as a
    gutter column for the whole file, with the commit's details on hover.

- **D218 — Log, graph, and file history**
  - Depends on: D216
  - Description: There is no view of a repository's history. Do it after the
    [version control plan](plans/vcs.md): a log view of commits with the
    branch graph, each commit's changed files opening in the diff view, and
    the same view filtered to one file's history across renames.

- **D219 — Merge conflict resolution**
  - Depends on: D216
  - Description: Conflicted files show only their markers. Do it after the
    [version control plan](plans/vcs.md): list conflicted files, accept ours,
    theirs, or both per conflict from the editor, and a three-way merge view
    built on the diff view.

- **D229 — Editor targets for diff tabs** 🚧
  - Depends on: none
  - Description: Vim mode does not work in a diff tab, which makes the diff
    hard to use for anyone who runs it. Extensions follow only tabs that
    expose a document and an editor handle, and a diff tab's editors are
    native views with neither. Give a diff tab's editors, at least the
    modified side's, targets that extensions can reach with
    `workbench.editor`, so Vim and other editor-driven packages attach to
    them. Decide how the targets relate to the document when the modified
    side is one, and what a snapshot side exposes. Part of the
    [version control plan](plans/vcs.md).

- **D228 — Diff toolbar design**
  - Depends on: none
  - Description: The diff tab's bar is a bare row of three small icon buttons
    (previous hunk, next hunk, read-only lock) with no labels, state, or
    grouping, and looks unfinished next to the rest of the workbench. Do it
    after the diff view ships: design the bar like the other tab toolbars,
    with clear button states, tooltips, a hunk position indicator, and room
    for the layout toggle.

### Package manager

- **D101 — Package manager** 🚧
  - Depends on: none
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
    preset, and choose a default key for Toggle Explorer against the presets
    (IntelliJ binds `cmd-1`, VS Code `cmd-shift-e`).

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

- **D135 — Reindent pasted text**
  - Depends on: none
  - Description: Lines are indented from the syntax tree on Enter, while typing
    a closer, and through Reindent Selected Lines and Reindent Buffer. Pasted
    lines keep the indentation they had where they were copied from. Do it when
    pasting code between different nesting depths needs a Reindent Selected
    Lines afterwards often enough to be friction: decide, as Zed's auto-indent
    on paste and VS Code's `editor.autoIndentOnPaste` do, whether to reindent
    every pasted line from the query or shift the block by the first line's
    change and keep its relative indentation, how pasting inside strings and
    comments behaves, and whether it needs a setting to turn it off.

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

- **D142 — Multiple and rectangular selections**
  - Depends on: none
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
    motions, and Vim's `it` and `at`, cannot be built. Do it when daily Vim use misses tree-sitter text
    objects, or structural selection is selected: add per-language
    `textobjects` queries beside `folds` and `indents`, and expose node queries
    through the extension API without exposing tree-sitter objects.

- **D147 — Key contexts on non-editor views**
  - Depends on: none
  - Description: Extensions set key contexts on editor views only. Terminals,
    the explorer, and contributed tree and text views resolve bindings by view
    kind alone. Do it when a consumer needs a modal tree, terminal, or
    contributed view, such as Vim keys in the explorer: extend the same
    lifecycle-owned contexts and fact-count resolution to that view.

### Vim mode

The bundled Vim mode is built on generic editor primitives, so a Helix or
Kakoune mode could use them too. The
[extensibility report](reports/extensibility.md) lists the gaps that remain.

- **D201 — Preview replacement results**
  - Depends on: none
  - Description: Vim's `:s` and Find's Replace change text without showing the
    result first; the user sees it only after the edit and undoes a wrong
    pattern. Missing: a live preview of what a replacement would produce while
    the pattern and replacement are typed, as Neovim's `inccommand` shows for
    `:s`, and the same for the Find bar's Replace. Do it when wrong
    substitutions cost daily editing time: compute the replaced text from
    `buffer.find` matches and the translated replacement, show it as
    decorations or a generated text surface without touching the buffer, and
    decide whether the preview needs a native expansion primitive so that it
    matches what `replace` would write.

- **D208 — Vim pane swap and rotate**
  - Depends on: none
  - Description: `ctrl-w x`, `r`, and `R` swap and rotate panes, which the
    workbench has no commands for. Do it when daily Vim use reaches for one:
    add the layout operations to the workbench in `knot-model`, expose them
    as commands, and bind the keys in the Vim package.

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

- **D223 — Vim replays that follow focus**
  - Depends on: none
  - Description: `.`, counted inserts, and macros replay an editor recording
    through the Vim package, and a replay targets the editor that started it:
    a macro stops when focus leaves that editor, because the package cannot
    observe focus changes while the replaying command awaits. In Vim a
    replay continues in whichever editor a recorded key moves focus to, so a macro
    can split, switch windows, or jump to another file and continue there.
    Do it when daily Vim use records a macro that crosses editors: listeners run during
    the replay, so follow focus changes in the
    package and retarget the remaining keys, stopping when focus leaves
    editors altogether.

### Terminal

- **D191 — Faster libghostty-vt builds**
  - Depends on: none
  - Description: The release archives are built for a baseline CPU. The
    Windows archive also has no SIMD, because MSVC has no tool that hides
    the simdutf and Highway symbols V8 links in other versions; macOS and
    Linux keep SIMD with only the `ghostty_*` API exported. Do it when
    terminal throughput matters: enable SIMD on Windows, for example by
    linking the DLL or a symbol-hiding step that works on COFF, and raise
    the CPU baseline per target, with runtime dispatch where the baseline
    must stay low.

- **D192 — Terminals that run a command**
  - Depends on: none
  - Description: `terminals.open` always starts the login shell, so an
    extension or personal configuration cannot open a terminal that runs a
    given program, and manual review commands can only tell the user what to
    type. Do it when an extension needs to run a program in a terminal, or
    when review commands should start their fixtures directly: add a command
    and arguments to the terminal options, run instead of the shell, and
    define how restart and exit treat such a terminal.

- **D042 — Additional terminal link destinations**
  - Depends on: none
  - Description: Only explicit and detected HTTP(S) destinations are supported.
    Arbitrary URI handlers and filesystem-path links require an opening policy.
    Do it when a concrete workflow needs file navigation or another URI scheme.

- **D039 — Shell integration**
  - Depends on: none
  - Description: Ordinary terminal input and output cover current shell
    workflows. Do it when daily use requires command boundaries or exit status:
    select supported shells and integration setup, and give the semantic
    prompts (OSC 133) that libghostty already parses a product contract.

### Platforms, accessibility, and website

- **D035 — macOS distribution**
  - Depends on: none
  - Description: Local `Knot.app` builds and the unsigned nightly cask cover
    dogfooding without release credentials. Do it when Knot is ready to run on
    other Macs: choose a release artifact, then add Developer ID signing,
    hardened runtime validation for V8, notarization, and an install/update
    path.

- **D188 — Good Windows citizen**
  - Depends on: none
  - Description: `knot.exe` is a console-subsystem program so that the command
    line launcher can print `--help`, errors, and exit codes, but starting it
    from Explorer or the Start menu also opens a console window. Do it before
    Knot is offered to Windows users: keep a console entry for the terminal (as
    VS Code's `code.cmd` wrapper does) and launch the editor itself without a
    console.

- **D189 — Windows installer**
  - Depends on: D188
  - Description: Windows has no release artifact; users run a built `knot.exe`
    and put it on the `PATH` with the palette install command. Do it when Knot
    is ready to run on other Windows machines: choose an installer format with
    a per-user install location, an "Add to PATH" option, file associations,
    and an update path.

- **D190 — Linux installer**
  - Depends on: none
  - Description: Linux has no release artifact; users run a built binary and
    link it into `~/.local/bin` with the palette install command. Do it when
    Knot is ready to run on other Linux machines: choose the package formats,
    ship `knot` on the `PATH`, a desktop entry and icon, and an update path.

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

## Lower priority

Items worth having that no current workflow needs. They follow the same shape
and may move into a section above when daily use asks for them.

- **D107 — Package signing and verified publishers**
  - Depends on: D101
  - Description: Installs from a git link trust the URL alone. Add keys, a
    signature format, verified publishers, and a way for a source to vouch for
    content. Do it when installs are common enough that "trust this URL" is not
    a sufficient story.

- **D108 — Hosted package registry**
  - Depends on: D101
  - Description: Packages are discovered on a website and installed from git
    links. Add an artifact index, upload and publication, accounts, and
    in-editor search. Do it if git installs prove too risky or awkward, or if
    discoverability in the editor is worth owning a server.

- **D116 — Private package sources and per-project package sets**
  - Depends on: D101, D089
  - Description: Installs cover public git links only. Add private
    repositories, SSH or token credentials in a platform keychain,
    filesystem-path sources, and per-project package sets. Each is a separate
    consent and storage problem. Do it when installs outgrow public links.

- **D206 — Remaining Ex commands**
  - Depends on: none
  - Description: The Ex line covers files, buffers, windows, line ranges, and
    `:sort`, but `:g` and `:v`, `:normal`, `/pat/` and `?pat?` addresses and `;`
    between addresses, `:reg`, `:marks` and `:jumps` (they need a list surface,
    D054), `:map` and its variants, `:command`, `:!cmd`, `:r`, range filters,
    `:cd`, `:h`, `:&`, `:pu`, `:sort` with a pattern, and Vim buffer numbers in
    `:b` and `:bd` are missing. `:normal` and `:g` with a normal-mode command
    need replaying keys through the mode, as macros do, and `:g` and `:v` can
    reuse the search patterns of `@knot/vim`. Do it when daily Vim use reaches
    for one of them: add it inside the Vim package over existing primitives, or
    add the missing generic primitive first.

- **D194 — Preserve-case replacement**
  - Depends on: none
  - Description: Replace inserts its text as typed. Do it when replacing
    identifiers across case styles becomes common: add a preserve-case toggle
    to the Replace bar that adapts each replacement to its match's case, as VS
    Code's and IntelliJ's do.

- **D195 — Multi-line Find and Replace fields**
  - Depends on: none
  - Description: Find bar fields are single-line, so a multi-line query or
    replacement is typed as `\n` in regex mode. Do it when that is too awkward
    in daily use: let bar fields grow to several lines, and seed the query from
    a multi-line selection.

- **D157 — Indent width separate from tab width**
  - Depends on: none
  - Description: `editor.tabSize` sets both how wide a tab displays and how
    far indentation steps, so Vim's `:set tabstop` and `:set shiftwidth` write
    the same setting. Only mixed styles lose, such as
    GNU-style C that indents by 4 and shows tabs as 8, turning 8 columns of
    indentation into one tab. Do it when real files in such a style need
    editing: add an indent width setting defaulting to the tab size, and carry
    it through indent and outdent, reindent and indent queries, Enter's
    indentation, indentation detection, the conversion commands, the status
    bar's picker, and `DocumentInfo.indentation`.

- **D011 — Remote filesystem providers**
  - Depends on: none
  - Description: The URI boundary is validated with local and memory providers.
    Do it when a specific remote workflow is selected.

- **D016 — Sandboxed workspace roots**
  - Depends on: none
  - Description: Local roots are trusted and intentionally follow symlinks. Do
    it when Knot opens untrusted workspaces under a security boundary.

- **D038 — Remote PTYs**
  - Depends on: none
  - Description: Local interactive sessions cover current terminal workflows.
    Do it when daily use requires remote shells: select transport,
    authentication, and disconnect behavior for a concrete workflow.

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

- **D021 — General extension UI or WebView escape hatch**
  - Depends on: D145
  - Description: Surface-specific semantic UI remains sufficient. Do it only
    when a selected extension requires genuinely arbitrary UI, and only after
    D145 has shown its surfaces fall short.

- **D043 — Persistent terminal sessions**
  - Depends on: none
  - Description: Live local sessions survive view reconstruction, but not
    application exit. Restored terminal tabs start fresh shells in their last
    working directory. Do it when those tabs need their processes or scrollback
    to continue; persistent processes and grid restoration need a separate
    lifecycle contract.

- **D170 — Overlay mode for docked tools**
  - Depends on: none
  - Description: A tool is always docked, resizing the center. IntelliJ's
    "undock" mode slides a tool over the center without resizing it, which
    suits a wide tool opened briefly. Do it when daily use shows tools opened
    briefly and dismissed, where the center reflowing is disruptive: add the
    mode per tool as a placement, with focus loss dismissing it.

- **D012 — Persistent undo and history trees**
  - Depends on: none
  - Description: Session-local linear undo is sufficient for dogfooding. Do it
    when users need edit history across restarts or branching history.
    Byte-offset view state already survives restarts and project switches.

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

- **D030 — Accessibility bridge**
  - Depends on: none
  - Description: gpui has a public AccessKit bridge (`Element::a11y_role`,
    `write_a11y_info`), and gpui-component's widgets publish roles through it,
    but the editor, trees, terminal, palette, and pickers publish nothing,
    though their semantic state is already retained. Do it when accessibility
    becomes a product priority, or a user needs a screen reader: give each
    native surface its roles and the editor its text and caret, starting from
    the semantic tree state.

- **D196 — Kitty graphics and OSC 52 clipboard in terminals**
  - Depends on: none
  - Description: libghostty parses Kitty graphics, and OSC 52 clipboard access
    has no callback, so Knot draws no inline images and programs cannot write
    the clipboard. Do it when a TUI workflow needs inline images or clipboard
    writes from a remote program: paint the images, and decide when a program
    may read or write the clipboard.

- **D032 — Extension source maps**
  - Depends on: none
  - Description: Built JavaScript loads and reports generated source locations
    without mapping them back to authoring files. Do it when debugging built
    extensions requires errors and stack traces to identify the original source
    locations.

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

- **D159 — Trim trailing whitespace on modified lines only**
  - Depends on: none
  - Description: Save trims every line. Trimming only the lines a change
    touched keeps saves from adding noise to the diff of a file that already
    had trailing whitespace, and needs a notion of modified lines that Knot
    lacks. Do it when a VCS or diff layer exists, such as a change gutter or
    diff view: add `"modified"` to the values of `files.trimTrailingWhitespace`
    (`"off"`, `"all"`) and trim only the lines that differ from the baseline.

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

- **D227 — Package unload signal**
  - Depends on: none
  - Description: A package cannot learn that another unloaded or restarted, so
    `@knot/vcs` finds out only when a call to a backend rejects
    `ContractNotFound`, and the gutter marks of a vanished backend stay until
    then. Do it when another package needs to react to a provider leaving
    before its next call: add an observable package lifecycle event to the
    extension API, with its native side, and let `@knot/vcs` drop a backend
    and clear its marks when it fires.
