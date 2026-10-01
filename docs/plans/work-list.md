# Work list

Work on branch `work-list`. `docs/todo.md` points here. Tick a step when it lands
and delete it; git has the history.

## How a step lands

This plan is a deletion program. A step removes a representation, a duplicated
shape, or a mode, and a step that only adds surface is reconsidered before it is
written. Four rules, each one paid for by a failure this plan already had:

- **Every cut names its code and the evidence.** A step quotes the file and count it
  measured (`31 SwarmState { .. }` sites, `app_fields=182`, the `SwarmMember`
  literal count), and the claim is grepped again before the step is written. 0.4
  shipped two claims that were false on this tree, and `internals/swarm.md` had
  repeated one of them.
- **A step lands whole; a big one lands in ordered stages.** A stage is one commit,
  and a symbol goes only once its last caller is gone, so each stage builds on its
  own and the order is the caller graph, outside in. Each stage compiles before it is
  committed; the full gate runs once, when the step is done. The doc line rides the
  code, and prose does not outrun landings: a plan that grows faster than the tree
  shrinks is not this plan.
- **A decision on a step's path is taken with that step.** A `(decide)` item is
  answered before the step starts, or the step is marked blocked on it.
- **A loss is named, never silent.** Capability is fixed and only surface goes. A
  step that drops power records it in `docs/todo.md` with why, the way the deleted
  overnight preflight did.

Zonytail's measure is the codebase after the step, not the diff: fields, `impl`
blocks, wildcard re-exports and lines per file, ratcheted by
`scripts/check_guardrails.sh`. A step that must grow records the reason in the same
commit, because the App shape ratchet only tightens.

**The go/no-go is where a run's finished-but-not-integrated nodes live.** It reads
one store: the close's record goes onto the owning row, so nothing durable needs the
plan and the tail stands. Reversing that answer, so a run in flight needs the plan
durable as well as the file, makes the model's central promise false: stop at 0.3,
keep what landed, and drop the tail rather than adapt it.

## The model

One file holds the open work: rows of `id`, `content`, optional `kind`, `parent`,
`blocked_by`, `assigned_to`, `note`, and `records` (the opaque closes of the rows
worked under this one). A row is free or held, and there is nothing else about it.

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
was spawned for. A long unattended run is therefore not a kind of run: it is this
permission held while a long list drains, and there is nothing else to build for it.
(The overnight subsystem, which was a second run with its own crate, supervisor,
durable store, UI and preflight, was deleted on that ground on 2026-10-01; its
preflight is now a decision in `docs/todo.md`.)

A swarm member has no human, so the run supplies every turn. A session with a human
never has turns taken from it: a turn that wants the agent owns it the moment the run
gives it up, and the run gives it up before it looks for the next row, so a person
typing between turns takes it and the run ends there. A row is therefore never
*offered*, because an offer is state the file cannot hold: the row is already
assigned, the loop picks it, and the only thing an offer would add is a second
in-memory copy of the list. Stored nowhere, read in the one place the next turn is
decided, which is the loop.

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

**A run's scope has one row at its top, the anchor.** It is the row `/auto t3` names;
for a run scoped to the whole list, the run's first row is the anchor, and every
root-level row of that run names the anchor as its `parent`. A closed row's record
goes onto its parent, so the anchor is where a run's root-level records accumulate
and where a resumed run finds its scope. The anchor closes last, since rule 3 keeps
it while a child names it, and its close is the run's end: the loop picks the anchor
like any other ready row once nothing else is left, so the summary is a turn and a run
needs no separate end-of-run summary. A row other rows belong to is not ready until
they are closed, which is what makes that "once nothing else is left" true without a
second way to say the same thing.

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
   close names that place and writes the record onto the row that owns the work: the
   parent for a node that has one. A drop is a completion whose result says so, so
   `cancelled` is not a state.
5. **One item type**, in `kcode-task-types`, the crate whose name is the concept.
6. **Hierarchy is `parent`, blocking is `blocked_by`,** two fields because they are
   two facts. `group` is deleted, since the parent chain is the grouping.
7. **The file stores those fields and nothing else:** no `status`, `priority`,
   `group`, `subsystem` or `file_scope`, and line order promises nothing. Position
   is the default order. A row reads as claimed from `assigned_to`, and as ready only
   against the file: a close deletes the row and removes its id from every
   dependent's `blocked_by`, so an entry there always names an open row and a row
   with none is ready.
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

Every step lands whole, proven by the gate. The one `kcode run` probe against its
own socket waits for the end of the list, where the user runs it, so no step is
gated on it. Widest shared shape first, so no step sweeps
call sites a later step reshapes: 0.2 removed the poke the permission replaced, and
0.3 makes the file the seed so 0.4 can delete the plan's durable half.

