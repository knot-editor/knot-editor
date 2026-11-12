# Website: screenshots and documentation

Status: 🚧 approved, not started. Roadmap item
[D168](../roadmap.md). Seeded from `docs/notes/website.md`
(removed with this plan). Extends the landing page in [`website/`](../../website/),
deployed to <https://knot-editor.com> by
[`pages.yml`](../../.github/workflows/pages.yml).

## Goal

A visitor sees what Knot looks like and can learn to use and extend it without
reading the repository. The site gains real screenshots and a documentation
section covering getting started, configuration and keybindings, the extension
guide, and a feature tour.

## Design

- **Single source of truth.** The documentation section is generated from
  Markdown under `docs/` at deploy time. User-facing pages are published;
  internal documents (`architecture*`, `decisions*`, `plans/`, `roadmap.md`,
  `reports/`) are not. Existing [extension-api.md](../extension-api.md) and
  [settings.md](../settings.md) are published as they stand, and the new
  user-facing pages are authored beside them.
- **Generator.** A static generator runs inside `pages.yml`, with its output
  placed under `website/` next to the hand-written landing page. mdBook is the
  leading candidate; checkpoint 2 starts by confirming it handles the
  `docs/` layout (it needs its sources under one root, so a small book
  directory of pages plus links or symlinks to the published files) and falls
  back to a short script if it does not. The landing page stays hand-written.
- **Screenshots.** Committed images under `website/` (light and dark hero,
  plus a few feature shots: split panes with a terminal, the command palette,
  a project switch). The hero appears on the landing page and is the
  `og:image`, upgrading the card from `summary` to `summary_large_image`.
- **Honest status.** Pages describe what works today and carry the existing
  "early development" caveat; no page documents unbuilt features.

## Exclusions

- The recorded demo (a `pre-init.js` keybinding that toggles a maximized
  lazygit terminal). The idea is kept here as a candidate
  user guide example, and a video is a later decision.
- A blog and a newsletter ([D167](../roadmap.md)), versioned docs, search
  beyond what the generator provides, a package directory, and analytics.
- Publishing architecture or decision documents.

## Deferred decisions

- Exact generator, confirmed in checkpoint 2.
- Whether the API reference is generated from `knot.d.ts` or only linked.
- Whether the demo video is worth producing, revisited after checkpoint 4.

## Checkpoints

### 1. Screenshots on the landing page

Capture and commit the hero and feature images, add them to
`website/index.html` and `style.css` (responsive, dark/light aware), and
switch the social card to a large image.

- **Implementation context:** `website/index.html` (hero and `og:*` meta),
  `website/style.css`; add images in `website/` (optimize for size). Use a
  clean demo project and a bundled theme pair (One Light/One Dark). No Rust
  changes; `pages.yml` already deploys `website/**`.
- **Validation:** open the built page in light and dark and at phone width;
  check the HTML for missing `alt` text and image dimensions.
- **Review gate:** the user approves which images represent Knot.

### 2. Documentation pipeline

Add the generator, a docs navigation with the existing extension guide and
settings reference, and the deploy wiring, with a link from the landing page.

- **Implementation context:** `pages.yml` (add a pinned generator install and
  build step; extend the `paths` filter to `docs/**`, and upload the combined
  output), a new book config and `SUMMARY` under a new directory such as
  `docs/site/`, plus `website/index.html` navigation. Mirror the pinned-action
  style already in `pages.yml`. Document the build in
  [development.md](../development.md) and add a `just` recipe for a local
  preview.
- **Validation:** a CI job (or `pages.yml` step) that fails on broken internal
  links; local build produces the same output the workflow deploys.
- **Review gate:** the user reviews the rendered site structure and theming
  against the landing page before any content is added.

### 3. Getting started and configuration

Write the getting-started page (build and run, first launch, command palette,
config directory, `pre-init.js` and `post-init.js`, a first keybinding) and
publish the settings reference and keybinding documentation.

- **Implementation context:** new Markdown beside the user-facing docs;
  [`src/app/personal_config.rs`](../../src/app/personal_config.rs)
  (`user_config_root`) for config locations, `docs/settings.md`,
  [extension-api.md](../extension-api.md#bind-keys-and-modal-contexts) for
  keymaps. Link rather than copy.
- **Validation:** every command in the page is run from a clean checkout;
  the link check from checkpoint 2 passes.
- **Review gate:** the user follows the page from scratch.

### 4. Extension guide and feature tour

Publish the extension guide with the TypeScript API reference, and add a
feature tour (workbench, projects, terminal, recovery, languages and themes)
using the checkpoint 1 screenshots.

- **Implementation context:** `docs/extension-api.md`,
  `crates/knot-host/src/js/knot.d.ts`, `scripts/extensions/examples/`,
  `docs/schemas/knot.schema.json`; decide here whether the reference is
  generated or linked.
- **Validation:** the examples shown are the ones tested under
  `scripts/extensions/`; link check passes.
- **Review gate:** the user reviews content accuracy and tone, then the
  landing page links the finished sections and the plan is removed with
  durable decisions moved to `docs/`.
