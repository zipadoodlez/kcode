#!/usr/bin/env bash
# The one test entry point.
#
#   scripts/test.sh                  library + primary binary tests (fast, minimal features)
#   scripts/test.sh lib [args...]    same, extra args passed through to cargo
#   scripts/test.sh crate <name>     one crate's tests
#   scripts/test.sh full             the CI-style suites, serial and timed
#
# Flags: --parallel (drop --test-threads=1 in full), --timeout-scale N,
# --last (print the last recorded test run for this tree instead of running).
# Failure classes and baselines: docs/dev/testing.md.
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
dev_cargo="$repo_root/scripts/dev_cargo.sh"

PARALLEL=0
LAST=0
TIMEOUT_SCALE="${KCODE_TEST_TIMEOUT_SCALE:-1}"

usage() {
    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

progress() {
    printf 'KCODE_PROGRESS {"kind":"indeterminate","message":"%s"}\n' "$1"
}

# Answer "do I need to rerun?" from the action log instead of rerunning. It never
# skips anything; it reports the last recorded test run and whether this tree
# still matches it. The hash must match worktree_hash() in scripts/dev_cargo_log.sh.
print_last_result() {
    local log="${KCODE_RUST_ACTION_LOG_PATH:-${KCODE_HOME:-$HOME/.kcode}/logs/rust-actions.jsonl}"
    if [[ ! -f "$log" ]]; then
        echo "no cargo action log yet: $log"
        return 0
    fi
    local current_hash
    current_hash=$( { git -C "$repo_root" rev-parse HEAD 2>/dev/null; git -C "$repo_root" status --porcelain 2>/dev/null; } | sha256sum | cut -d' ' -f1 )
    KCODE_LAST_REPO="$repo_root" KCODE_LAST_HASH="$current_hash" python3 - "$log" <<'PY'
import json, os, sys

log = sys.argv[1]
repo = os.environ["KCODE_LAST_REPO"]
current = os.environ["KCODE_LAST_HASH"]
runs = []
with open(log) as fh:
    for line in fh:
        line = line.strip()
        if not line:
            continue
        try:
            r = json.loads(line)
        except Exception:
            continue
        if r.get("repository") == repo and str(r.get("action", "")).startswith("test"):
            runs.append(r)
if not runs:
    print("no recorded test run for this repository")
    raise SystemExit(0)
r = runs[-1]
print(
    "last test run: {} exit={} {}ms at {}".format(
        "PASS" if r.get("success") else "FAIL",
        r.get("exit_code"),
        r.get("duration_ms"),
        r.get("started_at"),
    )
)
print("  cargo " + " ".join(r.get("argv", [])))
if r.get("tree_hash") == current:
    print("  tree: unchanged since that run")
elif r.get("tree_hash"):
    print("  tree: CHANGED since that run")
else:
    print("  tree: unknown (record predates tree hashing)")
PY
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
        --last) LAST=1; shift ;;
        --timeout-scale) TIMEOUT_SCALE="$2"; shift 2 ;;
        --timeout-scale=*) TIMEOUT_SCALE="${1#*=}"; shift ;;
        -h|--help) usage 0 ;;
        *) args+=("$1"); shift ;;
    esac
done

if (( LAST == 1 )); then
    print_last_result
    exit $?
fi

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
