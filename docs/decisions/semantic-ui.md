# Semantic UI decisions

Syntax, themes, decorations, and the native surfaces that extensions fill with
data. The [decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

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

References: [core and buffers](../architecture/core-and-buffers.md#syntax-state)
and [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages).

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

References: [semantic UI](../architecture/semantic-ui.md#themes-and-color-keys)
and [extension guide](../extension-api.md#register-a-theme-and-define-colors).

## Decoration styles

A decoration names a color key and exactly one required style, so no
contributor relies on a default; two styles are two overlapping contributions.
Themes change colors, never styles. Backgrounds paint as quads behind the text
rather than text-run backgrounds, which gpui paints over the selection.

Reference: [semantic UI](../architecture/semantic-ui.md).

## Extension tree content and actions

One registration declares a frame's content kind, provider or pushed text, and
actions. Groups are tree headers rather than a second list model. Actions are
command names with arguments over the captured-window route, and status names a
color key. Text frames push read-only text into a private buffer rendered by
the editor surface, avoiding a second renderer (D054).

Reference: [semantic UI](../architecture/semantic-ui.md).

## Extension transient interactions

Pickers, inputs, and prompts resolve with typed values, so dismissal is an
ordinary result. Pickers receive complete lists and filter locally, avoiding a
streaming scheduler (D118); one native picker serves everything. Matching uses
`nucleo-matcher` (MPL-2.0) because fzf-quality ranking is not worth redoing.
Highlights and input changes are pulled by the caller's turn rather than
pushed, because a pushed notification would queue behind the awaiting command.
One interaction owns a window at a time. Confirmations reuse the native gpui
prompt, and one shared `TextInput` uses gpui's native input path.

Reference: [semantic UI](../architecture/semantic-ui.md).

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

Reference: [semantic UI](../architecture/semantic-ui.md).

## Capability aggregation

Aggregation is feature-specific and view-owned, with deterministic merging,
provider-local failure, and stable item identities.

Reference: [semantic UI](../architecture/semantic-ui.md).
