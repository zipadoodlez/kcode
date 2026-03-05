#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cargo_exec="$repo_root/scripts/cargo_exec.sh"

run_cargo() {
  (cd "$repo_root" && "$cargo_exec" "$@")
}

echo "=== Phase 1 Refactor Verification ==="

echo "[1/6] Isolated environment sanity"
"$repo_root/scripts/refactor_shadow.sh" check

echo "[2/6] Build (debug)"
"$repo_root/scripts/refactor_shadow.sh" build

echo "[3/6] Compile + warning budget"
run_cargo check -q
"$repo_root/scripts/check_warning_budget.sh"

echo "[4/6] Security preflight"
"$repo_root/scripts/security_preflight.sh"

echo "[5/6] Full tests"
run_cargo test -q

echo "[6/6] E2E tests"
run_cargo test --test e2e -q

echo "=== Phase 1 verification passed ==="
