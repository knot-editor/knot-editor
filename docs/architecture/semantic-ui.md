# Semantic UI and generated text

Part of the [architecture](../architecture.md). Design rationale is recorded in
[decisions](../decisions.md); deferred work lives in [roadmap](../roadmap.md).

## Boundary

Extension APIs carry native, surface-specific semantic data. gpui objects,
drawing callbacks, layout protocols, V8 values, and Rust objects do not cross
the public boundary. Native surfaces own layout and interaction; painting and
input never call JavaScript synchronously.

## Semantic extension UI

Editor contributions follow one foreground-owned path:

```text
extension replacement set
        |
typed host request
        |
revision, range, and lifecycle validation
        |
BufferModel source replacement
        |
stable anchors and model notification
```

The request envelope supplies source identity. Replacement, explicit disposal,
buffer closure, and extension teardown remove the complete source set.
Built-ins and extensions use the same application-owned contribution registry.
Editor contributions include anchored decorations, gutter markers, and
command-backed actions.

Views paint styled text by composing each line's runs from ordered highlight
layers of line-relative `(byte range, style)` spans; a later layer overrides an
earlier one's color and font and adds its underline. Syntax sources resolve
into nvim-treesitter's standard capture names by longest dotted prefix, and the
`Theme` global maps each capture to a style. A decoration or gutter marker
names a color key instead, so a query can never draw one.

