# Swarm

A swarm runs work as a **task DAG**. You declare tasks and dependency edges; the
server's scheduler turns ready nodes into worker turns. Agents are fungible
workers, not entities you micromanage: the old coordinator/worktree-manager
roles are scheduler policy, not user-facing roles.

The graph is a single server-owned, versioned object (`jcode-plan`'s
`VersionedPlan`). Agents mutate it only through validated ops.

## Two modes: deep and light

One engine, two presets. Both use the same DAG data model and scheduler; only the
rigor machinery and the member cap differ.

| dimension | deep | light |
|---|---|---|
| goal | leave no nook unexplored | parallelize for speed |
| shape | recursive, self-deepening | mostly flat, one level |
| decomposition | mandatory (composite by default) | optional |
| critique/verify gate | required before a node closes | off (optional final check) |
| recursion | unbounded depth | root-only (no nesting) |
| handoff artifact | full typed schema | lightweight, free-form |
| cost | high, deliberate | low, fast |

## Ownership: a tree over the graph

The unit of mutation is **expanding a node you own**; you never edit arbitrary
nodes. Writes are partitioned by owner, so two owners never touch the same
region and the shared graph stays coherent without locks. Mutations are
append-style ops (`add nodes`, `add edges`, `complete node`), validated
server-side for acyclicity and ownership. New edges may only point at
already-existing upstream nodes, which preserves acyclicity by construction.

Each node records an origin (`seed`/`expand`/`gap`/`gate`), and a plan carries
`seeded_count`/`grown_count` so a deep plan that never outgrew its seed is
visibly under-explored.

## Node kinds

A node's fate flips at runtime, not at draft time:

- **Atomic** - the worker executes the task and writes a handoff artifact.
- **Composite** - the worker decomposes the node into a child sub-DAG it owns.
  The node becomes a join: it stays in progress until children complete, then the
  owner re-wakes, reads their artifacts, and writes one synthesized output. A
  composite owner plans and integrates (map then reduce); it does not execute
  leaf work.

Terminal action kinds are task-agnostic; only the artifact and "done" contract
change: `explore` (findings), `implement` (diff/commit ref), `verify` (pass/fail
and failures), `fix` (patch). A failing verify spawns `fix` nodes, the same way a
critique spawns gap nodes.

## Dataflow: the edge is the channel

On completion a node stores a typed **handoff artifact** on the node. When a
dependent becomes runnable, the scheduler assembles its input from its own prompt
plus the merged artifacts of its dependencies. Fan-out and fan-in fall out of
this naturally.

Artifacts are **by-reference by default**: "the API is in `crates/foo/api.rs`,
commit `abc123`", with the repo and git as the shared medium. Embed by value only
for things not in the repo (a decision, an analysis). This keeps context small,
which matters at depth.

The deep-mode artifact schema is required, not advisory:

`findings`, `evidence` (file:line or commit refs, not bare claims),
`edge_cases_considered`, `validation`, `open_questions`, `confidence`,
`what_i_did_not_check`.

`what_i_did_not_check` is the point: forcing an agent to name what it did *not*
explore surfaces the gaps the gate then turns into nodes.

## Gates (deep mode)

Comprehensiveness is structural, enforced by the engine in `jcode-plan/src/dag`:

- **Gate discipline.** Every composite node must have a critique (explore) or
  verify (code) dependent before it can close. Gates are adversarial and
  domain-agnostic: "what did this miss given its own stated scope" / "does the
  declared acceptance check pass".
- **Gap/failure becomes graph.** A gate that finds gaps or failures emits new
  child nodes, and the parent cannot close until they drain. `inject_gap` adds
  nodes at a gate's scope.
- **Root gate.** A deep seed auto-inserts a parent-less gate depending on every
  root-level node, so even a flat seed needs a final adversarial pass; that pass
  can inject new top-level nodes. Re-seeding widens the gate's scope and re-opens
  it.
- **Enumerated coverage.** A passing deep gate must address every done node in its
  scope by id, up to a cap of 20; above the cap only HIGH-confidence nodes may be
  skipped. "All good, no gaps" is rejected (`UncoveredSiblings`), and a pass is
  rejected if nodes entered the scope after dispatch (`StaleGateScope`).
- **Artifact-or-nothing.** Deep mode has no auto-complete: a turn that ends with
  its node still running is re-queued once (`no_artifact_requeues`) and failed on
  repeat. A deep node closes only via `expand_node` or `complete_node`.

## Coordination and communication

- Nested owners coordinate their own subtree through spawn prompts, DMs, and stop.
  The single per-swarm coordinator slot is only for the shared plan
  (`propose_plan`/`approve_plan`/`assign_task`/`task_control`), because there is
  exactly one `VersionedPlan` per swarm.
- A mid-tree member leaving reparents its children to their live grandparent
  (else the coordinator, else they become roots). Session renames rewrite
  children's report-back edges, so ownership, stop permission, and subtree scope
  survive churn.
- A worker must end each prompted turn with a useful final response; the server
  forwards it to its owner as the **completion report** (outcome, changes,
  validation, blockers), not a bare `done`.
- Communication is **dataflow first**: the dependency edge's artifact is the
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
swarm. There is deliberately no depth cap and no per-node fan-out cap; at the cap,
further spawns are refused. A configurable live-worker budget (32 by default)
throttles concurrency below that. The graph orders work but does not do mutual
exclusion: two subtrees editing the same files is still the no-locks case,
resolved by direct contact between the agents.

## Tool surface

The `communicate` tool carries the swarm actions: `task_graph` (seed),
`expand_node`, `complete_node`, `inject_gap`, `run_plan`, `fill_slots`, plus
`spawn`/`dm`/`broadcast`/`channel` and the shared-context ops as lower-level
escape hatches. The TUI shows a swarm info widget (agent/manager/coordinator graph)
and a plan info widget (the task DAG with per-node status and checkpoints).
