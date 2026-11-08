# Settings reference

Every setting Knot ships: native keys defined when the store is created, and
keys defined by bundled packages under `extensions/@knot/`. Native definitions
live in `crates/knot-model/src/settings.rs`, typed declarations in `crates/knot-host/src/js/knot.d.ts`,
and package definitions in the owning package's `src/extension.ts`. This page
lists keys, types, and defaults; it does not repeat how settings work. See
[read and write settings](extension-api.md#read-and-write-settings) for the
API, and `settings.rs` for the store.

## Scopes and precedence

Settings are one in-memory store shared by personal config and every package.
A value applies everywhere, or only where every fact of its scope matches:
`language` (a document's effective language ID), `view` (`editor`, `terminal`,
or a contributed view ID), and `document` (a document's ID; such values end
when it closes). A read picks the matching value with the most facts, ties go
to the latest write, and with no match the default applies. Types are
`boolean`, `integer`, `number`, `string`, and `strings` (a list of strings).

The Scope column below says which facts the reader supplies. "Any" means the
reading view passes its language, view kind, and document, so any scope
applies. "Global" means the key is read without context, so only unscoped
values take effect.

## Editor

| Key | Type | Default | Scope | Owner | Description |
| --- | --- | --- | --- | --- | --- |
| `editor.tabSize` | integer | `4` | Any (document scope used by the status bar tab-size picker); clamped to 1-32 | native | Columns per tab stop in editor views. |
| `editor.insertSpaces` | boolean | `true` | Any; a document whose text shows a clear indentation style gets its own document-scoped value | native | Indent with spaces rather than tabs. |
| `editor.autoClosingBrackets` | boolean | `true` | Any | native | Insert the closer of a typed bracket or quote where the language allows, and step over or delete it with the opener. |
| `editor.autoSurround` | boolean | `true` | Any | native | Wrap the selection when typing an opening bracket or quote over it. |
| `editor.softWrap` | boolean | `false` | Any; Toggle Soft Wrap writes document scope; `language-markdown` sets `{ language: "markdown" }` | native | Wrap long lines at the viewport width. |
| `editor.lineNumbers` | string: `on`, `off`, `relative` | `"on"` | Any; other text means `on` | native | Line numbers in the editor gutter; `relative` numbers lines by distance from the cursor. |
| `editor.folding` | boolean | `true` | Any | native | Show the fold lane in the editor gutter. |
| `editor.showFoldingControls` | string: `mouseover`, `always` | `"mouseover"` | Any; other text means `mouseover` | native | When the fold lane shows chevrons on foldable lines; collapsed folds always show one. |

## Files

| Key | Type | Default | Scope | Owner | Description |
| --- | --- | --- | --- | --- | --- |
| `files.trimTrailingWhitespace` | string: `off`, `all` | `"all"` | Any; other text means `all` | native | Trim trailing spaces and tabs from every line except the cursor's when saving. |
| `files.insertFinalNewline` | boolean | `true` | Any | native | End a non-empty file with a line terminator when saving. |

## Appearance

| Key | Type | Default | Scope | Owner | Description |
| --- | --- | --- | --- | --- | --- |
| `font.size` | number | `14` | Any; scope `{ view: "terminal" }` to size terminals separately; clamped to 6-72 | native | Text size in pixels of editors, text frames, and terminals. |
| `theme.mode` | string: `dark`, `light`, `system` | `"dark"` | Global; other text means `dark` | native | Which theme applies; `system` follows the system appearance. |
| `theme.dark` | string | `"One Dark"` | Global | native | Theme name used in dark mode. |
| `theme.light` | string | `"One Light"` | Global | native | Theme name used in light mode. |

The `themes` package writes the three `theme.*` keys when the user picks a theme.

## Workbench

| Key | Type | Default | Scope | Owner | Description |
| --- | --- | --- | --- | --- | --- |
| `keymap.preset` | string | `"knot"` | Global; an unregistered name selects `knot` | native | Keymap preset that supplies default key bindings. |
| `workbench.previewTabs` | boolean | `true` | Global | native | Open documents browsed from the explorer or search as preview tabs that the next preview replaces. |
| `statusBar.visible` | boolean | `true` | Global | native | Show the status bar below the workbench. |
| `statusBar.hiddenItems` | strings | `[]` | Global | native | IDs of status bar items the bar skips; hidden items keep running. |

## Bundled packages

| Key | Type | Default | Scope | Owner | Description |
| --- | --- | --- | --- | --- | --- |
| `vim.enabled` | boolean | `false` | Read with the editor's `document`, `language`, and `view: "editor"`, so scoped values turn Vim off for a language or view | `@knot/vim` | Edit with Vim's modes and keys. |

The other bundled packages (`go-to-line`, `language-*`, `projects`,
`status-bar`, `themes`) define no settings; some write native keys, as noted
above.
