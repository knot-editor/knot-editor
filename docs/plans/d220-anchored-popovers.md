# D220 — Anchored popovers

Status: 🚧 planned. Source: [roadmap D220](../roadmap.md), part of the
[version control plan](vcs.md), milestone 3. First consumer: the hunk popup
(D221); next: language-server hover (D067). Inline zones (D231) reuse its
content and sizing.

## Outcome

An extension can open a popover on an editor, anchored to a line range of its
buffer. The popover shows the same content a composite view does, a toolbar
of command buttons over a layout of editors, text frames, and trees, sized to
that content and scaled with zoom. It floats over the text without moving it,
follows the anchor through edits and scrolling, and closes on Escape, a click
outside, an edit, or the anchor's deletion, telling the package why. Views
inside it are ordinary views of the view tree, so Vim and other editor-driven
packages work in a popover's editor.

## Today

- Floating content in an editor is native only: the completion list
  (`src/app/completion.rs`, positioned by `EditorView::completion_anchor` in
  `src/app/editor/completion.rs`) is an absolutely positioned `div` in rems,
  clamped to the view and painted with the `popover.*` color keys; context
  menus (`src/app/context_menu.rs`) use gpui's `deferred` and `anchored`.
- Editor bars (`crates/knot-model/src/editor/bars.rs`,
  `src/app/editor/bars.rs`) are the precedent for extension-described,
  per-editor, per-owner surfaces opened from the editor handle (`openBar`),
  with key contexts for bindings and lifecycle cleanup through
  `EditorState::remove_owner_state`.
- Composite views (`src/app/composite_view.rs`, wire types and checks in
  `src/app/extensions/host/composites.rs`) build a toolbar and a layout of
  keyed members into views that join the view tree, keep their identity
  across updates, and route toolbar commands. Their sizes fill the tab, and
  member `pixels` are window pixels that zoom does not scale.
