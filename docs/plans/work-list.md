# Work list

Work on branch `work-list`. `docs/todo.md` points here. Tick a step when it lands
and delete it; git has the history.

## How a step lands

This plan is a deletion program. A step removes a representation, a duplicated
shape, or a mode, and a step that only adds surface is reconsidered before it is
written. Four rules, each one paid for by a failure this plan already had:

- **Every cut names its code and the evidence.** A step quotes the file and count it
  measured (`31 SwarmState { .. }` sites, `app_fields=188`, the `SwarmMember`
  literal count), and the claim is grepped again before the step is written. 0.4
  shipped two claims that were false on this tree, and `internals/swarm.md` had
  repeated one of them.
- **A step lands whole, in one change.** Deleting a subsystem is one commit, not a
  train of half-states. The doc line rides the code, and prose does not outrun
  landings: a plan that grows faster than the tree shrinks is not this plan.
- **A decision on a step's path is taken with that step.** A `(decide)` item is
  answered before the step starts, or the step is marked blocked on it.
- **A loss is named, never silent.** Capability is fixed and only surface goes. A
  step that drops power records it in `docs/todo.md` with why, the way the deleted
  overnight preflight did.

Zonytail's measure is the codebase after the step, not the diff: fields, `impl`
blocks, wildcard re-exports and lines per file, ratcheted by
`scripts/check_guardrails.sh`. A step that must grow records the reason in the same
commit, because the App shape ratchet only tightens.

**The go/no-go is the second `(decide)` under 0.3.** If a run in flight needs two
durable stores, the plan or the file, then the model's central promise is false:
stop at 0.3, keep what landed, and drop the tail rather than adapt it.

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

Every step lands whole, proven by the gate and, where behavior moves, one `kcode
run` probe against its own socket. Widest shared shape first, so no step sweeps
call sites a later step reshapes: 0.1 installs the permission so 0.2 can delete the
poke, and 0.3 gives the state one owner so 0.4 can delete the rest.

