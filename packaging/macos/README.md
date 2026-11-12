# Local macOS app

From the repository root, run `scripts/build-macos-app.sh` for a debug build or
`scripts/build-macos-app.sh --release` for a release build. The output is
`target/debug/Knot.app` or `target/release/Knot.app` respectively. If
`CARGO_TARGET_DIR` is set, the app is created there instead.

Every build is ad-hoc code signed. `--version ID` compiles `ID` in as
`KNOT_VERSION` and stamps it as `CFBundleVersion`.
`scripts/package-macos-app.sh ID` (`just app-package`) builds the release
bundle for `ID` and writes `dist/Knot-ID-macos-ARCH.zip` with a `.sha256` file.

Launch the bundle with `open target/debug/Knot.app`. Open a file with
`open -a target/debug/Knot.app path/to/file.txt`. The bundle is for local use;
stable distribution is tracked in D035 of [the roadmap](../../docs/roadmap.md).