- In the [view tree](../architecture/workbench-and-lifecycle.md#view-tree)
  an editor is a leaf; composites are the only views with children.
- Block rows (`EditorState::set_blocks`, `Editor.setBlocks`) put read-only
  lines between buffer lines; they are the base D231 extends.

## Design

Agreed in discussion:

- **The name is popover.** Panes are the workbench's split areas and hold
  tool windows, so "panel" would blur them; the completion list already
  paints as a popover. The API is `editor.openPopover`, `closePopover`, and
  `onDidClosePopover`, the view-tree kind `popover`, and the key context
  `popover:<id>`.
- **Content is the composite layout.** A popover takes a `toolbar` and a
  `layout` with the same members as a composite view: `editor` on an
  extension buffer or a document, `text` and `tree` frames of embedded views,
  rows, columns, and `tabs`, with `key`s that keep views across updates. One
  layout builder serves composites and popovers, so views, the view tree, and
  command routing are not built twice. Extensions still supply data, never
  rendering ([views and extension UI](../decisions/platform.md#views-and-extension-ui)).
  Content description and sizing do not depend on floating placement, so
  D231 reuses them unchanged.
- **Popovers size to their content.**
  - Width is `{ pixels }` or `"editor"`, the editor's text-area width, as
    IntelliJ's hunk popup is. It is clamped to the text area, with no
    minimum: in a narrow editor the popover narrows, and its editors wrap or
    scroll as they do anywhere.
  - Height is natural: an editor member is as tall as its lines up to
    `maxLines`, a text frame likewise, a tree up to `maxRows`; a row is as
    tall as its tallest member and a column sums its members; the toolbar
    adds its own height. In a row, `weight` and `pixels` share the width as
    in composites.
  - The total height is clamped to the larger of the space below and above
    the anchor, less a small margin. Clamped members scroll themselves, so
    `maxLines` and `maxRows` are the consumer's preference and the editor's
    size is the hard limit.
- **Zoom scales popovers like other content-sized chrome.** Heights follow
  zoom already: an editor member's line height is `font.size` times the zoom
  factor, and the toolbar is in rems. A popover's `width: { pixels }` and
  the `pixels` members inside it are logical pixels multiplied by the zoom
  factor, as the completion list and picker scale their constants, so zooming
  in does not squeeze larger text into the same width. Clamping comes after
  scaling, in window pixels. The shared layout builder takes the scale for
  `pixels`: 1 for composites, which keep their window pixels like tool
  windows, and the zoom factor for popovers. A zoom change relays out an
  open popover on the next frame; nothing closes.
- **The anchor is a line range** in the editor's buffer, held as an anchored
  range so edits move it. The popover sits below the range's last line if it
  fits, otherwise above the first if it fits, otherwise on the side with more
  room at the clamped height. It never covers the anchor's lines and is
  clamped to the editor's horizontal bounds.
- **The popover follows scrolling.** It moves with its anchor and is hidden,
  not closed, while the anchor is out of view or folded away, so the user can
  scroll for context and come back. Deleting the whole anchor closes it.
- **Several popovers per editor.** Popovers are keyed by package and ID on the
  editor view (`editor.openPopover(id, spec)`); opening an open ID updates it
  in place, keeping keyed members. Popovers paint in opening order, newest on
  top, with no collision avoidance. A package that wants one at a time reuses
  an ID or closes its previous one.
- **Popovers are children of their editor in the view tree.** A `popover`
  node is a child of its editor's node, and the popover's layout is its
  children, built as composite members are. Popover views are ordinary
  `ViewNodeTarget`s: `workbench.editor` reaches a popover's editor, a
  window's `focusedView` is the leaf focused inside a focused popover, and a
  view's `parents` (popover, editor, ..., tab root) tell a handler which
  editor a popover belongs to. Editors are no longer always leaves, so the
  view-tree walkers and `Workbench::focused_leaf` are checked for that
  assumption.
- **Non-modal; focus is opt-in.** With `focus: true` the popover's focused
  member takes focus; otherwise the editor keeps it. While a popover is open
  its editor carries a `popover:<id>` key context, so a package binds keys
  such as next and previous hunk without moving focus, as bars do.
- **Dismissal is native and reported.** A popover closes on:
  - Escape: the focused popover, else the newest; pressing again closes the
    next;
  - a click in the editor's text or anywhere outside every popover; a click
    inside one closes none;
  - any edit to the anchoring editor's buffer, except one typed into a view
    inside a popover; this closes all of that editor's popovers, with no
    option to keep them, so a stale hover never lingers and the hunk popup
    closes on revert as IntelliJ's does, while staging, which does not edit,
    keeps it; a consumer that wants to survive an edit reopens with the same
    ID, and a `closeOnEdit` option waits for a consumer that needs it;
  - the anchor's deletion, the editor closing, an explicit `closePopover`,
    and package unload.

  `onDidClosePopover` reports the popover ID and the reason; an explicit close
  and unload report nothing.
- **Commands.** A toolbar button runs its command with the anchoring editor's
  tab as target and the anchoring editor as its editor, wherever focus is, so
  the hunk popup's revert acts on the edited file. A command run with focus
  inside the popover captures the focused popover view as its focus.
- **Floating only.** Content that pushes lines apart is D231.

## Exclusions

- Inline zones, block rows with live content (D231).
- Hover triggering: showing after the mouse rests, closing when it leaves, and
  delays; D067 adds them as native policy, possibly as a popover option.
- Popovers anchored outside editors (trees, terminals, the gutter itself) or
  to a column rather than lines.
- Collision avoidance between popovers, resizing or dragging one, restoring
  them with their window, and a `closeOnEdit` option.
- A bundled consumer; D221 is the first.

## Checkpoints

Each checkpoint ends at a review gate: `cargo test` for the touched crates and
the product tests named below pass, the extension API typechecks, and the
behavior is shown in the running app before the next starts.

### 1. A popover on the screen ✅ implemented, awaiting review

`editor.openPopover(id, { anchor, toolbar, layout })` with a single `editor`
member on an extension buffer and a fixed height opens a popover below the
anchor's last line; `editor.closePopover(id)` closes it. The anchor follows
edits, and its deletion, the editor closing, and package unload close the
popover. Several IDs coexist, newest on top.

Validation: model unit tests for spec checks, anchoring, ordering, and owner
removal; product tests opening, updating, and closing popovers from a fixture
package, including unload and anchor deletion.

Scope decision: the checkpoint accepts the composite layout format but only
a root group with one `editor` member on an extension buffer sized
`{ pixels }`, which is the popover's height (logical pixels, scaled by zoom);
the popover spans the text area. Toolbar, width, and richer layouts come in
checkpoints 2 and 3. An anchor whose text an edit empties counts as deleted.

**Implementation context.** Add `crates/knot-model/src/editor/popovers.rs`
mirroring `editor/bars.rs` (spec type with `problem()`, an ordered set keyed
by `ExtensionKey` and ID, `remove_owner`), held on `EditorState` next to
`bars` in `crates/knot-model/src/editor.rs` and cleared in
`remove_owner_state`; anchor the line range in the buffer's
`AnchoredRangeStore` as view decorations are. Host side: `editorOpenPopover`
and `editorClosePopover` in `src/app/extensions/host/handlers.rs` beside
`editor_open_bar`, decoding the layout with `decode_composite_layout` and the
toolbar like `decode_composite_tab`; the handle methods in
`crates/knot-host/src/js/bootstrap.ts` and types in
`crates/knot-host/src/js/knot.d.ts` beside `openBar`. Render in
`src/app/editor/paint.rs` where the completion list is added, positioned like
`completion_anchor`, with the `popover.*` color keys. Product tests in a new
`src/app/product/tests/editor_popovers.rs`, following `editor_bars.rs`.

### 2. Full content in the view tree

Every composite member kind works in a popover: rows, columns, `tabs`, `text`
and `tree` frames of embedded views, document editors, and keyed members that
keep their views across `openPopover` updates. Toolbar buttons run their
command with the anchoring editor's tab as target. The `popover` node joins
the view tree under its editor, `workbench.editor` reaches its editors, and
Vim works in a popover's editor.

Validation: product tests for keyed updates, toolbar command targets,
document editors counting as views for closure, the `popover` node and its
`parents`, and `workbench.editor` on a popover's editor.

**Implementation context.** Extract the group building, keyed reuse
(`Kept`), and leaf handling of `GroupView::build` and `Group` in
`src/app/composite_view.rs` into a layout module both composites and
popovers use, leaving `CompositeTab` as the tab owner; the builder takes the
scale for `pixels` (1 here until checkpoint 3). Toolbar dispatch follows
`ToolbarButton` in `composite_view.rs` with the anchoring editor as capture.
View-tree children of an editor go through the `ViewTree` interface in
`src/app/view_tree.rs`, with node building after `Group::node`; check
`Workbench::focused_leaf` in `src/app/workbench.rs` and the snapshot code in
`src/app/application_observation.rs` for leaf assumptions, and add `popover`
to `ViewNodeInfo.kind` in `knot.d.ts`. Document view counting follows
checkpoint 12 of D229 in `src/app/workbench.rs`.

### 3. Size, zoom, and placement

Height is natural with `maxLines` and `maxRows` caps; width is pixels or
`"editor"`; `pixels` scale with zoom; the total is clamped to the text area
and to the larger free side. The popover goes below, above, or on the roomier
side, moves with scrolling, wrapping, and folding, is hidden while its anchor
is out of view or folded away, and relays out on zoom changes.

Validation: natural sizing and placement are pure model functions (layout
measures, anchor rows, viewport rows, row height, zoom factor → size and
position, or hidden) with unit tests for below, above, the roomier side,
horizontal clamping, zoom scaling, out of view, and folded anchors; editor
tests drive scrolling and zoom headless.

**Implementation context.** Sizing and placement go in
`crates/knot-model/src/editor/popovers.rs`, taking rows from the wrap map as
`completion_anchor` and block rows do (`EditorState` row queries); the view
measures members in `src/app/editor/layout.rs` and paints in
`src/app/editor/paint.rs`, using gpui's `deferred` so popovers paint above
the gutter and bars, as `src/app/context_menu.rs` does. The zoom factor and
`scaled` come from `src/app/zoom.rs`. Editor tests in a new
`src/app/editor/tests/popovers.rs` beside `blocks.rs`.

### 4. Focus, keys, and dismissal

`focus: true` focuses the popover's focused member; otherwise the editor
keeps focus. Each open popover adds `popover:<id>` to its editor's key
context, and bindings with `{ view: "editor", context: "popover:<id>" }` run
while it is open. Escape, clicks outside, and edits close popovers by the
rules above, and `onDidClosePopover` reports the ID and reason.

Validation: product tests for Escape order, clicks inside and outside, an
edit from the editor closing every popover while one typed into a popover
view does not, a toolbar command's edit closing it, key bindings under the
popover context, and the reported reasons.

**Implementation context.** Key contexts follow `editor_completion` in
`src/app/editor/paint.rs` and
[editor bar key routing](../architecture/commands.md#editor-bars); Escape is
an editor action like `completion_dismiss` in `src/app/editor/completion.rs`
that falls through when no popover is open. Click-outside hooks
`EditorView::on_mouse_down` and the window's mouse-down routing; edit closure
observes the buffer's commits in `src/app/editor.rs`, comparing the edit's
origin view with the popover's views. The close event is a coalesced editor
notification like `onDidEditBar` (`bootstrap.ts`, `handlers.rs`).

### 5. Documentation and decision

The extension guide gains "Show a popover" in
[`docs/extension-api/ui.md`](../extension-api/ui.md), next to editor bars;
the [semantic UI](../architecture/semantic-ui.md),
[editor views](../architecture/editor-and-languages.md#editor-views),
[view tree](../architecture/workbench-and-lifecycle.md#view-tree), and
[zoom](../architecture/workbench-and-lifecycle.md#zoom) references describe
popovers, sizing, zoom, placement, and dismissal; a decision entry in
[`docs/decisions/semantic-ui.md`](../decisions/semantic-ui.md) and its index
records composite content over text and tree only, floating over inline,
several popovers per editor over one, following scroll over closing on
scroll, popovers as editor children in the view tree over views outside it,
zoom-scaled pixels unlike composites, and closing on every external edit
over an option. The [version control plan](vcs.md) marks D220 done.

Validation: docs match the shipped behavior and `knot.d.ts`; review only.
