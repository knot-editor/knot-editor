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
| `files.hidden` | strings | `[".git", ".hg", ".svn", ".jj", "CVS", ".DS_Store", "Thumbs.db"]` | Global | native | Gitignore-style patterns of entries `files.list` and `files.search` never return; a bare name matches at any depth. |
| `files.ignored` | strings | `[]` | Global | native | Gitignore-style patterns of entries `files.list` marks `ignored` and `files.search` skips, in addition to the repository ignore files. |
| `files.respectGitignore` | boolean | `true` | Global | native | Mark entries (and skip them in `files.search`) that `.gitignore`, `.ignore`, `.git/info/exclude`, and the global Git excludes file match as ignored. `.gitignore` files and the Git excludes apply only inside a Git repository. |
| `files.watchDocuments` | boolean | `true` | Global | native | Watch open documents for changes made outside Knot and recheck them when the application becomes active. A clean document whose file changed reloads in place, editing only the changed lines, and Undo restores the previous text. Turning it off releases the watches and stops rechecking; Save still reports a conflict. |
| `files.autoReload` | boolean | `true` | Global | native | Reload a clean document whose file changed outside Knot. When off, the document is marked changed on disk and asks Reload or Keep once focused, like a document with unsaved changes. |

## Appearance

| Key | Type | Default | Scope | Owner | Description |
| --- | --- | --- | --- | --- | --- |
| `font.size` | number | `14` | Any; scope `{ view: "terminal" }` to size terminals separately; clamped to 6-72 | native | Text size in pixels of editors, text frames, and terminals. |
| `theme.mode` | string: `dark`, `light`, `system` | `"dark"` | Global; other text means `dark` | native | Which theme applies; `system` follows the system appearance. |
| `theme.dark` | string | `"One Dark"` | Global | native | Theme name used in dark mode. |
| `theme.light` | string | `"One Light"` | Global | native | Theme name used in light mode. |

| `icons.pack` | string | `""` | Global | native | Name of the registered icon pack that restyles icons; empty means no pack. |

The `themes` package writes the three `theme.*` keys when the user picks a theme,
and `icons.pack` when the user runs Select Icon Pack.

## Workbench

| Key | Type | Default | Scope | Owner | Description |
| --- | --- | --- | --- | --- | --- |
| `keymap.preset` | string | `"knot"` | Global; an unregistered name selects `knot` | native | Keymap preset that supplies default key bindings. |
| `workbench.previewTabs` | boolean | `true` | Global | native | Open documents browsed from the explorer or search as preview tabs that the next preview replaces. |
| `workbench.bottomDockLayout` | string | `"contained"` | Global; an unknown value is `contained` | native | Where the bottom dock sits: `"contained"` under the center between the side docks, or `"full"` across the window width under them. |
| `workbench.toolStripes` | boolean | `true` | Global | native | Show a stripe of tool buttons on the left and right window edges. |
| `statusBar.visible` | boolean | `true` | Global | native | Show the status bar below the workbench. |
| `statusBar.hiddenItems` | strings | `[]` | Global | native | IDs of status bar items the bar skips; hidden items keep running. |

## Completion

| Key | Type | Default | Scope | Owner | Description |
| --- | --- | --- | --- | --- | --- |
| `completion.styles` | strings | `["prefix", "flex"]` | Global; unknown names are ignored, and a list with none selects the default | native | How pickers and completion lists match, in order of preference: `"prefix"`, `"substring"`, `"flex"` (fuzzy), or `"orderless"` (space-separated parts in any order). A candidate matches if any style matches it; earlier styles rank first, and items that match equally well keep their usual order, such as most recently used first. |

## Bundled packages

| Key | Type | Default | Scope | Owner | Description |
| --- | --- | --- | --- | --- | --- |
| `vim.enabled` | boolean | `false` | Read with the editor's `document`, `language`, and `view: "editor"`, so scoped values turn Vim off for a language or view | `@knot/vim` | Edit with Vim's modes and keys. |
| `wordHighlight.enabled` | boolean | `true` | Global | `@knot/word-highlight` | Highlight every whole-word occurrence of the word at the cursor. |
| `selectionHighlight.enabled` | boolean | `true` | Global | `@knot/word-highlight` | Highlight the other occurrences of the selected text. |
| `wordHighlight.delay` | integer | `75` | Global | `@knot/word-highlight` | Milliseconds the cursor rests before the highlights update. |
| `explorer.autoReveal` | boolean | `true` | Global | `@knot/explorer` | Select the active file in the explorer whenever it changes. |
| `explorer.showIgnored` | boolean | `true` | Global | `@knot/explorer` | Show files and folders that ignore rules match, dimmed; off hides them. |
| `projects.rememberedLimit` | integer | `30` | Global | `@knot/projects` | How many unnamed projects to remember. The least recently opened are forgotten first; named and active projects, and projects holding unsaved text, are kept. |

The other bundled packages (`find`, `go-to-file`, `go-to-line`, `language-*`, `status-bar`, `themes`) define no settings; some write native keys, as noted
above.
