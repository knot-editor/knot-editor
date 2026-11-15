# Modern UI

Status: 🚧 in progress (checkpoints 1–3 done).

## Goal

Knot looks like a modern application, with IntelliJ's new UI and its Islands
look as the explicit visual target. The center and each dock are rounded cards
floating on a darker window background; the native title bar gives way to a
dark main toolbar; tool panels have headers rather than tabs; editor tabs show
an accent; trees and pickers use inset, rounded selection; spacing and type
sizes follow one shared scale.

The starting point was a screenshot review: a white native title bar on a dark
window, a letter placeholder (`P`) in the tool stripe, tool panels drawn as
closable tabs, a separate row for the Explorer's buttons, an empty right
stripe, a thick split divider, no active-tab accent, a deep tree indent with
square full-width selection and monochrome file icons (the picker's are
colored), a first tree row clipped under its header, text flush against the
terminal's and editor's left edge, inconsistent scrollbars, and a Go to File
picker whose selection spans only the text width, with no search icon and a
larger type size than the rest of the UI.

## Design

- **Tokens.** One application module owns the spacing steps, corner radii,
  chrome heights, and UI type sizes, all in zoom-scaled rems. Native surfaces
  use it instead of literal sizes, so the chrome stays coherent and zooms as one.
- **Islands.** One card per area: the center and each visible dock. Cards
  have rounded corners on the `window.background` color, separated by a small
  gap that is also the resize handle. Splits inside a card are 1px lines. The
  status bar and main toolbar sit on the window background, without borders.
- **Window frame.** A transparent title bar (gpui `TitlebarOptions`), with the
  macOS window buttons inline in a native main toolbar that also drags the
  window and zooms on double-click.
- **Toolbar items are extension contributions.** The toolbar reuses the status
  bar's item model and registry: an item gains a placement (status bar or
  toolbar), with the same per-window values, icon, tooltip, and click command.
  `@knot/projects` contributes the project widget (the window's project name;
  a click runs Open Project, which focuses the project's window or opens one and never replaces the current window), and `@knot/go-to-file` a search button (Go to
  File). Nothing else is added for now. This revises the
  [product window chrome](../decisions/platform.md#product-window-chrome-and-failure-notices)
  decision: the dropped prototype toolbar duplicated commands, menus, and
  keys, while this one holds only a few navigation widgets supplied by packages.
- **Dock headers.** Every dock pane has a header. Its tabs are plain text, the
  active one bright with an accent underline; a pane with one tool shows just
  its title. Tool tabs have no close button; a hide (`–`) button sits at the
  header's right. Terminal tabs keep a close button that appears on hover. A
  tree tool's frame actions (the Explorer's New File and New Folder) render at
  the header's right, replacing the tree's own action row; their keyboard
  focus stays in the tree.
- **Stripes.** Wider stripes with larger icons and a rounded, accent-tinted
  selected state; a stripe with no buttons is not painted. Every bundled tool
  defines an icon, so no letter fallback shows.
- **Editor tabs.** The center's tabs share the card background; the active tab
  has an accent underline. The close button shows on the active or hovered tab,
  and a dirty document shows a dot in its place. Pinned tabs use an icon, not
  an emoji.
- **Themes.** New color keys cover the window background, card background,
  tab accent, and toolbar; every bundled theme defines them from its own
  palette, and the native defaults are Knot Islands Dark/Light. The bundled
  `@knot/themes` package adds **Knot Islands Dark** and **Knot Islands Light**,
  modeled on IntelliJ's palette (neutral greys around `#1e1f22`/`#2b2d30`,
  accent `#3574f0`), and Knot Islands Dark becomes the default theme.

## Exclusions

- Toolbar features beyond the project widget and search: run controls, VCS
  branch, settings menu, toolbar customization.
- Status bar content such as breadcrumbs; this plan only restyles the bar.
- New layouts or interaction models: overlay docks ([D170](../roadmap.md)),
  floating tools, compact mode.
- Windows and Linux custom title bars beyond keeping them working with the
  native frame.

## Deferred decisions

- Whether pickers gain scope tabs (All, Files, Symbols, Actions), like
  Search Everywhere.
- Whether the focused card gets a visible emphasis; decide after living with
  checkpoint 2.

## Checkpoints

Each checkpoint ends with `just check`, a commit per verified step, and a
review gate in the real app through `Try:` commands in the personal
`post-init.js`.

### 1. Tokens and themes ✅

