#!/usr/bin/env bash
# Checks that release notes collect exactly the Release-note trailers of a range.
set -euo pipefail
script="$(cd "$(dirname "$0")" && pwd)/release-notes.sh"

repo="$(mktemp -d)"
trap 'rm -rf "$repo"' EXIT
cd "$repo"
git init -q
git config user.name test
git config user.email test@example.com
git config commit.gpgsign false

commit() { git commit -q --allow-empty "$@"; }
commit -m "Old work" -m "Release-note: Before the range."
commit -m "Start of range"
commit -m "Add search" -m "Body text." -m "Release-note: Search highlights every match."
commit -m "Refactor internals"
commit -m "Two notes" -m "Release-note: First change.
Release-note: Second change."

check() {
    if [ "$1" != "$2" ]; then
        printf 'expected:\n%s\ngot:\n%s\n' "$2" "$1" >&2
        exit 1
    fi
}

check "$("$script" HEAD HEAD~3)" "- Search highlights every match.
- First change.
- Second change."
check "$("$script" HEAD HEAD~1)" "- First change.
- Second change."
check "$("$script" HEAD~1 HEAD~2)" "No user-visible changes."
check "$("$script" HEAD | head -1)" "- Before the range."
echo "release-notes ok"
