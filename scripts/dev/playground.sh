#!/bin/sh
# Run Knot with the development playground as its personal config, leaving the
# real configuration untouched. Arguments go to knot. Removed with the D229 plan.
set -e
here=$(cd "$(dirname "$0")" && pwd)
XDG_CONFIG_HOME="$here/playground" exec cargo run -p knot-app -- "$@"
