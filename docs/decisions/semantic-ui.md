# Semantic UI decisions

Syntax, themes, decorations, and the native surfaces that extensions fill with
data. The [decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Syntax highlighting

Parse natively with tree-sitter over grammars compiled into the binary, and
let extensions contribute language definitions: an LSP language ID, the grammar,
detection patterns, and queries. Running a grammar is a mechanic like shaping:
the tree must follow every commit synchronously and painting never calls
JavaScript, while the definition is data that personal config and packages
replace through the same API the bundled language packages use.
Grammars come from upstream crates over the `tree-sitter` crate rather than
arborium, whose whole-string highlighter has no tree editing, old-tree reuse,
chunked input, progress callback, or byte-range query. SQL is left out because
its crate pins `cc ~1.2.1` for the whole build; runtime grammar loading is D065.

LSP language IDs and nvim-treesitter's capture names make the pieces a
language client needs already exist: IDs pass through unchanged and semantic
token types map onto the same names. Adopting Neovim's published capture list
lets adapted upstream queries keep their names and stops a missing leaf from
falling back to the wrong parent; Zed, Helix, and Neovim all resolve dotted
names by longest prefix. Decoration names stay outside the capture vocabulary
so a query cannot draw a diagnostic. Queries ship as TypeScript string modules,
normalized to predicates the Rust binding evaluates, so packages stay
JavaScript-only.

Every edit applies to the tree in the commit, then the foreground reparses
within 1 ms and continues in the background from a `TextSnapshot`, painting the
edited previous tree meanwhile. On Apple M1 Pro/macOS release builds, a
3,500-line, 96 KiB Rust file parsed initially in 10.5 ms and reparsed after valid
typing at p50 220 µs, p99 475 µs; a 70,000-line, 1.9 MiB file parsed in 206 ms
and reparsed at p50 5.4 ms, p99 13.4 ms. Typing that leaves an unclosed `(`
forces error recovery over the rest of the file: p50 21 ms and 444 ms. A
60-line highlight query over the visible range cost about 220 µs. The budget
therefore keeps ordinary files inline and moves huge files and broken syntax
off the foreground. Injections are nested layers rather than one combined tree;
a 1,400-line, 21 KiB Markdown file reparsed after typing at p50 115–150 µs,
while a 28,000-line file with 10,000 layers reparsed at p50 6–9 ms, mostly the
Markdown root and editing later layers' trees, and continues in the background.

Snapshots share the original text and sealed 16 KiB add-buffer chunks as
`Arc<str>` and copy the piece list. On a 29 MiB, 932k-line buffer with 9,988
pieces after 5,000 typing bursts, a snapshot cost p50 19 µs, p99 33 µs, and
streaming the whole text 0.39 ms, so a snapshot per budget overrun is cheap.

Queries compile off the foreground at registration: compiling every bundled
query took about 420 ms, C++ alone 130 ms. The grammars grew the release
binary from about 74 MB to 88.5 MB and add about 28 s of parallel C
compilation.

Each bundled language is its own package, such as `@knot/language-rust`, so
each language's definition and editing data live in one small package that
changes independently of the others; the grammars that have no package yet
share `@knot/language-others`. The queries adapted from nvim-treesitter share
one Apache-2.0 license file, `extensions/@knot/LICENSE-APACHE`. Packages cannot
import each other, so languages that compose one query family share a package:
TypeScript and JavaScript share the ECMAScript and JSX queries, as VS Code ships
them as one extension.

A document's language is buffer state, explicit or detected. An unregistered
explicit ID is kept and paints as plain text, so restoration and personal config
need no ordering against package startup, and the window layout records only
explicit languages, so restores detect again. Set Language Mode is native.

References: [core and buffers](../architecture/core-and-buffers.md#syntax-state)
and [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages).

## Themes

Themes are extension contributions registered like languages: the latest
registration of a name wins and teardown reveals the previous one, so personal
config and packages replace themes through the API the bundled `@knot/themes`
package uses. There is no `extends`: themes are code, so a derived theme
spreads `themes.get` and changes a few keys, and theme families are just
several registrations.

Color keys are definitions, like settings: a description and a default per
appearance, each a color or another key, as VS Code's contributed colors are.
Native keys live in one Rust table rather than a package, because native
surfaces paint before packages start and when they fail; the defaults are
therefore also the fallback for an unregistered selection, so there are no
separate fallback themes. Extensions define their own keys. Theme maps stay
open, so a theme can style a package that loads later and a key nobody
defines is inert, as an undefined setting is. Contributions name color keys
rather than severity tokens, so an extension can paint a decoration or badge
in its own key and still follow every theme; `error`, `warning`, and `info`
decorations paint above other keys so a diagnostic is never hidden.

Selection is the `theme.mode`, `theme.dark`, and `theme.light` settings, so
it follows the settings rule that later writes win. The theme resolves when
startup completes and when the selection changes, not on every registration:
themes rarely change mid-session, and resolving once keeps package load order
out of what the first frame shows. Select Theme lives in `@knot/themes` and
replays its choice at start, as any settings UI must (D091), so `post-init`
still overrides it.

Bundled themes ship in one package rather than one per family: they are small
palettes over one shared capture mapping, and Select Theme needs one owner.
They are ports of MIT-licensed themes; Tokyo Night's light variant is enkia's
Tokyo Night Light rather than tokyonight.nvim's Day, which is Apache-2.0.

References: [semantic UI](../architecture/semantic-ui.md#themes-and-color-keys)
and [extension guide](../extension-api.md#register-a-theme-and-define-colors).

## Decoration styles

A decoration names a color key and exactly one style: `background`, `line`,
`underline`, or `wavy`. The style is required, so no contribution relies on
a default that a later consumer would want changed, and a range that wants
two, such as a background and an underline, is two overlapping
contributions, which the store already allows, rather than a list of styles
on one. Themes change a key's color but never its style, because the style
is what the contributor means, as a diff band or a match highlight.

Backgrounds and line bands paint as quads behind the text rather than as
text-run backgrounds, which gpui paints over the selection quad and would
hide the selection under every match highlight; VS Code also keeps the
selection readable over find matches. Quads reuse the selection's row-span
geometry, so wrapped and right-to-left rows need nothing new. Backgrounds
stop at the text on each row rather than reaching the pane edge, so
occurrence highlights read as words, while a line band covers whole rows,
blank lines included, so diff hunks read as blocks. A line range ending at
column 0 of a later line leaves that line out, as indenting a selection
does, so `lineRange(a).start` to `lineRange(b + 1).start` covers lines `a`
through `b`, and an empty range bands its own line so a one-line marker
needs no length.

Reference: [semantic UI](../architecture/semantic-ui.md).

## Extension tree content and actions

One contributed registration declares its native content kind, provider or
pushed text, and frame actions. Groups are explicit tree headers rather than a
second list model, so grouping, expansion, and invalidation share one provider
tree. Row and group actions carry a command name plus JSON arguments and reuse
the captured-window command route, keeping native controls and activation
mutually exclusive without a parallel dispatch path. Row status names a theme
color key rather than a feature's change-state taxonomy, and action and
status glyphs stay out of the boundary until an extension-replaceable icon
system exists (D055).

Contributed text frames push a complete read-only value into a private buffer
rendered by the ordinary editor surface. This avoids a second text renderer and
reuses selection, copy, and scrolling, while the buffer stays outside document
identity and persistence. The bridge retains the last pushed text so later
windows present current content. The editor embedding is provisional; a
dedicated read-only presentation is tracked as D054.

Reference: [semantic UI](../architecture/semantic-ui.md).

## Extension transient interactions

Picker, text input, and confirmation/message interactions resolve with typed
values under `workbench`, so dismissal is an ordinary result and operational
failure stays a host error. The window is resolved before any focus change from
an explicit `WindowTarget` or the admitted command capture, which keeps an
awaited picker inside a command bound to its originating window.

Pickers receive a complete item list and filter locally, deliberately avoiding
streaming providers and a second scheduler (D118). One native picker serves the
palette, native pickers, and extensions, so every later picker inherits the
same matching and presentation. Matching uses `nucleo-matcher` rather than a
hand-written scorer: ranking quality is fzf's optimal alignment with boundary,
camelCase, separator, and consecutive bonuses, plus smart case, Unicode
normalization, and a path mode for Go to File, and that tuning is the part not
worth redoing. It is MPL-2.0, used unmodified as a dependency.

Highlight previews are pulled rather than pushed. Package roots run one at a
time, so a notification for each highlight would queue behind the command that
awaits the picker until it settles. The caller's own turn instead asks for the
next highlight or the outcome, and native code keeps only the latest unread
highlight, which coalesces a slow callback without a queue. An input's
`onChange` follows the same pulled session, and its returned message rides on
the next pull rather than a separate call, so validation and live previews
share one transport with pickers. One extension interaction owns a
window at a time and a second request rejects `OperationInProgress`; the
command palette stays an independent native surface rather than sharing that
slot. Window closure, extension unload, and caller cancellation settle the
interaction as cancellation, and the previous live focus is restored on close.

Confirmations and messages reuse the native gpui prompt. The platform already
tests these answers, restores focus, and covers the contract, so a parallel
rendered dialog would add surface without behavior.

One shared single-line `TextInput` implements gpui's input-handler contract and
backs both transient fields and the palette query, rather than assembling a
string from key events. Typing, IME preedit, and clipboard use gpui's native
path; the field owns only ordinary movement, selection, and pointer handling.

Reference: [semantic UI](../architecture/semantic-ui.md).

## Status bar

The bar is native and its items are extension registrations, so the shell owns
layout and painting while a package, including the bundled `@knot/status-bar`,
decides what the bar says; personal config replaces a bundled item by
registering its ID again, under the latest-registration rule of languages and
themes, and the Vim mode indicator uses the same API. `update` replaces the
whole value, which keeps a window's value independent of the shared one: a
partial merge would need a way to clear a field and a rule for what a window
inherits. A per-window value exists because the position and indentation of an
item describe one window's editor, so a single shared value could not serve two
windows; a window with no editor leaves the shared empty text to hide them.
Items sort by a priority where higher sits further left, as VS Code orders
them, and ties keep registration order. Hiding an item is a setting,
`statusBar.hiddenItems`, not unregistration, so a user hides a bundled item
without stopping its package. That needed a list of strings, so settings gained
a `strings` type rather than a delimited string. A click dispatches through the
same captured command target as keys and tree actions, so no second way to run
a command from the pointer exists.

Selection size counts characters up to 1 MiB and shows kilobytes beyond,
because counting sends the selection to JavaScript on every change. Indentation
is one picker with VS Code's actions and wording, including a size from 1 to 8,
but not its "Indent Using Spaces" and "Indent Using Tabs", which differ from the
Convert entries only by leaving the text alone, so users could not tell them
apart. The rest keep what other editors share: a changed tab size does not
rewrite the text, a conversion keeps each line's depth, and a language-aware
reindent is a separate action, as in Zed, Sublime Text, Emacs, Vim, and
JetBrains. The writes are ordinary document-scoped settings, so any extension
can offer the same choices.

## Generated text surfaces

Use ordinary buffers and views when generated output benefits from text
interaction. Controllers retain semantic identities instead of parsing displayed
text. A native search probe validated this boundary without adding feature
semantics to generic buffer ownership or a second presentation model.

Documents have no generated state. Cross-file search ships as a bundled
extension and contributed text frames use private buffers, so no product path
created a generated document; removing the state also removed its recovery,
closure, Save, and public `DocumentInfo.kind` special cases. Interactive
generated views return as an extension-facing contract (D027) rather than a
native document kind.

Reference: [semantic UI](../architecture/semantic-ui.md).

## Capability aggregation

Keep aggregation feature-specific and view-owned. Deterministic merging removes
arrival-order dependence; provider-local failure preserves other results. Stable
semantic item identities allow presentation replacement without restarting work
or recovering semantics from labels.

Reference: [semantic UI](../architecture/semantic-ui.md).
