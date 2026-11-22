# D212 — Resource decorations

Status: 🚧 in progress; checkpoint 1 is implemented and awaits its review gate. Source: [roadmap D212](../roadmap.md), part
of the [version control plan](vcs.md), whose architecture section this plan
refines without reopening.

## Outcome

A package can decorate resource URIs, and Knot paints those decorations wherever
a URI is shown. In trees, the label takes the decoration's color, a badge icon
appears at the right of the row, and hovering the row shows a tooltip. A tab
shows only the colored title, with a tooltip on hover. A decoration can also
color the folders above its URI, so a folder holding a changed file stands out.
Native code doesn't know which package decorates or why. Version control status
(D213) is the first consumer, and diagnostics can follow without native changes.

## Design

### API

```ts
const source = await decorations.createSource(); // Disposable
await source.set({
  "file:///repo/src/main.rs": {
    color: "vcs.modified",   // tints the label; optional
    badge: { text: "M" },    // or { icon: "vcs.modified" }; optional
    tooltip: "Modified",     // optional
    propagate: true,         // ancestors take the color; default false
  },
  "file:///repo/old.rs": null, // removes that URI's decoration
});
await source.clear();
```

- **Push, not pull.** A source keeps its own map from URI to decoration, and
  `set` changes only the URIs it names. Disposing the source, or tearing down
  the package's lifecycle, removes all of its decorations. A VS Code-style
  pull provider is rejected: native painting can't call JavaScript, so pulling
  would still need an async native cache with invalidation, and both sides
  would end up keeping the same state.
- **Application-wide.** URIs are normalized as resource state is everywhere
  else, and a decoration applies in every window, so there is no window
  argument.
