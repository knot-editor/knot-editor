# D233 — Preview diff tabs

Status: 🚧 planned. Source: [roadmap D233](../roadmap.md).

## Outcome

Browsing changes no longer piles up tabs. A click on a file in the Changes
view, or the gutter's Show Changes, opens its diff in the pane's preview tab,
the same slot the explorer's single click uses: the next preview, a diff or a
document, replaces it in place. Double-click or Enter opens a regular diff
tab. A working-copy diff no longer opens a background document tab for the
file it edits.

## Design

- **One preview slot per pane, for documents and composite views.** The
  layout model's preview flag, today limited to document tabs, covers every
  center tab surface that can be replaced: documents and composites. A
  preview open replaces the pane's current preview whatever its kind.
  Replacing a document preview removes the document when no view is left, as
  now; replacing a composite preview closes that instance as closing its tab
  does, so its owner sees the usual close and documents only it showed close.
- **Promotion is unchanged and applies to composites.** A composite preview
  becomes regular when pinned, double-clicked, moved, kept open by command, or
  edited, where edited means an edit to any document it shows
  (`composite_documents`). Opening it again without `preview` is the
  package's call: a composite instance gains `keep()`, the counterpart of
  Keep Tab Open.
- **`workbench.previewTabs` covers composites.** With it off, a preview
  request opens a regular tab.
- **Composite API.** `OpenCompositeOptions.preview` and
  `CompositeInstance.keep()`. The `diff` contract's `open` takes `preview` and
  the contract gains `keep(id)`.
- **Documents without a tab.** `documents.load(uri)` opens a file's document
  like `documents.open`, through the provider and reusing an already open
  document, but presents no tab. Its lifetime is ordinary: the document closes
  when the last view showing it closes, so a diff's editor that is its only
  view closes it with the tab. A loaded document no view ever shows stays open
  until closed; the caller is expected to show it.
- **Changes view gestures follow the explorer.** Click opens a preview diff,
  double-click or Enter a regular one. A row whose diff is already open reuses
  that tab, activating it and, for double-click or Enter, keeping it, instead
  of closing and reopening it. The working copy side comes from
  `documents.load`. The gutter's Show Changes opens a preview.

## Exclusions

- The multi-file diff view, D232.
- Preview tabs for terminals and tools.
- Making composite previews survive layouts or hot exit; composites stay out
  of both.

## Checkpoints

### 1. Composite preview tabs ✅

Done and reviewed: the shared preview slot in the layout model, `preview` on
`registerComposite(...).open`, `keep()` on the instance, and editors inside
composites observed for edit promotion. Behavior is covered by the model tests
and `src/app/product/tests/composite.rs`; the docs land with checkpoint 3.

### 2. Documents without a tab

Deliverables: `documents.load(uri)` returning the document's info without
presenting it.

Validation: app tests that it opens through the provider without a tab,
reuses an open document, and that the document closes with the last view
that shows it; an extension API test.

Review gate: folded into checkpoint 3's review.

Implementation context:
- `src/app/extensions/host/handlers.rs` `openDocument` / `OpenArguments`;
  add a sibling operation calling the open path in
  `src/app/product/operations.rs` `open_document` without the presentation
  step of `src/app/product/open.rs`.
- Closing a document whose last view goes: the rule `update_composite` and
  composite tab closure already apply; confirm it covers a document that
  never had a tab.
- `bootstrap.ts` and `knot.d.ts` `documents`.

### 3. Preview diffs in the Changes view and gutter

Deliverables: the `diff` contract's `preview` and `keep(id)`; Changes view
click, double-click, and Enter as designed, reusing a row's open diff; the
working copy loaded without a tab; Show Changes as a preview; docs and a
`Try:` personal config.

Validation: package tests for the gestures, reuse and keep, no document tab
for the working copy, and Show Changes opening a preview.

Review gate: user tries the Changes view and gutter in the real app.

Implementation context:
- `extensions/@knot/diff/src/contract.ts` and `extension.ts` `openDiff`
  (pass `preview` to `registration.open`, serve `keep`).
- `extensions/@knot/vcs/src/changes.ts`: `requestOf` (accept `click`),
  `showFileChanges` (reuse the open diff for the key, `documents.load` for the
  working copy), `closeDiff`.
- `extensions/@knot/vcs/src/extension.ts` `showChanges`.
- Tests: `crates/knot-ext-vcs/tests/vcs/changes.rs`, `compare.rs`.
- Docs: `docs/architecture/version-control.md` (Changes view gestures),
  `docs/architecture/workbench-and-lifecycle.md` (preview paragraph and
  composite views), a decision entry in
  `docs/decisions/workbench-and-lifecycle.md` for the shared preview slot.
  Then remove D233 from the roadmap and delete this plan.
