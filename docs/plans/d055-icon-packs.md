# D055 — Icon packs

Status: 🚧 approved, not started. Roadmap item
[D055](../roadmap.md); builds on the theme system (D064) and waits for
[D160](d160-gpui-component.md), whose gpui port changes the painting and
asset APIs this plan uses.

## Goal

Knot paints real icons instead of Unicode glyphs in trees, tabs, actions, and
the status bar. A builtin Lucide set gives good defaults, so the explorer shows
a color per file type with no extra assets. Extensions and personal config can
add icons for their own names, install a whole icon pack selected through a
setting, or replace a single icon.

## Design

- **Named icons.** An icon is identified by a flat dotted name, as color keys
  and settings are: `file`, `folder.open`, `file.rust`, `action.refresh`,
  `status.modified`. Contributions name icons, never glyphs. An unknown name
  paints the generic fallback icon and is not an error, just as an undefined
  color key does nothing.
- **Icon definitions.** A definition is `{ svg }` (an SVG document as text) or
  `{ lucide }` (the name of a builtin Lucide glyph), plus an optional `color`:
  - a theme color key: the icon is single-color and painted in that key;
  - `"original"`: the SVG's own colors are kept (`svg` only);
  - absent: the surface's ordinary icon color.

  Tinted icons go through gpui's `svg()` single-color shape rendering, served
  by an application `AssetSource` whose paths are keyed by content, so
  replacing a definition changes the path and invalidates gpui's raster
  cache. Full-color icons go through `img()` with an SVG image source.
- **Lookup.** For one name, the first of these that defines it wins:
  1. **override**: `icons.override(name, definition)`, intended mainly for
     personal config. The latest override wins, and disposing it reveals the
     previous one;
  2. **the active pack**: chosen by the `icons.pack` setting;
  3. **defaults**: the native builtin table, plus names extensions add with
     `icons.define(name, definition)`. Defining a name that already has a
     default is a `DuplicateIcon` error, mirroring `defineColor`;
  4. the generic fallback icon.

  Defining a name puts it at the default level, so a pack can restyle any
  extension's icons.
- **Packs** register like themes: `icons.registerPack({ name, icons })`
  returns a lifecycle-owned `Disposable`, and the latest registration of a
  name defines that pack. A pack maps names to definitions and falls back per
  name to the defaults below it. `icons.pack` is one text setting with no
  light/dark split (tinted icons already follow the theme). Empty means no
  pack, which is the default. Registrations and selection apply immediately
  and repaint every window: icons register during activation, before startup
  restores windows, so load order does not change the first frame.
- **Native defaults.** The builtin Lucide glyphs and the native default
  table live in Rust, for the same reason native color-key defaults do:
  native surfaces paint before packages start and when they fail. Lucide comes
  from the `lucide-static-svg` crate (ISC icons, MIT wrapper, no dependencies,
  tracking upstream releases; `Icon::all()`, `name()`, `svg_bytes()`). If it
  stops tracking upstream, vendor `lucide-static` through the existing npm
  toolchain in `build.rs` instead. `lucide-gpui` is rejected: it depends on
  `gpui-kit`, Longbridge's component bundle, which builds on a separate
  `gpui-pre` fork rather than Knot's `gpui`, and its own value is only an
  asset source over the same SVGs.
- **File icons** are resolved apart from the icons themselves. For a file,
  the resolver tries, in order, the names that a registered association gives
  for the exact file name and for the extension, then `file.<languageId>`
  for the detected language, then `file`. For a directory it tries the
  name-association name, then `folder`, using the `.open` variant of each
  when expanded. The first name that has a definition at any lookup level
  wins. `icons.associate({ files, extensions, folders })` registers
  name-to-icon associations; the latest claim of a pattern wins, and disposal
  reveals the previous one. Associations are global rather than per pack: a
  Material-style pack registers many of them, and a name the active pack
  does not define falls through to the next candidate, so a file never loses
  its icon. Language packages need no new field: they `define`
  `file.<languageId>` with a per-language color key (for example
  `icon.rust`) that they define with `themes.defineColor`, and bundled themes
  may restyle it.

## Surfaces