A decoration also names one style, which picks its paint path. `underline`
and `wavy` are text-run layers, one per key and style in the model's paint
order, which puts `error`, `warning`, and `info` above other keys with the
most severe on top. `background` and `line` paint as quads behind the text:
a background covers the decorated text of each visual row and stops at the
text, while a line decoration bands the whole text area across every row of
the lines it covers, blank ones included, leaving out a final line its range
only reaches at column 0. Behind the text, the cursor-line highlight paints
first, then line bands, backgrounds in the same severity order, the
selection, and the bracket match, so the selection stays readable over match
highlights. Decorations belong to the buffer, so every view of it shows
them. Gutter
markers share one lane, which shows the most severe marker of each line, the
latest among equals, and runs that contribution's command when clicked
([editor views](workbench-and-lifecycle.md#editor-views)).

## Themes and color keys

Native surfaces and contributions paint with color keys rather than colors.
A color definition gives a key a description and a default per appearance,
either a color or another key; native keys are defined in one table beside the
code that paints them, because native surfaces paint before packages start and
when they fail, and extensions define more through `themes.defineColor`. A
registered theme sets values for any keys, including ones nobody has defined
yet, and styles capture names or their dotted prefixes.

`ApplicationThemes` owns lifecycle-owned registrations, where the latest
registration of a name wins, and extension color definitions. The
`theme.mode`, `theme.dark`, and `theme.light` settings select a theme, which
resolves into the `Theme` global: each defined key takes the theme's value,
else its default for the theme's appearance, following references. An
unregistered selection resolves the defaults alone, so the native defaults are
also the fallback when the theme package fails. Resolution happens once when
startup completes and again when the selection settings or, in `system` mode,
the system appearance change; registering or removing a theme later takes
effect at the next selection change rather than repainting mid-session.

gpui-component's widgets take their colors from the same resolution: each
theme application switches gpui-component to the theme's appearance, which
loads its built-in colors, then sets each field of the `COLORS` table in
`src/app/components.rs` from its native key. Fields without a fitting key
keep the built-in colors.

The bundled `@knot/themes` package registers the themes the native selection
defaults name and owns Select Theme. It keeps the settings it wrote in its own
storage and writes them again at start, as any settings UI does, so personal
`post-init` still overrides the choice.

## Syntax highlighting and languages

Highlight layers compose in a fixed order: syntax, then contribution
underlines; selection and decoration backgrounds paint beneath the text. Semantic tokens from a future
language client (D067) slot in above syntax. Syntax highlights are derived and
never stored as anchored ranges: each view queries its visible lines, with some
overscan, once per [syntax generation](core-and-buffers.md#syntax-state), and
painting, caret, hit testing, and IME geometry share that query, so bold and
italic captures keep geometry consistent. An injected layer paints over its
host, clipped to its included ranges.

Grammars are compiled in and named by stable native grammar IDs. Language
definitions are contributed data: extensions and personal config register an
LSP language ID, the grammar it parses with, the file names, extensions, and
shebang interpreters it claims, and its highlight, injection, and indent queries, through
the same `languages` API the bundled `@knot/language-*` packages use. An
association claims patterns for an ID without defining the language, so
personal config redirects detection without replacing a package's language.

```text
languages.register --> background query compilation --> LanguageRegistry
                                                            |
Open / Save As / restore / registration change --> detection
                                                            |
documents.setLanguage / Set Language Mode --> explicit ID   |
                                                            v
                                      BufferModel language --> SyntaxState
```

- `ApplicationLanguages` owns the registry and tracks every buffer with a
  language. Registrations and associations are lifecycle-owned and share one
  order: the latest registration defines an ID, and the latest registration or
  association claiming a file name, extension, or interpreter wins it, so
  removal reveals the previous one.
- Queries compile on the background executor at registration, so a bad query
  rejects with its location and a package's registrations compile in
  parallel; only the insertion runs on the foreground.
- Detection tries exact file names, then extensions, then the shebang
  interpreter. It runs when a document enters the collection and when
  persistence moves it to a new resource, never while typing, and never for a
  document whose language is explicit.
- A buffer keeps its ID even while nothing registers it and reports
  `plaintext` meanwhile, so personal config and restoration need no ordering
  against package startup. A registration change re-detects open documents and
  reattaches syntax wherever the resolved language changed; a language that
  injects reparses too, since its injections may resolve differently.
  Injections resolve by language ID, its lowercase, or a registered extension.
- Startup awaits every package entry before restoring windows, and the bundled
  package awaits its registrations in `start()`, so restored documents paint
  highlighted from their first frame.
- A registration's editing data, its comment tokens and bracket pairs, is
  resolved with its defaults at registration and travels with its compiled
  syntax language, so native editing code reads it without calling
  JavaScript. Editing commands take the data of the deepest syntax layer at
  the selection start, so a code fence or an injected `<script>` uses its own
  language's comments with no lookup by ID. Markdown's inline language, the
  layer inside every paragraph, therefore registers Markdown's block comment
  too. A buffer without syntax has no editing data. The line and block
  comment toggles are plain functions over text and that configuration, with
  no buffer or view.
  Each bundled language package keeps line and block comment toggling cases
  in `test/cases/comments.jsonc`, which a product test runs through the native
  toggles on the registered configuration.
- Bracket matching reads the syntax tree with the registered pairs and no
  extra query. A bracket is a leaf token whose text is a pair's opener or
  closer, in the deepest layer holding it, and its partner is the first
  sibling in its direction with the paired text, skipping balanced pairs,
  so brackets inside strings and comments, which belong to larger leaves,
  never match, and a parse error leaves a bracket unmatched. Whether the
  caret is inside a string or comment, which `notIn` reads, comes from the
  `string` and `comment` highlight captures of the layer holding the text
  before it, dotted children included, so no grammar needs extra data. A
  string holds only offsets strictly inside it; a comment also holds its end
  unless its last line ends with the language's block comment closer.
  Markdown's inline language shares Markdown's bracket pairs, since it is the
  language at the caret inside paragraphs. Packages keep typing, Backspace,
  wrapping, jump, and highlight cases in `test/cases/brackets.jsonc`, run through
  an editor view; a language with bracket pairs must have cases.
- An optional `indents` query compiles with the others and drives native
  indentation. The evaluator follows nvim-treesitter's indent module over the
  tree of the deepest layer at a line's first non-blank byte, so a code fence
  indents as its language, and a layer without a query leaves its lines
  alone rather than falling back to its host's. It returns columns, or that
  a line keeps its indentation (inside an `@indent.ignore` node, such as a
  string) or moves with an earlier line (inside an `@indent.auto` node, such
  as a block comment). Within one reindent each line sees the indentation
  given to the lines above it. Inside a parse error, each opening bracket of
  the language's bracket pairs that the error leaves unclosed before the line
  indents it one level, at most once per row. Packages keep Enter, typing,
  and reindent cases in `test/cases/indents.jsonc`, which a product test runs
  through an editor view; a language with an indent query must have cases.
- An optional `folds` query lists the nodes whose lines can fold, as `@fold`
  captures, over every layer's tree. A multi-line node is a region from its
  first line through its last; a closing bracket that ends the node and
  starts the line stays visible, an indented block after a colon starts on
  the line of its header, and trailing blank lines are left out. A language
  without a query folds by indentation. The regions are candidates that
  Fold, Unfold, and Fold All choose among, cached per text revision and
  syntax generation on the buffer model; only collapsed folds are view
  state. Packages keep region cases in `test/cases/folds.jsonc`, which a
  product test runs through an editor view; a language with a folds query
  must have cases.
- The native Set Language Mode command offers the registered languages, or Auto
  Detect, in the window's transient picker. Contributed text frames take a
  fixed language at registration.

Tree providers use the reverse path:

```text
extension registration/invalidation
        |
native TreeView generation
        |
reverse runtime command
        |
extension getChildren callback
        |
generation and lifecycle validation
        |
native cached presentation
```

Completion providers use the same asynchronous reverse-runtime direction but
remain completion-specific: the shell snapshots ordered registrations, the
view-owned controller tracks one-shot provider states and merge policy, and a
native surface consumes semantic snapshots. Provider objects and request
machinery never enter surfaces, buffers, or `core`.

## Completion ownership and aggregation

The shell-owned `CompletionProviderRegistry` assigns monotonic registration
identities and order to lifecycle-owned, shell-wide providers. An editor
completion invocation snapshots registry membership, buffer identity and
revision, cursor and prefix range, and a view-local generation before the shell
fans requests out to independent extension runtimes.

```text
CompletionProviderRegistry snapshot
                 |
                 v
Editor -> CompletionController -> independent extension runtimes
   |                    ^                         |
   |                    +--- validated results ---+
   |                              |
   +----- native list surface <----+
```

Responses reach a controller only while the registration and lifecycle remain
live and weak editor identity, active generation, buffer handle, and public
revision still match. The controller merges partial results deterministically
and assigns stable semantic item identities. The native list surface presents
the current immutable snapshot and tracks selection by item identity, so
presentation updates never restart provider work. Acceptance
resolves the selected identity through the controller and performs one
revision-checked prefix replacement.

- Completion composition is feature-specific and independent of arrival timing.
  Filtering, ranking, and deduplication rules live in the
  [completion controller](../../crates/knot-model/src/completion.rs).
- Recoverable failure is local to one provider and request. Other candidates
  remain visible, and the provider participates again on the next invocation
  without re-registration.

Applicability, automatic triggers, streaming, active cancellation of superseded
requests, timeouts, backpressure, richer edits, and popup polish remain deferred.

## Icons

An icon is a flat dotted name (`file`, `folder.open`, `file.rust`,
`action.refresh`); contributions name icons and never carry glyphs.
`IconRegistry` in `knot-model` resolves a name to a definition, either SVG text
or a Lucide glyph name, with a color that is a theme color key, the SVG's own
colors (`original`, SVG only), or the surface's icon color. For one name the
first of these wins: an override (`icons.override`, latest wins, disposal
reveals the previous), the pack selected by the `icons.pack` setting, the
defaults, then the generic fallback icon. Defaults are the native table plus
names extensions `define`; defining a name that already has a default is a
`DuplicateIcon` error, and an unknown name paints the fallback rather than
failing. A pack (`icons.registerPack`) maps names to definitions and falls back
per name to the defaults, so it can restyle any package's icons.

The native table (`file`, `folder`, `folder.open`, `symbol`, `terminal`,
`tool.knot.explorer`, `tree.collapsed`, `tree.expanded`, `fallback`) lives in Rust, as native color-key defaults do,
because native surfaces paint before packages start and when they fail. Lucide
glyphs come from `gpui-kit-assets`, a dependency of gpui-component under the
ISC licence; Knot reads the SVG bytes and registers no asset source, and the
host rejects an unknown Lucide name when a definition registers. Registrations,
selection, and lifecycle teardown repaint every window, and icons register
during activation, before startup restores windows, so load order does not
change the first frame.

A tool's stripe button paints the icon named `tool.` plus the tool's ID, and
shows the title's initial when nothing defines it; the terminal button paints
`terminal`.

File icons resolve apart from the definitions. A file tries the name
`icons.associate` claims for its file name, then for its extension (longest
first, case-insensitive), then `file.<languageId>` for the detected language,
then `file`; a folder tries its claimed name and then `folder`, using the
`.open` variant of each when expanded. The first candidate that has a
definition at any level wins, so a pack that lacks an associated name never
leaves a file without an icon. Associations are global rather than per pack,
and the latest claim of a pattern wins. Each bundled `language-*` package
defines `file.<id>` with its own color key.

`AppIcon` paints a definition at the surrounding text size, so zoom needs no
extra work. Tinted icons go through gpui-component's `Icon`, which gpui keys by
the SVG bytes, so replacing a definition invalidates the cache without an
asset source; full-color icons go through gpui's `img()` with an SVG image.
Surfaces: tree rows and disclosure icons, document tabs and their drag labels,
tree actions, and status items.

## Extension trees

`TreeView` owns cached semantic items, expansion, selection, focus, scroll, and
per-parent loading/error generations. It renders and handles input from cached
foreground state only.
Each product window owns at most one instance of a contributed view. The
foreground semantic bridge retains weak references to live instances, accepts
view registration before windows open, and routes invalidations and responses
to each instance. Its generation rejects responses from a replaced request.
Clicking an item focuses its tree before emitting its command. The product
shell routes that event after the tree update through the captured window
command path.

Items are ordinary content or `group` headers. A group is neither activatable
nor disclosable: its children are fetched when it first appears and remain
visible, so a grouped list is one provider tree rather than a second model.
Items expose optional right-aligned status text and a badge dot in a named
color key. Items name an [icon](#icons).
Actions are labeled commands; an action with an `icon` renders as an icon
button that shows its label as a tooltip.

Frame, group, and row actions are semantic command references with JSON
arguments, never drawing callbacks. The native view renders them as controls
and emits the same captured-window command event as activation, with the action
arguments instead of null; the event names the session and the tree, which
must still have focus there when it runs. Action clicks stop propagation so
they cannot also activate the row. The same actions are reachable from the
keyboard by entering
an action-focus mode on the current row; keyboard navigation indexes the
rendered rows directly so scrolling stays aligned when a transient loading or
error row is present. A root invalidation keeps stale rows visible while the
refetch is pending and reloads group children when the refreshed result arrives,
so selection and navigation stay usable during a refresh.

A row's actions also form its context menu, a gpui-component popup menu that
the tree owns. Right-clicking a row selects it and opens the menu there;
`tree.show-context-menu` (Shift+F10) opens it below the selected row, needs a
window, and is unavailable outside a focused extension tree or for a row
without actions. Items emit the same captured-window command event as the
row's controls. The menu takes focus, receives the keys the keymap leaves
([keymaps](commands.md#keymaps)), closes when dismissed or when it loses
focus, and returns focus to the tree on dismissal.

Contributed text frames are a separate content kind. Each instance owns a
private read-only `BufferModel` rendered by the ordinary `EditorView`; it is not
registered with the document collection and has no resource or persistence.
The extension replaces the whole text through a foreground request, and the
bridge keeps the last text so a window created later presents current content.
A frame's editor has no gutter unless its registration asks for one.
Wrapping the editor surface is provisional; the [roadmap](../roadmap.md) tracks a
dedicated read-only presentation (D054).

Semantic tree state retains accessibility information. gpui exposes an
AccessKit bridge, but Knot's surfaces do not publish to it yet (D030 in the
[roadmap](../roadmap.md)).

Provider disposal follows the shared
[host lifecycle](extension-host.md#disposal-cancellation-and-teardown). Native
unregistration cancels retained requests across live surfaces. Completion removal
also removes that provider's already-completed aggregate results; other providers
retain their state. Tree invalidation acknowledges native acceptance separately
from later fetch/paint. Invalid provider results fail only their request.

## Extension transient interactions

Pickers and text prompts are window-local overlays owned by the product shell,
not by the extension runtime. The host operation resolves the originating
window from an explicit target or the admitted command capture, installs the
overlay on the foreground, and awaits its settled outcome. A window owns at most
one extension interaction; the palette is a separate native surface and may be
open at the same time. Native confirmation, message, and directory prompts occupy
the same slot without a rendered overlay; the folder picker normalizes and stats
its selected path through providers before delivering a URI.

```text
extension host request (showPicker/showInput/...)
        |
        v
window resolution (explicit target or command capture)
        |
        v
shell-owned overlay + shared TextInput
        |
        v
one settled outcome (selection, acceptance, dismissal, cancellation)
```

`TextInput` is one shared single-line gpui `EntityInputHandler`: typing, IME
preedit, and clipboard flow through gpui, while the field owns caret, selection,
keyboard movement, and pointer selection. It backs both transient fields and
the command palette query, so text entry has one native path. Trees, pickers,
prompts, and the field handle keys the keymap leaves unclaimed in their own
state, which the session calls whether or not a window shows it (see
[keymaps](commands.md#keymaps)): the field's keys first, then the picker's or
prompt's.

One native `Picker` serves the command palette, native pickers, and extension
pickers. It matches the supplied item list locally through the shared
`knot_model::matching` matcher, ranking label matches above description
matches above an item's hidden match text, and keeps the caller's order for an
empty query and equal scores; no provider, scheduler, or event bus is added. An extension
picker with `onHighlight` or `onQuery` reports the highlighted item and the
typed query through a pull loop in the caller's own turn, because package
roots are serial and a notification could not run while the command awaits the
picker:

```text
showPicker({ onHighlight, onQuery })
  beginPicker ──> highlighted(session, id) | queried(session, query) ─┐
  callback awaited                                                    │ repeat
  nextPickerEvent(session, rows?) ──> highlighted | queried ──────────┘
                                  └─> selected | dismissed
```

With `onQuery` the picker is created without rows and does no matching: each
pull of a query is answered by the rows the next pull carries, which the
picker shows in the order given with the supplied ranges, and a query is
answered before the highlight it replaces. Rows apply only while the field
still holds the query last delivered; otherwise they are dropped. Enter on
rows that do not answer the field waits for the rows that do, freezing the
field meanwhile, and then confirms the highlighted row.

The matcher serves every list that filters by a typed query. Its styles
(`prefix`, `substring`, `flex`, `orderless`) are the user's ordered
`completion.styles` setting: a candidate matches if any listed style matches
it, ordered by the first style that matches, then that style's score, then the
caller's order. It is native because the picker filters on every keystroke and
native painting never calls JavaScript synchronously; extensions reach the same
matcher through `completion.match`, with UTF-16 ranges, and pass the ranges back
as an input completion's `matches` for highlighting.

Native code keeps only the latest unread highlight, so a slow callback skips
intermediate items, and a settled picker wins over an unread highlight.
An input with `onChange` follows its typed value through the same session:
`beginInput` and `nextInputEvent` pull `changed(session, value)` until the
input is accepted or dismissed, and each pull carries the message the
callback returned for the previous value, which the input shows as a muted
line under its field, with the completion list it may return. Feedback
applies only while the field still holds the value last delivered to the
caller; otherwise it is dropped and the next pull delivers the current value.
Tab accepts the highlighted completion by replacing the whole field with its
value, which runs `onChange` again; Enter always submits the field text.
An input with a `prompt` shows that fixed text before its field, outside its
value, and dismisses on Backspace in an empty field, as a command line does.
Unloading the lifecycle drops its sessions with its transients.

Settlement happens once on acceptance, dismissal, native-prompt answer, window
closure, extension unload, or caller cancellation. Window closure and unload
release the overlay and settle it as cancellation; no remembered layout or
extension state is touched. Focus returns to the previously focused surface
when the overlay closes and that surface is still live.

## Generated text

Generated output that benefits from ordinary selection, scrolling, and copying
uses a `BufferModel` rendered by `EditorView` rather than a second presentation
model. Contributed [text frames](#extension-trees) are the current generated
surface; their private buffers stay outside document identity and persistence.
Feature controllers keep semantic identities beside the buffer instead of
recovering them from displayed text, so `core` and `BufferModel` carry no
feature semantics. Every application document is a user document with
persistence state; interactive generated views remain D027.
