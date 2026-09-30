# Work list

Work on branch `work-list`. `docs/todo.md` points here. Tick a step when it lands
and delete it; git has the history.

## Destination

One file holds the open work. One item type, one writer, one way work gets done:
a **run** over the rows.

A row is durable: `id`, `content`, optional `kind`, `parent`, `blocked_by`,
`assigned_to`, `note`. A run is live: it holds the rows it is working plus the
gates the machinery inserts, and it dies with the process. The run uses the row's
`id` as its node id, so nothing maps between the two and nothing is named twice.

A swarm is not a mode, it is the count. A run with one member and a run with
eight are the same code. Nothing declares a swarm: no start event, no coordinator
object, no swarm id. Membership and the coordinator are derived from who holds
what.

Rows become nodes once, when a run takes them: `kind`, `blocked_by` to
`depends_on`, `parent`, and position as priority. Three things cross back, and
only these: a claim, a note, a close. The close payload is the record, carrying
the durable target (commit, file, or decision) and the machine-readable parts
such as validation and what was not checked. Kill the process and the work is
untouched, because none of it was in the file.

Gates are the one node with no row. The machinery inserts them and names them
itself, and when a gate finds work, that work becomes a row.

## The two kinds of session

A swarm member has no human, so the run supplies every turn: dispatch wakes it,
it works, it closes, and it ends when nothing it can reach is ready. Autonomy
needs that stop condition, or it is a loop rather than autonomy.

A session with a human never has turns taken from it: dispatch offers it rows, at
a turn boundary and only when it has nothing else to do, and the session claims
one when it picks it up. The claim is always the worker's act, never the run's,
so an offered row is never stranded by somebody walking away.

`is_headless` decides which of the two a session is, and it is a property of the
session, not a mode of the run. That predicate is the whole difference between
today's two mechanisms, the client-side poke and the server-side dispatch.

## How long a run goes on

A run continues while a row it can reach is ready, and ends when none is. That is
the only stop rule and it is checkable: no ready row means nothing to do.

What it can reach is its scope, never the whole file. The user's instruction sets
that scope, and a run given no instruction proposes one, names the rows it will
work, and stops at that boundary. A woken member is bounded by the row or subtree
it was handed. Without a scope, a session with a human and two hundred open rows
would work forever.

The scope is structural, not a promise. A run works the rows it holds, plus rows
it creates under them, and it may not claim outside that set. So a run with
nothing held has nothing to loop over, and it takes another turn on its own only
while something in its scope is still ready. Eagerness therefore has exactly one
outlet, the claim, which is one visible act over named ids and is as easy to undo
as it was to make.

It also ends by yielding when the only thing that would unblock it is a person.
That needs no field: a row whose `assigned_to` names a person and whose
`blocked_by` names the rows waiting on it is what stops the loop, and that is what
we already call a decision.

A session with a human is its own stop. The human's prompt always wins at a turn
boundary, and the run resumes offering when that session is idle again, so the
user's hand is the control, continuously, and no switch decides whether the loop
may continue. An unattended run reports when it yields, and the report is the same
terminal event a tracked turn already fans out.

A bound on a run is a run property, never a row field: the loop's ceiling (turns,
rows, or a deadline) belongs with the run, beside the concurrency limit
`run_plan` already takes. The default is none, and the work boundary above is
what normally stops it. It will exist for cost, not for correctness.

The poke's switch and its repeat cap have a replacement that is strictly better.
The poke repeated blindly because it could not tell whether another nudge would
help, so it needed a fingerprint and a cap and a default of off. A run knows what
is ready and when it is blocked.

## Rules

Settled 2026-10-01. Not steps; what every step has to satisfy.

1. **One file per repo, `tasks.jsonl` at the repo root, and it supersedes
   `docs/todo.md`.** The home is found from git, not from the working directory, so
   a session in `crates/foo` reads the same list; work with no repo uses the same
   basename in a session scratch location. The file lives in the server's working
   directory, so a remote attach writes the remote checkout and git carries it
   back.
