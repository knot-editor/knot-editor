# D229 — Composite views and the diff as a package

Status: 🚧 planned. Source: [roadmap D229](../roadmap.md), part of the
[version control plan](vcs.md), milestone 3.

## Outcome

Built-in features are not privileged: the diff view becomes `@knot/diff`, a
package built only on public APIs, and the core gains what it needs so any
extension can mix and match the same parts. Along the way:

- Extensions see the views inside a tab as a tree: every view, including an
  editor nested at any depth, has its own identity, and focus names the
  focused leaf. Vim, the status bar, and every other editor-driven package
  work in any editor wherever it sits, so Vim works in a diff on both sides.
- Extensions register composite views: center tabs whose layout of child
  views (editors, text frames, trees, nested composites) the package
  describes and the user cannot rearrange.
- Extensions own buffers, use block rows, and link editors' scrolling with a
  connector strip between them.
- A diff side's editability follows its content, as in IntelliJ: either side
  can be an editable document, and a snapshot is read-only. The diff lock goes.

## Today

- The workbench maps each tab to one native view (`PresentationView` in
  `src/app/workbench.rs`): an editor, a terminal, a diff, or a contributed
  tool view. `DiffView` (`src/app/diff_view.rs`) is the only view holding
  others: two `EditorView`s it diffs, aligns, scroll-syncs, joins with a
  painted strip, and shows inline as block rows. Native commands and keymaps
  already resolve to the focused editor inside it through `CommandView`.
- Extensions see only tabs: `ViewTarget` names a tab presentation, with a
  document and buffer only for document tabs. Contributed views have a
  separate instance `ViewId`, and their content is one tree or text frame in
  a tool window.
- `workbench.editor(diffTab)` succeeds by accident and follows whichever side
  last had focus, as does a diff tab's command editor. Switching sides fires
  no `activeTargetChanged`.
- Extensions open a diff with the native `workbench.openDiff` (original a
  snapshot, modified a snapshot or a document); `@knot/vcs` uses it. The diff
  has a lock (`diff.toggle-read-only`), a per-view flag on the model `Editor`
  that extension buffer edits bypass.
- Packages find a target's tab by searching `pane.tabs` themselves.

## Design

Agreed in discussion:

- **The workbench is unchanged.** Panes and tabs, with drag, split, and
  relocation acting on tabs only. A tab presents one view.
- **A view lays out its own contents.** A view may contain child views to any
  depth, including internal tabs; its layout is its own, and the workbench
  cannot rearrange it, so nothing is dragged out of a composite.
- **Extensions see the view tree.** Every view has an identity and a kind
  (`editor`, `terminal`, `tree`, `text`, `composite`, …) and lists its
  children. The tab keeps its target for tab operations; views get their own.
  Focus reports the focused leaf view beside the focused tab. Editor views
  carry their document (null for an extension buffer), buffer, language, and
  read-only state.
- **Editors bind exactly.** `workbench.editor` takes an editor view at any
  depth and never follows focus; a command's editor is the leaf focused at
  admission.
- **A shared lookup** in the `knot` module resolves a view in a `WindowInfo`,
  and packages use it instead of searching `pane.tabs`.
