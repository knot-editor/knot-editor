# Platform and UI decisions

The UI framework, platform targets, headless view state, and product window
chrome. The [decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Platform and UI

Use gpui through public APIs so the editor and shell share one framework. Its
macOS text path passed styled, culled, IME, bidirectional, CJK, emoji,
long-line, and large-file fixtures; RTL alignment and logical-index mapping are
Knot's. Skia had lower p50 but worse p99 in primitive benchmarks, and gpui met
the bar, so iced and floem were not evaluated. macOS is the dogfooding
platform; Windows 11 and Linux (Wayland) passed bounded local smoke checks.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md).

## Views and extension UI

Share text and contributions through buffer models and keep interaction per
view. Extensions supply semantic data, never rendering callbacks; a widget tree
or WebView waits for an unmet use case. Vertical movement measures display
cells rather than shaped x, so editor state runs headless and tests check real
behavior, at the cost of slight drift beside proportional fallback glyphs.

References: [semantic UI](../architecture/semantic-ui.md) and
[editor views](../architecture/workbench-and-lifecycle.md#editor-views).

## Product window chrome and failure notices

A product window shows only product surfaces. The prototype toolbar duplicated
commands, and its status string was the only place some failures appeared.
Native failures with no visible effect now use the native warning prompt;
extensions receive failures as results and decide what to show. A failed
package startup opens the startup report.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#product-application-launch).
