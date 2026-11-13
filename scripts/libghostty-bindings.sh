#!/usr/bin/env bash
# Regenerates crates/knot-terminal/src/ffi.rs from the libghostty-vt headers
# that the crate's build script links: the downloaded release, or the install
# tree in KNOT_LIBGHOSTTY_DIR. Needs bindgen (`cargo install bindgen-cli`) and
# libclang.
#
# C23 gives every libghostty enum its declared `int` type, so the bindings are
# the same on every target.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -n "${KNOT_LIBGHOSTTY_DIR:-}" ]; then
    include="$KNOT_LIBGHOSTTY_DIR/include"
else
    # The build script runs before the library compiles, so its output
    # directory is reported even when the current bindings no longer build.
    out_dir="$({ cargo check -p knot-terminal --message-format=json 2>/dev/null || true; } \
        | jq -r 'select(.reason == "build-script-executed" and (.package_id | contains("knot-terminal"))) | .out_dir' \
        | tail -n 1)"
    if [ -z "$out_dir" ]; then
        echo "the knot-terminal build script did not run" >&2
        exit 1
    fi
    include="$out_dir/libghostty-vt/include"
fi

bindgen "$include/ghostty/vt.h" \
    --output crates/knot-terminal/src/ffi.rs \
    --rust-target 1.85 --rust-edition 2024 \
    --allowlist-item '(ghostty|Ghostty|GHOSTTY)_?.*' \
    --no-doc-comments --no-prepend-enum-name --merge-extern-blocks \
    --clang-macro-fallback \
    -- -I"$include" -std=c23 -DGHOSTTY_STATIC
