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
  repeated one of them. 0.4d is the third: `parent` looked like a duplicate of the
  row, but `row.parent` is the run's anchor hierarchy while `node_meta.parent` is the
  DAG decomposition, so the check belongs before the cut, and it caught the error.
- **A deletion and a behavior change are checked differently.** The build alone
  proves a deletion; a behavior change needs the build plus the tests that exercise
  it. So the build runs at the behavior boundary, not after every stage: go blind
  across deletion stages, and stop for tests where meaning moves (the liveness sweep,
  the busy/load change).
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
the run. **There is no gate.** A critique or verify pass was engine machinery for
making a swarm rigorous, and this model gets its rigor from the rows, the loop that
works them, and the record each close leaves: what a gate would have found is a row
someone typed or the run added under the row it came from.

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

These are the model the steps drive the tree to, not the tree. Where a rule and
the type disagree today, the type is behind: `TaskItem` still carries `status`,
`priority`, `group`, `subsystem` and `file_scope`, and B3 and C4 remove them.

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
   kind decides what the row's result has to be and may not be guessed.
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
starts. 0.1, 0.2 and 0.3 have landed whole and are gone from this list; what 0.2 owed
was 0.3's last stage, and the two losses 0.1 and 0.3 named are in `docs/todo.md`. 0.4's
gate stage (0.4a), row stage (0.4c), one-mode stage (0.4b), node-meta stage (0.4d),
liveness stage (0.4e) and 0.4f have landed: the plan's items are the file's open rows,
the holder and the artifact live where the list is, and a turn end is a close. **0.4g
is next**, and the one build + full test pass for 0.4f ran with it (three pre-existing
`session_flow` e2e failures, recorded in `docs/todo.md`).

### 0. One way work gets done

