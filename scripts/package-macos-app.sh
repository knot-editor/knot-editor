#!/usr/bin/env bash
# Builds the release app for a version and archives it with its checksum.
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: scripts/package-macos-app.sh VERSION" >&2
    exit 2
fi
version="$1"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

app="$(scripts/build-macos-app.sh --release --version "$version" | tail -n 1)"
dist="$repo_root/dist"
archive="Knot-$version-macos-$(uname -m).zip"
mkdir -p "$dist"
rm -f "$dist/$archive" "$dist/$archive.sha256"
ditto -c -k --keepParent "$app" "$dist/$archive"
(cd "$dist" && shasum -a 256 "$archive" > "$archive.sha256")

echo "$dist/$archive"
