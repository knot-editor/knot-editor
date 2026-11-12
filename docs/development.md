# Development and CI

The repository pins the Rust version in
[rust-toolchain.toml](../rust-toolchain.toml). Rustup selects this version for
local commands and CI, installing it when it is not already available. The
minimal profile includes the compiler and Cargo; Clippy and rustfmt are added
explicitly.

Run `just check` for local formatting, Clippy, Rust and extension tests, and
[actionlint](https://github.com/rhysd/actionlint) on the workflows (`brew install
actionlint`). Run `just app` to build
the macOS app bundle locally.

Rust tests run under [cargo-nextest](https://nexte.st), which runs each test
in its own process and in parallel across test binaries: install it with
`cargo install --locked cargo-nextest` and run `cargo nextest run
--workspace --all-targets`. Nextest does not run doc tests, so `just test` and CI also run
`cargo test --workspace --doc`. [`.config/nextest.toml`](../.config/nextest.toml) holds the
`ci` profile, which reports every failure instead of stopping at the first.

The [CI workflow](../.github/workflows/ci.yml) runs on pull requests and pushes
to `main`:

- The Rust job uses Ubuntu 24.04. It checks that formatting produces no diff,
  runs Clippy with warnings as errors, and tests all Rust targets under
  nextest, then doc tests. Clippy and tests use the committed `Cargo.lock`
  through `--locked`.
- The check-ci-setup job runs actionlint on every workflow file and tests the
  nightly cask rendering.
- The handler-platforms job runs on macOS and Windows. It tests handler
  declarations, linked inventory discovery, and doc tests, then builds the
  product binary. It does not run the full suite on those platforms.

The [audit workflow](../.github/workflows/audit.yml) scans `Cargo.lock` with
`cargo-audit` for known RustSec advisories. It runs on pull requests and pushes
to `main` only when `Cargo.lock` or a `Cargo.toml` changes, so a newly
published advisory never fails an unrelated change. It also runs weekly and on
demand to find new advisories against existing dependencies.

Every job selects the pinned toolchain before restoring its Rust cache. The
Rust jobs cache downloaded crates and compiled dependencies; the audit job
caches Cargo downloads and the `cargo-audit` executable. Pull requests restore
caches, while pushes and scheduled runs can save them. A newer run for the same
branch cancels an older run.

The workflow has read-only repository permissions and pins external actions to
commits. [Dependabot](../.github/dependabot.yml) checks for action updates
weekly.

Building Knot requires Node.js and npm. Bundled packages are compiled by the
domain group crates `knot-ext-languages`, `knot-ext-vim`, and
`knot-ext-workbench`, whose `build.rs` files call the `knot-bundled` helper. It
installs the lockfile-pinned TypeScript compiler in `scripts/extensions` when it
is missing or older than the lockfile, compiles each of the crate's packages
under `extensions/@knot` into Cargo's output directory, and embeds the emitted
modules. It reruns `tsc` only for packages whose sources or `tsconfig.json`
changed, or for all of them when the lockfile, `tsconfig.base.json`, or
`knot.d.ts` changed. The `knot` library never depends on compiled packages
(`knot-app` hands them to `knot::run`), so a TypeScript edit rebuilds and retests
only its group crate. `scripts/check-bundled-incremental.sh` verifies this. A new
package joins the group of its domain, or starts a group crate. CI installs a pinned Node in
every job that builds Rust. On macOS, release builds also need the Metal
Toolchain, because gpui precompiles its shaders outside debug builds:
`xcodebuild -downloadComponent MetalToolchain`.

`scripts/check-extensions.sh`, run by `just ext` and in CI, uses `npm ci` to check
declarations, every TypeScript package and its tests, authoring samples, and
positive/negative type fixtures, then runs the package tests under `node:test`
with an in-memory host (see the [extension guide](extension-api.md#write-and-test-in-typescript)).
It does not use a global `tsc`. Runtime contracts still require Rust/V8 tests.
Files under a package's `test/fixtures` are test inputs and are not compiled;
The bundled language packages keep highlight fixtures there, checked by a Rust test that
highlights them with the bundled queries (see `crates/knot-ext-languages/tests/languages/highlights.rs`).
Packages keep behavior cases as JSONC topic files in `test/cases/`. A language
package's `comments.jsonc` lists comment toggling cases, which
`crates/knot-ext-languages/tests/languages/editing.rs` runs through the editor's toggle
commands; `indents.jsonc`, `brackets.jsonc`, and `folds.jsonc` list
indentation, bracket, and fold cases, which `indents.rs`,
`brackets.rs`, and `folds.rs` beside it run through an
editor without a window. `@knot/vim` keeps one file per topic, each its own
parallel test in `crates/knot-ext-vim/tests/vim.rs`, which loads the language packages too.

Tests of bundled packages use only `knot::test_support` (feature
`test-support`), a driver that starts the real product with chosen packages
and reports what it shows as plain data, so they need none of the library's
internals. `test_support::cases` holds what every case runner shares:
discovery, parsing, the `KNOT_CASES` name filter, caret markers, generated
numbered texts, the key notation, and failure reports that name the file and
line of the case. `TestProduct` starts the packages without a window, and a
`TestEditor` shows each case in a fresh workbench and types its keys through
the window's router; a run settles once the window holds no keys
and the extension pool is quiet, so keys that leave a command waiting on a
picker or prompt go in unsettled. `TestProduct::launch` starts the whole
application instead, with its launch window, which `TestWindow` drives
through native keys and commands, and with package storage, recovery, and
the window record in files of its own, so a relaunch in the same process
restores what the previous run recorded. Each
Vim case starts from an empty clipboard and the package's hidden
`vim.reset`, which clears registers, their record, marks, and the last find,
so case order does not matter. A case may collapse `folds` before its keys
and expect them afterwards. Windowed product tests remain for rendering, IME,
pointer input, and the window adapter. Product tests keep extension storage,
recovery, and the state database in memory, so each test starts empty; tests
that reopen a store as a restart would, and the stores' own tests, use
files.

## Website

`website/` holds the static site served at <https://knot-editor.com>. It has no
build step: open `website/index.html` in a browser to preview it. The
[website workflow](../.github/workflows/pages.yml) publishes the folder to
GitHub Pages when a push to `main` changes it. The custom domain is configured
in the repository's Pages settings, with DNS records managed in Cloudflare.

## Manual terminal fixtures

Two scripts exercise terminal behavior that automated tests cannot judge. Run
them in a Knot terminal tab:

- `scripts/terminal-links-fixture.py` prints detected, wrapped, Unicode, and
  OSC 8 links for checking hover, Command-click/Control-click opening, and
  unsupported destinations.
- `scripts/terminal-mouse-fixture.py` enables mouse reporting and prints each
  decoded press, release, motion, and wheel report, for checking cell
  coordinates over wide and combining text and Shift-held local selection.
  `--tracking`, `--encoding`, and `--alternate` select the reporting mode,
  report encoding, and alternate screen.