- Workspace tree: file and folder icons through the resolver; the disclosure
  triangles become `tree.collapsed` / `tree.expanded` icons.
- Extension trees: `TreeItem.icon` becomes an icon name, which replaces the
  fixed `TreeIcon` enum.
- Tree row and frame toolbar actions: an optional `icon`. An action with an
  icon renders as an icon button and shows its label as a tooltip.
- Document tabs: the resolved file icon before the title.
- Status bar items: an optional `icon` before the text, painted in the item's
  `color` when one is set.

## Exclusions

- File icons for extension tree rows from a resource URI (VS Code's
  `resourceUri`). Rows name an icon explicitly for now.
- Icon fonts, raster formats, and per-appearance icon variants.
- Animated icons such as spinners, and icons in the command palette, menus,
  or the editor gutter.
- Package-shipped asset files: extensions supply SVG text or Lucide names.
- A settings UI for browsing icons.

## Deferred decisions

- The exact builtin name vocabulary beyond what the five surfaces need.
  Grow it as packages ask, rather than reproducing every Lucide name.
- Whether folder name associations should also match path suffixes such as
  `.github/workflows`. Plain names until a pack needs more.

## Checkpoints

### 1. Native icon registry and painting

- Deliverables: an application icon registry with the four-level lookup,
  the native default table (`file`, `folder`, `folder.open`, `symbol`,
  `tree.collapsed`, `tree.expanded`, and the fallback) over Lucide, an icon
  element that paints tinted and full-color definitions at the surface's text
  size and zoom, the content-keyed asset source, the `icons.pack` setting
  read on change, and a file resolver covering only language and generic
  candidates. No surface uses icons yet.
- Validation: unit tests for lookup precedence (override over pack over
  default over fallback, disposal revealing the previous level), for the
  invalidation of content keys, and for resolver candidates; a gpui test
  window that paints a tinted and a full-color icon through the asset
  source; a test that every native default names an existing Lucide glyph.
- **Implementation context:** add `src/app/icons.rs`, declared in
  `src/app/mod.rs` and exposed as an `ApplicationIcons` global. Mirror the
  registration, owner, and lifecycle structure of `ThemeRegistry` in
  `src/app/themes.rs`, and the native defaults table of
  `native_color_definitions` in `src/app/theme.rs`. Register the asset source
  with `Application::new().with_assets(..)` in `src/app/product/launch.rs`
  (around line 393), and in the test application setup. Detection uses
  `LanguageRegistry::detect(file_name, "")` in `src/app/languages.rs:295`.
  Add the setting beside `THEME_*` in `src/app/settings.rs` and observe it
  like `themes::apply`. Add an `icon` color key, the default icon tint, to the
  native color table.
- Review gate: the registry shape and the element's sizing API, before
  any surface depends on them.

### 2. Icons in the workspace and extension trees

- Deliverables: workspace tree rows paint resolved file and folder icons
  and icon disclosure triangles; extension tree rows paint their named icon.
  `TreeIcon` becomes a validated icon name in the protocol and `knot.d.ts`
  (`"file" | "folder" | "symbol"` remain valid names).
- Validation: tree view and workspace tree tests that rows carry the
  resolved icon name, including the `.open` variant for expanded folders and
  the fallback for an unknown name; existing tree keyboard and pointer tests
  pass unchanged.
- **Implementation context:** replace `TreeView::icon` and `disclosure` in
  `src/app/tree_view.rs` (around line 668), the same pair in
  `src/app/workspace_tree.rs` (around line 429), and `TreeIcon` in
  `src/host/protocol.rs:332` with its marshalling in
  `src/host/engine/marshal.rs` and `src/app/extensions/semantics.rs`.
- Review gate: icons look right at every zoom level in both bundled theme
  appearances, and the tree rows align as before.

### 3. Extension API and pack selection

- Deliverables: `icons.define`, `icons.override`, `icons.registerPack`, and
  `icons.listPacks` in `knot.d.ts`, `knot.js`, and `bootstrap.js`, with SVG
  validation at registration (`TypeError` for malformed SVG, an unknown
  Lucide name, or `"original"` with `lucide`), `DuplicateIcon`, and lifecycle
  teardown. A **Select Icon Pack** command in the `@knot/themes` package, next
  to Select Theme. The fake host in `scripts/extensions/` gains the
  namespace.
