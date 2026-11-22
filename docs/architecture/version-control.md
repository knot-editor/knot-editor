# Version control

Part of the [architecture](../architecture.md). [Decisions](../decisions.md)
records rationale ([backends](../decisions/bundled-packages.md#version-control-backends),
[Git baseline](../decisions/bundled-packages.md#git-baseline)). The contracts
are described in [packages](../extension-api/packages.md#contracts-knot-defines).

Version control is two bundled packages in the `knot-ext-vcs` crate. `@knot/vcs`
is backend-agnostic and owns the change gutter and navigation; a backend such as
`@knot/git` only answers questions about repositories. Several backends can be
active at once.

## Repositories

Each backend is asked for the repositories covering every window's roots: the
one containing a root and those nested below it. A document under none of them
is looked up from its own directory, as if it were a root, and the answer is
cached per directory. When several repositories contain a file, whichever
backend serves them, the deepest root wins. Repositories are found again when a
backend registers or is dropped, when a window opens or its roots change, and
when a backend reports a change.

A backend that stops is dropped when `@knot/vcs` next fails to bind its
contract, and its marks are cleared.

## Gutter

`@knot/vcs` follows every open document with a file through `documents.track`.
A backend supplies a baseline, which is text or none; a file with none (untracked,
ignored, or binary) has no gutter. The baseline is cached per document and
fetched again when the backend reports a change, when the document's file
changes, and when the document goes from dirty to clean.

Edits rediff the buffer against the cached baseline after a fixed 150 ms
debounce, using the extension API's `diff`. Each hunk becomes one contribution:
a `diff.added` bar for an insertion, a `diff.modified` bar for a replacement,
and a `diff.removed` wedge for a removal. The contributions are replaced at the
revision that was diffed, and a diff an edit raced is repeated. The marks run no
command.

## Navigation

`vcs.nextChange` and `vcs.previousChange` (Alt+F5 and Alt+Shift+F5 in editors,
`]c` and `[c` in Vim mode) diff the focused editor against its cached baseline
and move the caret to the first line of the nearest hunk, wrapping at the ends.
A removal's line is the one that followed the removed lines. A document without
a baseline or changes leaves the caret where it is.

## Git

`@knot/git` declares the `process` entitlement, implicitly granted to a bundled
package, and runs `git`; without it on the `PATH` it raises one notice and does
not register.

- **Repositories** come from `git rev-parse`. A root URI is derived from
  `--show-prefix` and the directory's own URI, never from Git's absolute path,
  so a symlinked directory still matches document and window URIs.
- **Baseline** is HEAD, read with `git cat-file --filters` so it is the
  checked-out text. A file in the index but not in HEAD has an empty baseline
  and shows as added. Reads set `GIT_OPTIONAL_LOCKS=0`.
- **Freshness** comes from `files.watch` with `includeHidden`, since `.git` is
  hidden. For each repository it has served, `@knot/git` watches the git
  directory (HEAD, the index, `packed-refs`) non-recursively, the common
  directory non-recursively when it differs, and the common directory's `refs`
  recursively, and reports `changed` to `vcs`. The object store is not watched.
  A watch ends when the git directory is gone, and the next lookup of a file in
  that repository watches again.

## Tests

`extensions/@knot/vcs/test` runs `@knot/vcs` over an in-memory host, including
moves and saves, which the product harness cannot drive. `crates/knot-ext-vcs/tests`
runs the packages in the launched product: `gutter.rs` over a fixed-baseline
backend and `git.rs` over temporary repositories. A missing `git` is untested.
