#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cargo_exec="$repo_root/scripts/cargo_exec.sh"

run_cargo() {
  (cd "$repo_root" && "$cargo_exec" "$@")
}

echo "=== Phase 1 Refactor Verification ==="

echo "[1/7] Isolated environment sanity"
"$repo_root/scripts/refactor_shadow.sh" check

echo "[2/7] Build (debug)"
"$repo_root/scripts/refactor_shadow.sh" build

echo "[3/7] Compile + budgets"
run_cargo check -q
"$repo_root/scripts/check_warning_budget.sh"
python3 "$repo_root/scripts/check_code_size_budget.py"

echo "[4/7] Security preflight"
"$repo_root/scripts/security_preflight.sh"

echo "[5/7] Full tests"
run_cargo test -q

echo "[6/7] E2E tests"
run_cargo test --test e2e -q

echo "[7/7] All-targets/all-features lint"
run_cargo check --all-targets --all-features
run_cargo clippy --all-targets --all-features -- -D warnings

echo "=== Phase 1 verification passed ==="
