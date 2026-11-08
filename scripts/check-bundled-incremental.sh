#!/usr/bin/env bash
# Checks that a bundled package edit rebuilds only its group crate: a Vim edit
# recompiles knot-ext-vim (and the binary crate that joins the groups) but not
# the knot library or the other groups, and a shared input recompiles every
# group crate.
set -euo pipefail
cd "$(dirname "$0")/.."

# The packages `cargo test --no-run` compiles, one per line. The root package
# is reported as `knot-lib`, since the binary crate in knot-app is also `knot`.
compiled() {
    cargo test --workspace --no-run -v 2>&1 \
        | sed -nE 's/.*Running `.*rustc.* --crate-name ([a-z_0-9]+) .* (crates\/[a-z-]+|src|tests)\/.*/\1 \2/p' \
        | awk '{ print ($2 == "src" || $2 == "tests") ? "knot-lib" : $1 }' | sort -u
}

cargo test --workspace --no-run >/dev/null 2>&1
source=extensions/@knot/vim/src/main.ts
trap 'git checkout -- "$source"' EXIT

sleep 1
echo "// incremental check" >> "$source"
rebuilt=$(compiled)
grep -qx knot_ext_vim <<<"$rebuilt" || { echo "expected knot-ext-vim rebuilt"; exit 1; }
for unchanged in knot-lib knot_ext_languages knot_ext_workbench knot_host knot_core; do
    ! grep -qx "$unchanged" <<<"$rebuilt" || { echo "$unchanged must stay fresh after a Vim edit"; exit 1; }
done

sleep 1
touch scripts/extensions/tsconfig.base.json
rebuilt=$(compiled)
for group in knot_ext_languages knot_ext_vim knot_ext_workbench; do
    grep -qx "$group" <<<"$rebuilt" || { echo "expected $group rebuilt after a shared input change"; exit 1; }
done
! grep -qx knot-lib <<<"$rebuilt" || { echo "knot-lib must stay fresh after a shared input change"; exit 1; }
echo "bundled incremental compilation ok"
