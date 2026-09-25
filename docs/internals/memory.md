# Memory

kcode runs a long-lived server, so memory is live session state plus allocator
retention plus non-heap mappings. The goal is not to freeze usage but to make
every change measurable, reviewable, and justified.

## Seeing where memory goes

Debug-socket commands:

| command | what it shows |
|---|---|
| `agent:memory` | one agent's process + session breakdown |
| `server:memory-incident` | fast cause classification and prescribed next actions |
| `server:memory` | full per-agent attribution walk |
| `server:memory-history` | recent process memory samples |

TUI: `:debug memory` (aggregate), `:debug memory-history`, and
`:debug markdown:memory` for the markdown highlight cache.

Runtime memory logging is on by default and writes daily JSONL under
`~/.kcode/logs/memory/`. Analyze the latest process lifetime with
`python scripts/analyze_runtime_memory_log.py --days 1`; it also takes
`--list-instances`, `--instance <id>`, `--json`, and `--all-instances`. Prefer
`--instance` for postmortems: comparing PSS across a server reload produces
false spikes.

## Budget

**Hard caps** are limits the caches already enforce; a regression means the bound
changed or was bypassed.

| metric | budget | source |
|---|---|---|
| `highlight_cache_entries` | `<= 256` | `crates/jcode-tui-markdown/src/lib.rs` (`HIGHLIGHT_CACHE_LIMIT`) |

If a hard cap changes: document the new limit and why, verify eviction still
works, and confirm no unbounded growth path was introduced.

**Ratchet expectations** are relationships, not caps. They may change only with
an explanation and updated tests:

| relationship | expectation |
|---|---|
| provider message cache vs transcript messages | same order of magnitude, normally tracking closely |
| session provider-cache JSON bytes vs canonical transcript JSON bytes | comparable, not diverging |
| transient provider materialization bytes | returns to ~zero outside materialization-heavy paths |
| display large-tool-output bytes | large values need explanation: raw tool output is being retained |

When changing memory-heavy code, capture: which counters moved, whether a hard
cap changed, whether duplication increased (canonical transcript, provider cache,
materialized provider view, display copy, side-panel copy), and whether logs can
still explain the growth. Prefer fixing duplication over raising a budget.

## Incident runbook

Start with one lightweight command that does not lock or serialize every
transcript, so it stays usable with thousands of resident sessions:

```bash
kcode debug 'server:memory-incident'
```

Preserve its JSON before changing any state. It reports RSS/PSS/allocator bytes,
15-minute growth, session counts, the swarms with the most resident agents, a
severity and primary-cause classification, and ordered cause-specific actions.

**Severity thresholds** start an investigation; they do not authorize cleanup.

| signal | warning | critical |
|---|---:|---:|
| PSS | 1 GiB | 2 GiB |
| PSS growth in 15 minutes | 256 MiB | 1 GiB |
| resident agent sessions | 128 | 512 |

### Causes and actions

1. **`runaway_live_session_population`** - live agents high or rising, headless
   outnumbering clients. Pause the producer, inspect the largest swarm
   (`swarm:list`, `swarm cleanup`), confirm work is disposable, then re-check.
   Only run `allocator:purge` if freed-but-held memory remains high; purge cannot
   free live runtimes.
2. **`allocator_retention`** - retained-resident at least 256 MiB and 25% of PSS,
   with live bytes well below anonymous PSS. Run `allocator:purge` as a before/after
   A/B; a large PSS drop confirms it. If it regrows repeatedly, fix allocation
   churn, not the budget.
3. **`session_payload_growth`** - tracked transcript/cache/tool/blob bytes explain
   at least half of live memory, one or more sessions dominating
   `top_by_json_bytes`. Run `server:memory`, compact or move large artifacts out
   of line, and tighten a cap before accepting a larger steady state.
4. **`unattributed_live_heap`** - live bytes over 1 GiB that sessions and
   retention do not explain, with attribution coverage below 50%. Add counters
   for the missing owner; if still unclear, use a `jemalloc-prof` build
   (`allocator:profile:on`, `allocator:profile:dump`). The system allocator cannot
   produce allocation-stack profiles, so never claim heap ownership from RSS alone.
5. **`non_heap_or_mapping_growth`** - PSS high but allocator live bytes are not.
   Inspect `smaps_rollup`, `pmap -x`, and per-thread `ps`. Look at model
   mappings, shared memory, thread stacks, and large anonymous mappings.

### Escalation ladder

Cheapest reliable evidence first: `server:memory-incident` (sub-second,
non-blocking) → runtime JSONL analyzer → `server:memory` (expensive) → allocator
purge A/B (retention only) → jemalloc heap profile (unexplained live heap) → OS
mapping and CPU profiler correlation.

### Closing an incident

Record: server ID/version/hash/uptime, PSS + anonymous PSS + allocator live +
retained-resident bytes, session counts, top swarms, 15-minute growth, the action
taken with before/after, and whether active work was preserved. An incident is
resolved only when the identified owner was reduced and PSS fell, retention was
proven and its recurrence fixed, mapping growth was bounded, a heap profile named
an owner and a test or cap was added, or the steady state was proven intentional
and given an explicit budget. "Memory dropped after restart" is not a resolution.