- **Composite views are declarative.** A package registers a composite kind
  and opens instances as center tabs with arguments; each instance's layout
  is data (rows, columns, and internal tabs with sizes, nested), whose
  children are editors on a document or an extension buffer, text frames,
  trees, or composites. The package replaces the layout while the tab is
  open, and kept children keep their identity. A toolbar of command buttons
  with state belongs to the instance. Instances belong to the package's
  lifecycle and are not restored. A document editor inside a composite
  counts as a view of its document for closure and view counts. This keeps
  the [rule](../decisions/platform.md#views-and-extension-ui) that extensions
  supply data, never rendering.
- **Extension buffers.** A package creates buffers with text and a language,
  read-only or editable, outside document identity, owned by its lifecycle.
  They are never dirty; a package can give one a key under which its text
  persists across restarts until the package clears it.
- **Native primitives fed by data.** Block rows on an editor that show lines
  of a source buffer with its highlighting; linked scrolling
  between two editors through a line map the package supplies; a connector
  strip child that joins line ranges of two sibling editors in color keys.
  Per-frame geometry stays native.
- **The diff package.** `@knot/diff` builds the diff from the native line
  diff, view decorations, gutter contributions, and the parts above, and
  offers opening a diff as a typed contract that `@knot/vcs` uses. Either side
  is a document or a snapshot; a document side is editable, a snapshot side
  read-only; there is no lock. Native `DiffView`, `workbench.openDiff`, and the
  native diff commands and settings are removed.

  This reverses the [diff view decision](../decisions/workbench-and-lifecycle.md#diff-view),
  which rejected a package that diffs and pushes hunks to the view for its
  latency and state. Built-in features should not be privileged, and the parts
  the diff needs serve any extension and the three-way merge. The cost stays
  small: `diff()` reads both buffers natively, so no text crosses to the
  package, and returns each side's revision, so the package debounces rediffs
  and drops stale results. What is new is pushing decorations, the line map,
  and block rows after each rediff.
- **Vim** follows editor views, refuses edits on a read-only editor with Vim's
  `'modifiable'` error, and maps `]c` and `[c` to the diff's hunk commands.

Rejected: a native diff with per-side targets, and diff-specific `sides` on
tabs, which keep built-in features privileged and give a three-way merge or a
mixed view nothing; nested workbenches with draggable tabs inside a view.

## Exclusions

- Restoring composite views and diffs: like diff tabs today, they stay out of
  window layouts, project sessions, and Reopen Closed Tab.
- Read-only documents (a lock, file permissions).
- Nested workbenches: panes and tabs inside a view that the user can
  rearrange.
- Vim's `do`, `dp`, `:diffget`, `:diffput`.
- A three-way merge view (D219), which builds on these parts.

## Deferred decisions

Each is decided at the review gate of the checkpoint named.

- Names for the view identity and the focused-view fields, and how they sit
  beside `ViewTarget` and the contributed view `ViewId` (1).
- The lookup's name and placement (5).
- The layout data's shape: sizing, internal tab strips, and how a child is
  named so layout updates keep it (10, 13).
- A block row's data shape: a source buffer range, and whether plain text is
  also accepted (15).
- Whether the connector strip and linked scrolling take line maps or hunks
  (16, 17).
- Settings and key context names `@knot/diff` takes over from native code (18).

## Checkpoints

Each phase's last checkpoint updates the architecture references, extension
guide, and decisions for that phase, so documentation never lags a phase.

### Phase A — The view tree

#### 1. View identity ✅

Each tab's `PresentationInfo.root` is a `ViewNodeInfo` tree. Views list their
children through the `ViewTree` interface in `src/app/view_tree.rs` and emit
`ViewTreeChanged`. `ViewNodeTarget` is the view's identity, `ViewTarget` stays
the tab's target, and `ViewId` the contributed view instance. `"diff"` leaves
the `ViewKind` and `kind` union with the native diff (21). Reference docs
follow in checkpoint 5.

#### 2. Focused view ✅

`WindowInfo.focusedView` is the focused leaf view of the focused tab and
`statusView` the editor view a status display describes, at any depth and for
any editor, not only document editors (a snapshot side counts). A diff emits
`ViewFocusChanged` when its focused side changes, which publishes
`presentationsChanged`. `statusTarget` is unchanged until checkpoint 5, which
also writes the reference docs.

#### 3. Exact editor handles ✅

`workbench.editor` takes a `ViewNodeTarget` of an editor view at any depth and
never follows focus; a handle goes stale when its view leaves the tree, and a
tab target or non-editor view is rejected. Moving a tab keeps its views, so a
handle survives the move. Vim attaches only to document tabs whose root is an
editor until checkpoint 6. The extension guide's
[editor handle](../extension-api/commands-and-keys.md) section describes it.

#### 4. Command captures ✅

A command captures the view that had focus in its tab at admission; its
`context.editor` is that exact editor, and the target becomes invalid once the
view leaves the tab. `CommandTarget.focusedView` and `viewChain` (the views
above it, parent first, the tab's root last) are described in the extension
guide's [commands](../extension-api/commands-and-keys.md) section. Targets
captured without a focused view (closes) still resolve the tab's live focused
editor.

#### 5. The lookup and its consumers ✅

`windows.findView(window, target)` resolves a `ViewNodeTarget` or a tab's
`ViewTarget` in a `WindowInfo` and returns `{ view, parents, tab, pane }`.
`@knot/chrome`, `word-highlight`, `go-to-file`, `explorer`, `vcs`, and `find`
use it and follow `statusView`, so the status bar follows a focused diff side.
Vim's `pane.tabs` searches wait for checkpoint 6. The status bar text in a
diff was checked by hand: the fake host has no diff tabs, and the Rust test
covers the lookup and `statusView`. Documented in the
[view tree](../architecture/workbench-and-lifecycle.md#view-tree) and
[application metadata](../architecture/extension-host.md#application-metadata-queries)
references and the [extension guide](../extension-api/application.md).

### Phase B — Vim

#### 6. Vim follows editor views ✅

Vim follows the window's `statusView` and a key's captured `focusedView`, so an
editor at any depth is followed, including one without a document (a snapshot
diff side). Each followed editor keeps its own state across side switches, and
a document-less editor's marks are per view. Global marks and the Ex commands
that need a file (`:w`, `:sav`, `:e`, `:set`, `:wq`) do nothing or answer
`E32` there. Covered by Rust tests on a real diff in both layouts; the pinned
request-count test is unchanged. Checked by hand in a diff.

#### 7. Read-only editors ✅

Vim refuses edits and insert mode on a read-only editor, and on any
`BufferReadOnly` rejection, with `E21` as for `'modifiable'`. Entering insert
mode asks the host whether the editor is read-only, one more request than
before in the pinned request-count test. Documented in the
[extension guide](../extension-api/commands-and-keys.md). Checked by hand on a
snapshot side.

### Phase C — Buffers

#### 8. Extension buffers

Deliverables: a package creates a buffer with text and a language, read-only
or editable, outside document identity, released with its lifecycle or on
disposal; the existing buffer API works on it. An extension buffer is never
dirty: no closing prompt, no unsaved-changes marker, and Save does nothing.
Creating one can take a `persist` key the package chooses (such as a commit
message per repository): the text is kept in the package's durable storage,
debounced and flushed at shutdown, and a buffer created with the same key
starts from it; the package clears it through the buffer, and disposal keeps
it. Phase C documentation.

Validation: host tests for creation, edits and `BufferReadOnly`, language,
disposal, teardown, Save, and persisted text across a restart, the shutdown
flush, and clearing.

Review gate: the API.

Implementation context: snapshot buffers in `Side::snapshot`
(`src/app/diff_view.rs`) and text frames' private buffers
(`src/app/product/tests/text_frames.rs`); `BufferModel::from_read_only_text`
in `crates/knot-model/src/buffer.rs`; buffer tracking in
`src/app/extensions/buffers.rs`.

### Phase D — Composite views

#### 9. Minimal composite

Deliverables: a package registers a composite kind and opens instances as
center tabs with arguments, each showing one editor on an extension buffer;
instances close with the lifecycle or on disposal; they appear in the view
tree, and Vim attaches to their editor. The package sets an instance's title,
icon, and optional resource URI, and changes them while it is open; with a URI
the tab shows that resource's decorations as a document tab does. As diff tabs
today, the title truncates with the full title in a tooltip.

Validation: product tests for open, focus, close, teardown, the view tree, and
tab title, icon, and decoration updates.

Review gate: registration and instance API.

Implementation context: `registerView`/`ViewContribution` in `knot.d.ts`;
`ExtensionView` and `PresentationView` in `src/app/workbench.rs`; tab surface
kinds (`TabSurfaceId`) in `crates/knot-model/src/workbench.rs`; contributed
views in `src/app/extensions/semantics.rs`. The composite implements the
child-view interface from checkpoint 1.

#### 10. Layout tree

Deliverables: rows and columns with sizes, nested composites. The layout names
the child focused on open, defaulting to the first editor in layout order, or
the first child without one; reselecting the tab restores the child focused
last.

Validation: layout and focus-traversal tests, and tests for focus on open and
on reselection.

Review gate: the layout data's shape.

Implementation context: the composite's native view beside `DiffView`'s
side-by-side layout in `src/app/diff_view.rs`.

#### 11. More child kinds

Deliverables: text frame and tree children; internal tabs.

Validation: tests per child kind and for tab switching inside a composite.

Review gate: child kinds by hand.

Implementation context: `TreeView` (`src/app/tree_view.rs`) and `TextView`
(`src/app/text_view.rs`) as children.

#### 12. Document editors as children

Deliverables: an editor child on a document counts as a view of it, once per
tab however many editors the composite holds on it, so removing one of them in
a layout update never prompts; closing the composite prompts for each dirty
final view. The tab shows the
unsaved-changes marker exactly when closing it would prompt; extensions cannot
set it.

Validation: closure tests with one and two dirty documents and with two
editors on one document; the marker with
and without another view of the dirty document.

Review gate: closure behavior.

Implementation context: `diff_documents`, `shown_document`, and `view_count`
in `crates/knot-model/src/workbench.rs` generalize to composites; the closure
path in `src/app/workbench.rs` and `src/app/product.rs`. `@knot/projects`
counts a document's views by `tab.document` (`src/layout.ts`, `src/adopt.ts`)
and must see composite children.

#### 13. Layout updates

Deliverables: the package replaces an open instance's layout; kept children
keep identity, handles, and focus. Removing the focused child moves focus to
the layout's initial focus child.

Validation: tests for kept, added, and removed children, including the focused
one.

Review gate: child naming.

Implementation context: the composite view from 9–10; stale-handle refresh
from 3.

#### 14. Toolbar

Deliverables: an instance's toolbar of command buttons with icons, tooltips,
and toggled or disabled state the package updates. A button runs its command
with its own instance as the target, wherever focus is. Phase D documentation.

Validation: tests for buttons running commands, including with focus outside
the instance, and state updates.

Review gate: the toolbar; phase D docs.

Implementation context: the diff toolbar in `src/app/diff_view.rs` and tree
frame actions (`TreeAction`) as models.

### Phase E — Editor primitives

#### 15. Block rows

Deliverables: a package sets read-only block rows above lines of an editor it
holds, each showing a line range of a source buffer with that buffer's
highlighting and an optional band color key; rows repaint when the source's
syntax arrives or its text changes.

Validation: tests for placement, edits around them, and highlighting from the
source buffer.

Review gate: the API.

Implementation context: `set_blocks` and `Block` in
`crates/knot-model/src/editor.rs`, the wrap map in `editor/wrap.rs`, and
existing tests in `src/app/editor/tests/blocks.rs`.

#### 16. Linked scrolling

Deliverables: a composite links two editor children's scrolling through a line
map the package supplies and replaces.

Validation: scroll-mapping tests matching today's `map_line` behavior.

Review gate: the link's data shape.

Implementation context: `sync_scroll` in `src/app/diff_view.rs` and
`knot_model::diff::map_line`.

#### 17. Connector strip

Deliverables: a child kind between two sibling editors that joins line ranges
in color keys, following their scroll. Phase E documentation.

Validation: tests for the strip's geometry against both editors.

Review gate: the strip by hand; phase E docs.

Implementation context: the connector painting and `CONNECTOR_WIDTH` in
`src/app/diff_view.rs`.

### Phase F — The diff package

#### 18. `@knot/diff` side by side

Deliverables: the package opens a diff as a composite through a typed
contract: either side a document or snapshot, bands, gutter wedges, rediff
after edits, linked scrolling, the strip, hunk commands, and a toolbar. Vim's
`]c` and `[c` run the hunk commands.

Validation: package behavior tests matching today's diff tests; Vim cases for
`]c`/`[c` wrapping.

Review gate: the diff by hand against the native one.

Implementation context: model on `@knot/vcs`/`@knot/git` for package layout
and contracts (`crates/knot-ext-vcs`); the native line diff from
[compare texts](../extension-api/application.md#compare-texts); behavior to
match in `src/app/diff_view.rs` and its tests; Vim bindings beside the named
keys in `extensions/@knot/vim/src/extension.ts`.

#### 19. Inline layout

Deliverables: the inline layout through block rows, the layout toggle, and the
layout setting.

Validation: tests for inline bands and toggling.

Review gate: inline by hand.

Implementation context: inline rows in `src/app/diff_view.rs`; `diff_layout`
in `src/app/settings.rs`.

#### 20. Switch `@knot/vcs`

Deliverables: `@knot/vcs` opens diffs through the contract; the native diff
stays for comparison.

Validation: vcs diff tests through the package.

Review gate: vcs diffs by hand against the native one.

Implementation context: `openDiff` use in `extensions/@knot/vcs/src/extension.ts`;
tests in `crates/knot-ext-vcs/tests/vcs/diff.rs`.

#### 21. Remove the native diff

Deliverables: native `DiffView`, `workbench.openDiff`, `diff.toggle-read-only`,
the per-view `Editor` read-only flag, and native diff commands and settings are
removed. Phase F documentation, including replacing the [diff view decision](../decisions/workbench-and-lifecycle.md#diff-view).
D229 leaves the roadmap, is marked ✅ in the [version control plan](vcs.md),
and this plan is removed.

Validation: the full suite.

Review gate: removal and docs.

Implementation context: `PresentationView::Diff`, `TabSurfaceId::Diff`, and
their callers; native command registration in `src/app/diff_view.rs`;
`set_read_only` in `crates/knot-model/src/editor.rs`.
