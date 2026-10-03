# Swarm

A swarm is a count, not a mode: one member and eight are the same code. What
exists is a **run**: a session working the rows it holds. The server adds no
second graph, no plan object, and no per-run identity beyond the session that roots
it.

## The rows are the one graph

The open work is one file per repo, `tasks.jsonl` at the root, found from git so a
session in `crates/foo` reads the same list; outside a repo it is a scratch file.
A row is `id`, `content`, optional `kind`, `parent`, `blocked_by`, `assigned_to`,
`note`, and `records` (the closes of the rows worked under it). The `todo` tool is
the only writer: it owns the write protocol and re-reads the file before each
write, so a human edit is an input, not a conflict.

Readiness is asked on read, never stored: a row is ready when it is not blocked,
has no open child, is held by me or nobody, is in my scope, and has not already
been worked in this run. A close deletes the row and removes its id from every
dependent's `blocked_by`, so an entry there always names an open row and a row
with none is ready.

## Membership is the spawn edge

A session belongs to the run rooted at the end of its `report_back_to_session_id`
chain; a session that reports back to nobody roots its own run. `swarm_root` in
`server/swarm.rs` is the one derivation, read by every path that once read a
stored swarm id. There is no swarm id to declare, no coordinator map, and no
per-session toggle. The coordinator is a derivation too: the session that roots
the run.

When a root leaves, its workers become roots of their own runs, so the spawn tree
holds no dangling report-back edges. A session rename rewrites its children's
report-back edges and the rows it held, so ownership and subtree scope survive
churn.

## The loop owns dispatch

The next turn is decided in one place, `server/live_turn.rs`: if a person is
attached to the session, they own the turn; if the session holds a ready row it has
not worked in this run, it takes that row; otherwise it sleeps. Nothing else starts
work.

A write that makes a row ready is what wakes the loop. The `todo` tool publishes
`TodoUpdated`, and the server's bus monitor calls `wake_ready_owners`, which starts
a turn for a headless holder that is idle. A ready row nobody holds wakes nobody:
the run's dispatch is what assigns it, and the assign is the write that wakes.

A run ends when none of its rows is ready. The anchor — the row a grant named, or
the run's first row for a whole-list run — is the last thing picked, and once the
run's rows are gone `end_spent_runs` stops the member sessions the run spawned.
Only headless members the run spawned are stopped, so a session a person sits in is
never reaped.

Continuing unattended is a permission, not a consequence of holding rows, and it is
read where the next turn is decided: a turn can grant it for one run, a headless
member holds it inherently, and the default is off.

## Handing work over is a message

The run picks a row and a session and sends a message naming the row and asking for
the work; the row's own words are the payload. Nothing is offered, queued, or
transferred but words — a row is already assigned, so an offer would only be a
second in-memory copy of the list. If the holder is headless and idle the server
starts its turn there and then; if a person is attached, the turn is theirs. A row
whose note cannot stand alone is a row that cannot be handed over.

`message` addresses one session, by id or unique friendly name. There is no group
send: a run's reach is the rows it holds, and the people reach it by attaching.

## The close is the report

A close carries a nonempty `result`, and the record goes onto the row that owns the
work — the parent for a node that has one — so root-level records accumulate on the
anchor and a resumed run finds its scope there. A parent's row cannot close while a
child names it, which is what makes the anchor the last row picked without a second
way to say the same thing.

The discipline lives in the close instruction, not in a schema: state what proves
it, what it showed, and what you did not check. Kinds are the run's word for the
work, not six result schemas. There is no completed state: a drop is a close whose
result says so.

## Stop

`stop` is the run's own lever, called by its root through the tool, and it ends the
target's subtree: naming the run's root ends the run, naming a member takes that
member and everything it spawned, deepest first. There is no batch form and no
`all` flag; the set is derived from the target. It is the only control verb:
`wake`, `retry`, `reassign` and `replace` were the loop's job done by hand, and
`cleanup` is what `end_spent_runs` now does.

## Limits

One hard cap and one throttle, both read where a spawn is admitted
(`server/comm_session.rs`): `MAX_SWARM_MEMBERS` = **1000** live members per run is
the hard stop, and `agents.swarm_max_concurrent_agents` (32 by default) is the
machine-safety budget below it. A run's own bound is a run property, never a row
field, and the default is none: no ready row ends it.

The graph orders work but does not do mutual exclusion. Two subtrees editing the
same files is still the no-locks case, resolved by direct contact between the
agents.

## Tool surface

The `communicate` tool carries the swarm actions: `spawn`, `stop`, `list_models`,
and `message`. Reading a run is reading the rows; controlling one is attaching to a
member, granting a turn, messaging a row's holder, or writing.