- [ ] **0.4. The cuts the run makes redundant.** Decided with this step (2026-10-01):
  **gates go, and the deep/light axis goes with them.** A gate is the one node with no
  row, and the row model has no room for a node that is not a row, so the pass a gate
  performed is either a row someone typed (a `critique` row whose close needs its
  record) or nothing. The loss is named in `docs/todo.md`. Stages, in this order, one
  commit each:
  - **0.4d. `origin` and the growth report are gone.** The persisted origin,
    `GrowthStats`/`growth_stats`, the `seeded_count`/`grown_count` snapshot fields and
    their renders, and the engine's `NodeOrigin`/`TaskNode.origin` are deleted. The
    `parent`/`expanded` cut is not a separate step: `row.parent` is the file's one
    hierarchy (the anchor adopts root rows and decomposition adds children), so the
    plan reading it is exactly 0.4f. See that stage.
  - **0.4e. Liveness is the member's.** Landed: the staleness sweep reads the member's
    activity clock (the turn loop's `session_metrics::record_activity`), and the
    per-task `heartbeat_count`/`checkpoint_count` are gone. The rest of the review is
    not separate work: removing `task_progress`, the heartbeat task and
    `touch_swarm_task_progress`, `assignment_loads`, `planner`, and `running_stale` is
    the same store-boundary cut as 0.4f, so it lives there. Liveness belongs to the
    member, not the task; fan-out is the plan's ready set (`blocked_by`), not a
    decision; busy is the member's in-flight work; and who integrates a decomposition is
    the row's holder (`assigned_to`).
  - **0.4f. `VersionedPlan` becomes a view of the file.** The durable snapshot goes,
    but not here: see s12 below, which measured it. It reads `parent` and
    composite from the rows: the file's `parent` is the one hierarchy (`anchor_from_rows`
    adopts root rows, `expand_row_on_disk` adds children), so `node_meta.parent` is
    deleted, and a node is composite when it has an open child or a nonempty `records`
    (a close leaves its record on its parent, so "was decomposed" survives the children's
    removal). `node_meta` keeps only the typed artifact, and the seed guard becomes a
    field comparison since a held root row legitimately has a parent. The plan-approval
    path (`propose_plan`/`approve_plan`/`reject_plan`, `resync_plan`) is a second writer
    of the same rows and a consumer of the shared context; decide it here.
    It also carries what 0.4e deferred, because it is the same store-boundary cut:
    `task_progress` shrinks to the reclaim counter: the assignment time fields go,
    because a claim is assumed to be worked while its holder lives, so the guard reads
    the claim and not a clock; the heartbeat task, `touch_swarm_task_progress` and the
    heartbeat staleness sweep go (liveness is the member's); `running_stale` goes: the
    sweep's flip, the load-time migration and the member-status variant go with it, and
    its readers fold it away (turn-end, the conflict check, the TUI's
    `normalize_plan_status_for_todo` and its running count: the node already renders
    `in_progress` with the same glyph and `warning_color()`, and the count is
    untouched), except `task_control`, whose handoff guard asks the member's clock
    whether the holder is still live — that check is what the sweep used to write down
    as `running_stale`, and it is the one reader that had to be derived rather than
    dropped; the failed reason reads the member's detail
    instead of `checkpoint_summary`; the writers and readers that timed an assignment
    (`requeue_existing_assignment`, the dispatch path, `active_assignment_conflict`)
    stop doing so, and the reclaim counter stays where it is until 0.4g decides the
    plan object and where runtime lives;
    `assignment_loads` stops being a per-task count and busy becomes the member's
    in-flight work; and `planner` goes once `expand_node` stops freeing the owner.
    Sequenced stages, one commit each, and the one build + test pass lands at the end of
    0.4f. Dropping fields from the persisted snapshot is safe: `SwarmTaskProgress` has
    no `deny_unknown_fields` (`swarm_persistence.rs:112-121`), so old files still load.
    - **s12. The plan's items become the file's open rows.** Read 2026-10-02, and two
      things in the paragraph above are wrong on this tree. `swarm_persistence.rs` is
      625 lines and its tests 926, and the tests are almost none of it: of 21 cases,
      3 touch the plan, and the rest cover member recovery, terminal retention, legacy
      migration, `.bak` rotation and per-swarm locking. That is the member and
      coordinator half, which is 0.4g's ownership cut, not this stage's. What the plan
      actually persists is three things: `version` (the client orders plan events by
      it), `participants` (no client reads it; the broadcast already falls back to the
      whole swarm, `swarm.rs:739`) and the reclaim counter. All three are 0.4g's
      `(decide)` about whether the plan object exists and where per-task runtime lives,
      so s12 empties the plan's half and leaves the file to 0.4g. Five moves, one
      commit each. The first two and the fourth are decided (2026-10-02), and together
      they are what makes the two lists one list.
      1. **An assignment is a file write.** Decided. The holder lives only in the plan
         today and the snapshot is what makes it durable, while the file's
         `assigned_to` is written only by the store's own paths
         (`anchor_from_words`, and the `todo` tool's `add`/`update`), so the list does
         not show who
         the plan handed work to, and "the file is the list" cannot be true while that
         holds. Expand and close already write the store first and let the plan follow;
         assign joins them. The paths that change a holder: dispatch
         (`handle_comm_assign_task`), `requeue_existing_assignment`,
         `reclaim_stranded_assignment` and its salvage sweep, and everything
         `task_control` routes through assign. One store function, one shape.
         Read, the write operation already exists: the `todo` tool's `update` sets
         `assigned_to` (`tool/todo.rs:103`) and its schema advertises the field
         (`:265`), and `kcode-base/src/todo.rs` already has the `*_on_disk` shape for
         the two mutations that rewrite the file (`close_row_on_disk`,
         `expand_row_on_disk`). So this move is one more function of that shape, in
         that home, plus the calls. No new concept, and the model can already hand a
         row to anyone through the tool.
      2. **The plan is refreshed from the file, not a peer store.** Landed. One write
         path, so one refresh: a row write goes through the store, and the plan's items
         follow the rows it wrote. Not a patch per site, which would leave two writers
         of one field, and not a re-read per site, which would be five parallel
         refreshes of one fact. The rule is field-level, not set-level: an item equals
         its row, an item whose row is gone is dropped, and a new item comes only from a
         seat (the seed) or from a row the store just added (an expansion's children),
         because the file is the repo's one list while a plan holds the rows its run
         seated (`seed_specs` leaves out a kindless row, a foreign row, and a row
         blocked outside the run). The one field the rows cannot give is `status`: a
         row's own status is what the store writes (`pending` for a fresh row), while an
         item's status is the run's lifecycle for that row, so it is carried by id.
         Where that lifecycle belongs, and whether it survives at all, is 0.4g's
         `(decide)` and B3; this move took the durable half.
         Three things fell out of it. The bridge's `apply_task_graph` stopped being a
         writer of items, so the closed-item ghost dies at its source and the bridge is
         a validation and op view over the rows (`to_task_graph`, kept). A run's own
         held rows became visible as rows, so the dispatch picker hands them over
         explicitly (`next_handover_runnable_item_id`: nobody holds it, or this run
         does) instead of relying on the seed dropping holders, and the double-assign
         guard stopped treating the requester's own claim as a conflict, because that
         claim is what a hand-over moves. And the debug `swarm:graph` op went with it:
         it existed to drive the engine's ops against plan-owned items, which is the
         shape this move removes, and nothing (no test, no doc, no script) referenced
         it.
      3. **The artifact's one home is the row's `record`, and the context a row
         carries is the work that closed under it.** Landed. The close writes
         `{"id","result","artifact"}` onto the owning row, and the plan kept a second
         copy in `node_meta`; `bridge`'s `upstream_context` was the only production
         reader. Reading those call sites settled a bigger point than the copy: the
         reader walked the row's `blocked_by` and read the *completed dependency's*
         artifact, and rule 7 clears that id when the dependency closes, so that path
         could only ever fire on a plan-built-by-hand state. A split row's synthesis
         turn therefore got no children's artifacts at all, which is the case the model
         exists for. So the context became the row's own `records`, which is the work
         that closed under it: a split row gets its children, a run's top row gets the
         run's results when it closes, and a plain leaf gets nothing. `NodeMeta` went
         whole: the type, the plan field, `prune_side_maps`'s arm, the snapshot field,
         the bridge's lift, the four-line write in the close handler, and the two debug
         dumps. The section heading loses the kind, because a closed row's kind is gone
         with it and the store keeps no engine vocabulary, so a record renders as
         `## <id>`.
      4. **A turn end is a close.** Landed. The turn-end path used to mark a plan item
         "done" without touching the file, which left the list showing work a run had
         finished and made the plan's items something other than the file's rows. Now a
         turn that ends without a close closes its own row through the store, exactly as
         `complete_node` does: the turn's report is the result and the artifact's
         findings (so the row that integrates it can read what it found), and when the
         turn reported nothing the result says so, because the alternative is a row the
         picker hands back forever. With this, no production path writes a terminal item
         status: the plan holds open rows with a runtime status of `pending`, `queued`,
         `running` or `failed`, and "completed" survives only as something a client
         normalizes. A turn that *errored* is not a close: the row stays, marked
         `failed`, because that is a state a person can still retry or reassign, and the
         salvage cap keeps its row for the same reason, so its plan-only "failed" stays
         (it is the one state a human can move, and 0.4g's runtime question owns where
         that status lives).
      5. **The engine stops carrying the artifact.** Landed. `TaskNode.output`,
         `dag::assemble_input`, `complete_node`'s artifact argument and the simulator's
         artifact plumbing all existed so the plan could keep its copy; with the record
         as the home they were a second, dead implementation of the context the bridge
         builds. The engine now keeps ownership, status and edges, and `HandoffArtifact`
         moved out of `dag` to `kcode_plan::artifact`, where its users are (the closer
         that writes it and the bridge that renders it), so the type no longer reads as
         the engine's.
      Then the single build + full test pass for 0.4f.
      What the decided moves change: a dispatch now writes the list file first and the
      plan follows it, so a failed write is a failed dispatch and nothing records an
      assignment the list does not have. The store goes first where the write is the
      fact that makes the operation true (assign, expand, reclaim); where the plan's
      decision is the fact and the file write is its mirror (close, salvage), the plan
      goes first and a failed mirror is logged. A reload then recovers holders from the
      file instead of the snapshot, and a hand edit of a holder becomes an input rather
      than a conflict.
      Losses to name: a dependent row no longer receives the artifact of the row it
      was blocked by, because the list keeps no edge to a closed row (rule 7) and
      carrying the artifact a second time to keep that path would be the duplicate this
      move removes. The gain is the join and the run end, which the code did not
      deliver before. Still owed by a later move: a turn that ends with no report needs
      its close rule.
  - **0.4g. The swarm state gets one owner**: the `coordinators` map, any stored swarm
    id (including the `KCODE_SWARM_ID` shared-swarm opt-in), the `features.swarm` flag
    and per-session toggle (stored membership), the `assign_role` action that writes the
    coordinator by hand, and the 31 `SwarmState { .. }` rebuild sites (`docs/todo.md`
    §1's condense). Membership becoming derived is a behavior change, so this stage
    carries its own build + test pass; the one named above lands at the end of 0.4f.
    - `(decide)` **Does the plan object exist, and where does per-task runtime live?**
      `VersionedPlan` is a per-swarm cache of the rows plus `version`, `participants`
      and the reclaim counter. The rows are the file and membership is the swarm;
      `participants` is a hand-kept subset that the broadcast already falls back to
      the whole swarm for (`swarm.rs:794`) and that no client reads. What the file
      cannot give is `version`, which the client uses to drop out-of-order plan events
      (`server_events.rs:2002`), and the counter. Answer this before the counter moves
      again, so it moves once.
    - `(decide)` **Is a claim a lock for a run and not for the user's session?** This
      is what makes `assign_task` a sufficient takeover, and it decides the verb set
      with it. The vision names four levers over a session you are not sitting in —
      "message it, stop it, wake it, retry it, reassign or replace its work" — so
      `wake`/`retry`/`reassign`/`replace` stay, and `start`/`resume`/`salvage` are the
      candidates: `start` and `resume` are `retry` restricted to an idle holder, and
      `salvage` is `reassign` plus the prior worker's tool-call summaries, where the
      vision says the payload is the row's own words and kind and carried context
      belongs in the row's `note`. Their home travels with them: this policy is all in
      `kcode-plan` (`TaskControlAction`, its status tables, the target picker), put
      there by `2345c002` "Move swarm task control policy into plan crate", while the
      crate named for swarms holds none of it and only `kcode-app-core` reads it, so the
      cut and the move are one pass. Answering it also settles the handoff guard, which
      today refuses a handover while the holder is live (read from the member's clock
      since 0.4f): the vision's answer is that the displaced holder finds out on its
      next write instead.
  `parse_kind`/`kind_str` stay, since they are what reads and writes a row's word.
  `Synthesize` stays as well: the word lives on the row (`tool/todo.rs` offers every
  `KINDS` entry), so a run's own join row has a word to be typed with. Nothing in the
  engine branches on it, so it costs one enum variant and one word.

- [ ] **0.5. One level, low, and no second way to say it.** Effort stops being a design
  variable: every turn runs at one level, the session's, whose default is low, and the
  only way to change it is the one a human has.
  - A session's level defaults to low (`tui/app/state_ui.rs:1947` reads `"default"`
    today, which is the provider's own choice), `/effort` and the model picker remain
    the user's lever, and the `swarm`/`swarm-deep` rungs went with 0.4b's axis.
  - A spawned session inherits its creator's level:
    `resolve_swarm_spawn_effort` (`server/comm_session.rs:558`) collapses to that, and
    the `effort` argument goes from `spawn`, `assign_task`, `assign_next`, `fill_slots`
    and `run_plan` (`tool/communicate.rs:1298`, `:1449`, `:2606`, `:2911`, `:2963`,
    with its schema text at `:1688`, `:1801`; `wire.rs:560`, `:682`) along with the
    `agents.swarm_effort` pin (`kcode-config-types/src/lib.rs:461`). F1 deletes those
    actions outright, so the argument text is touched twice if the tail lands; accepted,
    because 0.5 is kept and the tail is droppable.

### A. Audit

- [ ] **A1. A braid audit of the tree.** Run `/braid-audit` once over the repo and fold
  what it finds into this list: unscoped concepts, duplicate representations, modes and
  dead paths, ranked by surface removed. It runs after 0.4 and 0.5 so it audits the
  settled model rather than one in flight, and it feeds the tail (D, E, F) and the
  `todo.md` §4 and §5 items. A finding is a step or a deletion, never a standalone
  report.

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
  when `blocked_by` is empty and liveness comes from the member, not the item.

### C. The list reaches the client

- [ ] **C4.** Move `subsystem` and `file_scope` off the shared type onto the
  worker's own record. They are the scheduler's inputs (assignment affinity matches
  them against a worker's metadata), not list fields, and neither destination the
  old text named can hold them: 0.4 deletes both `SwarmPlanItemSpec` and the
  `node_meta` side-map.
- [ ] **C5.** The client renders the list from server events instead of reading
  the file itself. Today it resolves the repo from its own working directory,
  which is the same thing for a local session and the wrong repo for a remote
  attach.
  - `(decide)` **A session's todo view shows the rows it holds, and adopting a row
    is the session's decision, not automatic.** The view already claims the scope
    and does not do it: `todos_view.rs:373` is `load_current_session_todos`, the
    panel text says "dedicated to the current session's todo list" (`:269`) and
    the placeholder "Waiting for a session todo list" (`:524`), but the read is
    `load_tasks`, which returns the whole repo file. The engine is already scoped
    to the session's holdings (`bridge.rs:90` in `seed_specs`; `live_turn.rs:259`
    for ready work), so the view is the odd one out. Open: filter for every
    session, or only for runs, so a plain session still sees the whole list? And
    how does a session first *see* rows it does not hold, so that adopting one is
    a decision it can make? The model does not scope the user's session to its
    holdings, so the answer cannot be a new rule.

### D. Delete what the file makes redundant

- [ ] **D1.** The topic channels and the shared-context key-value store, the
  removal `internals/swarm.md` recorded as pending and never did, plus the member
  projection `AgentInfo`, `SwarmMemberStatus`, `MemberStatic` and `SwarmMember`
  write four times by hand. With the projection one type, member appearance follows
  the typed status: `kcode-tui-render` takes `SwarmLifecycleStatus` (a dependency on
  the data-only `kcode-session-types`), so the string matches in `swarm_gallery.rs`
  and `is_active_status` become enum matches; one module owns accent, glyph, label
  and sort rank, with an `is_working` predicate kept distinct from lifecycle
  `is_active` so a stalled node does not spin; the duplicate map
  `info_widget_swarm_background::swarm_status_style` goes; and the swarm-path
  `Color::Rgb` literals become `kcode_tui_style` role accessors so `/colors` can
  recolor them (the small default shift is accepted). The projection's `RunningStale`
  goes with it: 0.4f removes its only producer (a plan item's status), so the variant
  is unreachable.

Gated by C4 and C5.

### E. The server shape

- [ ] **E1.** `ClientContext` for `handle_client`'s 28 arguments.
- [ ] **E2.** Fold swarm ownership in, which is gated on 0.4g: `todo.md` §1's
  condense must land before this split, since the request context is designed to hold
  that state.
- [ ] **E3.** Name the prologue, move the largest arms out, and settle the
  turn-lifecycle locals. `todo.md` §2's H1 to H5 in order.

### F. The swarm tool surface

- [ ] **F1. The tool keeps the model's verbs; the app takes the rest.** After 0.4 a node
  is a row, so the model's verbs are row verbs, which the `todo` tool already serves.
  The orchestration actions (`spawn`, `assign_task`/`assign_next`/`fill_slots`,
  `run_plan`, `cleanup`, `await_members`) become the run's own behavior, not calls. The
  exact cut is decided at the step; likely one task tool.
- [ ] **F2.** Then split what remains of `tool/communicate.rs` along its seams (swarm
  coordination, the run-plan driver, capacity cleanup, the formatters around `execute`),
  if anything does. Cheaper once 0.3 has given the state one owner.

### G. Close out

- [ ] **G1.** Restore the two size ratchets in `scripts/check_guardrails.sh` and
  re-baseline both with `--update`. They are paused, with the reason at the call
  site.
- [ ] **G2.** The one live `kcode run` probe against its own socket, in a scratch
  repo, run by the user: the permission (a granted turn continues, a wake does not)
  and the poke's removal (no client continuation; every non-retryable error gets the
  short budget). It is the step's landing proof, so it runs when the list is done,
  not per step.
