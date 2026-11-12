# Daily-driver editor

Status: 🚧 approved, in progress. Promotes D007, D008, D010, D031, D034,
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

## Phase 2 — Foundations

- ✅ **D070 — Application settings**
- ✅ **D121 — Terminals start in the workspace root**
- ✅ **D127 — Restored terminals keep their working directory**
- ✅ **D064 — Theming system**
- ✅ **D071 — Command palette and shared picker**
- ✅ **D086 — Keymap presets**
- ✅ **D072 — Workbench navigation and layout commands**

## Phase 3 — Editing

- ✅ **D073 — Language editing configuration**
- ✅ **D074 — Editing commands**
- ✅ **D134 — Tree-sitter indentation**
- ✅ **D075 — Bracket matching and auto-closing**
- ✅ **D068 — Editor gutter and line numbers**
- ✅ **D076 — Go to Line**
- ✅ **D077 — Status bar**
- ✅ **D078 — Zoom, font size, and soft wrap**
- ✅ **D079 — Save hygiene**
- ✅ **D031 — Current-file Find**

- **D080 — Word occurrence highlighting extension** (depends on D064, D031)
  - 🚧 [Plan](d080-word-highlight.md)
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
- ✅ **D010 — Filesystem mutation API**

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
- ✅ **D143 — Input completion**

- **D089 — Project-local settings** (depends on D070, D122)
  - Settings read from a file in the repository, attached to folders rather
    than to projects, because a folder may belong to several projects. Add
    D070's `path` scope fact with its glob syntax and path normalization.
  - Decide the file's location and format (declarative data or code), and
    whether settings for one particular root set live in the
    `@knot/projects` record. Workspace trust (D124) stays deferred, so
    project settings must not run code until it exists.

- ✅ **D081 — Workspace ignore rules and file enumeration**
- ✅ **D034 — Explorer as a bundled extension**
- ✅ **D173 — Workspace file index**
- ✅ **D082 — Go to File**
- ✅ **D083 — Recent files**

- **D084 — Project selector modal** (depends on D071)
  - `@knot/projects` replaces its Projects tool with a picker modal: recent
    projects first, then switch, new, rename, and delete as picker actions or
    companion commands.
  - Remove the Projects tool registration and its tree code.

- ✅ **D120 — Remembered projects**

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
