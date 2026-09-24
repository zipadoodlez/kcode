# Per-session edit statistics

Kcode records cumulative edit counters for each session:

```json
{"added": 120, "removed": 35, "approximate": false}
```

These are **cumulative changed lines**, not a net worktree diff. Replacing a line
counts one addition and one removal. Editing it again counts again. Built-in
`write`, `edit`, `multiedit`, `patch`, and `apply_patch` mutations are covered,
including successful subcalls inside `batch`. Shell commands, MCP tools, and
external editors are not covered. No repository-wide git state is consulted.

## New tool executions

Each successful filesystem mutation records a full before/after line diff,
independent of truncated tool previews. No-op edits add zero. Failed/proposed
edits add nothing. If a multi-file tool partially succeeds and later fails,
mutations already performed still count. Ordinary moves count content changes
rather than treating an unchanged file as deleted and recreated.

Small counters live at `$JCODE_HOME/sessions/edit-stats/<session-id>.json`
(default `~/.kcode`). A per-session OS file lock serializes concurrent updates,
and atomic replacement prevents readers from observing partial JSON. Counters
survive transcript compaction and are separate for agents sharing a worktree.
The first counter seeds a best-effort estimate from older persisted history.
Forked legacy history is not attributed to the child.

Limits: file mutation and counter persistence are not one transaction. A crash
or disk error between them can lose accounting. Concurrent external writes to
the same file can race the tool's before-content read. Unreadable old text and
legacy migration are marked approximate. This is not an audit log.

## Legacy history

When no counter exists yet, the first write seeds a best-effort estimate from the
session's persisted snapshot plus journal, capped at 32 MiB each. Missing,
malformed, oversized, and forked legacy records remain unavailable. Truncated
output, compaction, aliases in old batch headers, and unsupported legacy formats
may omit edits. `approximate: true` is an estimate, not a strict lower bound; old
history cannot generally be reconstructed exactly. `SessionEditStats` and the
recording path live in `jcode-app-core`; there is no session-list or SDK export.
