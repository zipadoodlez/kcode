# Work list

Work on branch `work-list`. `docs/todo.md` points here. Tick a step when it lands
and delete it; git has the history.

## The model

One file holds the open work: rows of `id`, `content`, optional `kind`, `parent`,
`blocked_by`, `assigned_to`, `note`. A row is free or held, and there is nothing
else about it.

A **run** is a session working the rows it holds. A swarm is a count, not a mode:
one member and eight are the same code. Membership, the coordinator, liveness, and
the ready set are derived, never stored.

Rows become nodes once, when a run takes them. Three things cross back, and only
these: a claim, a note, a close. The close carries the record, so the durable
target and the machine-readable parts travel together, and nothing durable lives in
the run. Gates are the one node with no row, and when a gate finds work, that work
becomes a row.

**What a run may do on its own initiative stops at what it holds. What the user's
session may do does not.** The session the user is talking to changes any row,
including taking one that another session holds, and that session finds out on its
next write. A claim is a convenience, never a lock against the user.

A run continues while a row in its scope is ready, and ends when none is. It yields
when only a person can unblock it, and a row assigned to a person with rows blocked
by it is what we call a decision. A run with no instruction proposes a scope, names
the rows, and stops at that boundary. A bound on a run is a run property, never a
row field, and the default is none because the scope is what stops it.

A swarm member has no human, so the run supplies every turn. A session with a human
never has turns taken from it: dispatch offers it rows at a turn boundary, and the
session claims one when it picks it up, so an offered row is never stranded. That
is one mechanism with two triggers, a schedule and a person, not two mechanisms.

Having a human is derived from live state that already exists, a client attached to
the session, and it is read in exactly one place, the loop's decision about who
supplies the next turn. Stored nowhere, and read anywhere else, it becomes the mode
boundary this model deleted.

A run adjusts by editing rows, since it has no plan of its own to mutate. It adds
work under the row it came from, leaves notes, clears a blocker that no longer
blocks, retypes a row whose kind was wrong, and closes what it holds. Picking is a
claim and not a move, so there is no reorder act. A close always states its
outcome, so a drop is "dropped because X" and a split is "split into a, b, c" with
the children added underneath.

## Rules

1. **One file per repo**, `tasks.jsonl` at the root, found from git so a session in
   `crates/foo` reads the same list; outside a repo, a session scratch location.
2. **The tool is the only writer.** It owns the write protocol and re-reads the file
   before each write, so a human edit is an input, not a conflict. A claim rides the
   work's commit.
3. **One close action, with a nonempty result.** A parent's result is its children
   integrated, a leaf's is its own work, and a parent's row cannot go while a child
   names it.
4. **There is no completed state.** A row goes when its result is durable, and the
   close names that place. A drop is a completion whose result says so, so
   `cancelled` is not a state.
5. **One item type**, in `kcode-task-types`, the crate whose name is the concept.
6. **Hierarchy is `parent`, blocking is `blocked_by`,** two fields because they are
   two facts. `group` is deleted, since the parent chain is the grouping.
7. **The file stores those fields and nothing else:** no `status`, `priority`,
   `group`, `subsystem` or `file_scope`, and line order promises nothing. Position
   is the default order, and a row reads as open, blocked, or claimed from its own
   fields.
8. **A row with no `kind` is not seedable.** Typing it once records it, because the
   kind decides the artifact and the gate and may not be guessed.
9. **The check is a rule, not a field.** The close requires a nonempty result and the
   tool description names the check; a skipped check shows only in the commit.
10. **Refer to a task by its words with the user.** The file keys on `id`; the
    conversation does not.

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