Done and approved. The color keys (`window.background`, `card.background`,
`tab.accent`, `toolbar.background`) are native keys whose defaults match Knot
Islands Dark/Light; every bundled theme defines them
([themes](../architecture/semantic-ui.md#themes-and-color-keys)). Shared sizes
live in `src/app/tokens.rs`; spacing steps and type sizes join it when a
checkpoint first uses them.

### 2. Island cards and window frame ✅

Done and approved. Areas paint as rounded cards with gaps that double as resize
handles, and the macOS title bar is transparent over a draggable toolbar row
([windows](../architecture/workbench-and-lifecycle.md#windows)). The row exists
on macOS only until checkpoint 3 gives the toolbar content. gpui clips to
rectangles, so a child's background must match the card's near a rounded
corner; check the non-Islands themes for pokes. Whether the focused card gets
an emphasis is still open (see deferred decisions).

### 3. Main toolbar ✅

Done and approved. Status items carry a `placement` (status bar or toolbar), and
the toolbar row renders the toolbar ones with the same per-window values,
tooltip, and click command ([status bar](../architecture/workbench-and-lifecycle.md#status-bar),
[extension guide](../extension-api.md#show-status-items),
[decision](../decisions/platform.md#product-window-chrome-and-failure-notices)).
`@knot/projects` shows each window's project and `@knot/go-to-file` a search
button. The project widget and the Projects tool run Open Project, which focuses
the project's window or opens one and leaves the current window alone; Switch
Project remains a palette command.

### 4. Dock headers and stripes ✅ (pending review)

Delivered: dock headers, frame actions in the header (the tree's action row is
removed), the hide button, hover close for terminals, wider accent-tinted
stripes, empty stripes hidden (shown while a button is dragged), and the
Projects tool icon `tool.builtin.projects`. Tests: `tests/dock_header.rs` and
the stripe drag test.

Deliverables: dock pane headers with text tabs, accent underline, hide
button, hover close for terminals, and tree frame actions at the right; the
tree's own action row removed; restyled stripes, empty stripes hidden, an
icon for the Projects tool.

Validation: product tests for hide from the header, frame actions clicked
from the header and focused by keyboard, and the empty stripe.

Review gate: Explorer, Projects, and terminals in docks, grouped and alone.

**Implementation context**: the pane and tab rendering in
`src/app/product/render.rs`, branching on the pane's area; frame actions in
`src/app/tree_view.rs` (the `extension-tree-frame-actions` row) exposed to
the header; `src/app/product/stripes.rs` and `Workbench::stripes`; the
Projects tool icon defined in `extensions/@knot/projects/src/tree.ts` like
`tool.builtin.explorer` in `@knot/explorer`.

### 5. Editor tabs

Deliverables: center tabs on the card background with an accent underline,
close on active or hover, dirty dot, icon for pinned tabs, consistent height
and padding.

Validation: product tests for close by pointer, dirty indicator, and pinned
indicator selectors.

Review gate: tabs with clean, dirty, pinned, and preview documents.

**Implementation context**: the tab element in `src/app/product/render.rs`;
document dirty state from `src/app/documents.rs`; icons through
`src/app/icons.rs`.

### 6. Trees and pickers

Deliverables: tree rows with a smaller indent, inset rounded selection and
hover, colored file icons matching the picker, and the clipped first row
fixed; picker rows full width, inset and rounded, a search icon and
placeholder in the input, no separate title row, and the shared type size.

Validation: tree and picker tests for row geometry where practical; existing
speed search, reveal, and picker tests stay green.

Review gate: Explorer, Projects, Go to File, the command palette, and Open
Recent.

**Implementation context**: row rendering in `src/app/tree_view.rs`; icon
resolution in `src/app/icons.rs` (why tree file icons lose their color);
`src/app/picker.rs` and `src/app/command_palette.rs`; the input in
`src/app/text_input.rs`.

### 7. Polish

Deliverables: left padding in editors and terminals, thin overlay
scrollbars that fade in while scrolling, and the status bar restyled to the
token scale.

Validation: editor layout tests for the text origin; terminal mouse tests
for the padded cell grid.

Review gate: a full session like the original screenshot, compared with it.

**Implementation context**: scrollbars in `src/app/editor/paint.rs` and
`src/app/editor/layout.rs`; terminal geometry in `src/app/terminal/`;
`src/app/components.rs` for gpui-component scrollbars elsewhere.