After that, in the order of the moves: delete (D, the topic channels, the shared
context, and the member projection written four times), then the re-cores (E, F,
and the `app.rs` re-core, whose detail `docs/todo.md` §2 owns). The re-cores are the
tail and they are droppable: the least evidence and the most churn, nothing upstream
depends on them, and finishing the model does not need them. Dropping them costs the
duplication they would have removed and nothing else.

**Here is where the work stands**, and a step in flight is finished before a new one
starts. 0.1 has landed whole and is gone from this list: the loop, the permission and
the row rung, each with its tests, and its two riders were cut with the loss named in
`docs/todo.md`. 0.2 has landed whole too; what it owed is its own step below, after
0.3. Next is 0.3, staged below and in flight: its stages land in order, so a resume
starts at the first stage whose work is not in git. Each stage compiles before it is
committed (`dev/post-change.md` says which check); the full gate runs once, when the
step is done.

### 0. One way work gets done

- [ ] **0.3. Rows are the run's seed source.** The `todo` tool's file is what seeds a
  run: the node id is the row id, `kind` rides on the row, the file's `blocked_by` is
  the node's dependency edge (rules 6 and 7 name it; there is no rename), position is
  priority, and gates get engine names. A run whose grant never typed an anchor (a
  headless member holds the permission inherently) gets one from the seed, written
  through the tool's own write path, so rule 2 still has one writer. Testable against a
  scratch repo, so it needs no migration. Stages, in this order, one commit each and
  each compiling alone: the order is the caller graph, writer before reader, delete
  last.
  - **S1. The row carries `kind`.** `TaskItem` gains `kind: Option<String>`
    (`kcode-task-types/src/lib.rs:203`), and the `todo` tool is its writer: `TodoInput`
    takes it on add and update, the schema names the six engine words, and the list
    output shows it. Nothing reads it yet, so `NodeMeta.kind` stays authoritative and
    the engine does not move.
  - **S2. The row is the source of the engine's kind, and the side-table is kept in
    step for one stage.** `bridge::to_task_graph` (`kcode-plan/src/bridge.rs:94`) lifts
    a node's kind from its row, falling back to `NodeMeta.kind` for a plan persisted
    before rows carried one, and `apply_task_graph` (`:124`) writes the row's kind
    while still writing the side-table's copy, so a plan in flight across this change
    loses nothing. `upstream_context` (`:191`) labels a dependency's artifact from its
    row. `parse_kind` (`:30`) stops guessing: absent or unrecognised is no kind, and
    the two boundaries that must still answer absence answer it where they own it, the
    wire's optional `kind` (`comm_graph.rs:29`, `debug_swarm_write.rs:573`, the
    documented `explore` default) and the lift (a kindless legacy node can only render
    as `Explore`). The row's writer refuses a word the engine cannot read
    (`tool/todo.rs`), because a typo would otherwise be a row no run could seat (rule
    8).
  - **S3. `NodeMeta.kind` goes**, with its write in `apply_task_graph`
    (`bridge.rs:161`), the persistence round-trip (`swarm_persistence.rs:113`, `:281`,
    `:304`) and the fixtures that build it (`bridge.rs` tests,
    `swarm_persistence_tests.rs:470`), since its last reader is gone.
    `parse_kind`/`kind_str`/`KINDS` stay: they are the vocabulary that reads and writes
    a row's word.
  - **S4a. The seed is the rows the run holds.** The seed path
    (`handle_comm_seed_graph`, `comm_graph.rs:221`) reads the seeder's list instead of a
    list the caller types: `bridge::seed_specs` lifts the rows a session holds into
    nodes, the row's id is the node's id, and a row already in the plan is not seeded
    again. The wire's `nodes` payload goes, and with it the id-collision remap
    machinery the model's invented ids needed, since a row's id is unique by
    construction. `expand_node` and `inject_gap` still take node specs; 0.4 deletes the
    spec once the row model owns a decomposition too.
  - **S4b. A run that typed no scope makes its own anchor.** `todo::anchor_from_rows`:
    the first row the session holds that belongs to nothing is the run's top row, and
    the other rows it holds that belong to nothing are changed to belong to it. A row
    that already belongs to something keeps its parent, so a run never adopts another
    run's structure, and a call with nothing to change writes nothing, so it is safe
    to repeat. The run calls it where a run starts (`live_turn::continue_with_next_row`,
    when the grant has no scope) and the seed calls it before it reads the rows, so the
    plan holds the anchor as one of its nodes. A row other rows belong to waits for them
    (`next_held_ready_row`), so the anchor is the last row the run closes and its close
    is the run's end-of-run result. The words case is the other half: the caller passes
    the word `synthesize` to `todo::anchor_from_words`, because a fresh anchor is a row
    whose result is its children integrated and a row with no word is not seedable (rule
    8), while the store still learns no engine vocabulary (rule 5).
  - **S5. A close keeps its record on the row that owns the work.** `TaskItem` gains
    one opaque field, `records`: the closes of the work done under this row. The close
    writes a node's record onto its parent's row, and it takes the machine-readable
    parts the gate validates (findings, `what_i_did_not_check` and the confidence)
    alongside the result, because the gate reads them from the row. One write path, two
    callers: the `todo` tool's close and the engine's `complete_node` (`comm_graph.rs:409`).
  - **S6. The command-line poke goes.** `src/cli/commands.rs`: the `_with_auto_poke`
    run paths (`:737`, `:773`) and the ndjson path that pokes too (`:817`),
    `run_command_auto_poke_max_turns` (`:662`), `next_headless_poke` (`:704`),
    `incomplete_poke_todos` (`:689`), and `build_auto_poke_message` (its last producer,
    `kcode-base/src/todo.rs:54`); plus the `features.auto_poke` rename
    (`kcode-config-types/src/lib.rs:930`). It waits for S4: a plan-driven member must
    not be driven twice, and a headless run has no plan until rows seed one. The two
    poke-named survivors (`is_non_retryable_auto_poke_error`,
    `is_auto_poke_connectivity_error`, in `commands_auto_poke_errors.rs`) are really
    turn-error classifiers and rename with it.
  - Not stages, because no code moves with them: `assigned_to` stays an opaque holder
    string (a holder naming no live session is never picked, so a person-held row is
    inert by construction, and a claim whose session died is what `run_plan` already
    reports as a stall), and no per-node progress is stored (the one place a run leaves
    words is the row's `note`, and a stalled node is derived from the member activity
    clock where its marker renders).
