# Jcode Server Memory Incident Runbook

Status: active operational runbook
Updated: 2026-07-14

This runbook answers two questions:

1. What is using the jcode server's memory?
2. What is the safest next action for that specific cause?

The goal is not to react to every high RSS value. The goal is to distinguish live application state, allocator retention, and non-heap mappings before changing or stopping anything.

## One-command triage

Run:

```bash
jcode debug 'server:memory-incident'
```

This is the first command during an incident. It is intentionally lightweight. It does not lock or serialize every Agent transcript, so it remains useful when thousands of sessions are resident.

The report includes:

- RSS, PSS, anonymous PSS, and allocator live bytes
- 15-minute PSS growth
- live, headless, detached, and connected session counts
- status counts for resident sessions
- the swarms with the most resident Agents
- a severity and primary-cause classification
- ordered, cause-specific actions

Preserve the JSON output in the incident notes before changing state.

## Offline timeline analysis

Runtime memory logging is enabled by default and writes daily JSONL files under:

```text
~/.jcode/logs/memory/
```

Analyze the latest server process lifetime:

```bash
python scripts/analyze_runtime_memory_log.py --days 1
```

The analyzer selects the latest server and client process instances by default. This is important because comparing PSS across a server reload produces false spikes. Use `--all-instances` only for explicit cross-instance forensics.

List recorded process lifetimes and select a pre-reload incident directly:

```bash
python scripts/analyze_runtime_memory_log.py --days 1 --list-instances
python scripts/analyze_runtime_memory_log.py --days 1 --instance <server-instance-id>
```

Prefer `--instance` for postmortems. It preserves one coherent process lifetime without mixing a high-memory server with its low-memory replacement.

For machine-readable output:

```bash
python scripts/analyze_runtime_memory_log.py --days 1 --json > /tmp/jcode-memory-analysis.json
```

## Severity thresholds

The built-in incident report uses these initial operational thresholds:

| Signal | Warning | Critical |
|---|---:|---:|
| PSS | 1 GiB | 2 GiB |
| PSS growth in 15 minutes | 256 MiB | 1 GiB |
| Resident Agent sessions | 128 | 512 |

A threshold starts an investigation. It does not authorize destructive cleanup by itself.

## Decision tree

### 1. `runaway_live_session_population`

Evidence:

- live Agent count is high or rising rapidly
- headless or detached sessions greatly outnumber attached clients
- allocator live bytes rise with session count
- one or more swarms dominate `top_live_swarms`

Actions:

1. Pause or cap the producer creating sessions.
2. Run `jcode debug 'swarm:list'` and inspect the largest live swarm.
3. From the owning coordinator, use `swarm list` and `swarm cleanup` to remove workers it no longer needs.
4. Do not destroy sessions blindly. Confirm that active work is disposable first.
5. Re-run `server:memory-incident`. Require live sessions, allocator live bytes, and PSS to fall together.
6. Only then run `allocator:purge` if freed-but-held memory remains high.

Why: allocator purge cannot free live Agent runtimes.

### 2. `allocator_retention`

Evidence:

- allocator retained-resident estimate is at least 256 MiB
- retained-resident memory is at least 25% of PSS
- allocator live bytes are materially below anonymous PSS

Actions:

```bash
jcode debug 'server:memory-incident' > /tmp/before.json
jcode debug 'allocator:purge'
jcode debug 'server:memory-incident' > /tmp/after.json
```

A large PSS drop confirms allocator retention. If it repeatedly regrows, inspect allocation churn and allocator decay rather than raising memory budgets.

### 3. `session_payload_growth`

Evidence:

- tracked transcript/provider-cache/tool/blob bytes explain at least half of allocator live memory
- one or more sessions dominate `top_by_json_bytes`

Actions:

1. Run `jcode debug 'server:memory'` for the full attribution walk.
2. Inspect provider cache, tool results, large blobs, and payload text.
3. Compact, summarize, truncate, or move large artifacts out of line.
4. Add or tighten a hard cap before accepting a larger steady state.

### 4. `unattributed_live_heap`

Evidence:

- allocator live bytes exceed 1 GiB
- session population and allocator retention do not explain the heap
- live-heap attribution coverage remains below 50%

Actions:

1. Capture `server:memory` and the runtime log analysis.
2. Add counters for any obvious missing owner.
3. If ownership is still unclear, use a `jemalloc-prof` build:

```bash
jcode debug 'allocator:profile:on'
jcode debug 'allocator:profile:dump /tmp/jcode-server.heap'
```

The normal system-allocator build cannot produce allocation-stack profiles. Do not claim heap ownership from RSS alone.

### 5. `non_heap_or_mapping_growth`

Evidence:

- PSS is high but allocator live bytes are not
- file-backed, shared-memory, or thread-stack mappings are growing

Actions:

```bash
cat /proc/<server-pid>/smaps_rollup
pmap -x <server-pid> | sort -k3 -nr | head -40
ps -T -p <server-pid> -o pid,tid,%cpu,time,comm,wchan:32
```

Investigate model mappings, shared memory, thread creation, or large anonymous mappings outside the allocator.

## Escalation ladder

Use the cheapest reliable evidence first:

1. `server:memory-incident`, normally sub-second and non-blocking
2. runtime JSONL analyzer, process-lifetime trend and incident classification
3. `server:memory`, expensive per-Agent attribution
4. allocator purge A/B test, only for retention
5. jemalloc heap profile, only for unexplained live heap
6. OS mapping and CPU profiler correlation

## Required incident record

Save:

- server ID, version, git hash, and uptime
- PSS, anonymous PSS, allocator live, and retained-resident bytes
- live/headless/connected session counts
- top live swarms and status counts
- 15-minute growth
- the chosen action and before/after measurements
- whether active work was preserved

## Resolution criteria

An incident is resolved only when one of these is true:

- the identified live owner was reduced and PSS fell accordingly
- allocator purge proved retention and the recurrence mechanism was corrected
- mapping growth was identified and bounded
- a heap profile identified an owner and a regression test or cap was added
- the high steady state was proven intentional, documented, and given an explicit budget

Do not close an incident with only “memory dropped after restart.” A restart erases evidence and does not identify the cause.