2. **Never hand-authored.** The tool owns the write protocol and re-reads the
   file before each write, so a human edit is an input, not a conflict.
3. **One close action, for every entry.** The holder produces the result the
   entry owes and the row is removed. A parent's result is its children's
   results integrated, a leaf's is its own work, and nothing else differs.
   Closing can also add: work that reveals work goes in with the same `add`
   action, and rule 6 makes a task's spawns land before its close.
4. **There is no completed state.** A row goes when its result is durable: for
   code, the commit that lands the work, with the delete in the same commit; for
   an exploration or a decision, after the result is written somewhere durable.
   The close names that place, so a row whose work had no code still has a home.
5. **A drop is a completion whose result says so.** One precondition, a nonempty
   result. `cancelled` is not a state, so the four-word status vocabulary and its
   helpers (`canonical_todo_status`, `todo_status_is_completed`,
   `todo_status_is_cancelled`) go with it.
6. **A parent's row cannot go while a child names it.** That is the only
   enforcement, and it needs no status tracking.
7. **The server is the only writer.** Agents claim a row by setting
   `assigned_to` before any work, so an unclaimed row is free. Many claimers,
   one writer. A claim rides the work's commit, so the tree is dirty from claim
   to landing.
8. **One item type**, in `kcode-task-types`, the crate whose name is the concept.
9. **The run is the only way work gets done.** Everything that executes work is a
   run over rows, with one member or several, and no branch anywhere asks which.
   `is_headless` decides who supplies a turn, and the loop ends when nothing it can
   reach is ready, or when only a person can unblock it. No switch, no repeat cap.
10. **Hierarchy is one optional `parent` field on a flat list**, while blocking
    is order, so they are two fields. A parent's row stays after its children are
    gone, because the integration is still owed. `group` is deleted, because the
    parent chain is the grouping.
11. **Membership and the coordinator are derived, never stored.** The holder of a
    parent coordinates the holders of its children, which deletes the
    `coordinators` map and any swarm id.
12. **The durable/live seam.** Who holds an entry is durable and lives in the
    file. Whether that session is alive and busy is live server state that dies
    with the process. A run never holds durable truth. Today one member record
    welds the two.
13. **The file is JSON Lines, one task per line**, flat: `id`, `content`,
    optional `kind`, optional `parent`, `blocked_by`, `assigned_to`, `note`. A row
    is a task and the only row kind, so a note that is not a task is a doc.
    Nothing else is stored: not `status`, `priority`, `group`, `subsystem` or
    `file_scope`, and line order promises nothing. A row reads as open, blocked,
    or claimed from its own fields.
14. **A row with no `kind` is not seedable.** It can be listed and edited, but a
    run may not guess whether the work is an investigation or a change, because
    the guess decides the artifact and the gate. Typing it once records it.
15. **The check is a rule, not a field.** The close action requires a nonempty
    result, and the tool description names the check and asks for its actual
    result. Nothing for the harness to judge. A skipped check shows only in the
    commit.
16. **Refer to a task by its words when talking to the user.** The file keys on
    `id`; the conversation does not.

## Steps

Widest shared shape first, so no step sweeps call sites a later step reshapes.
Every step lands whole, proven by the gate and, where behavior moves, one
`kcode run` probe against its own socket.

### 0. One way work gets done

- [ ] **0.1. A run can hand work to the session that is asking.** Two lines
  forbid it today: `filter_swarm_agent_candidates` skips `req_session_id`, and
  `is_drivable_auto_worker` accepts only headless workers or ones the requester
  owns. Delivery is already generic, since a member is driven by a message that
  wakes it. Prove with one test that a one-member run drives its own session to a
  node.
