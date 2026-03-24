#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage:
  scripts/bench_compile.sh <target> [--cold]

Targets:
  check            Run cargo check --quiet
  build            Run cargo build --quiet
  release-jcode    Run scripts/dev_cargo.sh build --release -p jcode --bin jcode --quiet

Options:
  --cold           Run cargo clean before timing the target

Examples:
  scripts/bench_compile.sh check
  scripts/bench_compile.sh release-jcode
  scripts/bench_compile.sh check --cold
USAGE
}

target="${1:-}"
shift || true

if [[ -z "$target" ]]; then
  usage
  exit 1
fi

cold=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --cold)
      cold=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'error: unknown argument: %s\n' "$1" >&2
      exit 1
      ;;
  esac
  shift
done

case "$target" in
  check)
    cmd=(cargo check --quiet)
    ;;
  build)
    cmd=(cargo build --quiet)
    ;;
  release-jcode)
    cmd=(scripts/dev_cargo.sh build --release -p jcode --bin jcode --quiet)
    ;;
  *)
    printf 'error: unsupported target: %s\n' "$target" >&2
    usage
    exit 1
    ;;
esac

if [[ $cold -eq 1 ]]; then
  echo 'bench_compile: running cargo clean' >&2
  cargo clean
fi

printf 'bench_compile: target=%s cold=%s\n' "$target" "$cold" >&2
printf 'bench_compile: command=%s\n' "${cmd[*]}" >&2

TIMEFORMAT=$'real %R\nuser %U\nsys %S'
{ time "${cmd[@]}"; } 2>&1
