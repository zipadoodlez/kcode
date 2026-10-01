#!/usr/bin/env bash
# Cargo-action log for scripts/dev_cargo.sh.
#
# One JSONL record per cargo action, with duration, host-wide gate wait, profile,
# exit code and argv, so compile and test latency can be inspected across
# sessions and after daemon restarts. This is deliberately separate from
# session/tool history.
#
# Sourced into dev_cargo.sh's shell, so it shares `repo_root`, `cargo_argv` and
# `cargo_gate_wait_ms`. It is not runnable on its own.

rust_action_log_started_ns=""
rust_action_log_started_at=""
rust_action_log_path=""
rust_action_log_execution="local"
cargo_gate_wait_ms=0

start_rust_action_log() {
  case "${KCODE_RUST_ACTION_LOG:-1}" in
    0|false|no|off) return ;;
  esac

  local state_root="${KCODE_HOME:-${HOME:+$HOME/.kcode}}"
  [[ -n "$state_root" ]] || state_root="$repo_root/target/kcode-state"
  rust_action_log_path="${KCODE_RUST_ACTION_LOG_PATH:-$state_root/logs/rust-actions.jsonl}"
  rust_action_log_started_ns=$(date +%s%N)
  rust_action_log_started_at=$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)
  trap 'record_rust_action_log "$?"' EXIT
}

record_rust_action_log() {
  local exit_code="$1"
  [[ -n "$rust_action_log_started_ns" && -n "$rust_action_log_path" ]] || return 0
  trap - EXIT

  local finished_ns duration_ms profile action
  finished_ns=$(date +%s%N)
  duration_ms=$(( (finished_ns - rust_action_log_started_ns) / 1000000 ))
  profile=$(selected_profile "${cargo_argv[@]}")
  action="${cargo_argv[0]:-unknown}"
  mkdir -p "$(dirname "$rust_action_log_path")" 2>/dev/null || return 0

  KCODE_LOG_STARTED_AT="$rust_action_log_started_at" \
  KCODE_LOG_DURATION_MS="$duration_ms" \
  KCODE_LOG_GATE_WAIT_MS="$cargo_gate_wait_ms" \
  KCODE_LOG_EXIT_CODE="$exit_code" \
  KCODE_LOG_ACTION="$action" \
  KCODE_LOG_PROFILE="$profile" \
  KCODE_LOG_REPO="$repo_root" \
  KCODE_LOG_EXECUTION="$rust_action_log_execution" \
  python3 - "$rust_action_log_path" "${cargo_argv[@]}" <<'PY' || true
import json
import os
import sys

path = sys.argv[1]
record = {
    "started_at": os.environ["KCODE_LOG_STARTED_AT"],
    "duration_ms": int(os.environ["KCODE_LOG_DURATION_MS"]),
    "gate_wait_ms": int(os.environ["KCODE_LOG_GATE_WAIT_MS"]),
    "execution_duration_ms": max(
        0,
        int(os.environ["KCODE_LOG_DURATION_MS"])
        - int(os.environ["KCODE_LOG_GATE_WAIT_MS"]),
    ),
    "exit_code": int(os.environ["KCODE_LOG_EXIT_CODE"]),
    "success": os.environ["KCODE_LOG_EXIT_CODE"] == "0",
    "action": os.environ["KCODE_LOG_ACTION"],
    "profile": os.environ["KCODE_LOG_PROFILE"],
    "repository": os.environ["KCODE_LOG_REPO"],
    "execution": os.environ["KCODE_LOG_EXECUTION"],
    "argv": sys.argv[2:],
}
line = (json.dumps(record, separators=(",", ":")) + "\n").encode()
fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
try:
    os.write(fd, line)
finally:
    os.close(fd)
PY
  return 0
}
