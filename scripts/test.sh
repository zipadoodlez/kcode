#!/usr/bin/env bash
# The one test entry point.
#
#   scripts/test.sh                  library + primary binary tests (fast, minimal features)
#   scripts/test.sh lib [args...]    same, extra args passed through to cargo
#   scripts/test.sh crate <name>     one crate's tests
#   scripts/test.sh full             the CI-style suites, serial and timed
#
# Flags: --parallel (drop --test-threads=1 in full), --timeout-scale N.
# Failure classes and baselines: docs/dev/testing.md.
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
dev_cargo="$repo_root/scripts/dev_cargo.sh"

PARALLEL=0
TIMEOUT_SCALE="${KCODE_TEST_TIMEOUT_SCALE:-1}"

usage() {
    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

progress() {
    printf 'KCODE_PROGRESS {"kind":"indeterminate","message":"%s"}\n' "$1"
}

# One timed suite in `full` mode. Stops the group on timeout and reports
# elapsed seconds so a slow suite is visible without a report script.
run_suite() {
    local name=$1 base_timeout=$2
    shift 2
    local timeout_s=$(( base_timeout * TIMEOUT_SCALE ))
    (( timeout_s > 0 )) || timeout_s=1
    local started elapsed code
    started=$(date +%s)
    progress "Running ${name} (timeout ${timeout_s}s)"
    printf '\n=== %s ===\n$ cargo test %s\n' "$name" "$*"
    ( cd "$repo_root" && timeout --signal=TERM --kill-after=5 "$timeout_s" "$dev_cargo" test "$@" )
    code=$?
    elapsed=$(( $(date +%s) - started ))
    printf '=== %s exit=%s elapsed=%ss timeout=%ss ===\n' "$name" "$code" "$elapsed" "$timeout_s"
    if (( code == 124 )); then
        printf '=== %s timed out after %ss ===\n' "$name" "$elapsed"
    fi
    return "$code"
}

run_lib() {
    # The default feature set drags in the ONNX, Bedrock and PDF stacks on every
    # inner-loop run. Keep the fast path minimal unless overridden.
    export KCODE_DEV_FEATURE_PROFILE="${KCODE_DEV_FEATURE_PROFILE:-minimal}"
    local code
    ( cd "$repo_root" && "$dev_cargo" test --lib --bin kcode "$@" )
    code=$?
    # Optional startup regression check, only when a release binary happens to exist.
    if (( code == 0 )) && [[ -x "$repo_root/target/release/kcode" ]]; then
        printf '\n=== Startup regression check (release binary) ===\n'
        "$repo_root/scripts/check_startup_budget.sh" "$repo_root/target/release/kcode" || code=$?
    else
        printf '\nSkipping startup check: build a release binary first (cargo build --release)\n'
    fi
    return "$code"
}

run_crate() {
    local name=$1
    shift
    ( cd "$repo_root" && "$dev_cargo" test -p "$name" "$@" )
}

run_full() {
    local -a threads=()
    (( PARALLEL == 1 )) || threads=(-- --test-threads=1)
    run_suite lib-bins 1800 --lib --bins "${threads[@]}" || return $?
    run_suite provider-matrix 900 --test provider_matrix "${threads[@]}" || return $?
    run_suite e2e 1800 --test e2e "${threads[@]}" || return $?
}

args=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --parallel) PARALLEL=1; shift ;;
        --timeout-scale) TIMEOUT_SCALE="$2"; shift 2 ;;
        --timeout-scale=*) TIMEOUT_SCALE="${1#*=}"; shift ;;
        -h|--help) usage 0 ;;
        *) args+=("$1"); shift ;;
    esac
done

mode="${args[0]:-lib}"
rest=("${args[@]:1}")

case "$mode" in
    lib) run_lib "${rest[@]}" ;;
    crate)
        (( ${#rest[@]} >= 1 )) || { echo "crate mode needs a crate name" >&2; usage 2; }
        run_crate "${rest[@]}"
        ;;
    full)
        run_full || exit $?
        if [[ "${KCODE_REAL_PROVIDER:-0}" == 1 ]]; then
            "$repo_root/scripts/real_provider_smoke.sh" || exit $?
        fi
        if [[ "${KCODE_REAL_AUTH_TEST:-0}" == 1 ]]; then
            "$repo_root/scripts/test_auth_e2e.sh" || exit $?
        fi
        printf '\nAll selected test suites passed.\n'
        ;;
    *) echo "unknown mode: $mode" >&2; usage 2 ;;
esac
