# Daily-driver editor

Status: 🚧 approved, not started. Promotes D007, D008, D010, D031, D034, D053,
D060, D064, D068, and D089 from the [roadmap](../roadmap.md) and adds
D069–D088, D120–D123, D127, D134, D143, and D173.

## Goal

Make Knot an editor its author can use every day: no demo or test surfaces,
complete workbench and editing commands, a discoverable palette, in-file and
cross-file navigation, themes, and a workspace explorer and project selector
built as ordinary bundled extensions.

## How this plan is executed

This plan is an index. Each item below is a brief, not a design, and this
file stays high level. Before implementing an item, analyze it in its own
session and write its design in a separate plan, `docs/plans/d0xx-<slug>.md`,
however small the item is. Link that plan from the item here, and never
add design detail to this file. Mark progress here with ✅ and 🚧. Remove
each item's plan once it is complete, and remove this plan once every item's
durable behavior lives in the architecture references and decisions.

Each item lists what it depends on. Items in the same phase are independent
unless their dependencies say otherwise.

Nothing that exists today is a constraint. Knot has no users, so every
current command, binding, setting, surface, API, and stored format may be
redesigned or removed, with no migration or compatibility path. These briefs
name current behavior only as a starting point for analysis.

## Ordering rationale

The order puts shared foundations ahead of their consumers, so no surface has
to be rebuilt later:

- **Settings before themes, zoom, soft wrap, and save hygiene.** Each of these
  needs somewhere to keep a value and a way for personal config to set it.
- **Themes before any new surface.** The palette, status bar, find bar, gutter,
  and occurrence highlights all take their colors from theme keys, not from
  constants that would need migrating later.
- **The palette and shared picker before every picker consumer.** Go to File,
  Go to Line, recent files, and the project selector all rely on one fuzzy
  matcher and one picker presentation.
- **Language editing configuration before editing commands and brackets.**
  Toggle comment and auto-closing read per-language data from the same
  registration. Language-aware indentation (D134) follows the editing
  commands directly, so Enter indents after an opening bracket before the
  rest of the editing phase lands.
- **The keymap preset mechanism before new commands; extra presets last.**
  New commands bind into a single Knot preset as they land. The VS Code, Zed,
  IntelliJ, and Sublime presets are filled in once the command set is stable.
- **The file read API before project-local settings.** D089 reads a file from
  the repository, so it follows D122 in the files phase.
- **Entitlements before anything that spawns processes or uses the network.**
  Cross-file search wraps ripgrep and is the first consumer. Git, language
  servers, formatters, and AI integrations follow later.
- **Ignore rules, file watching, filesystem mutation, and file enumeration
  before the explorer, Go to File, and search.** The extensions consume these
  public primitives and need no private bypass.

## Phase 1 — Clean slate