- [ ] **0.4. The cuts the run makes redundant**: the wire node spec, the plan's
  durable per-node state (`node_meta`, 85 sites in 19 files, and `task_progress`),
  the persisted plan itself (`swarm_persistence.rs`, 650 lines plus 1,218 of tests;
  `VersionedPlan` becomes an in-memory view built from the file), the `coordinators`
  map, any stored swarm id, and the 31 `SwarmState { .. }` rebuild sites. One
  thing blocks the `node_meta` half: `is_gate` lives there and a gate is the one node
  with no row, so the step first settles whether a gate is re-derived per run from
  `requires_gates`, with nothing durable, or carried in the file. `parent` duplicates
  the row already, and `expanded`, `planner` and `origin` are run state.
  `parse_kind`/`kind_str` stay, since they are what reads and writes a row's word.
  `Synthesize` stays as well: it counted as residue while the wire node spec was its
  only producer, and the word lives on the row now (`tool/todo.rs` offers every
  `KINDS` entry), so a run's own join row has a word to be typed with. Nothing in the
  engine branches on it, so it costs one enum variant and one word.
  The deep/light flag is not residue,
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
- [ ] **B3.** Drop `group`, `status` and `priority` from the type. The gate has
  already landed: `parent` is what `group` was grouping by, the close action is what
  makes a completed row unrepresentable rather than stored, and 0.3's "position is
  priority" is what `priority` becomes. Every `add` still writes `status` and
  `priority` today. Dropping `status` is the larger half, because the plan
  classifies by it everywhere: `summarize_plan_graph`
  (`kcode-plan/src/lib.rs:508-560`), `completed_item_ids`, `is_active_status`
  (`:267`), `newly_ready_item_ids` (`:813`, read by the swarm path at
  `server/swarm.rs:830`), the task-control actions (`:358`, `:672`), and
  `status_from_plan`/`status_to_plan` (`bridge.rs:74`). Afterwards a row is ready
  when `blocked_by` is empty and liveness comes from the member, not the item. The
  stored `running_stale` (`server/swarm.rs:556`) goes with it, since the stall is
  derived.

### C. The list reaches the client

- [ ] **C3.** The two sub-items in `todo.md` §1: member appearance follows the
  typed status, and a stalled plan node is visible. Fold into 0.4: that step removes
  the plan statuses these render, and decision 1 made the stall derived, so the
  rendering change is measured against the tree after it, not before.
- [ ] **C4.** Move `subsystem` and `file_scope` off the shared type onto the
  worker's own record. They are the scheduler's inputs (assignment affinity matches
  them against a worker's metadata), not list fields, and neither destination the
  old text named can hold them: 0.4 deletes both `SwarmPlanItemSpec` and the
  `node_meta` side-map.
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
- [ ] **G2.** The one live `kcode run` probe against its own socket, in a scratch
  repo, run by the user: the permission (a granted turn continues, a wake does not)
  and the poke's removal (no client continuation; every non-retryable error gets the
  short budget). It is the step's landing proof, so it runs when the list is done,
  not per step.
