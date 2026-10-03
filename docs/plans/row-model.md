# Row model

Tick a step when it lands and delete it; git has the history. The open items this plan
leaves behind are split by subject: the app's shape in `plans/app-shape.md`, the
request path in `plans/server-shape.md`, the test tree in `plans/test-tree.md`, and the
loose ends in `plans/hygiene.md`.

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
  step that drops power records it in `docs/what-was-removed.md` with why, the way the
  deleted overnight preflight did.

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
preflight is recorded in `docs/what-was-removed.md`.)

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
and the `app.rs` re-core, whose detail `plans/app-shape.md` owns). The re-cores are the
tail and they are droppable: the least evidence and the most churn, nothing upstream
depends on them, and finishing the model does not need them. Dropping them costs the
duplication they would have removed and nothing else.

**Here is where the work stands**, and a step in flight is finished before a new one
starts. 0.1, 0.2 and 0.3 have landed whole and are gone from this list; what 0.2 owed
was 0.3's last stage, and the two losses 0.1 and 0.3 named are in `docs/what-was-removed.md`. 0.4's
gate stage (0.4a), row stage (0.4c), one-mode stage (0.4b), node-meta stage (0.4d),
liveness stage (0.4e) and 0.4f have landed: the plan's items are the file's open rows,
the holder and the artifact live where the list is, and a turn end is a close. **0.4g's
g1-g5 have landed** (no version, no participant list, no durable plan, no item cache,
no reclaim cap, no stored membership, and one act where seven handoff verbs were: a run
keeps one sparse status per row, reads everything else from the list, derives who is in
it, and closes only the rows it holds). **0.4g is whole, and 0.4 with it.** 0.4f's one
build + full test pass ran with it (three pre-existing `session_flow` e2e failures,
recorded in `plans/test-tree.md`). **0.5 has landed too**, its gate and live probe with
it, so no row is open; **A1's audit has run too** (2026-10-03, folded as A2-A8 and the
`plans/hygiene.md` items, A8 added by the C5 read), and **A2 and A3 have landed** with
it; the open work is A4, A5, A7 and A9, C5's scope is decided (the rows this session
may work), and the tail is what the audit feeds.

### 0. One way work gets done

