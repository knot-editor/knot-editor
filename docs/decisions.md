# Architecture Decisions

Validated choices, why they were made, the alternatives they rejected, and the
evidence that decided them. [Architecture](architecture.md) and its subsystem
references own current behavior; API contracts live in code.
[Roadmap](roadmap.md) and [active plans](plans/) own deferred and proposed work.
The [documentation guide](README.md#decisions) sets the entry rules.

Each line names a decision and its gist. Open only the topic files a task
touches.

## [Platform and UI](decisions/platform.md)

- [Platform and UI](decisions/platform.md#platform-and-ui): gpui through public
  APIs; macOS first, with Windows and Linux smoke-checked.
- [Crate boundaries](decisions/platform.md#crate-boundaries): a compiler-enforced
  gpui-free model, bundled packages in group crates injected into the library.
- [gpui snapshots and gpui-component](decisions/platform.md#gpui-snapshots-and-gpui-component):
  pinned `gpui-pre` snapshots with gpui-component for presentational leaf
  widgets only; the keymap matches first and the Knot theme paints them.
- [Views and extension UI](decisions/platform.md#views-and-extension-ui):
  extensions supply semantic data, never rendering callbacks; movement uses
  display cells, so views run headless.
- [Product window chrome and failure notices](decisions/platform.md#product-window-chrome-and-failure-notices):
  only product surfaces, with a toolbar of package-contributed widgets rather than
  fixed commands; the Islands look is theme values, not a layout mode; silent
  native failures raise notices, and prompts only ask for a decision.
- [Command-line launcher](decisions/platform.md#command-line-launcher): the
  editor owns one socket for every build; macOS cold start through `open -a`,
  not a fork; paths open in their project's window.
- [Application log](decisions/platform.md#application-log): `tracing` with a
  rotating file instead of an in-memory viewer; `knot::config` marks warnings
  the user can fix, and each site raises its own notice.
- [Nightly distribution](decisions/platform.md#nightly-distribution):
  unsigned prereleases through a versioned cask in a separate tap; one build
  identity; Apple Silicon only.

## [Core and buffers](decisions/core-and-buffers.md)

- [Text buffer and stable positions](decisions/core-and-buffers.md#text-buffer-and-stable-positions):
  a hand-built stable-ID piece tree with per-backing line indexes and
  piece locators.
- [Anchored ranges and contributions](decisions/core-and-buffers.md#anchored-ranges-and-contributions):
  geometry in core, feature policy in the application.
- [Reversible edits](decisions/core-and-buffers.md#reversible-edits): undo
  through ordinary mutations on shared model history; content identity apart
  from revisions.
- [Line diff for extensions](decisions/core-and-buffers.md#line-diff-for-extensions):
  one native, stateless diff over strings or buffers; no text handles or
  per-buffer method.

## [Editing](decisions/editing.md)

- [Language editing configuration](decisions/editing.md#language-editing-configuration):
  comments and brackets on the language registration; one bracket list.
- [Indentation](decisions/editing.md#indentation): nvim-treesitter indent
  queries, evaluated after the commit on the reparsed tree.
- [Brackets](decisions/editing.md#brackets): native matching and auto-closing
  inside the opener's undo step; matching within one syntax node.
- [Editor gutter](decisions/editing.md#editor-gutter): fixed native lanes; a marker lane and
  a bar lane, both always reserved; native `diff.*` color keys.
- [Editing commands](decisions/editing.md#editing-commands): VS Code behavior
  and bindings; detected indentation becomes a document-scoped setting.
- [Zoom, text size, and soft wrap](decisions/editing.md#zoom-text-size-and-soft-wrap):
  one application zoom level; wrapping by monospace cells; row and logical-line
  movement.
- [Typography](decisions/editing.md#typography): `font.*` settings per scope;
  line height as a multiple of the font's height; terminal ligatures only for
  punctuation runs.
- [Folding](decisions/editing.md#folding): per-view anchored folds as zero-row
  lines in the wrap map.

## [Semantic UI](decisions/semantic-ui.md)

- [Syntax highlighting](decisions/semantic-ui.md#syntax-highlighting): native
  tree-sitter over compiled-in grammars; languages contributed as data.
- [Themes](decisions/semantic-ui.md#themes): registered contributions, native
  color-key defaults, selection through settings.
- [Decoration styles](decisions/semantic-ui.md#decoration-styles): a color key
  plus exactly one required style; backgrounds as quads.
- [View-owned editor decorations](decisions/semantic-ui.md#view-owned-editor-decorations):
  sets per editor view and lifecycle; buffer-wide sets keyed by view ID
  rejected.
- [Icons](decisions/semantic-ui.md#icons): named icons in override, pack,
  default, fallback layers; associations apart from packs; native Lucide
  defaults; live application; file and folder references resolved natively.
- [Extension tree content and actions](decisions/semantic-ui.md#extension-tree-content-and-actions):
  one frame registration; text frames reuse the editor surface; general
  gesture, reveal, speed search, and inline editing rather than explorer-only;
  drops run a command on rows only.
- [Resource decorations](decisions/semantic-ui.md#resource-decorations): push
  sources instead of a pull provider; native painting on any row or tab with a
  resource; icon or text badges, none on tabs; selection keeps its foreground.
- [Extension transient interactions](decisions/semantic-ui.md#extension-transient-interactions):
  typed results, complete lists filtered natively or rows supplied per query,
  pushed callbacks; whole-value completion items with menu accept; one native matcher
  with user-ordered styles; recency owned by extensions.
- [Editor bars](decisions/semantic-ui.md#editor-bars): non-modal data-described
  strips rather than the modal input; native matching and replacement, Find in
  a bundled package, Replace as a second bar.
- [Editor popovers](decisions/semantic-ui.md#editor-popovers): floating
  data-described content with the composite layout, several per editor,
  children of their editor in the view tree, zoom-scaled pixels, closed by every
  external edit; Escape ahead of the keymap.
- [Status bar](decisions/semantic-ui.md#status-bar): native bar, extension
  items with per-window values.
- [Generated text surfaces](decisions/semantic-ui.md#generated-text-surfaces):
  ordinary buffers and views; documents have no generated state.
- [Capability aggregation](decisions/semantic-ui.md#capability-aggregation):
  feature-specific and view-owned merging.

## [Commands and settings](decisions/commands-and-settings.md)

- [Commands and keymaps](decisions/commands-and-settings.md#commands-and-keymaps):
  one command namespace, Knot-side key matching, latest-write-wins keymaps.
- [Application settings](decisions/commands-and-settings.md#application-settings):
  Emacs-like variables in one store rebuilt at launch; scopes as fact sets.
- [Menu items](decisions/commands-and-settings.md#menu-items): menus are
  registrations scoped like keybindings; they act on the clicked surface
  without focusing it; the menu bar stays native and contributions join it.

## [Extension host](decisions/extension-host.md)

- [Extension runtime](decisions/extension-host.md#extension-runtime): direct V8,
  persistent isolates, a bounded worker pool, interleaved tasks with
  continuation-carried attribution, external UTF-16 snapshots.
- [Extension timers](decisions/extension-host.md#extension-timers): Node
  semantics in a host-owned table; a required driver sleeps and calls back.
- [Handler declarations](decisions/extension-host.md#handler-declarations):
  annotated async handlers generate adapters; no registration tables.
- [Installed extension directories](decisions/extension-host.md#installed-extension-directories):
  inspectable built JavaScript with a JSONC manifest; isolates never share
  modules.
- [Bundled extension packages](decisions/extension-host.md#bundled-extension-packages):
  compiled into the binary under the reserved `@knot` scope.
- [Extension authoring and tests](decisions/extension-host.md#extension-authoring-and-tests):
  TypeScript per file in `build.rs`; behavior cases without a native window,
  on a fixture language and a reused editor; cheap per-test products, not
  shared ones.
- [Editor handles](decisions/extension-host.md#editor-handles): bound to one tab
  presentation; coalesced notifications carrying the current selection; one-request reads
  of the caret's lines; open/close undo groups.
- [Extension entitlements](decisions/extension-host.md#extension-entitlements):
  consent, not a sandbox; coarse declarations asked at startup; denial means
  not loaded; native grant management.
- [Extension restart](decisions/extension-host.md#extension-restart): a minimal
  escape hatch reusing terminating unload.
- [Personal configuration](decisions/extension-host.md#personal-configuration):
  one config directory with pre- and post-init modules; failure is fatal.
- [Extension contracts](decisions/extension-host.md#extension-contracts): typed
  schema contracts imported as modules; host-side validation of both sides.
- [Application observation](decisions/extension-host.md#application-observation):
  atomic registration, snapshot, and watermark; no second catalog.
- [Extension record storage](decisions/extension-host.md#extension-record-storage):
  one SQLite database per package identity; JSON records as the atomic unit.

## [Bundled packages](decisions/bundled-packages.md)

- [Go to Line and the editor handle](decisions/bundled-packages.md#go-to-line-and-the-editor-handle):
  a bundled package that proves the editor API.
- [Search Everywhere](decisions/bundled-packages.md#search-everywhere): one
  tabbed picker for files, buffers, and commands; scope by tab, not prefix; a
  minimal native palette as the fallback.
- [Bundled Vim mode](decisions/bundled-packages.md#bundled-vim-mode): public
  APIs only, a JavaScript state machine over typed text; `.` and macros replay
  editor recordings; search and `:s` on the native regex find and replace,
  Vim patterns translated to its syntax; Ex commands over generic document and
  file primitives.
- [Bundled explorer](decisions/bundled-packages.md#bundled-explorer): public
  API only, every gap closed generally; the taken item stays in the extension.
- [Bundled project registry](decisions/bundled-packages.md#bundled-project-registry):
  `@knot/projects` owns records; native code owns workspaces and roots; every
  opened folder set is remembered.
- [Project sessions](decisions/bundled-packages.md#project-sessions): active or
  suspended; durable suspension before switching; hand-opened folders restore
  through the switch.
- [Project inference](decisions/bundled-packages.md#project-inference): the
  nearest `.git` ancestor, adopted without asking only by a rootless window
  opened for the file; known projects open through the switch.
- [Version control backends](decisions/bundled-packages.md#version-control-backends):
  a contract per backend, defined by a factory in `@knot/vcs`, plus one the
  backends call.
- [Version control status](decisions/bundled-packages.md#version-control-status):
  against the baseline, not the index; disk only; a snapshot painted by
  `@knot/vcs`, which sets folder colors itself; untracked files listed one by
  one up to a limit.
- [Hunk popup](decisions/bundled-packages.md#hunk-popup): the baseline's lines
  only, toolbar-only for an insertion, anchored to the hunk; "hunk" names
  everywhere.
- [Git baseline](decisions/bundled-packages.md#git-baseline): HEAD through
  `cat-file --filters`, not the index; roots derived from URIs, not Git's
  absolute paths; watching, not polling, with a hidden-entry opt-in.

## [Workbench and lifecycle](decisions/workbench-and-lifecycle.md)

- [Workbench layout and view lifetime](decisions/workbench-and-lifecycle.md#workbench-layout-and-view-lifetime):
  a center and six slots of tool windows as split trees, protected closure,
  terminals in a container, stripes that replace within a slot.
- [Workbench navigation](decisions/workbench-and-lifecycle.md#workbench-navigation):
  a command for every operation; momentary state is never recorded.
- [Terminal](decisions/workbench-and-lifecycle.md#terminal): libghostty-vt from
  prebuilt static archives; sessions outlive their single attachment; an explicit name beats a program's
  title; user
  preferences as new-session defaults, scrollback deferred for engine support.
- [Composite views and the diff package](decisions/workbench-and-lifecycle.md#composite-views-and-the-diff-package):
  views lay out their own contents as data with identified children; the diff is
  a package on public parts; editability follows content; IntelliJ-style
  alignment without filler rows; inline as block rows.
- [Window layouts and restoration by position](decisions/workbench-and-lifecycle.md#window-layouts-and-restoration-by-position):
  one versioned JSON layout for hot exit and project sessions.
- [Session replacement contract](decisions/workbench-and-lifecycle.md#session-replacement-contract):
  staged reads and one revalidated protected commit.
- [Native window hot exit and launch precedence](decisions/workbench-and-lifecycle.md#native-window-hot-exit-and-launch-precedence):
  the application restores windows before launch requests.
- [Application state database](decisions/workbench-and-lifecycle.md#application-state-database):
  one SQLite file for UI state, separate from recovery.

## [Documents and persistence](decisions/documents-and-persistence.md)

- [URI workspaces and filesystem providers](decisions/documents-and-persistence.md#uri-workspaces-and-filesystem-providers):
  normalized URIs over an async byte-oriented provider boundary.
- [Multi-root workspaces](decisions/documents-and-persistence.md#multi-root-workspaces):
  an ordered root list, replaced whole; nested roots allowed.
- [Application document and root identity](decisions/documents-and-persistence.md#application-document-and-root-identity):
  document UUIDs and generational root identities.
- [Semantic resource operations](decisions/documents-and-persistence.md#semantic-resource-operations):
  shared product operations that return committed semantic data.
- [Folder selection](decisions/documents-and-persistence.md#folder-selection):
  `workspaces.pickFolder` returns a checked directory without committing roots.
- [Extension file reads](decisions/documents-and-persistence.md#extension-file-reads):
  ungated, text-only `files` reads over any provider; no bytes or list options.
- [Extension file mutation](decisions/documents-and-persistence.md#extension-file-mutation):
  no-clobber with parents created, trash by default, rename-following only for
  Knot-issued renames; no prompts or closing.
- [Workspace ignore policy](decisions/documents-and-persistence.md#workspace-ignore-policy):
  one native, setting-driven policy for hidden and ignored entries; one
  classifier for walks rather than `rg --files` or a `WalkBuilder` fast path;
  no rule registration.
- [Workspace file index](decisions/documents-and-persistence.md#workspace-file-index):
  a native tree per root shared by root URI; queries wait for the first scan;
  bounded native search with tiered ranking instead of a full file list; no
  disk fallback, lazy ignored directories, or saved snapshots.
- [File watching](decisions/documents-and-persistence.md#file-watching):
  watches requested by documents and extensions, plus the file index's root watch; one scan once the
  watch is live; version comparison
  rather than a save window; a line-diff reload; a prompt on focus.
- [Unsaved-document recovery store](decisions/documents-and-persistence.md#unsaved-document-recovery-store):
  an application-owned database independent of extension health.
- [Unsaved-document restore and hot exit](decisions/documents-and-persistence.md#unsaved-document-restore-and-hot-exit):
  recovered text applied as one edit; quit is a hot exit.
- [Retained recovery captures](decisions/documents-and-persistence.md#retained-recovery-captures):
  application-stamped owners and immutable captures.
