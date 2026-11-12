# Nightly distribution through Homebrew

Status: 🚧 in progress; checkpoint 1 awaits review.

## Goal

`brew tap knot-editor/knot && brew install --cask knot@nightly` installs the
latest nightly Knot on an Apple Silicon Mac, and `brew upgrade` follows later
nightlies. Windows and Linux packaging come later and reuse the nightly
trigger and release shape.

## Design

- **Trigger.** A scheduled workflow (`nightly.yml`) plus manual dispatch. The
  scheduled run skips when `main` has no commits since the last nightly tag,
  and builds only when CI is green on the head commit. A manual dispatch
  always builds, skips the no-new-commits check, and has an input to skip the
  green-CI wait, so a pushed change can be installed within one build time.
- **Version.** `nightly.YYYYMMDD.HHMM` (UTC). It is the release tag
  (`nightly-YYYYMMDD-HHMM`), the cask `version`, and the bundle's
  `CFBundleVersion`. The crate and `CFBundleShortVersionString` keep their own
  version. Every build has a distinct version, so `brew upgrade` sees manual
  runs and no release is overwritten.
- **Build identity.** One value, `KNOT_VERSION`, set by the packaging step at
  compile time and read with `option_env!`: the nightly id for nightlies, the
  tag without its `v` for tagged releases, and the Cargo version plus `-dev`
  for local builds. The short commit hash is embedded beside it. The app shows
  it in a `Knot: About` palette command and `knot --version`, and the bundle's
  `Info.plist` carries it. Packaging takes the version as an argument and does
  not care which channel produced it, so a tag-triggered release workflow can
  reuse it later.
- **Package.** `Knot.app` from `scripts/build-macos-app.sh --release`,
  ad-hoc code signed, zipped with `ditto` (preserves bundle metadata), with a
  SHA-256. Apple Silicon only, built on `macos-latest`. No Developer ID or
  notarization.
- **Release.** A GitHub prerelease per nightly holding the zip; the last 7 are
  kept by count, older ones deleted with their tags.
- **Tap.** A separate `knot-editor/homebrew-knot` repository holding
  `Casks/knot@nightly.rb`. The cask is rendered from a template kept in this
  repo (`packaging/homebrew/`), so cask shape changes go through review here,
  and the nightly job commits the rendered file to the tap with a deploy key
  stored as a secret. The cask installs `Knot.app`, links the `knot` binary
  onto `PATH` with `binary`, and removes the quarantine attribute in
  `postflight` because the app is unsigned for Gatekeeper. The plain `knot`
  cask name stays free for a future stable channel.

## Exclusions

- Developer ID signing, notarization, and Sparkle-style self-update.
- The tag-triggered release workflow and stable `knot` cask (the packaging
  and build identity are written to serve them), a macOS menu-bar About item,
  and an extension API for the version.
- Intel or universal builds, Windows, Linux, and a stable release channel.
- Submitting to `homebrew/cask`.

## Deferred decisions

- Notarization once Knot is shared beyond the author's machines.
- How the Windows and Linux packages are published (winget, Scoop, deb/rpm,
  Homebrew on Linux) when those platforms are built.

## Checkpoints

### 1. Build identity ✅ (awaiting review)

Compile the version and commit into the binary, and show them in `knot
--version` and a `Knot: About` command.

- **Implementation context:** a `build.rs` for the root `knot` package that
  reads `KNOT_VERSION` (fallback: Cargo version plus `-dev`) and the commit
  from `git rev-parse --short HEAD` (fallback: `unknown`), emitting
  `rerun-if-env-changed`; a small build-info module in `src/app`; `--version`
  handled in `entry::parse_launch_arguments` (`src/app/entry.rs`, called from
  `run` in `src/app/mod.rs`) before the product starts; the About command
  mirrors `show_keybindings` (`src/app/product/show_keybindings.rs`, handler
  in `src/app/product/commands.rs`, declaration in
  `src/app/commands/declarations.rs`).
- **Validation:** tests of the argument parse and of the About command through
  the product test driver (see `src/app/product/tests/keybindings.rs`); a build
  with `KNOT_VERSION` set reports it.
- **Review gate:** you run `knot --version` and `Knot: About` from a local
  build.
- **Done:** `build.rs`, `src/app/build_info.rs`, `entry::is_version_request`
  (checked in `run` before the product starts), and `app.about` in
  `src/app/product/about.rs`, shown as a picker of version and commit. `just
  check` passes.

### 2. Nightly macOS archive

Extend the app bundle script so a build takes a version argument, sets
`KNOT_VERSION` for the compile, stamps the bundle, ad-hoc signs, and emit `Knot-nightly.YYYYMMDD.HHMM-macos-arm64.zip` and its SHA-256.

- **Implementation context:** `scripts/build-macos-app.sh` (add a version
  argument and stamp via `PlistBuddy` on `packaging/macos/Info.plist`'s copy inside
  the bundle, then `codesign --force --deep --sign -`), a new
  `scripts/package-macos-app.sh` wrapping it, and the `just app-release`
  recipe in `justfile`. Release builds need the Metal toolchain noted in
  `docs/development.md`.
- **Validation:** run the script locally; `codesign --verify --deep`,
  `plutil -p` shows the stamp, the unzipped app launches.
- **Review gate:** you open the zip's app on your machine.

### 3. Nightly workflow and releases

Add the scheduled workflow: decide whether to build, wait for/verify green CI,
package, publish the prerelease, prune old ones.

- **Implementation context:** new `.github/workflows/nightly.yml` modeled on
  `ci.yml` (pinned action SHAs, `persist-credentials: false`, pinned Node 24,
  `Swatinem/rust-cache`); `permissions: contents: write`; use `gh` for the
  last-tag lookup, the CI status check
  (`gh run list --workflow ci.yml --commit <sha>`), release creation, and
  pruning. Call the checkpoint 2 script.
- **Validation:** scheduled-path logic with and without new commits; a red-CI
  commit is skipped; manual dispatch builds regardless and two runs the same
  day produce distinct releases; pruning keeps 7.
- **Review gate:** you inspect the first prerelease and its asset.

### 4. Homebrew tap and cask publishing

Create the tap, the cask template, and the publish step.

- **Implementation context:** `packaging/homebrew/knot@nightly.rb.in`
  (`version`, `sha256`, `url` to the release asset, `app "Knot.app"`,
  `binary "#{appdir}/Knot.app/Contents/MacOS/knot", target: "knot"`,
  `postflight` with `xattr -dr com.apple.quarantine`, `zap` for the config
  and state directories chosen by `user_config_root` in
  `src/app/personal_config.rs`); a final job in `nightly.yml` that renders the
  template and pushes to the tap with the deploy key. The repository and the
  secret are created by you (outward-facing, so not done without your say).
- **Validation:** `brew audit --cask --online` and `brew style` on the
  rendered cask in the workflow; the template render is unit-checked by a
  small script test with fixed inputs.
- **Review gate:** on your work machine, tap, install, launch `Knot` and the
  `knot` command, then trigger a manual run and `brew upgrade` to receive it.

### 5. Documentation

Record the nightly pipeline and install steps, and the rationale for the
choices above.

- **Implementation context:** `docs/development.md` (a release section),
  `README.md` and the website getting-started text for the install commands,
  a `docs/decisions/` entry (unsigned plus quarantine removal, one build identity, separate tap,
  Apple Silicon only) with its index line, and removal of this plan with
  durable content preserved.
- **Validation:** `just check`.
- **Review gate:** you read the docs.