- [x] **0.4. The cuts the run makes redundant.** Landed whole, stage by stage in
  git: the gate and the deep/light axis, the row model, one spawn level, the growth
  report, liveness on the member's clock, and `VersionedPlan` becoming a view of the
  file (0.4f: the holder and the artifact live where the list is, a turn end is a
  close, and the engine carries no artifact), and 0.4g below. Losses are in
  `docs/what-was-removed.md`.
  - **0.4g. The swarm state gets one owner.** One step, five commits, one gate, decided
    2026-10-02. All four questions are answered: the plan object goes; per-row run state
    lives in memory on the runtime owner; a claim is a lock for a run and not for the
    user's session; the verb cut rides this step rather than a separate 0.4h. The losses
    are in `docs/what-was-removed.md`. Measured before the stage: the `SwarmState { .. }` rebuild
    sites were 26 (31 in the older note), `VersionedPlan` appeared in 48 files, and
    `swarm_persistence.rs` was 621 lines with 924 test lines. Measured after g3: 25
    rebuild sites, no `VersionedPlan` anywhere, 469 lines with 676 test lines, and
    `swarm_coordinators` referenced 329 times, which is g4's number to move.
    1. **g1. Ordering replaces `version`.** Landed. `version` guarded a stale durable
       write and client ordering; the first left with the field (the persist guard went
       with it, one piece of g2 pulled forward because the field's last caller was the
       guard) and the second is now construction: the event is built and sent while a
       per-swarm lock is held. g1 held the plan's lock for that; g3b deleted the plan, so
       the lock is `swarm_operation_lock` (the one the persist path already takes), and
       holding it across the send cannot block because a member's `event_tx` is an
       `mpsc::UnboundedSender` (`server/state.rs:209`). Deleted `version` and
       `participants` (the broadcast's recipients are the swarm's sessions, which was
       already its fallback), the client's `plan_version` and stale-regression branch,
       `RemoteSwarmPlanSnapshot.version`, the replay record's fields, the
       `swarm:plan_version:` debug read, and member notices built from the participant
       list. The broadcast's ordering is pinned by
       `swarm_plan_broadcasts_cannot_invert_on_one_member_channel`, which replaces the
       test that demonstrated the inversion.
    2. **g2. The plan stops being durable** (behavior boundary: restart). Landed. Deleted
       `PersistedVersionedPlan` and the snapshot's plan field, the plan converters, the
       dormant-plan retention rule, its `KCODE_SWARM_DORMANT_PLAN_RETENTION_SECS` env var
       and constant, the plan argument threaded through the persist callers, and
       `swarm:clear_plan`'s re-persist; `SwarmRuntime::has_any_state` came to mean durable
       state, so the plan no longer counts. It also took g3's first half, because
       deleting the durable copy leaves nothing to source the in-memory plan after a
       restart: the load path seated the swarm's rows from the list. g3b then deleted
       that seating too, so a loaded swarm now holds no plan at all and the reads go
       straight to the list; the load tests pin the member and coordinator round trip.
    3. **g3. The item cache dies** (behavior boundary: the status readers). Landed in
       three commits: g3a removed the reclaim cap and its counter, g3b removed the plan
       object, g3c removed the summary's completed segment. `VersionedPlan`,
       `SwarmTaskProgress`, `sync_rows`, `drop_row`, `set_row_status`, `prune_side_maps`,
       `rename_session`, `is_composite`, `execution_state`, `plan_definition` and the
       debug-only plan DTOs are gone, every item read comes from the list, and what
       replaces them is one rule and one map: a swarm's rows are the open rows its
       members hold, the rows nobody holds, and the rows whose holder left the swarm (the
       sweep's case, and the one that decided the scope), the run keeps a sparse `status`
       per row id while the process lives, and every read uses the run's status when it
       set one and the row's own otherwise. Re-measured facts: the scope rule is
       `swarm_rows` (`swarm.rs`), and a session rename now rewrites its rows' holder
       (`rename_row_holder_on_disk`), since the in-memory copy that used to carry it is
       gone.
    4. **g4. Membership is derived** (behavior boundary: membership). Landed. Membership
       is the spawn edge: a session belongs to the run rooted at the end of its
       `report_back_to_session_id` chain, and `swarm_root` (`server/swarm.rs`) is the one
       derivation every reader calls. That makes the run's root its coordinator, since
       spawning is root-only (0.4b) and the root is the session holding the run's anchor
       row, so the coordinator slot and its election path are gone with the map that held
       them. Deleted: `SwarmMember.swarm_id`/`swarm_enabled`/`role`, `SwarmState.coordinators`
       and `SwarmState.swarms_by_id` (the state is `members` + `runs`), the stored swarm id
       (`KCODE_SWARM_ID`, `swarm_id_for_dir`/`swarm_id_for_session`, and the now-dead
       `git_common_dir_for`), `features.swarm`/`KCODE_SWARM_ENABLED` and the per-session
       toggle (`FeatureToggle::Swarm`, `/swarm on|off`, now `/swarm [status]`), `assign_role`
       (wire variant, tool action, handler and both dispatch arms), the debug `swarm:id:` and
       `swarm:clear_coordinator` ops, and the hook env `KCODE_SPAWN_SWARM_ID`
       (`KCODE_SPAWN_COORDINATOR_SESSION_ID` stays; `docs/user/hooks.md` moved with it).
       **A refinement of the decision**, recorded in `docs/what-was-removed.md`: the decision said
       membership derives from who holds rows under the anchor, and it landed as the spawn
       edge instead, because a worker holds no row between `spawn` and its first assignment
       and because a per-query row read is the item cache g3 deleted under another name.
       Rows under the anchor stay what they already were: the run's scope (`swarm_rows`).
       Measured: 95 files, +887/-3554 lines; no `VersionedPlan`, no `swarms_by_id`, no
       `coordinators`, and no `SwarmState { .. }` literal survives.
    5. **g5. The verb set** (behavior boundary: the user's levers). Landed. One guard,
       one line: `close_row_on_disk` (`kcode-base/src/todo.rs`) refuses a row whose
       `assigned_to` is not the caller, so a turn that ends after its row moved is
       refused rather than closing work that is no longer its own. The person's path is
       a different function (the `todo` tool closes in memory), so it still closes
       anything. Deleted: `TaskControlAction` and its seven variants, both status
       tables, the target-resolution helper, `RestartInstructionPrefix`'s texts,
       `build_control_assignment_text`, `handle_comm_task_control` and its helpers
       (`task_snapshot_for`, `task_id_for_target_session`, `requeue_existing_assignment`,
       `TaskSnapshot`, `plan_graph_status_for`, `active_swarm_member`,
       `task_agent_session`, `format_salvage_message`), `Request::CommTaskControl` and
       `CommTaskControlResponse` with both dispatch arms, the double-assignment refusal
       (`active_assignment_conflict`/`active_assignment_error`), the five tool actions
       (`start`, `start_task`, `resume`, `reassign`, `replace`, `salvage`), the dead
       `requeue_failed` DAG op, and the TUI's action names for them.
       Two refinements of the decision, both in `docs/what-was-removed.md`: the takeover is gated on
       `swarm_is_root` (the 0.4b derivation) rather than on "a session with a client",
       since a headless root reaches only its own run's rows either way; and `retry`/`wake`
       are the assign request with a fixed sentence, carrying a `redispatch` flag so the
       replay layer cannot answer a deliberate retry from the record of the last one.
       `require_plan_driver_swarm` became `require_run_root`, which now checks something.
    Then the gate: one build and the full suite for 0.4f's tail and g1-g5, with the
    three known `session_flow` failures recorded.
  `parse_kind`/`kind_str` stay, since they are what reads and writes a row's word.
  `Synthesize` stays as well: the word lives on the row (`tool/todo.rs` offers every
  `KINDS` entry), so a run's own join row has a word to be typed with. Nothing in the
  engine branches on it, so it costs one enum variant and one word.

### 0.5. One level, low, and no second way to say it

Landed whole 2026-10-03, stage by stage in git: one level and the session's, with no
argument to say otherwise; the `low` default moved into the model table; and config
stopped setting a level at all, with the picker reading the session's. One order and one
home remain: the session's stored level, else the model table, else nothing, read where
the request is built, so a model with no ladder still means `default`. Gate (t5):
`check_guardrails.sh` green; `test.sh full` green bar the three `session_flow` reds; the
touched crates' own suites green bar the reds `plans/test-tree.md` records; and the live
probe on its own socket against a local mock, where a fresh session reported `low`,
`/effort high` then a resume kept `high` and the request carried `high`, and a config
still carrying `provider.openai_reasoning_effort`/`agents.swarm_effort` started clean and
sent `low`. Losses: `docs/what-was-removed.md`.

### A. Audit

- [x] **A1. A braid audit of the tree.** Ran 2026-10-03 with `/braid-audit` over the
  settled 0.5 tree, row-list first. The row-list findings are A2-A7; the ones with
  another owner went to `plans/hygiene.md` (the provider dispatch fan-out, the provider
  tiers, the mislaid catchup types, `tasks.bak`). Measured, row-list tier: the
  coordination machine is 16,395 lines in `kcode-app-core/src/server/{comm*,swarm*}`
  (in-file tests included), 6,551 in `tool/communicate*`, 5,957 in the TUI and render
  `swarm*` files, 498 in `kcode-swarm-core`, over 23 `Comm*` wire requests and 19
  `SwarmState { .. }` literals. D1/D2/F1 still own that cut; A2-A7 are what they miss.
  B3's status cut measured again: five status helpers in `kcode-plan/src/lib.rs`,
  `canonical_todo_status` plus its two wrappers in `kcode-base/src/todo.rs`, and
  `normalize_plan_status_for_todo`, `status_badge` and a second `priority_rank` in the
  TUI.
- [x] **A2. The goals/initiatives store is a second work list with no producer.**
  Decided delete (no external writer expected) and landed 2026-10-03: deleted
  `kcode-base/src/goal.rs` (620) with `goal_tests.rs` (82), the `Goal*` types in
  `kcode-task-types` (185), the `/initiatives` and `/goals` commands with their
  registry, suggestion, help and status-notice entries (`commands.rs`,
  `commands_dispatch.rs`, `state_ui_input_helpers.rs`, `input_help.rs`,
  `ui_overlays.rs`, the remote key-handling call site), the goal-panel tests, and the
  README row. The loss is recorded in `docs/what-was-removed.md`. Gate: the build and
  clippy `--all-targets --all-features -- -D warnings` on `kcode-task-types`,
  `kcode-base` and `kcode-tui`, `check_guardrails.sh` green, `kcode-tui` serial 1760
  passed with its one known red
  (`test_remote_fallback_provider_suggestions_normalize_bare_openai_openrouter_routes`),
  `kcode-base` green, and `scripts/test.sh full` green bar the three `session_flow` e2e
  reds `plans/test-tree.md` records. Two `kcode-tui` tests that only used the removed
  command as their input were repointed at commands the tree still has, not dropped.
- [x] **A3. The mission store goes.** Landed 2026-10-03: deleted `mission.rs` (185)
  and `pub mod mission`, `prompt/mission_continuation.md` (58) with
  `MISSION_CONTINUATION_TEMPLATE`, the reminder hook (`mission_turn_reminder`) with
  its two call sites, the disabled-command shim with its local and remote call sites,
  and the TUI's `current_turn_system_reminder` field with its initializers and debug
  reporting, since the hook was its only writer and no send ever read it (every
  `begin_remote_send` passed `None`). The loss is in `docs/what-was-removed.md`:
  a reminder from a mission file left by an older build is gone, and `/mission` and
  `/goal` are unknown slash text now, which the tree sends to the model as a prompt.
  Gate: `check_guardrails.sh` green with the App shape baseline tightened and recorded
  (`app_fields` 181 -> 180), clippy `--all-targets --all-features -- -D warnings` on
  `kcode-app-core`, `kcode-base` and `kcode-tui`, `kcode-tui` serial 1759 passed with
  its one known red, `kcode-app-core` 948 passed, `kcode-base` green, and
  `scripts/test.sh full` green bar the three `session_flow` e2e reds.
- [ ] **A4. The member runtime stops carrying the rows.** `SwarmMemberRuntime.todo_items`
  (`protocol/src/lib.rs:450`) is the third copy of a session's rows, after the file and
  the run's sparse status: folded from `TodoEvent`s by `compact_todo_items` and the two
  `update_active_todo_*` helpers (`server/background_tasks.rs:362-505`), then mapped
  again into `GalleryTodo` for the gallery. Read the rows where the list is, keep only
  the per-row status the run set, and delete `SwarmTodoItem`, `SwarmToolIntent`, the
  compact and update helpers and the TUI mapping (`info_widget_swarm_gallery.rs:98`).
  Gated by C5; the gallery is the only consumer. Two things the audit's first pass
  missed. `tool_intents` is not a row fact: it is the gallery's "which tool is this
  worker running" display, nested under the compacted item by
  `update_active_todo_tool`, so it moves onto `SwarmMemberStatus` rather than dying
  with the cache. And `member_runtime_extras` reads the whole list per member per
  `swarm list` for a `completed/total` counter (`comm_sync.rs:133-140`), which the
  forwarded bus event already carries.
- [ ] **A5. One read of the list and one render, not four.** The inline card, the
  side-panel page (its own comment says "legacy"), the pinned band and the info
  widget's pips each render the list, and the client keeps four caches over one file:
  three hashes in `todos_view.rs`, a 1s TTL cache with a refresh thread
  (`helpers.rs:969-...`), and the transcript's previous-list parse in
  `ui_todo_changes.rs`. `commands_improve.rs` repeats the same load six times, and
  `turn_notify.rs:220`, `state_ui.rs:1811` and `remote/key_handling.rs` (four sites)
  read it again. Decide which surfaces stay, then keep one read and one model with the
  renderers as pure functions; C5 makes the read a server event and lands first.
- [x] **A6. What 0.4 claimed to delete and did not.** Landed 2026-10-03: deleted
  `SwarmPlanItemSpec` (`kcode-plan/src/lib.rs`) with the serde import it alone used, and
  gated the simulator (`#[cfg(test)] pub mod sim;` in `dag/mod.rs`, its doc link
  reworded so a non-test build does not point at a test-only module). Gate: clippy
  `--all-targets --all-features -- -D warnings` green on `kcode-plan`, `kcode-base` and
  `kcode-tui`; `kcode-plan` 34 passed.
- [ ] **A7. One artifact shape.** `tool/todo.rs:144-168` hand-builds the close artifact
  from three fields while `kcode-plan/artifact.rs` owns the seven-field
  `HandoffArtifact`; a field added on one side drifts silently. The tool builds a
  `HandoffArtifact` and serializes it.
- [x] **A8. The client stops writing the rows into session files.** Landed
  2026-10-03, writers only: deleted `record_swarm_status_event` and
  `record_swarm_plan_event`, the TUI's `persist_swarm_status_snapshot` and
  `persist_swarm_plan_snapshot` with their call sites, so no swarm status or plan
  broadcast triggers a full session save any more. The two `StoredReplayEventKind`
  variants and their redaction arms stay: `load_from_path` parses the snapshot with
  no tolerance, and 52 of the 2437 session files on this machine carry those events,
  so removing the variants would make real sessions unloadable. The redaction test
  now builds the legacy shapes directly and round-trips them through
  `write_json_fast` + `load_from_path`, so the compat property is pinned. A deeper
  read of the same log is A9. Gate: `kcode-base` 1097 passed serial with the new
  round trip, `check_guardrails.sh` green, `kcode-tui` serial 1759 passed with its one
  known red, and `scripts/test.sh full` green bar the three `session_flow` e2e reds.
- [x] **A9. The replay-event log has no reader.** Landed 2026-10-03: deleted
  `record_replay_display_message`, the TUI's `persist_replay_display_message` with its
  four call sites, `mark_replay_events_append_dirty`, the journal's
  `append_replay_events` and the persist state's `replay_events_len`/`_mode`, the delta
  computation and its `delta_replay_events` telemetry, and the memory-profile
  accounting for the field (`SessionMemoryProfileSnapshot.replay_event_count` included).
  The field, the three variants and the redaction arms stay for A8's load-compat reason;
  new sessions simply never fill the log. Two behavior notes: a display notice no longer
  triggers `session.save()`, so it no longer re-stamps `updated_at` (the resume picker's
  recency stops moving on a notice), and the memory-profile payload loses its
  replay-event keys. Gate: `kcode-base` 1097 passed serial with the round-trip test,
  `check_guardrails.sh` green, `kcode-tui` serial 1759 passed with its one known red,
  and `scripts/test.sh full` green bar the three `session_flow` e2e reds.

### B. The file is the list

Last of the file work, whenever we want it.

- [ ] **B2.** This repo's own list migrates: the open checkboxes in `plans/*.md`
  become the first content of `tasks.jsonl`. Its landed records have already moved, as
  they land, to `docs/what-was-removed.md`; what is left to migrate is the open items.
- [ ] **B3.** Drop `group`, `status` and `priority` from the type. The gate has
  already landed: `parent` is what `group` was grouping by, the close action is what
  makes a completed row unrepresentable rather than stored, and 0.3's "position is
  priority" is what `priority` becomes. Every `add` still writes `status` and
  `priority` today. Dropping `status` is the larger half, because the plan
  classifies by it everywhere: `summarize_plan_graph`, `completed_item_ids`,
  `is_active_status`, `newly_ready_item_ids` (read by the swarm path), the task-control
  actions, and `status_from_plan`. `status_to_plan` is already gone: 0.4f's s12 stopped
  lowering the engine's statuses back into items, so the engine's own statuses no longer
  reach a row. Afterwards a row is ready when `blocked_by` is empty and liveness comes
  from the member, not the item.

### C. The list reaches the client

- [ ] **C4.** Move `subsystem` and `file_scope` off the shared type onto the
  worker's own record. They are the scheduler's inputs (assignment affinity matches
  them against a worker's metadata), not list fields, and neither destination the
  old text named can hold them: 0.4 deleted the `node_meta` side-map and claimed
  `SwarmPlanItemSpec`, which survived (A6).
- [ ] **C5.** The client renders the list from server events instead of reading
  the file itself. Today it resolves the repo from its own working directory,
  which is the same thing for a local session and the wrong repo for a remote
  attach. Two facts make this plumbing rather than new machinery: a run's rows
  already cross as `ServerEvent::SwarmPlan { swarm_id, items, reason, summary }`
  (`wire.rs:837`, produced at `server/swarm.rs:836` and `:894`, applied at
  `remote/server_events.rs:1995`, where `plan_items` already feeds the info widget
  and plan progress), and the full rows already cross the bus on every write
  (`BusEvent::TodoUpdated { session_id, todos }`, `tool/todo.rs:307`), which the
  server only folds into the compacted member cache (`server.rs:2065`). What is
  missing is the arm that forwards that bus event to the owning session's clients
  and the client rendering its surfaces from it; A4 then deletes the member cache,
  including `SwarmTodoItem`, which is lossy (capped content, no id, `parent`,
  `blocked_by` or `kind`) and so cannot stand in for the list.
  - **The view is the rows this session may work, computed once on the server**
    (decided 2026-10-03). A session in a run gets the run's rows (`swarm_rows`,
    `server/swarm.rs:12`); a session outside a run gets the whole list. Measured:
    `swarm_rows` already keeps every unheld row and every row held outside the run,
    so this differs from "always the whole list" only when two runs share one repo,
    and the client renders one list either way, so no user-visible mode appears.
    `Action::List` takes the same rule, which is what ends its whole-file read
    (`tool/todo.rs:64` over `load_tasks` at `:303`). Rejected: scoping every session
    to its holdings, which would hide rows the person may pick up and contradict
    "the user's session changes any row".

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
- [ ] **D2. The swarm/comm condense.** The `SwarmState` handle pair is still
  threaded to ~40 functions and rebuilt as a literal at each request arm, so the
  request context that ends it is the remaining half. Give the swarm state its one
  owner and delete the pair. Engine: `internals/swarm.md`. (A1 measured 19
  `SwarmState { .. }` literals and 244 `swarm_runs` references on the settled tree.)

### E. The server shape

The request-path split is `plans/server-shape.md`'s H1-H5, and this plan owns none of
it: H2 (the swarm state in the client context) waits on D2 above, which is the only
row-model work that path carries.

### F. The swarm tool surface

- [ ] **F1. The tool keeps the model's verbs; the app takes the rest.** After 0.4 a node
  is a row, so the model's verbs are row verbs, which the `todo` tool already serves.
  The orchestration actions (`spawn`, `assign_task`/`assign_next`/`fill_slots`,
  `run_plan`, `cleanup`, `await_members`) become the run's own behavior, not calls. The
  exact cut is decided at the step; likely one task tool.
F2's shape lands with F1: what remains of `tool/communicate.rs` after the verb cut is
`plans/server-shape.md`'s condense task, which owns the file.

### G. Close out

- [ ] **G1.** Restore the two size ratchets in `scripts/check_guardrails.sh` and
  re-baseline both with `--update`. They are paused, with the reason at the call
  site.
- [ ] **G2.** The one live `kcode run` probe against its own socket, in a scratch
  repo, run by the user: the permission (a granted turn continues, a wake does not)
  and the poke's removal (no client continuation; every non-retryable error gets the
  short budget). It is the step's landing proof, so it runs when the list is done,
  not per step.
- [ ] **G3.** The work list gets its user doc under `docs/user/`: `tasks.jsonl` at the
  repo root, the `todo` tool's actions, and the close's required result are described
  only in this plan, which is a design rather than a manual. It waits until the list
  settles, which is worth writing once: the row gains its `kind` at 0.3, and B3's
  field cuts land before the shape stops moving.
