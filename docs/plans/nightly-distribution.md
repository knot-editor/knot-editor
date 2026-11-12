# Nightly distribution through Homebrew

Status: 🚧 in progress; checkpoints 1 and 2 are complete.

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

### 1. Build identity ✅

`knot --version` and `Knot: About` report the version and commit compiled in
by `build.rs` (`KNOT_VERSION`, else the Cargo version plus `-dev`). About shows
them as picker rows until D180 adds a modal surface. Code: `build.rs`,
`src/app/build_info.rs`, `entry::is_version_request`,
`src/app/product/about.rs`.

### 2. Nightly macOS archive ✅

`scripts/build-macos-app.sh --version ID` compiles `ID` in as `KNOT_VERSION`,
stamps `CFBundleVersion`, and ad-hoc signs. `scripts/package-macos-app.sh ID`
(`just app-package`) writes `dist/Knot-ID-macos-arm64.zip` and its `.sha256`.
See [the macOS packaging notes](../../packaging/macos/README.md).

### 3. Nightly workflow and releases 🚧

✅ `.github/workflows/nightly.yml` written: a `decide` job (no-new-commits check
for scheduled runs, wait for green CI unless `skip_ci_wait`), then a `release`
job on `macos-latest` that packages, publishes the prerelease, and prunes to 7.
The `gh`/`jq` queries and pruning filter were checked locally; the workflow
itself can only run once pushed to `main`. Remaining: the review gate.

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
