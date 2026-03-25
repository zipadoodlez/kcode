#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

log() {
  printf 'dev_cargo: %s\n' "$*" >&2
}

selected_linker_mode="not-configured"
selected_linker_desc=""
sccache_status="disabled"

append_rustflags() {
  local new_flag="$1"
  if [[ -z "${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS:-}" ]]; then
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="$new_flag"
  else
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS} ${new_flag}"
  fi
}

maybe_enable_sccache() {
  if [[ -n "${RUSTC_WRAPPER:-}" ]]; then
    sccache_status="external:${RUSTC_WRAPPER}"
    log "keeping existing RUSTC_WRAPPER=${RUSTC_WRAPPER}"
    return
  fi
  if command -v sccache >/dev/null 2>&1; then
    sccache --start-server >/dev/null 2>&1 || true
    export RUSTC_WRAPPER=sccache
    sccache_status="enabled"
    log "using sccache"
  else
    sccache_status="not-found"
    log "sccache not found; using direct rustc"
  fi
}

configure_linux_linker() {
  local requested_mode="${JCODE_FAST_LINKER:-auto}"
  local mode="$requested_mode"

  case "$mode" in
    auto)
      if command -v ld.lld >/dev/null 2>&1 && command -v clang >/dev/null 2>&1; then
        mode="lld"
      elif command -v mold >/dev/null 2>&1 && command -v clang >/dev/null 2>&1; then
        mode="mold"
      else
        mode="system"
      fi
      ;;
    lld|mold|system)
      ;;
    *)
      printf 'error: unsupported JCODE_FAST_LINKER=%s (expected auto|lld|mold|system)\n' "$mode" >&2
      exit 1
      ;;
  esac

  selected_linker_mode="$mode"
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER:-clang}"

  case "$mode" in
    lld)
      append_rustflags "-C link-arg=-fuse-ld=lld"
      selected_linker_desc="clang + lld"
      log "using clang + lld"
      ;;
    mold)
      append_rustflags "-C link-arg=-fuse-ld=mold"
      selected_linker_desc="clang + mold"
      log "using clang + mold"
      ;;
    system)
      selected_linker_desc="system linker settings"
      if [[ "$requested_mode" == "auto" ]]; then
        log "no supported fast linker detected; using system linker settings"
      else
        log "using system linker settings"
      fi
      ;;
  esac
}

print_setup() {
  cat <<EOF
repo_root=$repo_root
os=$(uname -s)
arch=$(uname -m)
sccache_status=$sccache_status
rustc_wrapper=${RUSTC_WRAPPER:-<unset>}
linker_mode=$selected_linker_mode
linker_desc=${selected_linker_desc:-<none>}
linker=${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER:-<unset>}
rustflags=${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS:-<unset>}
EOF
}

maybe_enable_sccache

if [[ "$(uname -s)" == "Linux" ]] && [[ "$(uname -m)" == "x86_64" ]]; then
  configure_linux_linker
fi

if [[ "${1:-}" == "--print-setup" ]]; then
  print_setup
  exit 0
fi

exec cargo "$@"