- ✅ **D069 — Remove demo and test-only commands**
  - Move Terminal to New Window and the explorer placement commands are gone;
    tab drag moves terminals, and the Move Tab to Dock commands move tools.
    Developer tooling carries a `Developer:` title
    prefix. The window toolbar and status string are gone: failures use a
    warning prompt, and a failed package startup opens the extension report
    ([workbench and lifecycle](../architecture/workbench-and-lifecycle.md#product-application-launch)).

## Phase 2 — Foundations

- ✅ **D070 — Application settings**
  - One in-memory `settings` store shared by personal config and packages:
    the latest write wins, scoped values (`language`, `view`) beat unscoped
    ones, and definitions give keys a type, default, and description. Editors
    resolve `editor.tabSize` per language and view and repaint on change.
    `languages.associate` claims file patterns for a language, so detection
    overrides compose instead of living in one map setting
    ([extension host](../architecture/extension-host.md#settings),
    [decisions](../decisions/commands-and-settings.md#application-settings)).

- ✅ **D121 — Terminals start in the workspace root**
  - A new terminal starts in the root of the window's most recently focused
    document, else the first root, else home. Restored terminals fall back to
    the same rule (D127), and `terminals.open` takes an explicit directory
    ([workbench and lifecycle](../architecture/workbench-and-lifecycle.md#terminal-sessions-and-views),
    [decisions](../decisions/workbench-and-lifecycle.md#terminal)).

- ✅ **D127 — Restored terminals keep their working directory**
  - A terminal restored in place starts in the directory its shell was last
    in, read from the shell process and recorded in the window layout; a
    missing directory falls back to D121's rule. New terminals opened from a
    focused terminal start in its directory. D128 moves the query to
    libghostty
    ([workbench and lifecycle](../architecture/workbench-and-lifecycle.md#terminal-sessions-and-views),
    [decisions](../decisions/workbench-and-lifecycle.md#terminal)).

- ✅ **D064 — Theming system**
  - Themes are extension contributions registered like languages, and every
    native surface paints from color keys whose native defaults are also the
    fallback. Extensions define their own keys and name them in decorations,
    gutter markers, and badges. `theme.mode`, `theme.dark`, and `theme.light`
    select the theme, and the bundled `@knot/themes` package ships One Dark,
    One Light, Catppuccin, Tokyo Night, and Solarized with a remembered Select
    Theme command
    ([semantic UI](../architecture/semantic-ui.md#themes-and-color-keys),
    [decisions](../decisions/semantic-ui.md#themes)).

- ✅ **D071 — Command palette and shared picker**
  - One native fuzzy-matching picker (`nucleo-matcher`) serves the palette,
    native pickers, and `workbench.showPicker`, with highlighted matches,
    item detail and hints, and pulled highlight previews that Select Theme
    uses. Palette rows show titles, descriptions, and bindings, hide input
    commands, and rank recently used commands first across restarts, kept in
    the application state database
    ([semantic UI](../architecture/semantic-ui.md#extension-transient-interactions),
    [commands](../architecture/commands.md#dispatch),
    [decisions](../decisions/semantic-ui.md#extension-transient-interactions)).

- ✅ **D086 — Keymap presets** (promotes D060)
  - Default bindings are a preset in one slot beneath every extension and
    personal binding. The native Knot preset is correct on macOS, Windows, and
    Linux; packages register further presets, and `keymap.preset` selects
    one. Extensions read `knot.platform`. Show Keybindings lists every binding
    with its owner and overrides, and menus show the effective keys
    ([commands](../architecture/commands.md#keymaps),
    [decisions](../decisions/commands-and-settings.md#commands-and-keymaps),
    [extension guide](../extension-api.md#bind-keys-and-modal-contexts)).

- ✅ **D072 — Workbench navigation and layout commands**
  - Splits record ratios and every divider drags; grow, shrink, equalize,
    split left and up, directional focus and tab moves, and maximize work
    from the keyboard. Tabs switch in order, by position, and by recent use on
    ctrl-tab without an overlay. Pinned tabs, Close Other Tabs, Close Pane,
    Reopen Closed Tab, preview tabs from the explorer and `documents.open`,
    and the Move Tab to Dock commands complete the set; New Terminal moved to ctrl-`
    ([workbench and lifecycle](../architecture/workbench-and-lifecycle.md#workbench-and-protected-closure),
    [commands](../architecture/commands.md#keymaps),
    [decisions](../decisions/workbench-and-lifecycle.md#workbench-navigation)).

## Phase 3 — Editing

- ✅ **D073 — Language editing configuration**
  - Language registrations carry comment tokens, bracket pairs with
    auto-closing flags, and `autoCloseBefore`; `languages.get` returns the
    full definition to derive from. The registry resolves the data for
    native code beside a pure comment toggle, and the Rust, ECMAScript,
    Python, TOML, and Markdown packages supply it with per-package toggling
    cases. Indentation is left to D134's `indents` queries. D074 and D075
    consume it
    ([semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages),
    [decisions](../decisions/editing.md#language-editing-configuration),
    [extension guide](../extension-api.md#register-a-language)).

- ✅ **D074 — Editing commands**
  - Enter keeps the indentation before the caret; Tab, Shift-Tab, and
    indent and outdent lines follow the document's detected tabs or spaces,
    else `editor.insertSpaces`. Line and block comments toggle with the
    language at the cursor, including inside injections. Lines copy, move,
    delete, and join; Home is smart; words split at VS Code's separators,
    and drags after a double- or triple-click extend by words or lines. Each
    command is one undo step that restores its selection
    ([editor views](../architecture/workbench-and-lifecycle.md#editor-views),
    [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages),
    [decisions](../decisions/editing.md#editing-commands)).

- ✅ **D134 — Tree-sitter indentation**
  - Language registrations take an optional `indents` query with
    nvim-treesitter's captures, evaluated natively. Enter indents the new
    line from it once the tree covers the edit, within Enter's undo step;
    Enter between a bracket pair opens an indented middle line; typing a
    closer such as `}` or Python's `else:` outdents its line; Reindent Lines
    recomputes a selection. Unclosed brackets inside a parse error indent the
    lines after them. Rust, ECMAScript, Python, and TOML ship queries with
    `test/cases/indents.jsonc` cases
    ([semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages),
    [editor views](../architecture/workbench-and-lifecycle.md#editor-views),
    [decisions](../decisions/editing.md#indentation),
    [extension guide](../extension-api.md#register-a-language)).

- ✅ **D075 — Bracket matching and auto-closing**
  - The bracket next to the caret and its partner are highlighted from the
    syntax tree, and Jump to Matching Bracket (⌘⇧\) moves between them or to
    the enclosing closer. Typing an opener closes it by VS Code's rules,
    including `notIn` strings and comments from highlight captures; closers
    Knot inserted are stepped over and deleted in pairs, and openers wrap a
    selection. `editor.autoClosingBrackets` and `editor.autoSurround` turn
    them off, and packages keep `test/cases/brackets.jsonc` cases
    ([editor views](../architecture/workbench-and-lifecycle.md#editor-views),
    [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages),
    [decisions](../decisions/editing.md#brackets)).

- ✅ **D068 — Editor gutter and line numbers**
  - The gutter is a marker lane, always reserved, then line numbers sized to
    the line count. `editor.lineNumbers` takes `"on"`, `"off"`, or
    `"relative"`; the cursor line's number and, as in VS Code, its background
    are highlighted. Number clicks and drags select lines, and a line shows
    its most severe marker, whose command a click runs. Text frames opt in
    with `gutter: true`
    ([editor views](../architecture/workbench-and-lifecycle.md#editor-views),
    [decisions](../decisions/editing.md#editor-gutter),
    [extension guide](../extension-api.md#read-only-text-frames)).

- ✅ **D076 — Go to Line** (depends on D071)
  - The bundled `@knot/go-to-line` package, bound to ctrl-G in editors.
    Typing `N`, `N:C`, or `N,C` moves the caret there, centered when off
    screen, Enter keeps it, and dismissing restores the view. It added
    `context.editor` (a first slice of D109), buffer line lookups, and an
    input `onChange` with a message line
    ([editor views](../architecture/workbench-and-lifecycle.md#editor-views),
    [semantic UI](../architecture/semantic-ui.md#extension-transient-interactions),
    [extension guide](../extension-api.md#register-a-command)).

- **D077 — Status bar**
  - ✅ Complete in the editor API initiative.

- ✅ **D078 — Zoom, font size, and soft wrap** (depends on D070)
  - ⌘= / ⌘- / ⌘0 zoom the whole application, remembered across restarts;
    `font.size` sets the text size per view scope; `editor.softWrap`, on for
    Markdown and toggled per document by Toggle Soft Wrap, wraps at the
    viewport width, and movement goes by wrapped row with `-logical` twins
    ([zoom](../architecture/workbench-and-lifecycle.md#zoom),
    [editor views](../architecture/workbench-and-lifecycle.md#editor-views),
    [decisions](../decisions/editing.md#zoom-text-size-and-soft-wrap)).

- ✅ **D079 — Save hygiene** (depends on D070)
  - Save and Save As trim trailing whitespace (except on the cursor's line)
    and ensure a final newline, per `files.trimTrailingWhitespace` and
    `files.insertFinalNewline`. The cleanup is one undoable edit applied before
    the content snapshot is captured; Trim Trailing Whitespace trims on demand
    ([documents and persistence](../architecture/documents-and-persistence.md#saving),
    [settings](../settings.md#files)).
  - Whether a formatter hook belongs here is decided in its session; language
    servers (D067) remain out of scope.

- **D031 — Current-file Find** (depends on D064)
  - Bound to ⌘F. The query field takes focus. Matching is live and highlights
    every match. Enter and ⇧Enter (and ⌘G / ⌘⇧G) move between matches, and the
    current match follows edits. Esc closes the bar.
  - Use selection as the find string (⌘E).
  - Match highlighting uses a layer that D080 can reuse.
  - Replace and the advanced modes (D001) stay deferred. Revisit D001 in this
    item's session, because case and whole-word options are cheap once
    matching exists.

- **D080 — Word occurrence highlighting extension** (depends on D064, D031)
  - When the cursor rests on a word, a bundled JavaScript extension highlights
    every other occurrence in the buffer. It uses the syntax tree or word
    boundaries, debounced after cursor movement.
  - Requires a new public observation API for cursor and selection changes.
    Today only the debounced `viewStateChanged` window event exists. Size
    the API for other cursor-driven extensions and keep it off the painting
    path.
  - Highlights are anchored contributions naming a theme color key, so
    they follow edits without help from the extension.

## Phase 4 — Files and workspace

- ✅ **D008 — File watching and automatic reload**
  - Documents hold native watches, and extensions watch any URI through
    `files.watch`. A clean document reloads in place as a line diff and keeps
    its view state; a dirty one is marked changed on disk and offers Reload or
    Keep on focus; a deleted file leaves a dirty document. Activation rechecks
    every document
    ([architecture](../architecture/documents-and-persistence.md#detecting-external-changes),
    [decisions](../decisions/documents-and-persistence.md#file-watching),
    [settings](../settings.md)).

- **D010 — Filesystem mutation API**
  - Create file, create directory, rename or move, and delete (to the trash
    where the platform supports it), through providers and a public API.
  - Open documents follow renames, and deletion of their file is reported to
    them.

- **D087 — Extension entitlements**
  - Packages declare entitlements in `knot.jsonc`. Start with process
    spawning and outbound HTTP.
  - Who grants them:
    - An installed package prompts once, on its first load, and again when an
      update adds an entitlement. The grant is stored by canonical package
      identity.
    - Bundled packages declare entitlements through the same path but are
      granted by shipping with the binary.
    - Personal config is implicitly granted.
  - A missing or denied entitlement makes the API reject with a typed error
    instead of failing the package. Provide a way to review and revoke grants.
  - The APIs:
    - spawning a process with arguments, working directory, and environment,
      streaming stdout and stderr, writing stdin, observing exit, and killing
      the process (processes are cleaned up when the lifecycle tears down)
    - a `fetch`-shaped HTTP client
  - This provides consent and visibility, not a sandbox. Runtime isolation is
    not a security boundary, and the decision record must say so.
  - Decide how bundled consumers locate tools: bundle `rg` with Knot, or
    require it on PATH.

- ✅ **D122 — Extension file read, list, and stat API**
  - `knot.files` reads text, lists one directory, and stats a URI through the
    registered providers; reads are ungated and see stored content only
    ([architecture](../architecture/documents-and-persistence.md#uri-providers),
    [decisions](../decisions/documents-and-persistence.md#extension-file-reads)).

- ✅ **D143 — Input completion** (depends on D071)
  - Shipped ([architecture](../architecture/semantic-ui.md#extension-transient-interactions),
    [decisions](../decisions/semantic-ui.md#extension-transient-interactions)).
  - `workbench.showInput` offers completion items as the text changes,
    through the pulled input session `onChange` already uses, with keys to
    move through and accept them.
  - Matching is one native matcher shared by the picker, input completion,
    and extensions (`completion.match`), with the user-selectable styles
    `prefix`, `substring`, `flex`, and `orderless` in `completion.styles`.
  - The first consumer is Vim's Ex line (D058): it completes command names,
    `:set` options, and `:e` paths.

- **D089 — Project-local settings** (depends on D070, D122)
  - Settings read from a file in the repository, attached to folders rather
    than to projects, because a folder may belong to several projects. Add
    D070's `path` scope fact with its glob syntax and path normalization.
  - Decide the file's location and format (declarative data or code), and
    whether settings for one particular root set live in the
    `@knot/projects` record. Workspace trust (D124) stays deferred, so
    project settings must not run code until it exists.

- ✅ **D081 — Workspace ignore rules and file enumeration** (depends on D122,
  D143)
  - Shipped ([architecture](../architecture/documents-and-persistence.md#uri-providers),
    [decisions](../decisions/documents-and-persistence.md#workspace-ignore-policy),
    [settings](../settings.md)).
  - `files.list` hides and marks entries, `files.search` matches visible
    files, and `files.isIgnored` answers one-off checks, all through one native
    policy.
  - Vim's Ex line completes `:e` paths from the base `:e` opens against.

- **D034 / D053 — Explorer as a bundled extension** (depends on D008, D010, D122,
  D081, D072)
  - A bundled `@knot/explorer` package replaces the native explorer tool and
    its commands, using the contributed tree API.
  - It provides:
    - multi-root browsing
    - single click opens a preview tab, double click a regular one (D072)
    - new, rename, and delete actions
    - reveal the active file
    - refresh on watch events
    - filtering
  - Removing the native explorer also removes its private workspace-tree path.
    Any gap this exposes in the public tree API (such as context menus,
    keyboard actions on rows, or inline rename) is closed here.
  - It is an ordinary tool for tool-layout memory: it has a stable tool ID
    and a default placement.

- ✅ **D173 — Workspace file index** (depends on D008, D081)
  - Shipped ([architecture](../architecture/documents-and-persistence.md#workspace-file-index),
    [decisions](../decisions/documents-and-persistence.md#workspace-file-index)).
  - `files.list` and `files.isIgnored` answer from a native in-memory tree of
    each root, and `files.search` returns the best ranked matches and whether
    more matched; `files.walk` is removed.

- **D082 — Go to File** (depends on D071, D081, D173)
  - ✅ Done.
  - A bundled extension bound to ⌘P that fuzzy-matches workspace paths.
  - Multi-root paths are labeled by root, and open and recent files rank
    first.
  - Searches with `files.search` on every keystroke, one call for all roots.

- **D083 — Recent files** (depends on D071)
  - A bundled extension that records opened documents in extension storage
    and offers Open Recent….
  - Reopen closed tab (D072) is unrelated: it is native per-window state.

- **D084 — Project selector modal** (depends on D071)
  - `@knot/projects` replaces its Projects tool with a picker modal: recent
    projects first, then switch, new, rename, and delete as picker actions or
    companion commands.
  - Remove the Projects tool registration and its tree code.

- ✅ **D120 — Remembered projects** (depends on D070, D084)
  - Every opened root set is remembered as a project with an inferred name,
    opening a suspended project's folder restores it, and Open Folder opens
    a new window unless the current one has no roots.

- **D123 — Project inference extension** (depends on D122)
  - A bundled extension that finds the version-control root above an opened
    document outside the window's roots and offers to open it as a project,
    as Emacs's project.el infers projects. Decide which markers count
    (`.git` first), whether a window without roots switches without asking,
    and how often the offer may repeat for the same folder.

- **D085 — Cross-file search** (depends on D087, D081, D071, D072; promotes
  D007)
  - A bundled extension that wraps ripgrep through process spawning (D087)
    and parses its `--json` output. It passes the effective ignore rules
    (D081).
  - It provides:
    - a query input with case, regex, whole-word, and glob options
    - streamed results in a tree grouped by file (contributed tree groups
      and row actions already exist)
    - opening a result at its range, as a preview tab
    - cancelling a superseded search
    - refresh
  - Stale-target recovery after edits (D007) is decided here.

## Phase 5 — Keymap presets

- **D088 — VS Code, Zed, IntelliJ, and Sublime presets** (depends on D086 and
  the command set from earlier phases)
  - Map each editor's default bindings onto Knot's commands, per platform.
    Commands with no equivalent stay unbound. Add a test that every preset
    binding names a registered command.
  - Add a Select Keymap Preset picker. Decide whether first launch asks for a
    preset.
