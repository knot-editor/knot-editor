# D055 — Icon packs

Status: 🚧 in progress; checkpoints 1–4 implemented; 4 awaits review. Roadmap item
[D055](../roadmap.md); builds on the theme system (D064) and on the
[gpui snapshots](../decisions/platform.md#gpui-snapshots-and-gpui-component),
whose painting and asset APIs this plan uses.

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

  Tinted icons go through gpui-component's `Icon` (gpui's `svg()` single-color
  shape rendering) fed with the SVG bytes through `Icon::data`; gpui keys its
  raster cache by a hash of the bytes, so replacing a definition invalidates
  it with no asset source. `Icon` supplies sizing at the surface's text size,
  so zoom needs no extra work. Full-color icons go through gpui's `img()` with
  an `ImageFormat::Svg` image source.
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
  from `gpui-kit-assets`, which gpui-component already
  depends on (ISC icons, the full Lucide catalog; `AllAssets::get("icons/<name>.svg")`
  returns the bytes of a Lucide name, `IconName::ALL` lists them). Knot adds
  no new crate; it reads the bytes and never registers `AllAssets` as an
  asset source.
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
  icon renders as an icon button and shows its label as a tooltip
  (gpui-component's `Tooltip`, as in `src/app/product/render.rs`). Buttons
  stay native, since action focus and key routing are Knot's.
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

### 1. Native icon registry and painting ✅

Done and reviewed: `IconRegistry` in `crates/knot-model/src/icons.rs` (lookup
levels, packs, lifecycle removal, file and folder resolver, native table), the
`ApplicationIcons` global and `AppIcon` element in `src/app/icons.rs`, the
`icons.pack` setting, and the `icon` color key that every bundled theme sets.
`gpui-kit-assets` is a direct dependency, pinned like gpui-component. The
module carries `#[allow(dead_code)]` in `src/app/mod.rs`; checkpoint 2 removes
it once a surface uses icons.

### 2. Icons in the workspace and extension trees ✅

Done and reviewed: both trees paint `AppIcon` row and disclosure icons
(`tree.collapsed` / `tree.expanded`), and `TreeItem.icon` is an icon-name
string, validated for characters at the host boundary in
`crates/knot-host/src/engine/marshal.rs`. An unknown name paints the fallback
icon. `mod icons` no longer needs `#[allow(dead_code)]`.

### 3. Extension API and pack selection ✅

Done and reviewed: `icons.define`, `icons.override`, `icons.registerPack`, and
`icons.listPacks` (handlers in `src/app/extensions/host/handlers.rs`,
`IconSpec` validation in `crates/knot-model/src/icons.rs`), lifecycle teardown
that repaints, the fake host, and Select Icon Pack in `@knot/themes`
(`icon-pack.ts`). `docs/schemas/knot.schema.json` only describes package
manifests, so the `icons.pack` setting is declared in `knot.d.ts`; its row in
`docs/settings.md` lands with checkpoint 6.

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
  `defineColor` in `crates/knot-host/src/js/bootstrap.js:1355` and the `themes` export in
  `crates/knot-host/src/js/knot.js:83`. Call teardown next to
  `themes::remove_lifecycle` in `src/app/extensions/host.rs:2612`. Update
  `docs/schemas/knot.schema.json` for the setting. Select Theme lives in
  `extensions/@knot/themes/src/selection.ts`.
- Review gate: a personal-config `post-init.js` with `try.*` commands that
  register a full-color pack, select it, and override one icon.

### 4. File associations, language icons, and tabs ✅

Implemented, awaiting review: `icons.associate` and the full resolver in
`IconRegistry` (file name, longest extension, language, generic; named folders
with `.open` forms), `file.<id>` icons with `icon.<id>` color keys in each
`language-*` package (their own default colors; the bundled themes do not
restyle them), and the document tab and tab drag label icons.

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
  in `LanguageRegistry::associate` (`crates/knot-model/src/languages.rs`). Tab titles are built in
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
  `docs/settings.md`; Lucide's ISC licence (via `gpui-kit-assets`) noted wherever bundled
  licences are listed; the gpui-component paragraph of the platform decision
  updated to name `Icon` as an adopted leaf; D055 removed from the roadmap and its ordering line; this plan
  removed.
- Validation: the `review-docs` checks for the touched files.
- Review gate: the docs describe the shipped behavior only.
