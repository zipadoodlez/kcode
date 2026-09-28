# Benchmarking

## Terminal-Bench 2.0 (via Harbor)

The current cleanest path for running kcode on Terminal-Bench 2.0 is through
Harbor, using the adapter and helpers in `scripts/`:

| script | role |
|---|---|
| `kcode_harbor_agent.py` | Harbor custom-agent adapter for kcode |
| `run_terminal_bench_harbor.sh` | wires Harbor to the adapter and a Linux-compatible binary |
| `run_terminal_bench_campaign.py` | sequential campaign runner, stitchable artifact layout |
| `build_linux_compat.sh` | builds a Linux kcode artifact against an older glibc baseline |

Terminal-Bench task containers often use an older glibc than a locally built host
binary, so the adapter should use the compat binary:

```sh
scripts/build_linux_compat.sh /tmp/kcode-compat-dist
```

`run_terminal_bench_harbor.sh` builds it automatically if it is missing.

### Quick start

```sh
scripts/run_terminal_bench_harbor.sh \
  --include-task-name regex-log \
  --n-tasks 1 --n-concurrent 1 \
  --jobs-dir /tmp/kcode-tb2 --job-name regex-log-pilot --yes
```

Or point Harbor at the remote dataset with `--dataset terminal-bench@2.0`.

### Sequential campaigns

To run a few tasks at a time while keeping one coherent artifact set:

```sh
python scripts/run_terminal_bench_campaign.py \
  --campaign-dir ~/tb2-kcode-campaign \
  --task regex-log --task largest-eigenval --task cancel-async-tasks
```

It runs with `--n-concurrent 1`, keeps Harbor jobs under
`campaign-dir/harbor-jobs/`, writes a pinned `campaign.json`, refuses to mix runs
when key settings drift, and appends per-task outcomes to `results.jsonl`.

### Environment

| variable | meaning |
|---|---|
| `KCODE_HARBOR_BINARY` | Linux-compatible kcode binary to upload into the container |
| `KCODE_HARBOR_BINARY_DIR` | output dir for the auto-built compat binary |
| `KCODE_HARBOR_OPENAI_AUTH` | path to the OpenAI OAuth file |
| `KCODE_HARBOR_CA_BUNDLE` | optional host CA bundle to upload |
| `KCODE_TB_MODEL` | Harbor model string (default `openai/gpt-5.4`) |
| `KCODE_TB_PATH` | local Terminal-Bench path (default `/tmp/terminal-bench-2`) |
| `KCODE_OPENAI_REASONING_EFFORT` | default `high` |
| `KCODE_OPENAI_SERVICE_TIER` | default `priority` |

The adapter expects OpenAI OAuth at `~/.kcode/openai-auth.json`. Each trial gets a
fresh in-container home (`/tmp/kcode-home`), so memories and auth state are
isolated per trial.

The path has been validated with real Harbor runs on `regex-log`,
`largest-eigenval`, and `cancel-async-tasks`, all passing in-container with
verifier reward `1.0`.

## Compile-time benchmarking

`scripts/compile_time_probe.sh` measures the build critical path, and
`scripts/compile_isolation_report.py` reports LOC, inline tests, `async_trait`
usage, and dependency-boundary advisories. The compile-time isolation effort
itself is an uncommitted idea tracked in [../todo.md](../todo.md).