- Validation: host handler tests for every operation and error; a
  windowless product test in which a package defines a name, a pack replaces
  it, an override wins over both, and teardown of each restores the previous
  level; a `node:test` case for Select Icon Pack through the real editor.
- **Implementation context:** handlers follow `register_theme`,
  `unregister_theme`, and `define_color` in
  `src/app/extensions/host/handlers.rs` (around line 3774), with
  `#[host_operation]`. The JavaScript side follows `registerTheme` and
  `defineColor` in `src/host/js/bootstrap.js:1355` and the `themes` export in
  `src/host/js/knot.js:83`. Call teardown next to
  `themes::remove_lifecycle` in `src/app/extensions/host.rs:2612`. Update
  `docs/schemas/knot.schema.json` for the setting. Select Theme lives in
  `extensions/@knot/themes/src/selection.ts`.
- Review gate: a personal-config `post-init.js` with `try.*` commands that
  register a full-color pack, select it, and override one icon.

### 4. File associations, language icons, and tabs

- Deliverables: `icons.associate` and the full resolver (file-name and
  extension associations, language, generic; named folders and `.open`
  variants). Each bundled `language-*` package defines `file.<id>` with its
  own color key, and bundled themes set those keys where the palette offers a
  better color. Document tabs show the resolved file icon. Changes to
  associations and languages repaint open trees and tabs.
- Validation: resolver unit tests for candidate order, pattern precedence
  and disposal, and fall-through when the active pack lacks an associated
  name; a product test in which a tab's icon follows a language change; a
  `node:test` case per language package that its icon resolves.
- **Implementation context:** the association store mirrors pattern claims
  in `languages.associate` (`src/app/languages.rs`). Tab titles are built in
  `src/app/product/render.rs` (around line 197); also paint the icon in the
  drag label from `tab_drag_label` in `src/app/product/tab_drag.rs`. Language
  packages register in `extensions/@knot/language-*/src/extension.ts`, and
  theme palettes live in `extensions/@knot/themes/src/themes/`.
- Review gate: the explorer and tabs in a real Rust/TypeScript project
  look like a familiar colored file tree.

### 5. Action and status bar icons

- Deliverables: an optional `icon` on `TreeAction` (row and frame toolbar)
  and on `StatusItemOptions`; icon buttons with label tooltips; status items
  that paint the icon in their color. Bundled packages that contribute
  actions or status items adopt icons where they help (`@knot/projects`,
  `@knot/status-bar`).
- Validation: semantics decoding tests for the new fields; tree view and
  status bar tests that an icon action keeps keyboard action focus and its
  command routing, and that a status item renders with and without an icon.
- **Implementation context:** `TreeAction` in `knot.d.ts` (around line 1223)
  and its decoding in `src/app/extensions/semantics.rs`; the toolbar and row
  action rendering in `TreeView::render` (`src/app/tree_view.rs`); status
  items in `src/app/status_bar.rs` (`register`, `update`, `items`) with tests
  in `src/app/status_bar/tests.rs`. Packages use `icons.define` for the names
  they introduce.
- Review gate: icon-only toolbars stay discoverable through their tooltips
  and keyboard focus.

### 6. Documentation

- Deliverables: an icons section in
  `docs/architecture/semantic-ui.md` (lookup levels, resolver, ownership of
  native defaults, painting paths) replacing the deferral note near line 268;
  an **Icons** decision in `docs/decisions/semantic-ui.md` with its index line
  (define and override as separate levels, associations apart from packs,
  native Lucide defaults, live application); the tree decision's D055
  sentence updated; an extension guide section on defining icons, packs, and
  associations, replacing `docs/extension-api.md:254`; `icons.pack` in
  `docs/settings.md`; Lucide's ISC licence noted wherever bundled licences
  are listed; D055 removed from the roadmap and its ordering line; this plan
  removed.
- Validation: the `review-docs` checks for the touched files.
- Review gate: the docs describe the shipped behavior only.
