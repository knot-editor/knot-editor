# Platform and UI decisions

The UI framework, platform targets, headless view state, and product window
chrome. The [decisions index](../decisions.md) lists every topic, and the
[documentation guide](../README.md#decisions) sets the entry rules.

## Platform and UI

Use gpui through public APIs so the editor and shell share a framework rather
than maintaining a separate renderer. Its macOS text path passed styled,
viewport-culled text, selection, IME geometry, annotation, split-pane, Arabic,
CJK, emoji, long-line, and large-file fixtures. Public bidirectional-text
limitations require Knot's RTL alignment and logical-index mapping.

In separate rendering-primitive benchmarks on Apple M1 Pro/macOS release builds,
Skia had lower p50 times on every fixture but higher and less predictable p99
spikes. Those measurements did not compare complete frameworks; gpui met the
product bar, so iced and floem were not evaluated.

macOS is the initial dogfooding platform. Windows 11 passed local startup,
config, editing, persistence, commands, terminal, and protected-closure checks
on one desktop session. Linux passed limited framework and native product smoke
checks on Omarchy/Hyprland Wayland. These establish bounded local evidence, not
broad platform certification. Local macOS packaging stays independent of signing
and distribution credentials.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md).

## Views and extension UI

Share text and contributions through buffer models; keep interaction per view.
Surface-specific semantic data lets extensions customize behavior without
blocking native painting. A general widget tree or WebView needs a concrete
unmet use case first.

Vertical movement measures display cells rather than a shaped x, so movement
needs no window, editor state and its commands run headless, and tests check
what users get, since the gpui test platform never shapes real text. The cost
is a cell's drift beside proportional fallback glyphs and on right-to-left
lines, which Knot's monospace text rarely shows.

References: [semantic UI](../architecture/semantic-ui.md) and
[editor views](../architecture/workbench-and-lifecycle.md#editor-views).

## Product window chrome and failure notices

A product window shows only product surfaces. The prototype toolbar duplicated
commands, menus, and keys, and its free-form status string was the only place
some failures appeared, so native failures with no other visible effect use the
native warning prompt instead. Success and progress notices were dropped because
their effect is visible. Extension calls get failures as results, so a package
decides whether to tell the user. A failed package startup opens the startup
report, because nothing else passively shows that a package failed.

Reference: [workbench and lifecycle](../architecture/workbench-and-lifecycle.md#product-application-launch).
