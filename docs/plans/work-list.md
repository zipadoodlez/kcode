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
row field, and the default is none. The scope stops the run, but only because a row
worked once in a run is not picked again there: a row left open and unblocked ends
that run instead of spinning on it.

Continuing on its own is a permission, not a consequence of holding rows. Holding a
row says who owes the work, not that the run may take another turn. The default is
off, so a session stops at the end of its turn unless it was allowed to keep going,
and the permission is read in the one place that decides the next turn. The user's
words can grant it for one run, which is what "work the list until it is done"
means. A headless member holds the permission inherently, because that is what it
was spawned for.

A swarm member has no human, so the run supplies every turn. A session with a human
never has turns taken from it: dispatch offers it rows at a turn boundary, and the
session claims one when it picks it up, so an offered row is never stranded. That
is one mechanism with two triggers, a schedule and a person, not two mechanisms.

Having a human is derived from live state that already exists, a client attached to
the session, and it is read in exactly one place, the loop's decision about who
supplies the next turn. Stored nowhere, and read anywhere else, it becomes the mode
boundary this model deleted.

Controlling a session you are not sitting in is attaching to it, and the server
already does the flip: a client attaching sets that member's `is_headless` to false,
so the run stops supplying turns and starts offering them, and detaching puts it
back. Without a client the levers are the run's: message it, stop it, wake it,
retry it, reassign or replace its work, and watch it through its status events.

Handing a row over is only a message. The run picks a row and a session, delivers a
message naming the row and asking for the work, and the row's own words plus the
kind are the whole payload. If nobody is attached to that session the server starts
a turn for it there and then, which is what a member always was; if a person is
attached, the message waits for a turn boundary and is only ever sent while that
session holds rows that are ready. Nothing is queued, nothing is transferred but
words, and the message is the entire mechanism. A row whose note cannot stand alone
is a row that cannot be handed over.

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
   is the default order. A row reads as claimed from `assigned_to`, and as ready only
   against the file, since a blocker that closes is deleted and its id is left
   behind in `blocked_by`.
8. **A row with no `kind` is not seedable.** Typing it once records it, because the
   kind decides the artifact and the gate and may not be guessed.
9. **The check is a rule, not a field.** The close requires a nonempty result and the
   tool description names the check; a skipped check shows only in the commit.
10. **Refer to a task by its words with the user.** The file keys on `id`; the
    conversation does not.
11. **Continuing unattended is a permission, default off.** Holding a row says who
    owes it, not that a session may take another turn. A turn can grant it for one
    run, a headless member holds it inherently, and it is read where the next turn is
    decided. Typing always wins, so the stop lever for a session with a human is the
    user's next turn.

## Steps

Widest shared shape first, so no step sweeps call sites a later step reshapes.
Every step lands whole, proven by the gate and, where behavior moves, one
`kcode run` probe against its own socket.

### 0. One way work gets done

- [ ] **0.1. A session that holds ready rows keeps going without a client nudge.**
  The blocker I first named here was wrong. The auto-pick filter does not stop
  this, because an explicit `target_session` bypasses it, and `run_plan`'s loop
  already drives members, but only ones it owns, and it stalls with "assigned to a
  session run_plan cannot drive" on the rest. What is actually missing is the
  continuation: when a session is idle and attached and holds a row whose blockers
  are all gone, the server starts one tracked turn naming that row, and takes
  another when that turn ends, until it holds nothing ready. Home: beside
  `idle_live_agent` and `spawn_tracked_live_turn` in `live_turn.rs`, which already
  provide the idle guard and the externally started turn. Trigger: the
  turn-terminal path, so the loop is the session's own turns. Prove: a session
  holding a ready row is continued, one holding a blocked row is not, one holding
  nothing is not, and a row it already worked in this run is not picked again.
  Landed: the loop and its pick are in, with the run's own bound in place: a row
  worked once in the run is not picked again, so a row left open ends the run
  instead of spinning it (`TurnSeed` carries the row id into the loop's `worked`
  set, which the pick skips). The loop end to end is proven in `server/tests.rs`:
  a session holding two ready rows takes three turns and then stops, a row whose
  blocker is still open is left alone, and nothing ready takes one turn. It is off
  unless `features.auto_poke` is on and only for attended sessions, because a member
  is driven by its plan until rows seed a run. Owed, read against rule 11: the
  config flag is a stand-in for the permission, and `may_continue_on_its_own` fuses
  the permission with having a human and is read at two sites (`live_turn.rs:215`,
  `:347`) where the model names one. The user's words cannot yet grant it for one
  run, a headless member is excluded where rule 11 says it holds the permission
  inherently, and
  "typing always wins" cannot hold while the loop holds the agent's
  `OwnedMutexGuard` across every turn (`live_turn.rs:244-352`), so a user's message
  waits for the whole run instead of stopping it.
