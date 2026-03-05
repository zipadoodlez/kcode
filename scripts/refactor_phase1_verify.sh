#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cargo_exec="$repo_root/scripts/cargo_exec.sh"

run_cargo() {
  (cd "$repo_root" && "$cargo_exec" "$@")
}

echo "=== Phase 1 Refactor Verification ==="

echo "[1/5] Isolated environment sanity"
"$repo_root/scripts/refactor_shadow.sh" check

echo "[2/5] Build (debug)"
"$repo_root/scripts/refactor_shadow.sh" build

echo "[3/5] Compile + warning budget"
run_cargo check -q
"$repo_root/scripts/check_warning_budget.sh"

echo "[4/5] Full tests"
run_cargo test -q

echo "[5/5] E2E tests"
run_cargo test --test e2e -q

echo "=== Phase 1 verification passed ==="
