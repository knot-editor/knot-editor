#!/usr/bin/env bash
# Prints the release notes for the commits after PREV up to and including REV:
# one bullet per `Release-note:` trailer. Without PREV, covers all history.
set -euo pipefail

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "usage: $0 REV [PREV]" >&2
    exit 2
fi
rev="$1"
range="${2:+$2..}$rev"

notes="$(git log "$range" --reverse --format='%(trailers:key=Release-note,valueonly,unfold)' |
    sed -e '/^$/d' -e 's/^/- /')"
if [ -n "$notes" ]; then
    printf '%s\n' "$notes"
else
    echo "No user-visible changes."
fi