- [ ] **0.2. Delete the client poke.** With dispatch covering it, the TUI's
  auto-poke machine goes: `auto_poke_incomplete_todos`,
  `last_auto_poke_fingerprint`, `total_pokes_sent`, `morning_report_poked`,
  `final_wrap_poked`, `overnight_auto_poke`, its keybinding toggle, its overlay
  line, and its tests. The toggle defaults off today, so nothing is lost.
- [ ] **0.3. Rows are the run's seed source** (this is C1). `kind` rides on the
  row, the node id is the row id, `blocked_by` becomes `depends_on`, position is
  priority, and gates get engine names. Testable against a scratch repo, so it
  needs no migration.
- [ ] **(decide)** what a running run shows in the file while a node is in
  flight, and what a stalled node shows. The row's `note` is the durable half of
  `SwarmTaskProgress.last_detail`; one of the two has to win.
- [ ] **(decide)** where a run's finished-but-not-integrated nodes live. The file
  deletes a row when its result is durable, but a gate must name every done node
  in its scope by id, and a run that resumes after a restart needs them too.
  Either the live graph is the gate's input and the persisted plan carries those
  nodes until the run closes, which is two durable stores while a run runs, or
  the gate reads something else.
- [ ] **0.4. The cuts the run makes redundant** (this is C2, plus the mode
  residue): the wire node spec and its kind parse and default, the `coordinators`
  map, any stored swarm id, the 31 `SwarmState { .. }` rebuild sites, and the
  `Synthesize` kind that no production code can reach. The deep/light flag comes
  too, once we know what still keys on it: `Mode::is_deep` has no production
  caller and gates are inserted unconditionally, while `internals/swarm.md`
  describes deep as a preset with real differences, so the flag is either residue
  or the doc is stale. The `session_effort` side-table, whose only stated job is
  to default that flag, goes with it.

### B. The file is the list

Last of the file work, whenever we want it.

- [ ] **B2.** This repo's own list migrates: `docs/todo.md` becomes the first
  content of `tasks.jsonl`, its standing decisions move to
  `docs/what-was-removed.md`, and the twelve references follow.
- [ ] **B3.** Drop `group` and `status` from the type, once the file's `parent`
  field and the close action exist: `parent` is what `group` was grouping by, and
  the close action is what makes a completed row unrepresentable rather than
  stored.

### C. The list reaches the client

- [ ] **C3.** The two sub-items in `todo.md` §1: member appearance follows the
  typed status, and a stalled plan node is visible. Gate this on 0.3.
- [ ] **C4.** Move `subsystem` and `file_scope` off the shared type into the
  plan's own per-item state. They are the scheduler's inputs (assignment
  affinity matches them against a worker's metadata), not list fields, and
  `SwarmPlanItemSpec` already carries them on the plan side.
- [ ] **C5.** The client renders the list from server events instead of reading
  the file itself. Today it resolves the repo from its own working directory,
  which is the same thing for a local session and the wrong repo for a remote
  attach.

### D. Delete what the file makes redundant

- [ ] **D1.** The topic channels and the shared-context key-value store, the
  removal `internals/swarm.md` recorded as pending and never did, plus the member
  projection `AgentInfo`, `SwarmMemberStatus`, `MemberStatic` and `SwarmMember`
  write four times by hand.

Gated by C.

### E. The server shape

- [ ] **E1.** `ClientContext` for `handle_client`'s 28 arguments.
- [ ] **E2.** Fold swarm ownership in, which is gated on 0.3.
- [ ] **E3.** Name the prologue, move the largest arms out, and settle the
  turn-lifecycle locals. `todo.md` §2's H1 to H5 in order.

### F. `tool/communicate.rs`

- [ ] **F1.** Split the 3369 lines along the seams it already has: swarm
  coordination, the run-plan driver, capacity cleanup, and the formatters around
  `execute`. Cheaper once 0.3 has given the state one owner.

### G. Close out

- [ ] **G1.** Restore the two size ratchets in `scripts/check_guardrails.sh` and
  re-baseline both with `--update`. They are paused, with the reason at the call
  site.
