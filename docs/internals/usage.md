# Usage and statistics

kcode records several things locally, none of them billing: a route-usage ledger
that ranks models by how you actually use them, a ChatGPT OAuth cost estimate,
per-turn response statistics, and per-session edit counters.

## Route usage ledger

The shared runtime attaches `usage: Option<ModelUsage>` to each
`jcode-provider-core` `ModelRoute`. Routes travel in `comm_list_models` and in
pushed `model_usage_updated` events.

```json
{
  "count": 12,
  "last_used_unix_secs": 1789210000,
  "tracking_started_unix_secs": 1789209000,
  "selection_count": 8,
  "last_selected_unix_secs": 1789208000
}
```

- `count` is tracked agent turns with at least one persisted assistant response
  on the route. A turn anchors to a persisted input message; tool, truncated, and
  reload continuations count once per route; a new input starts a new turn. It
  measures usage, not success.
- `last_used_unix_secs` can advance during a long turn without raising `count`.
- `tracking_started_unix_secs` marks this ledger's coverage. `count: 0` means
  "none recorded here", not "never used".
- `selection_count` / `last_selected_unix_secs` come from the legacy picker file
  `model_picker_usage.json` and measure picker selections, not requests.
- Missing `usage` means unknown (including older servers), never "never used".

The ledger is `model-usage-v1.sqlite3` under `JCODE_HOME` (`~/.kcode`). SQLite
serializes concurrent writers; the primary key combines durable session/input-turn
ID with route identity, so continuations do not double-count. Reads never create
or modify it. Recording is shared by streaming and blocking paths across desktop,
TUI, CLI, and swarm agents; debug sessions and hidden auxiliary calls are not
counted, and a response is attributed to the model that served it. Older sessions
cannot be backfilled: their messages store token counts but not route identity.

`jcode_usage_types::compare_model_usage` orders routes best-first by tracked turn
count, last-used, historical selection count, then last-selected. Search
relevance and explicit current/favorite policy rank ahead of it, with a stable
model/route tie-break.

Clients opt into `model_usage_updated` deltas with
`get_model_catalog.subscribe_usage_updates: true` (default false). Each delta
carries one route, so a client updates its cache without a full catalog rebuild
or the Agent lock; monotonic merging prevents a late snapshot from regressing a
count.

## ChatGPT OAuth API-equivalent usage

The Accounts views show locally recorded ChatGPT OAuth token usage and an
estimated API-equivalent cost. It is **not** a charge to the subscription, an
invoice, or a balance.

- **Today** uses the local calendar date from local midnight, not a rolling or
  UTC window. **Lifetime** accumulates since kcode began recording.
- Only this installation's observed usage is counted; website and other clients
  are not, and historical sessions are not backfilled.
- Usage is separated by saved OAuth account label and stored in
  `openai_oauth_usage.json` under the kcode data directory, independent of any
  API-key spend.

The estimate uses kcode's curated OpenAI price table for the response's model and
service tier: cached input at the cached rate, other input at the normal rate,
output at the output rate, with reasoning tokens already in output not counted
again. Unknown prices and incomplete reports are shown as unknown, never priced
as free; unknown cost is distinct from a measured zero. Rates are dated
observations and are re-verified against OpenAI's model documentation when they
change.

The terminal account details and the desktop view expose the same today and
lifetime summaries, and `kcode usage --json` includes them under the matching
provider report's `extra_info`, so clients need not read the ledger directly.

## History response stats

`HistoryMessage.response_stats` (`jcode_session_types::ResponseStats`) carries
optional per-turn metrics; old history still deserializes and absent metrics are
omitted from JSON.

| field | meaning |
|---|---|
| `duration_secs` | whole-turn wall-clock, including tools. **Currently absent in restored history**: stored assistant messages do not persist it, and timestamps are not used to invent it. |
| `input_tokens`, `output_tokens` | sums of persisted provider-call usage. Raw provider counts, not normalized: OpenAI folds cache reads into input, Anthropic reports them separately. |
| `cache_read_tokens`, `cache_creation_tokens` | sums of the persisted cache-input counters. Do not blindly add them to input. |

Aggregation runs over the whole stored transcript, including tool-only rounds and
history hidden by compaction. Real user prompts delimit turns; tool results,
reminders, and automatic continuations do not. Totals appear once, on the last
assistant message of the turn, and only if that row is rendered with no pending
tool calls: this is inferred from the transcript, not a persisted `done` event.

Each field is independently unknown if any contributing round lacks it or the sum
overflows `u64`; unknown is not zero. These are per-turn provider-call totals, not
unique context size, session usage, or tokens per second. The same context can be
counted again in each tool round, and per-call provider identity is not stored.

## Session edit stats

Cumulative changed-line counters per session:

```json
{"added": 120, "removed": 35, "approximate": false}
```

These are **cumulative changed lines**, not a net worktree diff: re-editing a line
counts again. Built-in `write`, `edit`, `multiedit`, `patch`, and `apply_patch`
mutations count (including successful subcalls inside `batch`); shell commands,
MCP tools, and external editors do not. No repository git state is consulted.
Failed or proposed edits add nothing, no-op edits add zero, and a partially
successful multi-file tool still counts the mutations it performed.

Counters live at `$JCODE_HOME/sessions/edit-stats/<session-id>.json` (default
`~/.kcode`), serialized by a per-session file lock with atomic replacement. They
survive compaction and are separate per agent even when agents share a worktree.
When no counter exists, the first write seeds a best-effort estimate from older
persisted history (capped at 32 MiB per snapshot/journal), marked
`approximate: true`; forked legacy history is not attributed to the child. This is
not an audit log: mutation and persistence are not one transaction, so a crash
between them can lose accounting.

