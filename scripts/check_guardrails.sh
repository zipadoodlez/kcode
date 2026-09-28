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
#   scripts/check_guardrails.sh --skip-slow  # skip cargo check/clippy/machete
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
# Before rustfmt: a `mod x;` with no file makes rustfmt fail with "Error writing
# files: failed to resolve mod", which reads like a formatting problem and hides
# every gate behind it. Naming the real cause first turns a confusing Format
# failure into an obvious one (221159294).
run_gate "module declarations resolve" python3 scripts/check_module_files.py
if $FIX; then
    cargo fmt --all
fi
run_gate "cargo fmt --all --check" cargo fmt --all --check

echo ""
echo "=== Quality Guardrails ==="
if $SKIP_SLOW; then
    echo "⏭  cargo check / clippy / machete (--skip-slow)"
else
    run_gate "cargo check --all-targets --all-features" \
        cargo check --all-targets --all-features -j "$JOBS"
    run_gate "cargo clippy -- -D warnings" \
        cargo clippy --all-targets --all-features -j "$JOBS" -- -D warnings
fi

# A stale lockfile otherwise passes the fast jobs and only fails at the
# release "Build release binary" step.
run_gate "Cargo.lock is up to date" cargo metadata --locked --format-version 1
run_gate "warning budget" bash scripts/check_warning_budget.sh
run_ratchet "panic-prone usage ratchet" check_panic_budget.py
# Both size ratchets were re-baselined to this fork on 2026-09-27 but left out
# of the gate, so they guarded nothing; a ratchet that never runs is not a
# ratchet. They measure file-size drift, separate from the `App` shape ratchet
# below (app.rs may shrink while the field/impl/glob counts stay flat).
run_ratchet "code size budget" check_code_size_budget.py
run_ratchet "test size budget" check_test_size_budget.py
run_gate "crate dependency boundaries" python3 scripts/check_dependency_boundaries.py
run_gate "wildcard re-export ratchet" python3 scripts/check_wildcard_reexport_budget.py
# The `App` re-core may only shrink, so its field/impl/glob counts are ratcheted
# separately from file size (app.rs could shrink while fields regroup inward).
run_ratchet "App shape ratchet" check_app_shape.py

# Onboarding state-space invariants. The onboarding flow is a graph, and the
# properties that keep users unstuck (no dead ends, every failure has a recovery
# edge, an escape hatch everywhere, bounded keystrokes to a settled state) are
# checkable in microseconds. Every onboarding bug we have shipped was a violated
# invariant that nobody could see by reading one screen's code, so this gate is
# cheap insurance against the whole class.
run_gate "onboarding state-space invariants" \
    cargo test --profile selfdev -p kcode-tui -j "$JOBS" onboarding_graph::

if $SKIP_SLOW; then
    :
elif command -v cargo-machete >/dev/null 2>&1; then
    run_gate "unused dependencies (cargo machete)" cargo machete
else
    echo "⏭  cargo machete (not installed: cargo install cargo-machete --locked)"
fi

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
