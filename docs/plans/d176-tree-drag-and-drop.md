# D176: Drag and drop in trees

Status: 🚧 approved, checkpoints 1 and 2 done. Roadmap item [D176](../roadmap.md).
References: [extension trees](../architecture/semantic-ui.md#extension-trees),
[tree content and actions](../decisions/semantic-ui.md#extension-tree-content-and-actions),
[workbench and lifecycle](../architecture/workbench-and-lifecycle.md).

## Outcome

In the explorer, dragging a file or folder onto a folder moves it there, and
Option+drag copies it. Dropping onto a file uses that file's folder, as Paste
does. Dragging a file row onto the editor area opens it in the pane under the
pointer, or in a new split at a pane edge, with the same previews as tab
dragging. Every contributed tree can opt into this; the explorer is the first
user.

## Design

- **Row payload.** `TreeItem` gains `draggable`, `dropTarget`, and `resource`.
  `draggable` is the only switch that makes a row draggable. Inside the tree,
  it can be dropped onto `dropTarget` rows. A `resource` URI also makes the
  editor area a drop target, opened natively. A row without `draggable` can't
  be dragged, whatever else it carries.
- **Drop command.** The view contribution gains a `drop` command. A drop
  inside the tree runs it over the captured-window command route, like row
  actions. Its argument is
  `{ source: { id, arguments }, target: { id, arguments }, copy }`, where
  `copy` is true when Option is held at drop time. The extension decides what
  a drop means and rejects one it can't do, such as a folder dropped into
  itself. There is no separate callback path.
- **In-tree interaction.** The drag starts after the usual pointer
  threshold. The row under the pointer is highlighted when it is a
  `dropTarget` other than the source. Hovering briefly over a collapsed
  target expands it. Dragging near the top or bottom edge scrolls. Escape or
  a drop elsewhere cancels. Drops go onto rows only; there are no
  between-row positions.
- **Editor drop.** A row with a `resource` shows the pane and split-edge
  previews that tab dragging uses, and it opens the resource as a pinned tab
  there through the ordinary open path. Folders and resources that fail to
  open report through that same open path. No preview tab is involved.
- **Explorer.** Rows of files and folders are `draggable`, with their URI as
  `resource`. Folders and files are `dropTarget`s, and roots are drop
  targets but never draggable. The explorer reuses Paste's move (rename) and
  copy, with its conflict and "already there" handling. A move is not
  confirmed.

## Exclusions

- Dragging rows out to Finder or other apps (D205), and dropping OS files into
  trees.
- Dragging between trees or windows, and dropping onto terminals or tool
  windows.
- Multi-row drags. The payload becomes a list with multi-select (D177).
- Dropping between rows to reorder.

## Deferred decisions

- How long to hover before a target expands, and the size of the edge
  scroll zone. Both are tuned in checkpoint 1 against how the result feels.

## Checkpoints

### 1. Drag and drop inside a tree ✅

Deliverables: the `draggable`, `dropTarget`, and `drop` contract, the native
drag interaction in `TreeView`, and the drop command event.

Validation: model tests for item validation and drop argument conversion.
`TreeView` gpui tests for the start threshold, target highlighting, rejecting
a drop on the source or a non-target row, the Option copy flag, auto-expand,
edge scroll, and cancellation. A host test that a drop reaches the
contribution's command with the right arguments.

Implementation context:
- Contract: `crates/knot-protocol/src/lib.rs` (`TreeItem` around line 415,
  and the view contribution), `crates/knot-host/src/js/knot.d.ts`
  (`TreeItem`, `ViewContribution`), `docs/schemas/knot.schema.json`, and an
  example under `scripts/extensions/examples/`.
- Conversion and validation: `src/app/extensions/semantics.rs`, the same path
  that carries row `arguments` and actions.
- Native: `src/app/tree_view.rs`. Row rendering is around the `on_click` and
  `on_mouse_down` handlers near line 1637. Mirror the gpui
  `on_drag` / `drag_over` / `on_drop` use in `src/app/product/stripes.rs`.
  Emit a `TreeViewEvent` alongside `InvokeCommand`, routed by the window
  through the captured-window command path that row actions use.

Review gate: in a `Try:` tree registered by `post-init.js`, drag rows between
targets and check the reported arguments, highlighting, expansion, and
scrolling.

Done: the contract and native drag live in `src/app/tree_view/drag.rs`, with
the 700 ms hover expansion and 28 px edge scroll zone kept after review.
`TreeItem.resource` lands with checkpoint 3.

### 2. Explorer moves and copies by drag ✅

Done: explorer rows declare the payload, and the `drop` command in
`extensions/@knot/explorer/src/clipboard.ts` moves or copies through the helper
Paste shares. A folder dropped into itself or a descendant is refused.

### 3. Dropping a resource on the editor area

Deliverables: `TreeItem.resource`. Over the center, a resource drag shows the
tab-drag pane and split previews, and a drop opens the resource pinned in that
pane or new split. Explorer file rows set `resource`.

Validation: product gpui tests for dropping into a pane, onto a pane edge, and
into the empty center; for a row without a resource offering no center
target; and for a failing resource reporting through the open notice.
Explorer tests confirm rows carry their URI.

Implementation context:
- `TreeItem.resource` as in checkpoint 1. The resource travels in the drag
  value.
- Center targets: `src/app/product/tab_drag.rs`. `TabDragState::target_at`
  and the pane, split, and empty-center geometry already paint
  per frame. Look up targets as for a document surface, and keep this drag
  inside its window, so the cross-window coordinator is not involved.
- Opening: `src/app/product/open.rs` (`start_open_operation_with`,
  `present_open_document`) with an `OpenTarget` (`src/app/product.rs`) for
  the destination pane. A split edge first creates the pane through the
  workbench split used by tab relocation.

Review gate: drag explorer files into panes, onto split edges, and into an
empty center.

### Closing

Update [semantic UI](../architecture/semantic-ui.md#extension-trees) and the
tree decision with drag and drop, update the
[extension guide](../extension-api/README.md), and remove this plan.
