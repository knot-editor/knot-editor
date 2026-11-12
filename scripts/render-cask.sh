#!/usr/bin/env bash
# Prints the nightly Homebrew cask for a version and the SHA-256 of its archive.
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
    echo "usage: scripts/render-cask.sh VERSION SHA256 [OWNER/REPO]" >&2
    exit 2
fi
version="$1"
sha256="$2"
repo="${3:-knot-editor/knot-editor}"

# nightly.YYYYMMDD.HHMM is released as the tag nightly-YYYYMMDD-HHMM.
tag="nightly-${version#nightly.}"
tag="${tag/./-}"

template="$(dirname "${BASH_SOURCE[0]}")/../packaging/homebrew/knot@nightly.rb.in"
sed -e "s|@VERSION@|$version|g" -e "s|@SHA256@|$sha256|g" \
    -e "s|@REPO@|$repo|g" -e "s|@TAG@|$tag|g" "$template"
