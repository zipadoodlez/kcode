#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cargo_exec="$repo_root/scripts/cargo_exec.sh"

run_cargo() {
  (cd "$repo_root" && "$cargo_exec" "$@")
}

echo "=== Fast test loop (library + primary kcode binary) ==="
# The default product feature set includes the local ONNX embedding stack, AWS
# Bedrock SDK, and PDF extraction. Those integrations have dedicated/full-suite
# coverage, but compiling them on every inner-loop test adds hundreds of crates
# and substantial peak RSS. Keep the fast loop minimal unless explicitly
# overridden with KCODE_DEV_FEATURE_PROFILE=default/full.
export KCODE_DEV_FEATURE_PROFILE="${KCODE_DEV_FEATURE_PROFILE:-minimal}"
echo "Feature profile: $KCODE_DEV_FEATURE_PROFILE"

# Only the primary `kcode` binary contains unit tests. `test_api` and
# `kcode-harness` are executable smoke tools with no #[test] functions, so
# `--bins` needlessly builds and links two additional copies of the full graph.
run_cargo test --lib --bin kcode "$@"

echo ""
if [[ -x "$repo_root/target/release/kcode" ]]; then
  echo "=== Startup regression check (release binary) ==="
  "$repo_root/scripts/check_startup_budget.sh" "$repo_root/target/release/kcode"
  echo ""
else
  echo "Skipping startup regression check: build release first with cargo build --release"
  echo ""
fi

echo "For full coverage, run: scripts/test_e2e.sh"
