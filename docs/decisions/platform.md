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

## Crate boundaries

The gpui boundary is enforced by the compiler: a workspace whose text,
transport, extension-runtime, and model crates cannot see gpui, rather than a
convention inside one crate. `knot-protocol` sits below both `knot-model` and
`knot-host` so the model can use transport types without depending on V8.
`knot-model` holds code that takes plain data and returns results as data, so
its tests are plain `#[test]`s and double as the second consumer of every API;
an API that a plain test cannot call without imitating gpui is redesigned. Knot
owns key, color, and string vocabulary and converts at the view edge only, and
workbench tabs hold identities with the product session owning the views. No
Knot runtime abstraction over spawn, notify, observe, or globals was added:
nothing measured needed one, and it would duplicate gpui's own. The gpui
application is instead a thin shell: with one framework and one
implementation, an abstraction would copy gpui's shape without easing a
migration, whereas behavior in model functions leaves a port only the views
and a mechanical shell to rewrite.

Bundled packages are compiled in domain group crates and injected into a
library-only `knot` as `&'static [BundledSource]` by the `knot-app` binary. Cargo
recompiles every dependent of a changed crate, so any edge from the library
(even a dev-dependency) to compiled extension output would rebuild the library
and its test binary on a TypeScript edit. One crate per package is too fine and
one crate for all too coarse; per-package fingerprints in `OUT_DIR` rerun `tsc`
only for changed packages. Extension tests are integration tests against the
real editor and packages, never stubs.

Warm debug rebuilds after touching one file, before and after the split:

| | editor file | vim `main.ts` | core `lib.rs` | tests |
| --- | --- | --- | --- | --- |
| Before | 3.3 s | 4.0 s | 3.5 s | 23 s |
| After | 2.8 s | 1.4 s | 3.4 s | 12.5 s |

A clean build could not be measured. The gain is the Vim edit and the test
loop; an editor-file rebuild is dominated by the gpui shell and barely moved.

References: [architecture](../architecture.md#system-map) and
[development](../development.md).

## gpui snapshots and gpui-component

Knot depends on `gpui-pre` and `gpui-pre-platform`, weekly crates.io
snapshots of Zed's gpui, together with gpui-component, all at exact versions
and upgraded together when gpui-component releases. Zed's last crates.io
release, 0.2.2, was a year behind its repository, and gpui-component pins an
exact `gpui-pre`, so adopting it required the port. The snapshots have one
publisher; if they stop, pinning a Zed git commit carries the same code, but
gpui-component would then need a fork. On Apple M1 Pro the port took the
full suite from 10.8 s to 10.0 s and the release binary from 91.7 MB to
93.3 MB (+1.7%). Only the `knot-app` binary links the platform crate: gpui's
macOS constructor scans the executable's directory for a bundle, and with
`target/debug/deps` full of stale files every linked test binary paid for it
at load, taking the suite to 105 s.

Knot adopts gpui-component only for leaf widgets whose state is purely
presentational, such as tooltips, popup menus, and `Icon`, each when a feature
needs it. Its dock, code editor, `Input`, and theme registry are rejected: Knot's
workbench, editor, terminal, pickers, and key routing are product state that
runs without a window, and its single-line `TextInput` stays native because
windowless sessions and package tests drive it, which `Input` cannot provide.
A component that takes focus is adopted only for a surface with no windowless
path.

Knot's keymap still matches every key first; a focused component popup gets
only the keys the keymap leaves. Letting gpui dispatch first would let a
component's bindings shadow global and extension bindings, and the keymap is
the one place users and extensions change keys.

gpui-component is painted from the Knot theme through one table from its
color fields to native color keys, applied with every theme application.
Extensions see no new API and themes set no component colors; fields without
a fitting key keep gpui-component's built-in colors for the appearance, since
adding a native key per field would grow the native table with keys no Knot
surface paints with.

References: [architecture](../architecture.md#system-map),
[commands](../architecture/commands.md#keymaps), and
[semantic UI](../architecture/semantic-ui.md#themes-and-color-keys).

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
