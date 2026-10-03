#!/usr/bin/env bash
# The boundary: the expensive checks, run once per tree instead of per change.
#
#   scripts/gate.sh    # full gate + full test suite, stamped by tree hash
#
# What CI used to do, moved local: the gate (`check_guardrails.sh`) plus the
# full suite (`test.sh full`). The stamp is the point: it is keyed to the tree
# (HEAD plus working-tree status), so twenty commits cost one boundary, not
# twenty, and an unchanged tree is never checked twice.
#
# Triggered automatically by `.githooks/pre-push`. Safe to run by hand at the
# end of a step; that is the same boundary, just earlier.
#
# Caveat: the stamp covers the working tree, so a tree with uncommitted edits
# gets its own stamp. A push boundary is only as strong as the tree it measured.
set -uo pipefail
cd "$(dirname "$0")/.."

state_root="${KCODE_HOME:-${HOME:+$HOME/.kcode}}"
[[ -n "$state_root" ]] || state_root="$PWD/target/kcode-state"
stamp_dir="${KCODE_GATE_STAMP_DIR:-$state_root/logs/gate-stamps}"

hash=$( { git rev-parse HEAD; git status --porcelain; } | sha256sum | cut -d' ' -f1 )
stamp_file="$stamp_dir/$hash"

if [[ -f "$stamp_file" ]]; then
    echo "boundary: $hash already passed, skipping"
    exit 0
fi

echo "boundary: proving $hash (gate with all features, then the full suite)"
scripts/check_guardrails.sh --all-features || exit 1
scripts/test.sh full || exit 1

mkdir -p "$stamp_dir"
: > "$stamp_file"
find "$stamp_dir" -type f -mtime +14 -delete 2>/dev/null || true
echo "boundary: $hash passed"
