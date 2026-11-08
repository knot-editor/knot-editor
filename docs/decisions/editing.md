# Editing decisions

Native editing behavior over the buffer and its syntax tree. The [decisions
index](../decisions.md) lists every topic, and the [documentation
guide](../README.md#decisions) sets the entry rules.

## Language editing configuration

Comment tokens and bracket pairs describe a language, so they live on the
language registration rather than in settings; enabling a feature stays a
setting. One bracket list with per-pair flags serves matching, auto-closing,
and surrounding, as in Zed, instead of VS Code's three lists. Indentation comes
from tree-sitter queries, not regexes. Bundled data drops pairs that text
matching would mislead, such as `<` in Rust.

Packages keep behavior cases as JSONC beside their data, run natively by a
generic Rust test, because a package test against the fake host could only
restate the data.

References: [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages)
and [extension guide](../extension-api.md#register-a-language).

## Indentation

Use nvim-treesitter's indent captures and semantics, evaluated natively, so its
queries adapt with light edits. Knot departs where upstream mishandles ordinary
code, such as string contents, block comments, unclosed brackets inside parse
errors, and Python dedent after `return`.

Enter commits first and indents from the reparsed tree, as Zed and Neovim do,
because Helix's pre-edit tree cannot see the typed text. The correction joins
the edit's undo step and is dropped if another commit lands first. Typing only
outdents, and only when the match reaches the caret.

References: [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages)
and [editor views](../architecture/workbench-and-lifecycle.md#editor-views).

## Brackets

Matching, auto-closing, and wrapping are native over the registered pairs,
because the closer must land in the opener's commit and undo step, which an
asynchronous extension command cannot join. Data stays programmable through the
registration, and settings turn behavior off.

Matching pairs tokens within one syntax node rather than scanning text, so
strings and comments never match and no per-language query is needed.
Behavior follows VS Code's defaults; overtyping and pair deletion apply only to
closers the view inserted, so fixing unbalanced code can still add a `)`.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views)
and [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages).

## Editor gutter

The gutter is a fixed order of native lanes, and extensions share one marker
lane, because nothing needs a second lane and each would need its own width and
input rules. The marker lane is always reserved so text never shifts when a
diagnostic appears. Documents show the gutter and text frames opt in through
their registration, since one setting default cannot express that split.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views)
and [extension guide](../extension-api.md#read-only-text-frames).

## Editing commands

Editing commands follow VS Code's behavior and default bindings, the
conventions most users already know; macOS keeps Cocoa's text keys. Each
command is one model batch recording the selection before and after. Move
Lines deletes and reinserts the neighbouring line so anchored contributions
survive.

Indentation style is detected once on load and written as a document-scoped
`editor.insertSpaces`, so it is one ordinary setting that the status bar and
extensions change. A mixed file falls back to the default rather than guessing.

Words split at VS Code's separators, not at UAX #29 boundaries alone, which
kept `some.thing` as one word.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views)
and [semantic UI](../architecture/semantic-ui.md#syntax-highlighting-and-languages).

## Zoom, text size, and soft wrap

Zoom is one application level, as in VS Code, so everything including chrome
scales consistently. It is a state-database record, not a setting, because
settings are rebuilt from code at launch. `font.size` is the per-scope setting.

Wrapping measures monospace cells with `unicode-width` rather than shaping,
which would cost a shaping pass per line on every resize. Views rewrap only
changed lines from a per-revision log on the model, since the extension change
queue cannot be consumed by one view. Toggle Soft Wrap writes a
document-scoped setting so every view agrees.

Row and logical-line movement both exist, as Emacs pairs `next-line` with
`next-logical-line`, so a keymap chooses per key.

References: [editor views](../architecture/workbench-and-lifecycle.md#editor-views)
and [zoom](../architecture/workbench-and-lifecycle.md#zoom).

## Folding

Fold state belongs to the view, as anchored byte ranges, so two views of one
document fold independently and edits keep a fold over its text. Hidden lines
are zero-row lines in the wrap map rather than a second line-to-row mapping, so
the viewport, scrolling, row queries, and painting count visible rows with no
parallel bookkeeping. Regions come from a per-language `folds` query, with
indentation for languages without one, because every language folds usefully
at once and a package refines it later; regions are only candidates, and just
collapsed folds are state and persist with the tab's view state. The caret
never rests in hidden lines, and revealing a hidden position expands its folds,
as VS Code does, rather than refusing the move.

Rejected: fold state in the buffer, which would couple views and need an
ownership story across splits; and a fold-aware second row space, which every
row consumer would have to translate through.

Reference: [editor views](../architecture/workbench-and-lifecycle.md#editor-views).
