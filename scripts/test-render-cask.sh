#!/usr/bin/env bash
# Checks that the cask renders with fixed inputs and leaves no placeholder.
set -euo pipefail
cd "$(dirname "$0")/.."

sha=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
out="$(scripts/render-cask.sh nightly.20261112.0917 "$sha" example/knot)"

expect() {
    if ! grep -qF -- "$1" <<<"$out"; then
        echo "rendered cask lacks: $1" >&2
        exit 1
    fi
}
expect 'version "nightly.20261112.0917"'
expect "sha256 \"$sha\""
expect 'releases/download/nightly-20261112-0917/Knot-#{version}-macos-arm64.zip'
expect 'homepage "https://github.com/example/knot"'
if grep -q '@[A-Z0-9]*@' <<<"$out"; then
    echo "rendered cask keeps a placeholder" >&2
    exit 1
fi
echo "render-cask ok"
