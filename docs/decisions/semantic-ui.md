# Semantic UI decisions

Syntax, themes, decorations, and the native surfaces that extensions fill with
data. The [decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Syntax highlighting

Parse natively with tree-sitter over grammars compiled into the binary, while
extensions contribute language definitions as data. Running a grammar is a
mechanic like shaping: the tree must follow every commit synchronously and
painting never calls JavaScript, while the definition is data that personal
config and packages replace through the same API the bundled packages use.
Grammars come from upstream crates over the `tree-sitter` crate rather than
arborium, whose whole-string highlighter has no tree editing, old-tree reuse,
chunked input, progress callback, or byte-range query. SQL is left out because
its crate pins `cc ~1.2.1` for the whole build; runtime grammar loading is D065.

LSP language IDs and nvim-treesitter's capture names mean the vocabulary a
language client needs already exists: IDs pass through unchanged and semantic
token types map onto the same names. Adopting Neovim's published capture list
lets adapted upstream queries keep their names and stops a missing leaf from
falling back to the wrong parent; Zed, Helix, and Neovim all resolve dotted
names by longest prefix. Decoration names stay outside the capture vocabulary
so a query cannot draw a diagnostic. Queries ship as TypeScript string modules,
so packages stay JavaScript-only.

The 1 ms foreground budget comes from measurement. On Apple M1 Pro/macOS release
builds, a 3,500-line, 96 KiB Rust file parsed initially in 10.5 ms and reparsed
after valid typing at p50 220 µs, p99 475 µs; a 70,000-line, 1.9 MiB file parsed
in 206 ms and reparsed at p50 5.4 ms, p99 13.4 ms. Typing that leaves an unclosed
`(` forces error recovery over the rest of the file: p50 21 ms and 444 ms. A
60-line highlight query over the visible range cost about 220 µs. The budget
therefore keeps ordinary files inline and moves huge files and broken syntax off
the foreground. Injections are nested layers rather than one combined tree; a
1,400-line, 21 KiB Markdown file reparsed after typing at p50 115–150 µs, while a
28,000-line file with 10,000 layers reparsed at p50 6–9 ms, mostly the Markdown
root and editing later layers' trees.

The background path is cheap because snapshots share the original text and
sealed 16 KiB add-buffer chunks as `Arc<str>` and copy only the piece list. On a
29 MiB, 932k-line buffer with 9,988 pieces after 5,000 typing bursts, a snapshot
cost p50 19 µs, p99 33 µs, and streaming the whole text 0.39 ms.

Queries compile off the foreground at registration because compiling every
bundled query took about 420 ms, C++ alone 130 ms. The grammars grew the release
binary from about 74 MB to 88.5 MB and add about 28 s of parallel C compilation.

Each bundled language is its own package, such as `@knot/language-rust`, so its
definition and editing data change independently of the others. Packages cannot
import each other, so languages that compose one query family share a package:
TypeScript and JavaScript share the ECMAScript and JSX queries, as VS Code ships
them as one extension.

A document's language is buffer state, and an unregistered explicit ID is kept
rather than rejected, so restoration and personal config need no ordering
against package startup. Layouts record only explicit languages, so restores
detect again.

References: [core and buffers](../architecture/core-and-buffers.md#syntax-state)
and [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages).

## Themes

Themes are contributions registered like languages, so personal config and
packages replace them through the API the bundled `@knot/themes` package uses.
There is no `extends`: themes are code, so a derived theme spreads `themes.get`
and changes a few keys, and theme families are just several registrations.

Native color keys and their defaults live in one Rust table rather than a
package, because native surfaces paint before packages start and when they fail;
the defaults are therefore also the fallback, so there are no separate fallback
themes. Theme maps stay open, so a theme can style a package that loads later
and a key nobody defines is inert, as an undefined setting is. Contributions
name color keys rather than severity tokens, so an extension can paint in its
own key and still follow every theme.

Selection is settings, so it follows the rule that later writes win. The theme
resolves at startup completion and on selection changes, not on every
registration: themes rarely change mid-session, and resolving once keeps
package load order out of what the first frame shows.

Bundled themes ship in one package rather than one per family: they are small
palettes over one shared capture mapping, and Select Theme needs one owner.
They are ports of MIT-licensed themes; Tokyo Night's light variant is enkia's
Tokyo Night Light rather than tokyonight.nvim's Day, which is Apache-2.0.

References: [semantic UI](../architecture/semantic-ui.md#themes-and-color-keys)
and [extension guide](../extension-api.md#register-a-theme-and-define-colors).

## Decoration styles

A decoration names a color key and exactly one required style, so no
contribution relies on a default that a later consumer would want changed; a
range that wants two styles is two overlapping contributions, which the store
already allows, rather than a list on one. Themes change a key's color but never
its style, because the style is what the contributor means, as a diff band or a
match highlight.

Backgrounds and line bands paint as quads behind the text rather than as
text-run backgrounds, which gpui paints over the selection and would hide it
under every match highlight; VS Code also keeps the selection readable over find
matches. Quads reuse the selection's row-span geometry, so wrapped and
right-to-left rows need nothing new. Backgrounds stop at the text so occurrence
highlights read as words, while line bands cover whole rows so diff hunks read
as blocks.

Reference: [semantic UI](../architecture/semantic-ui.md).

## Extension tree content and actions

One registration declares a frame's content kind, provider or pushed text, and
actions. Groups are tree headers rather than a second list model, so grouping,
expansion, and invalidation share one provider tree. Actions are command names
with JSON arguments over the captured-window command route, so native controls
need no parallel dispatch path. Row status names a color key rather than a
feature's change-state taxonomy, and glyphs stay out of the boundary: items and
actions name [icons](#icons).

Text frames push read-only text into a private buffer rendered by the ordinary
editor surface, which avoids a second text renderer and reuses selection, copy,
and scrolling. The embedding is provisional; a dedicated read-only presentation
is D054.

Reference: [semantic UI](../architecture/semantic-ui.md#extension-trees).

## Icons

Icons are names, never glyphs, resolved by a registry in layers: override,
selected pack, defaults, fallback. `define` and `override` are separate levels
because they have different jobs: a package defining the icon for its own name
must not silently replace another package's, so a duplicate default is an error
like `defineColor`, while personal config must be able to replace any icon, so
the latest override wins. Defining a name at the default level lets a pack
restyle every package's icons. An unknown name paints a fallback and is not an
error, as an undefined color key does nothing.

File and folder associations are global rather than part of a pack, so a
Material-style pack registers many of them without every other pack having to
repeat the language table; a name the active pack lacks falls through to the
next candidate. Language packages define `file.<id>` rather than carrying an
icon field, which keeps language definitions free of presentation.

The native table and Lucide glyphs stay in Rust: native surfaces paint before
packages start and when they fail. Lucide comes from `gpui-kit-assets`, already
a gpui-component dependency, so Knot adds no crate and no asset source; gpui
caches rasters by the SVG bytes, so a replaced definition needs no
invalidation. Registrations apply live to every window instead of at startup,
because packs and selection are cheap to repaint and a restart-only pack would
make Select Icon Pack unusable. Package-shipped asset files, icon fonts, raster
formats, and per-appearance variants are rejected for now: SVG text and Lucide
names cover the surfaces, and tinted icons already follow the theme.

Reference: [semantic UI](../architecture/semantic-ui.md#icons).

## Extension transient interactions

Pickers, inputs, and prompts resolve with typed values, so dismissal is an
ordinary result and operational failure stays a host error. Resolving the
window from the command capture keeps an awaited picker in the window that
invoked it.

Pickers receive a complete item list and filter natively, avoiding streaming
providers and a second scheduler (D118). One native picker serves the palette,
native pickers, and extensions, so every picker inherits the same matching.
Matching uses `nucleo-matcher` rather than a hand-written scorer, because fzf's
ranking quality, with its boundary, camelCase, and path bonuses, is the part not
worth redoing; it is MPL-2.0, used unmodified as a dependency.

Highlights and input changes are pulled by the caller's own turn rather than
pushed, because package roots run one at a time and a pushed notification
would queue behind the command awaiting the picker until it settled. Keeping
only the latest unread highlight coalesces a slow callback without a queue.

Confirmations and messages reuse the native gpui prompt, which already handles
answers and focus, so a rendered dialog would add surface without behavior. One
shared `TextInput` on gpui's input-handler contract backs every field and the
palette query, so typing, IME, and clipboard have one native path.

Reference: [semantic UI](../architecture/semantic-ui.md#extension-transient-interactions).

## Status bar

The bar is native and its items are extension registrations, so the shell owns
layout and painting while packages, including the bundled `@knot/status-bar`,
decide what it says, and personal config replaces a bundled item by registering
its ID again. Values can be per window because the position and indentation an
item shows describe one window's editor. `update` replaces the whole value,
because a partial merge would need a way to clear a field and a rule for what a
window inherits from the shared value. Hiding is the `statusBar.hiddenItems`
setting rather than unregistration, so a user hides a bundled item without
stopping its package; that needed a list of strings, so settings gained a
`strings` type rather than a delimited string. A click dispatches through the
captured command target, so the pointer gets no second way to run a command.

Selection size stops counting characters past 1 MiB because counting sends the
selection to JavaScript on every change. The indentation picker follows VS
Code's actions but drops its "Indent Using Spaces" and "Indent Using Tabs",
which differ from the Convert entries only by leaving the text alone, so users
could not tell them apart. A changed tab size never rewrites text, and reindent
is a separate action, as in Zed, Sublime Text, Emacs, Vim, and JetBrains.

Reference: [status bar](../architecture/workbench-and-lifecycle.md#status-bar).

## Generated text surfaces

Generated output that benefits from text interaction uses ordinary buffers and
views, and controllers keep semantic identities rather than parsing displayed
text. A native search probe validated this boundary without adding feature
semantics to buffers or a second presentation model.

Documents have no generated state: cross-file search is a bundled extension and
text frames use private buffers, so no product path created a generated
document, and removing the state dropped its recovery, closure, Save, and
`DocumentInfo.kind` special cases. Interactive generated views return as an
extension-facing contract (D027) rather than a native document kind.

Reference: [semantic UI](../architecture/semantic-ui.md#generated-text).

## Capability aggregation

Aggregation is feature-specific and view-owned rather than a generic framework.
Deterministic merging removes arrival-order dependence, provider-local failure
preserves other results, and stable item identities let presentation update
without restarting work or recovering semantics from labels.

Reference: [semantic UI](../architecture/semantic-ui.md#completion-ownership-and-aggregation).
