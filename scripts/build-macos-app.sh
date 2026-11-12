#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "Knot.app can only be built on macOS" >&2
    exit 1
fi

usage="usage: scripts/build-macos-app.sh [--release] [--version ID]"
profile=debug
version=
while [[ $# -gt 0 ]]; do
    case "$1" in
        --release)
            profile=release
            shift
            ;;
        --version)
            [[ $# -ge 2 ]] || { echo "$usage" >&2; exit 2; }
            version="$2"
            shift 2
            ;;
        *)
            echo "$usage" >&2
            exit 2
            ;;
    esac
done

# The version is compiled into the binary and stamped on the bundle.
if [[ -n "$version" ]]; then
    export KNOT_VERSION="$version"
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
if [[ "$profile" == release ]]; then
    cargo build --release -p knot-app
else
    cargo build -p knot-app
fi

target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
app="$target_dir/$profile/Knot.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 755 "$target_dir/$profile/knot" "$app/Contents/MacOS/knot"
install -m 644 packaging/macos/Info.plist "$app/Contents/Info.plist"
install -m 644 assets/macos/knot.icns "$app/Contents/Resources/knot.icns"

if [[ -n "$version" ]]; then
    /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$app/Contents/Info.plist"
fi
# An ad-hoc signature makes the bundle valid on Apple Silicon without a
# Developer ID.
codesign --force --deep --sign - "$app"

echo "$app"