- [ ] **0.2. Delete the client poke**, which this replaces. With the server
  continuing a session that holds ready rows, the TUI's auto-poke machine goes:
  `auto_poke_incomplete_todos`, `last_auto_poke_fingerprint`, `total_pokes_sent`,
  `morning_report_poked`, `final_wrap_poked`, `overnight_auto_poke`, its keybinding
  toggle, its overlay line, its tests, and `build_auto_poke_message` in the store
  crate. The toggle defaults off today, so nothing is lost. Two cuts in that list
  are not free: `overnight_auto_poke` is the overnight run's own continuation, which
  `/overnight` starts, so the cut has to say what drives an overnight run once the
  server loop is the only continuation; and `build_auto_poke_message` is still
  called by the command-line paths this step defers
  (`src/cli/commands.rs:719`), so it goes with them, not here. The command-line
  variant (`src/cli/commands.rs`, the `_with_auto_poke` run paths and
  `run_command_auto_poke_max_turns`) waits for 0.3, because a plan-driven member
  must not be driven twice, and a headless run has no plan until rows seed one.
- [ ] **0.3. Rows are the run's seed source.** `kind` rides on the row, the node id
  is the row id, the file's `blocked_by` is the node's dependency edge (rules 6 and
  7 name it; there is no rename), position is priority, and gates get engine names.
  Testable against a scratch repo, so it needs no migration.
- [ ] **(decide)** what a running run shows in the file while a node is in
  flight, and what a stalled node shows. The row's `note` is the durable half of
  `SwarmTaskProgress.last_detail`; one of the two has to win.
- [ ] **(decide)** where a run's finished-but-not-integrated nodes live. The file
  deletes a row when its result is durable, but a gate must name every done node
  in its scope by id, and a run that resumes after a restart needs them too.
  Either the live graph is the gate's input and the persisted plan carries those
  nodes until the run closes, which is two durable stores while a run runs, or
  the gate reads something else.
- [ ] **(decide)** the `kind` vocabulary and its engine mapping. A row's `kind`
  decides its artifact and its gate (rule 8), and `TaskItem` has no `kind` field
  today, so 0.3 cannot land without it: the values, whether they are `kcode-plan`'s
  `NodeKind` by name as "gates get engine names" suggests, and what a gate's row
  says when gates are the one node with no row.
- [ ] **(decide)** whether a session with a human is *given* a ready row as a
  server-started turn, which is 0.1's landed shape and shows up as a synthetic user
  message, or *offered* it at a turn boundary to claim when it picks it up, which is
  what the model says. They are not one mechanism, and only the offer answers
  "typing always wins".
- [ ] **(decide)** how a row names a person. `assigned_to` holds a session id and
  the tool defaults it to the writer's session, so no reader can tell a live holder
  from a human the run must yield on, which is the model's "decision".
- [ ] **0.4. The cuts the run makes redundant**: the wire node spec and its kind
  parse and default, the `coordinators` map, any stored swarm id, the 31
  `SwarmState { .. }` rebuild sites, and the `Synthesize` kind, reachable only
  through the wire kind parse this step deletes. The deep/light flag is not residue,
  as `internals/swarm.md` says: there is no `Mode::is_deep`, and `requires_gates`
  guards gate insertion (`kcode-plan/src/dag/ops.rs:67`, `:308`), gate-pass
  validation (`:401`) and artifact validation (`:709`), while
  `parse_mode(..) == Mode::Deep` gates deep-participant graph driving
  (`server/comm_control.rs:2625`). It goes only once the row model owns gates and
  the recursion rule, which is `session_effort`'s second job:
  `server/comm_session.rs:1332` reads the root's effort to allow recursive
  spawning, so that rule needs a home before the side-table goes.

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