- **The badge is an icon or a short text**, tagged because both are strings.
  `{ icon }` names an icon resolved through the
  [icon registry](../architecture/semantic-ui.md#icons), so packs and
  overrides can restyle it; an unknown name paints the fallback icon, as
  everywhere else icons are named. `{ text }` is one or two characters
  (grapheme clusters), such as VS Code's `M` or `U`; longer or empty text is
  rejected. Either is painted in the decoration's color, except an icon whose
  definition has its own color.

### Merging

- When several sources decorate one URI, `color` and `badge` are each picked
  separately. Like gutter marks, `error`, `warning`, and `info` win first,
  then the most recently created source. Tooltips from every source are
  joined, one per line.
- A propagated color reaches every ancestor URI of the same scheme and
  authority. It only colors: it carries no badge or tooltip, and it ranks
  below any decoration set directly on the ancestor. When several propagated
  colors meet on one ancestor, the same severity-then-latest rule picks one.

### Surfaces

- **Tree rows**: every row with a `resource`, in every tree. The explorer
  needs no change, and the D214 changes view is decorated too. The
  decoration's color tints the label, its badge sits right-aligned after the
  row's own `statusText`, and its tooltip shows on hovering the row. A row's
  own `badge` dot and `statusText` stay as they are.
- **Document tabs**: the decoration's color tints the title, and its tooltip
  shows on hovering the tab. Tabs show no badge, as in IntelliJ, because they
  already carry the icon, the dirty dot, and the pin. The dirty dot and
  " (deleted)" stay native and unchanged.
- Everything repaints when decorations change, the same way icon and status
  item changes repaint every window.

### Icons for version control

Lucide is bundled in full, with 1,830 glyphs, and a consistent square family
covers the common VCS statuses: `square-dot` or `square-pen` for modified,
`square-plus` for added, `square-minus` for deleted, `square-arrow-right` for
renamed, `square-dashed` for untracked, `square-slash` or `eye-off` for
ignored, and `git-merge-conflict` for conflicts. Lucide has no full set of
letters (only `square-m` and `square-x` exist), which is why badges can also be
text. D213 chooses between letters and icons.

## Exclusions

- Pickers (Go to File, Search Everywhere), tab drag labels, the status bar, and
  editor breadcrumbs.
- Badges on tabs.
- Styles beyond color, such as strikethrough or faded text for ignored files.
- VCS statuses, `vcs.*` color keys, badges, and the mapping between them
  (D213).
- A setting to turn folder propagation off; D213 decides which statuses
  propagate.

## Deferred decisions

- Whether a tree row's `statusText` beats a decoration's badge, or the two sit
  side by side when both are present. Side by side is the plan; checkpoint 2
  reviews it in the running app.
- How tooltips appear on tree rows, which have none today. Checkpoint 2
  decides between gpui-component's `Tooltip`, as on terminal tabs, and
  something lighter.
- Whether tinting the label is readable enough on a selected row, or whether
  selection should override the color as IntelliJ does. Checkpoint 2 settles
  this.

## Checkpoints

### 1. Registry and extension API ✅

Deliverables: a `knot-model` registry of decoration sources owned per
lifecycle, with the merge and propagation rules and a lookup by URI; the
application global that repaints on change and is removed at lifecycle
teardown; and `decorations.createSource` with `set`, `clear`, and dispose in
the `knot` module, with its declaration.

Validation: plain model tests for merging by field (an icon badge and a text
badge compete as one field), severity-then-latest ordering, joined tooltips,
propagation reaching ancestors only within the same scheme and authority, a
direct decoration beating a propagated color, removal undoing propagation,
and lifecycle removal. Product tests for the API: validation of malformed
URIs, decorations, icon names, and badge text that is empty or longer than
two characters, URI normalization, partial `set`, `clear`, dispose, and
teardown.

Review gate: the API shape and the merge rules before anything paints.
Pending the user's review.

Scope decisions: a propagated color fills in on an ancestor whose direct
decoration has no color; a badge ranks by the severity of the decoration that
carries it; the model keeps a resolved map rebuilt on each change, so a lookup
per painted row is one hash probe; the in-memory `knot/testing` host fakes the
API with `decorationsOf(uri)`.

**Implementation context:**
- Mirror `crates/knot-model/src/status_bar.rs` (lifecycle-owned registry,
  tests in `status_bar/tests.rs`) for a new `resource_decorations.rs` module,
  and `src/app/status_bar.rs` for the `ApplicationResourceDecorations` global.
- Keys are `knot_model::resource::ResourceUri`; walk ancestors with
  `ResourceUri::parent`. Rank colors with `decoration_precedence` in
  `crates/knot-model/src/buffer.rs` (line 54), the rule gutter marks use.
- Host wiring: native handlers next to `register_status_item` in
  `src/app/extensions/host/handlers.rs` (around line 6125); the JavaScript
  side next to `registerStatusItem` in `crates/knot-host/src/js/bootstrap.ts`
  (line 1419), using `ownership` and `disposableHandle` as `overrideIcon`
  does (line 2369); export from `crates/knot-host/src/js/knot.ts` (line 108
  shows the pattern) and declare in `crates/knot-host/src/js/knot.d.ts`.
- Teardown: call the new `remove_lifecycle` next to the status bar and icon
  calls in `src/app/extensions/host.rs` (line 3788).
- Product tests in a new `src/app/product/tests/resource_decorations.rs`,
  modeled on `status_bar.rs` there.

### 2. Tree rows

Deliverables: decorated rows in every tree, with label color, badge icon,
tooltip, and propagated folder colors. The explorer is decorated without any
change to it.

Validation: product tests in which a test package decorates files under a
root shown in the explorer: rows take the color and an icon or text badge,
a folder takes the propagated color, a row's own `statusText` and `badge`
coexist with the decoration, and changes and disposal repaint. Product-driver accessors for a
row's label color, badge, and tooltip. A look at the running app in light and
dark themes, selected and unselected.

Review gate: the visual result in the explorer, which settles the deferred
decisions on layout, tooltips, and selection.

**Implementation context:**
- Row painting in `src/app/tree_view.rs`: the label and the
  `status_text`/`badge` children around lines 1580–1645; a text badge
  mirrors the `status_text` element, in the decoration's color. The row's resource
  comes from `TreeItem.resource` (`crates/knot-protocol/src/lib.rs`, line
  512), parsed to `ResourceUri` once when the item is cached, not on every
  paint.
- Paint badges with `crate::app::icons::AppIcon` and `icon_definition`
  (`src/app/icons.rs`), with `.tinted(...)` as the bell on terminal tabs does
  in `src/app/product/render.rs`.
- Tooltip pattern: the terminal-tab tooltip in `render.rs` (around line 518).
- Driver accessors next to `stripe_badge` in
  `src/app/product/driver/window.rs` (line 116); explorer fixtures as in
  `src/app/product/tests/tree_drag.rs` and `crates/knot-ext-workbench/tests`.

### 3. Tabs and documentation

Deliverables: document tabs tinted with their resource's decoration color,
with its tooltip; the architecture reference, the extension guide, and the
decision record.

Validation: product tests that a decorated document's tab takes the color
and tooltip and keeps its dirty dot, " (deleted)" suffix, preview italics,
and pin; untitled documents and terminal and tool tabs are untouched;
disposal restores the theme's tab foreground. Run `review-docs`-style checks
of the touched documents against the code.

Review gate: D212 complete; D213 can build on it.

**Implementation context:**
- Tab rendering in `src/app/product/render.rs` (lines 388–600): override the
  `TAB_ACTIVE_FOREGROUND`/`TAB_INACTIVE_FOREGROUND` text color for document
  tabs whose `resource_uri` has a decoration, and add the tooltip the way
  terminal tabs do.
- Tests next to `src/app/product/tests/editor_tabs.rs`.
- Docs: a "Resource decorations" section in
  `docs/architecture/semantic-ui.md` (beside Icons and Extension trees); a
  "Decorate resources" section in `docs/extension-api/ui.md`; a "Resource
  decorations" entry in `docs/decisions/semantic-ui.md` covering push sources
  instead of a pull provider, native painting on any row with a resource,
  icon or text badges, and no badges on tabs, plus its line in `docs/decisions.md`.
