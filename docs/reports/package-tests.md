# Package behavior tests

Findings from reviewing the tests of the bundled Vim package (D058) as the
case count grew, with the shared runners the language packages use. They
support D154.

## Current design

Package behavior is tested at three levels:

- `node:test` covers pure modules only: for Vim, the key parser, motions,
  text objects, registers, and the jumplist. No test stubs a native command
  (see [extension authoring and tests](../decisions.md#extension-authoring-and-tests)).
- JSONC case files beside a package run through the real editor from a Rust
  product test. Language packages keep `comments.jsonc`, `indents.jsonc`,
  and `brackets.jsonc`, run against an `EditorView` in a test window.
  `@knot/vim` keeps `test/cases.jsonc`, run in a full product window with
  every bundled package started, by pressing real keystrokes without
  settling between them, so ordered key delivery, native commands, undo
  groups, and indent queries all take part.
- Bespoke product tests in `src/app/product/tests/vim.rs` cover what a case
  cannot express: the status bar, caret shapes, mouse drags, a package
  restart, soft wrap, and global marks across tabs.

The layering is sound and worth keeping: cases are readable, reviewed
beside the package, and exercise the native paths the package relies on.

## Problems

### Speed

The Vim cases run serially in one `#[gpui::test]`. On Apple M1 Pro/macOS
debug builds, 333 cases took 85 s, about 255 ms each: opening the product
window 51 ms, pressing the case's keys and settling 195 ms, and closing the
window and releasing the document 10 ms. With the scrolling cases, 364
cases took 94 s. Cases should take milliseconds.

Most of the cost is pressing keys, not the fixture. That path is not
profiled yet. Candidates:

- host round trips per key, each a foreground request and a resumed
  isolate turn: a motion reads the selection, then a line (range and
  snapshot) and the tab-size setting, then sets the selection; the
  selection listener then reads them again; and the status bar items are
  updated twice after every key even when nothing changed;
- the cost of each round trip in a debug build;
- the runner's `wait_until` polling loop and `run_until_parked` after every
  case.

Round trips per key also set real typing latency in the release app, so
reducing them helps users as well as tests.

### State leaking between cases

Each case gets a fresh document and editor, so per-editor state (mode,
jumplist, pending keys, the `ctrl-d` amount) starts clean. Package-wide
state does not: the registers, the clipboard, the last `f`/`t` for `;`,
global marks, and the persisted register record carry over from earlier
cases. Today's cases appear to set up what they read, but nothing enforces
it, so a case's result can depend on its position in the file.

### Duplicated runners

Finding case files, parsing JSONC, the `‸` marker notation, and failure
reporting live in `language_indents.rs`; `language_editing.rs` keeps its
own copies of the first two; `vim.rs` reuses some and has its own
`marked`. The key notation (`<esc>`, `<c-x>`, `<lt>`) and the product
window fixture that presses keys and settles are in `vim.rs`, though
nothing in them is specific to Vim.

### The case format

`cases.jsonc` is one flat file of about 400 lines, grouped only by
comments. Each new kind of expectation adds an optional field to one
struct: `language`, `mode`, and now `view` with `lines`, `top`, and
`afterTop`. Scrolling cases need 30-line texts written out in full in
every case, because the format has no shared or generated text. A failure
names the case but not its file or line, and before `KNOT_CASES` no single
case could be run.

### Parser tests pinned to internals

`keys.test.ts` compares exact `Command` objects, so reshaping a command
rewrites tests that the behavior cases already cover. Parser tests earn
their place where cases cannot see the result, such as the pending-key
display and the count limit.

## Direction

- **Fast cases.** Profile the key path first. Then reuse one window and
  editor across cases, replacing the document rather than creating and
  removing windows, and cut the package's round trips per key. A window
  stays necessary: key routing, key contexts, the key hold, and the
  visible lines all live in it. A fake host is not the answer, because the
  cases exist to exercise the native commands a fake would stub.
- **A reset between cases** that makes package-wide state deterministic:
  at least the clipboard, and a way to reset a package's own state, such
  as a fresh lifecycle or a test-only reset, chosen by cost.
- **One shared harness** for finding case files, parsing them, markers,
  filtering by name, and collecting failures with their file, used by the
  comment, indent, bracket, and Vim runners. The key notation and the
  key-driven product fixture move beside it, so another key-driven
  package, such as a Helix mode or a modal tree, reuses them.
- **Topic files** under a package's `test/cases/`, each its own gpui test,
  so they run in parallel and fail separately, with expectations grouped in
  objects (`view`, later others) rather than loose fields, and shared or
  generated starting texts for cases that need long documents.
- **Fewer parser tests**, limited to what cases cannot express.

D133's TypeScript case modules would replace the JSONC format; whether
they are worth adding in the same work is part of D154.
