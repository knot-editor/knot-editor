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
  only product surfaces; silent native failures use the native warning prompt.

## [Core and buffers](decisions/core-and-buffers.md)

- [Text buffer and stable positions](decisions/core-and-buffers.md#text-buffer-and-stable-positions):
  a hand-built stable-ID piece table with an eager line index.
- [Anchored ranges and contributions](decisions/core-and-buffers.md#anchored-ranges-and-contributions):
  geometry in core, feature policy in the application.
- [Reversible edits](decisions/core-and-buffers.md#reversible-edits): undo
  through ordinary mutations on shared model history; content identity apart
  from revisions.

## [Editing](decisions/editing.md)

- [Language editing configuration](decisions/editing.md#language-editing-configuration):
  comments and brackets on the language registration; one bracket list.
- [Indentation](decisions/editing.md#indentation): nvim-treesitter indent
  queries, evaluated after the commit on the reparsed tree.
- [Brackets](decisions/editing.md#brackets): native matching and auto-closing
  inside the opener's undo step; matching within one syntax node.
- [Editor gutter](decisions/editing.md#editor-gutter): fixed native lanes and
  one shared, always-reserved marker lane.
- [Editing commands](decisions/editing.md#editing-commands): VS Code behavior
  and bindings; detected indentation becomes a document-scoped setting.
- [Zoom, text size, and soft wrap](decisions/editing.md#zoom-text-size-and-soft-wrap):
  one application zoom level; wrapping by monospace cells; row and logical-line
  movement.
- [Folding](decisions/editing.md#folding): per-view anchored folds as zero-row
  lines in the wrap map.

## [Semantic UI](decisions/semantic-ui.md)

- [Syntax highlighting](decisions/semantic-ui.md#syntax-highlighting): native
  tree-sitter over compiled-in grammars; languages contributed as data.
- [Themes](decisions/semantic-ui.md#themes): registered contributions, native
  color-key defaults, selection through settings.
- [Decoration styles](decisions/semantic-ui.md#decoration-styles): a color key
  plus exactly one required style; backgrounds as quads.
- [Icons](decisions/semantic-ui.md#icons): named icons in override, pack,
  default, fallback layers; associations apart from packs; native Lucide
  defaults; live application.
- [Extension tree content and actions](decisions/semantic-ui.md#extension-tree-content-and-actions):
  one frame registration; text frames reuse the editor surface.
- [Extension transient interactions](decisions/semantic-ui.md#extension-transient-interactions):
  typed results, complete lists filtered natively, pulled rather than pushed
  updates.
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

## [Extension host](decisions/extension-host.md)

- [Extension runtime](decisions/extension-host.md#extension-runtime): direct V8,
  persistent isolates, a bounded worker pool, external UTF-16 snapshots.
- [Handler declarations](decisions/extension-host.md#handler-declarations):
  annotated async handlers generate adapters; no registration tables.
- [Installed extension directories](decisions/extension-host.md#installed-extension-directories):
  inspectable built JavaScript with a JSONC manifest; isolates never share
  modules.
- [Bundled extension packages](decisions/extension-host.md#bundled-extension-packages):
  compiled into the binary under the reserved `@knot` scope.
- [Extension authoring and tests](decisions/extension-host.md#extension-authoring-and-tests):
  TypeScript per file in `build.rs`; behavior cases in a windowless session.
- [Editor handles](decisions/extension-host.md#editor-handles): bound to one tab
  presentation; payload-free coalesced notifications; open/close undo groups.
- [Extension restart](decisions/extension-host.md#extension-restart): a minimal
  escape hatch reusing terminating unload.
- [Personal configuration](decisions/extension-host.md#personal-configuration):
  one config directory with pre- and post-init modules; failure is fatal.
- [Application observation](decisions/extension-host.md#application-observation):
  atomic registration, snapshot, and watermark; no second catalog.
- [Extension record storage](decisions/extension-host.md#extension-record-storage):
  one SQLite database per package identity; JSON records as the atomic unit.

## [Bundled packages](decisions/bundled-packages.md)

- [Go to Line and the editor handle](decisions/bundled-packages.md#go-to-line-and-the-editor-handle):
  a bundled package that proves the editor API.
- [Bundled Vim mode](decisions/bundled-packages.md#bundled-vim-mode): public
  APIs only, a JavaScript state machine over typed text.
- [Bundled project registry](decisions/bundled-packages.md#bundled-project-registry):
  `@knot/projects` owns records; native code owns workspaces and roots.
- [Project sessions](decisions/bundled-packages.md#project-sessions): active or
  suspended; durable suspension before switching.

## [Workbench and lifecycle](decisions/workbench-and-lifecycle.md)

- [Workbench layout and view lifetime](decisions/workbench-and-lifecycle.md#workbench-layout-and-view-lifetime):
  a center and three dock areas of split trees, protected closure, tools as
  tabs, stripes that replace.
- [Workbench navigation](decisions/workbench-and-lifecycle.md#workbench-navigation):
  a command for every operation; momentary state is never recorded.
- [Terminal](decisions/workbench-and-lifecycle.md#terminal): `alacritty_terminal`
  sessions that outlive their single attachment.
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
- [Unsaved-document recovery store](decisions/documents-and-persistence.md#unsaved-document-recovery-store):
  an application-owned database independent of extension health.
- [Unsaved-document restore and hot exit](decisions/documents-and-persistence.md#unsaved-document-restore-and-hot-exit):
  recovered text applied as one edit; quit is a hot exit.
- [Retained recovery captures](decisions/documents-and-persistence.md#retained-recovery-captures):
  application-stamped owners and immutable captures.
