# Swarm

A swarm runs work as a **task DAG**. You declare tasks and dependency edges; the
server's scheduler turns ready nodes into worker turns. Agents are fungible
workers, not entities you micromanage: the old coordinator/worktree-manager
roles are scheduler policy, not user-facing roles.

The graph is the repo's work list. A run reads the rows it works: the open rows its
members hold, the rows nobody holds, and the rows whose holder is no longer a member
(the sweep's case). What a run keeps of its own is one sparse map keyed by row id, the
lifecycle status a turn set for that row. Every read uses the run's status when it set
one and the row's own otherwise, which is why a row with no run entry is work with no
bookkeeping.

0.4g removed the rest of the plan object: its `version` (ordering comes from sending
inside the per-swarm lock that mutated), its `participants` (the recipients are the
swarm's sessions), its durability (a restart forgets the run's statuses and the rows
come back from the list), and the item cache itself, which had become a second copy of
rows the store owns. The reclaim cap went with it: releasing the claim in the list is
the whole recovery, and the bound belongs to the loop that repeats the work.

The swarm is one executor of the repo's work list (`plans/work-list.md`). The
list is the shared contract and the work lives there; a run holds its own lifecycle for
the rows it works and reads everything else from the file.
The seed is the rows the
seeding session holds: `bridge::seed_specs` lifts them into nodes (the row's id,
words, kind and `blocked_by` are the node's, its position is its priority), a row
whose kind the engine cannot read stays out rather than being given a guessed
one, and a row the list already has is not seeded again.

## One engine

There is one engine and no mode axis. The former deep/light presets, the
auto-inserted critique/verify gate, the gate-pass audit, the deep artifact
validity check and the artifact-or-nothing turn-end rule were removed in 0.4a of
`plans/work-list.md`; the engine keeps the DAG model, the scheduler, the typed
handoff artifact and decomposition. What was lost (the automatic gate insertion
and the refusal to close without an artifact) is named in `docs/todo.md`.

## Ownership: a tree over the rows

The unit of mutation is **decomposing a row you hold**; you never edit arbitrary
rows. Writes go through one store, the file's read-modify-write (rule 2): a
decomposition adds child rows with `parent` set and blocks the parent on them, so
the parent is a join and is picked again once they close. A child may block only on
rows that already exist, and the store refuses a cycle, which keeps the graph
acyclic by construction. Only the row's holder may decompose it.

A row's `parent` is the file's one hierarchy: `anchor_from_rows` adopts root rows and
`expand_row_on_disk` adds children, so anchor grouping and decomposition are the same
relation. Nothing keeps a second copy of it; a row is composite when it has an open
child or a nonempty `records`, which is how "was decomposed" survives its children's
removal.

Liveness is the member's, not the task's: the turn loop marks the session's activity
(`session_metrics::record_activity`) and a task is alive when its assignee is. A member
holds one row at a time, so "busy" is the member's in-flight work; and fan-out is not a
decision but the list's ready set (`blocked_by`), which is why a linear chain reuses one
member while independent rows fan out on their own.

## Node kinds

A node's fate flips at runtime, not at draft time:

- **Atomic** - the worker executes the task and writes a handoff artifact.
- **Composite** - the worker decomposes the row into child rows. The row becomes a
  join: it keeps its holder (the one that decomposed it is the one that integrates),
  is blocked until its children close, then is picked again, reads what closed under
  it, and writes one synthesized output. A composite owner plans and integrates (map
  then reduce); it does not execute leaf work.

Terminal action kinds are task-agnostic; only the close's result and artifact
change: `explore` (findings), `implement` (diff/commit ref), `verify` (pass/fail
and failures), `fix` (patch). A worker that finds follow-up work adds it as rows
or children, rather than an engine inserting it.

## Dataflow: the close's record is the channel

A close writes `{"id","result","artifact"}` onto the row that owns the work, so the
results of earlier work accumulate where the row that integrates them sits. A row's
context is therefore its own `records`: a split row reads its children when its join
turn runs, and a run's top row reads the run when it closes. Fan-out and fan-in fall
out of this naturally. A row blocked by another gets no artifact from it, because the
list keeps no edge to a closed row: what a row receives is the work that closed under
it, which is why a join or a run end is the place context accumulates.

Artifacts are **by-reference by default**: "the API is in `crates/foo/api.rs`,
commit `abc123`", with the repo and git as the shared medium. Embed by value only
for things not in the repo (a decision, an analysis). This keeps context small,
which matters at depth.

The handoff artifact schema:

`findings`, `evidence` (file:line or commit refs, not bare claims),
`edge_cases_considered`, `validation`, `open_questions`, `confidence`,
`what_i_did_not_check`.

`what_i_did_not_check` is the point: forcing an agent to name what it did *not*
explore surfaces the gaps its caller (or a follow-up row) can widen.

## Coordination and communication

- A member's deeper work is rows the run dispatches; only the root session starts
  agents. Membership is the whole authorization to drive a swarm's rows: a member's
  record names its swarm, and the run drives the rows it holds. The per-swarm
  coordinator slot is an election record only (who owns the subtree) until 0.4g's g4
  derives it from the anchor row.
- When the root leaves, its workers reparent to the live coordinator, or become
  roots, so the spawn tree never holds dangling report-back edges. Session renames
  rewrite children's report-back edges and the holder of the rows the session held
  (`rename_row_holder_on_disk`), so ownership, stop permission, and subtree scope
  survive churn.
- A worker must end each prompted turn with a useful final response; the server
  forwards it to its owner as the **completion report** (outcome, changes,
  validation, blockers), not a bare `done`.
- Communication is **dataflow first**: the close's record on the owning row is the
  primary channel. Direct DMs and subtree-scoped broadcasts are the exception
  path (conflict resolution, clarifying questions up the tree); broadcasts are
  subtree-scoped so they cannot become a member-cap-sized storm. Topic channels
  and the shared-context key-value store still exist but are being deprecated:
  migration steps 3-4 (migrate flows off them, then remove) are pending.
- Inter-agent messages are delivered as notifications, queued as soft interrupts
  and injected into running agents at safe points, so they interleave without
  starting a new turn. Completed or idle agents do not resume on a notification;
  the coordinator must assign, wake, or respawn them. Recovery handoffs are
  explicit: retry (same assignee), reassign (existing agent), replace (new
  assignee after safe checks), salvage (reassign with preserved progress).

## Limits

Runaway prevention is one cap: `MAX_SWARM_MEMBERS` = **1000** live members per
swarm. Only the root session starts agents (0.4b), so the agent tree is one level
and there is no recursive-spawn depth to cap; at the cap, further spawns are
refused. Work decomposition is still a tree, arbitrarily deep: a child row can be
decomposed again. A configurable live-worker budget (32 by default) throttles
concurrency below that. The graph orders work but does not do mutual exclusion: two
subtrees editing the same files is still the no-locks case, resolved by direct
contact between the agents.

## Tool surface

The `communicate` tool carries the swarm actions: `task_graph` (seed the run's rows
from the rows this session holds), `expand_node`, `complete_node`,
`run_plan`, `fill_slots`, plus
`spawn`/`dm`/`broadcast`/`channel` and the shared-context ops as lower-level
escape hatches. The TUI shows a swarm info widget (agent/manager/coordinator graph)
and a plan info widget (the run's rows with the status a turn set for each).
