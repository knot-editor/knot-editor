# Extensibility

November 2026. Compares how far Knot's extension API reaches with Emacs,
Neovim, VS Code, and Zed, and lists what still separates it from Neovim's
depth. The current contract is `crates/knot-host/src/js/knot.d.ts`; runtime
behavior is owned by [extension host](../architecture/extension-host.md) and
[commands](../architecture/commands.md), and the open items by the
[roadmap](../roadmap.md).

## Summary

Knot scripts about as much of the editor as other editors do. It keeps
mechanics native and builds workflows as bundled extensions on the public API,
as VS Code does, and close to Neovim's C core with its growing Lua layer.

What really differs between editors is how deep a script can reach. Emacs can
redefine or advise anything. Neovim can't redefine its C motions, but it can
drive every buffer, window, cursor, mark, and mode, hook every lifecycle
event, and draw virtual text and floating windows. VS Code and Knot expose a
curated API, and Zed exposes very little. The target is Neovim's depth,
keeping Knot's rule that bundled code gets no private bypass.

The editing half of that depth is largely in place. The bundled Vim mode
(`@knot/vim`) was the forcing function: it runs on public APIs only, and its
needs produced the long-lived editor handle, key contexts with ordered key
delivery, undo groups, marks, screen-row queries, folds, input recording,
per-view decorations, and editor bars. The gaps that remain are mostly the
other half: lifecycle hooks, presentation that extensions own, interactive
generated buffers, and concurrency inside a package. The language client
(D067) and the version control work drive them.

## How far scripts reach

| | Emacs | Neovim | VS Code | Zed | Knot |
| --- | --- | --- | --- | --- | --- |
| Scripted share | Nearly everything above a C core | C core, growing Lua layer | TS core; many built-ins are bundled extensions | Nearly everything native | Mechanics native; workflows bundled on the public API |
| Script reach | Total: any function can be redefined or advised | Deep: buffers, windows, cursors, extmarks, modes, autocmds, floats | Curated; no editor internals or DOM; webviews | Narrow: languages, themes, LSP adapters | Curated semantic API, deep on editing |
| Key handling | Synchronous | Synchronous | Asynchronous, `when` contexts | Native key contexts | Asynchronous handlers; native ordered delivery and extension-set key contexts |
| Isolation | None | Lua in-process; remote plugins | One shared extension-host process | wasmtime per extension | Persistent V8 isolate per package, in-process |
| Configuration | Code | Code | JSON | JSON | Code (pre-init / post-init) |

Because handlers are asynchronous, anything that must land in the same commit
as typed text, such as auto-closing brackets, stays native. Neovim's autopair
plugins work only because its Lua runs synchronously
([brackets decision](../decisions/editing.md#brackets)).

## Remaining gaps

| Gap | Today | Who needs it | Item |
| --- | --- | --- | --- |
| Several and rectangular selections | One selection per view and handle | Vim visual-block, multi-cursor editing | D142 |
| Syntax-tree queries and text objects | Only `folds` and `indents` queries, used natively | Vim `if`, `ia`, `ac`, `it`; structural selection | D144 |
| Key contexts on non-editor views | Terminals, trees, and contributed views resolve by view kind | Modal keys in the explorer or terminal | D147 |
| Lifecycle hooks and native command overrides | Changes are observed after they commit | Format on save, LSP, customizing a native command | D115 |
| Virtual text and inlay hints | Decorations and gutter markers only | LSP inlay hints and diagnostics, blame, inline suggestions | D145 |
| Floating panels anchored to a position | None | Hunk popup, LSP hover | D220 |
| Interactive generated buffers | Read-only contributed text frames | Magit- or oil-style views, task output | D027 |
| Previewing a replacement | `:s` and Replace edit without a preview | Vim `inccommand`, Find | D201 |

Settings per buffer and language, which Neovim has through buffer-local
options, are covered: settings resolve by language and view.

## Guardrails

- **Generic primitives.** Key contexts, selection sets, undo groups, recorded
  input, and screen-row queries, not registers, operators, or modes. A Helix
  or Kakoune mode should fit on the same primitives, with no one feature's
  vocabulary in the boundary.
- **No private bypass.** If a bundled package needs something, every
  extension gets it.
- **Native measures stay native.** Expose queries over what native code
  measures (display cells, rows, folds, matches) rather than reimplementing
  it in JavaScript, and read what one key needs in one request
  ([editor handles decision](../decisions/extension-host.md#editor-handles)).