After that, in the order of the moves: delete (D, the topic channels, the shared
context, and the member projection written four times), then the re-cores (E, F,
and the `app.rs` re-core). The re-cores are the tail and they are droppable: the
least evidence and the most churn, nothing upstream depends on them, and finishing
the model does not need them. Dropping them costs the duplication they would have
removed and nothing else.

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
  Landed, and proven end to end in `server/tests.rs`: the loop and its pick, with
  the run's own bound in place, so a row worked once in the run is not picked again
  and a row left open ends the run instead of spinning it (`TurnSeed` carries the
  row id into the loop's `worked` set, which the pick skips). A session holding two
  ready rows takes three turns and then stops, a row whose blocker is still open is
  left alone, nothing ready takes one turn, and a turn nobody granted takes no
  second row. The permission landed 2026-10-01: it rides the turn that grants it
  (`may_continue` on `Request::Message`, set by `/auto <words>` and spent with that
  turn), the standing default is `features.auto_poke` for a project where every
  turn of yours may, and it is read in one place, the turn-terminal path that
  decides the run. `may_continue_on_its_own` is gone, so the permission is no longer
  fused with having a human and its two readers are one. A run's permission is its
  own parameter, so a wake (a DM, a background completion, a resume) never continues
  over rows: that is rule 11, and it is a behavior change for a session that had the
  standing default on. Headless stays excluded by the reservation until 0.3 makes a
  member's turn a row's. "Typing always wins" holds: the reservation is given up
  before the next row is looked for, so a turn already waiting for the agent takes
  it and the run ends there. `auto_poke` is now a misnomer for the standing default,
  and the name goes with the last poke, the command-line one 0.3 unblocks, because
  until then both read it.
  Owed by the step's own landing rule: the permission moved behavior (a granted turn
  continues, a wake does not), so it wants one `kcode run` probe against its own
  socket and a scratch repo, not only the in-process tests. The proof so far is the
  pick and the loop under a mock provider.
  The standing default's other half, `may_continue || features.auto_poke` in
  `record_processing_completion`, landed with a test 2026-10-01
  (`the_standing_default_continues_an_ungranted_turn`): one attached session holding
  a ready row, driven twice through the real path, with the default off nothing runs
  and with it on the row's turn does. It had to land before 0.2 stage 4, which
  removes the config key's last client reader.
  Small, still owed: the row rung (`/auto t3`, the pick's optional subtree filter),
  an optional bound so "work until 07:00" is the run property the model says (the
  duration parser and the target-wake label are in git history), and a
  quota-projection warning built from the existing provider usage reports. The
  resource snapshot and a run-end summary stay undecided in `docs/todo.md`.
- [ ] **0.2. Delete the client poke**, which this replaces. With the server
  continuing a session that holds ready rows, the TUI's auto-poke machine goes:
  `auto_poke_incomplete_todos` (16 files, 65 uses), `last_auto_poke_fingerprint`,
  the `/poke` command with the `auto_poke_toggle` keybinding it is bound to (ctrl+p
  by default, with its template line, its `display_summary` entry and its test), its
  overlay line, its help and its completions, and its tests. `total_pokes_sent`, `morning_report_poked` and `final_wrap_poked` already
  went with the overnight subsystem, and the toggle defaults off, so nothing the
  user had is lost.
  Two things in it are power rather than poke, and both are settled. A
  guardrail-stopped turn does return `Ok`: the refusal breaks the stream rather than
  the turn (`turn_streaming_mpsc.rs:1164`), so a run advances past it. It is not
  re-homed anyway, and the reason is the run's own bound: a row is attempted once, so
  a refusing provider costs at most one call per row the session holds, and the run
  still ends. The breaker existed to stop an unbounded re-poke loop, and that loop is
  what this step deletes. The non-retryable-error classifier stays, because it is the
  client's retry policy and a failed turn already ends the run.
  The docs that name the poke go with it: the `/poke` examples in
  `dev/message-voice.md`, the `/poke` help, and its completion entries.
  The pass, in the order the caller graph allows, so every stage leaves a tree that
  builds: a symbol goes only once its last caller is gone. Five stages, each landed
  and committed on its own.
  1. **The tests**, because they are the only callers of the poke API from outside it
     (`commands_tests.rs` and eleven files under `app/tests/`). Done 2026-10-01: 21
     poke-only tests, 850 lines, with the two `Esc`-interrupt tests removed rather
     than trimmed, since their subject was the poke disarm and a neighbouring test
     keeps the interrupt covered.
  2. **The user surface**: the command and its arms (`commands.rs`, `key_handling.rs`),
     the registry and completion entries, the help arm, the `auto_poke_toggle`
     keybinding end to end, and the hotkey and overlay lines.
  3. **The scheduler**: `schedule_turn_end_followups` with `conclude_completed_turn`'s
     use of it, `schedule_auto_poke_followup_if_needed`, `build_poke_message`,
     `queue_poke_message`, and the breaker trio with `turn_guardrail_stopped`, which
     only the breaker reads.
  4. **The state**: `auto_poke_incomplete_todos`, `last_auto_poke_fingerprint`,
     `disable_auto_poke`, `clear_queued_poke_messages`, and the poke arms in
     `stop_auto_poke_for_non_retryable_error`, `remote.rs`, `model_context.rs`,
     `server_events.rs` and `tui_lifecycle.rs`.
  5. **The docs** that name it.
  Two traps. The five retry tests at `remote_events_reload_04.rs` 202, 268, 310, 366
  and 425 are not poke tests: they use the poke only to create a queued follow-up, so
  they are retargeted at the retry path in stage 3 (create the follow-up directly,
  rename away from the poke) rather than deleted, which would cut retry coverage.
  `commands_tests.rs:132` needs only a rename, since the classifier it tests survives.
  And an existing `auto_poke_toggle` line in a user's config becomes an unknown key
  that is silently ignored, which is §5's known hazard, so it is named there.
  `build_auto_poke_message` is still called by the command-line paths this step
  defers (`src/cli/commands.rs:719`), so it, the `features.auto_poke` rename, and the
  poke-named survivors go with those, not here: `is_non_retryable_auto_poke_error`
  (really a turn-error classifier, read by the client's retry path) and
  `is_auto_poke_connectivity_error` (read by the network-wait path, whose call site
  also ORs `network_retry::classify_message`, which the function already covers). The command-line variant (`src/cli/commands.rs`, the
  `_with_auto_poke` run paths and `run_command_auto_poke_max_turns`) waits for 0.3,
  because a plan-driven member must not be driven twice, and a headless run has no
  plan until rows seed one.
- [ ] **0.3. Rows are the run's seed source.** `kind` rides on the row, the node id
  is the row id, the file's `blocked_by` is the node's dependency edge (rules 6 and
  7 name it; there is no rename), position is priority, and gates get engine names.
  Testable against a scratch repo, so it needs no migration.
- [ ] **0.3's `kind` is the engine's word, stored once on the row.** `bridge.rs`
  already owns the only vocabulary (`parse_kind` reads it, `kind_str` writes it), so
  a row holds that word and the store learns no enum and takes no dependency on the
  engine (rule 5). The one change is that `parse_kind` stops guessing: it defaults an
  unknown word to `Explore`, which is what rule 8 forbids, so absent or unrecognised
  has to mean the row is not seedable. Gate kinds are never typed on a row, since a
  gate is the one node with no row.
- [ ] `assigned_to` stays an opaque holder string, with no marker for a person. The
  mechanism never needs one: a holder naming no live session is never picked by any
  session, so a person-held row is inert by construction, and a claim whose session
  died is what `run_plan` already reports as a stall. Only the narration has to tell
  them apart, and the string is the narration (rule 10).
- [ ] **(decide)** what a running run shows in the file while a node is in
  flight, and what a stalled node shows. The row's `note` is the durable half of
  `SwarmTaskProgress.last_detail`; one of the two has to win.
- [ ] **(decide)** where a run's finished-but-not-integrated nodes live. The file
  deletes a row when its result is durable, but a gate must name every done node
  in its scope by id, and a run that resumes after a restart needs them too.
  Either the live graph is the gate's input and the persisted plan carries those
  nodes until the run closes, which is two durable stores while a run runs, or
  the gate reads something else.
- [ ] **0.4. The cuts the run makes redundant**: the wire node spec, the
  `coordinators` map, any stored swarm id, the 31
  `SwarmState { .. }` rebuild sites, and the `Synthesize` kind, reachable only
  through the wire spec this step deletes. `parse_kind`/`kind_str` stay, since they
  are what reads a row's kind. The deep/light flag is not residue,
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
