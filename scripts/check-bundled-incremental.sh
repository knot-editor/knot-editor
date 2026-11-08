#!/usr/bin/env bash
# Checks that knot-bundled recompiles only the bundled packages whose inputs
# changed: one package's source reruns tsc for that package alone, and a shared
# input reruns it for all of them.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -p knot-bundled >/dev/null
out=$(ls -dt target/debug/build/knot-bundled-*/out | head -1)/extensions

stamps() { for d in "$out"/*/dist; do echo "$(stat -f %m "$d" 2>/dev/null || stat -c %Y "$d") $d"; done; }
changed() { diff <(echo "$1") <(echo "$2") | grep -c '^>' || true; }

source=extensions/@knot/vim/src/main.ts
trap 'git checkout -- "$source"' EXIT

before=$(stamps)
sleep 1
echo "// incremental check" >> "$source"
cargo build -p knot-bundled >/dev/null
after=$(stamps)
[ "$(changed "$before" "$after")" = 1 ] || { echo "expected exactly one package recompiled"; exit 1; }
grep -q "vim/dist" <<<"$(diff <(echo "$before") <(echo "$after"))" || { echo "expected vim recompiled"; exit 1; }

sleep 1
touch scripts/extensions/tsconfig.base.json
cargo build -p knot-bundled >/dev/null
packages=$(ls -d extensions/@knot/*/ | wc -l | tr -d ' ')
[ "$(changed "$after" "$(stamps)")" = "$packages" ] || { echo "expected all $packages packages recompiled"; exit 1; }
echo "bundled incremental compilation ok"
