# Islands look: match IntelliJ's new UI

Status: 🚧 checkpoint 1 approved ✅; checkpoint 2 waits for the IntelliJ light reference screenshot.

## Goal

Knot Islands Dark (and Light) look as close to IntelliJ's new UI Islands theme
as Knot's chrome allows, with a calmer focused-pane border in the spirit of
herdr. The user reviews by looking at the running app next to the reference
screenshots, not by reading values.

## Design

- **Theme values only.** The Islands look stays theme data, per
  [platform decisions](../decisions/platform.md#product-window-chrome-and-failure-notices):
  colors and the chrome values in `extensions/@knot/themes/src/{chrome.ts,themes/islands.ts}`.
  A needed look that no theme value can express is added as a new theme value,
  not hard-coded.
- **Colors from the reference.** Sample the IntelliJ screenshot for window,
  island, tab, selection, tree, status bar, and terminal colors, and replace the
  current guesses.
- **Focused pane.** Replace the translucent blue `card.focusedBorder` with a
  softer neutral/cool tone sampled from herdr, still clearly distinguishable.
- **Self-driven iteration.** The implementer renders Knot, compares it to the
  reference, and adjusts until the difference is small. Spacing, radius, tab
  height and weight are tuned in that loop, not decided up front.

- **Other dark themes.** Catppuccin and Tokyo Night already share
  `ISLANDS_CHROME`, so the shape polish (corner radius, spacing, tabs)
  reaches them automatically. Each keeps its own palette; only the focused-pane
  border is re-derived per palette (a soft tone from its own colors, not the
  blue accent), and any chrome value that clashes with a palette is fixed in
  that theme. One Dark and Solarized use `FLAT_CHROME` and keep their flat
  look; they only get the focused-border treatment if it reads well there.
- **Light themes are in scope.** They get the same treatment as the dark ones,
  as a second pass once the dark themes are agreed. The user will supply an
  IntelliJ light reference screenshot at that point; until then Islands Light
  is not tuned by guesswork.

## Exclusions

- New layout modes, icon sets, or fonts. Matching IntelliJ's colors exactly
  applies to Islands only; other themes keep their palettes.

## Checkpoint 1: Islands Dark matches the reference ✅

Approved. Islands Dark takes its colors from IntelliJ's Islands Dark, tabs are
pills (`tabRadius`, `tabInset` chrome values), and every dark theme marks the
focused pane with a soft border derived from its own palette. The values live
in `extensions/@knot/themes/src/`; the theme model is
[semantic UI](../architecture/semantic-ui.md#themes-and-color-keys).

## Checkpoint 2: Light themes

**Deliverables:** Islands Light matched to the user's IntelliJ light
screenshot, and the focused-border and chrome treatment applied to the other
light themes (`one-light.ts`, Solarized Light, light Catppuccin).

**Implementation context:** same files as checkpoint 1, light variants.
Starts only when the user has provided the reference screenshot.

**Validation:** `just check`; before/after screenshots beside the reference.

**Review gate:** the user looks at the light themes in the running app.
