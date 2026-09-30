#!/usr/bin/env bash
# Run every guardrail gate locally. This is the fork's only gate: the GitHub
# Actions CI it used to mirror was removed, so nothing checks a push for you.
#
# Run it before committing. The alternative is discovering a broken gate at the
# next build, after the change is buried under others.
#
# Usage:
#   scripts/check_guardrails.sh              # check only, non-zero on failure
#   scripts/check_guardrails.sh --fix        # rustfmt + rebaseline ratchets
#   scripts/check_guardrails.sh --skip-slow  # skip cargo clippy
#
# Note: this runs on your local `stable` toolchain. If it is behind the latest
# stable, clippy can pass here and fail on a machine that has updated, so the
# script prints the local stable version to compare.

set -uo pipefail
cd "$(dirname "$0")/.."

FIX=false
SKIP_SLOW=false
for arg in "$@"; do
    case "$arg" in
        --fix) FIX=true ;;
        --skip-slow) SKIP_SLOW=true ;;
        -h|--help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown flag: $arg (try --help)" >&2; exit 2 ;;
    esac
done

FAILED=()
JOBS="${CARGO_BUILD_JOBS:-2}"

run_gate() {
    local label=$1
    shift
    printf '▸ %s' "$label"
    local output
    if output=$("$@" 2>&1); then
        printf '\r✅ %s\n' "$label"
        return 0
    fi
    printf '\r❌ %s\n' "$label"
    printf '%s\n' "$output" | tail -20 | sed 's/^/    /'
    FAILED+=("$label")
    return 1
}

# Ratchet scripts share a --update flag to accept intentional growth.
run_ratchet() {
    local label=$1 script=$2
    if $FIX; then
        python3 "scripts/$script" --update >/dev/null 2>&1
    fi
    run_gate "$label" python3 "scripts/$script"
}

echo "=== Format ==="
# A `mod x;` with no file is caught by rustfmt itself ("failed to resolve mod
# `x`: ... does not exist") and by `cargo check` (E0583), so no separate
# detector runs here.
if $FIX; then
    cargo fmt --all
fi
run_gate "cargo fmt --all --check" cargo fmt --all --check

echo ""
echo "=== Quality Guardrails ==="
if $SKIP_SLOW; then
    echo "⏭  cargo clippy (--skip-slow)"
else
    # clippy compiles every target with every feature, so it subsumes a
    # separate `cargo check`: a compile error fails it too, and the two run
    # different compiler drivers, so having both compiles the tree twice.
    run_gate "cargo clippy -- -D warnings" \
        cargo clippy --all-targets --all-features -j "$JOBS" -- -D warnings
fi

# A stale lockfile otherwise passes the fast jobs and only fails at the
# release "Build release binary" step.
run_gate "Cargo.lock is up to date" cargo metadata --locked --format-version 1
# Both size ratchets were re-baselined to this fork on 2026-09-27 but left out
# of the gate, so they guarded nothing; a ratchet that never runs is not a
# ratchet. They measure file-size drift, separate from the `App` shape ratchet
# below (app.rs may shrink while the field/impl/glob counts stay flat).
# braid: paused for the work-list project, restore when it lands
# (`docs/plans/work-list.md`, step G1). A type merge and a file rewrite move
# lines between files faster than a per-commit baseline can follow, and this
# ratchet only tightens, so re-baselining mid-project leaves looser caps behind.
# restore: uncomment both lines and re-baseline with `--update` at the end.
# run_ratchet "code size budget" check_code_size_budget.py
# run_ratchet "test size budget" check_test_size_budget.py
run_gate "crate dependency boundaries" python3 scripts/check_dependency_boundaries.py
run_ratchet "wildcard re-export ratchet" check_wildcard_reexport_budget.py
# The `App` re-core may only shrink, so its field/impl/glob counts are ratcheted
# separately from file size (app.rs could shrink while fields regroup inward).
run_ratchet "App shape ratchet" check_app_shape.py

echo ""
# CI installs the current `stable`; a stale local toolchain hides new lints.
if command -v rustup >/dev/null 2>&1; then
    installed="$(rustup run stable rustc --version 2>/dev/null | awk '{print $2}')"
    if [[ -n "$installed" ]]; then
        echo "toolchain: stable = $installed (other checkouts may be ahead; run \`rustup update stable\` if clippy disagrees)"
    fi
fi

if (( ${#FAILED[@]} )); then
    echo ""
    echo "❌ ${#FAILED[@]} gate(s) failed:"
    for f in "${FAILED[@]}"; do
        echo "   - $f"
    done
    if ! $FIX; then
        echo ""
        echo "For formatting and intentional ratchet growth: scripts/check_guardrails.sh --fix"
    fi
    exit 1
fi

echo "✅ All guardrail gates pass."
