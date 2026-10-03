# What this fork removed

kcode is jcode minus a large amount of surface area, and then its own model minus
another: the fork cut, the account cut, and the model cut. This is the ledger of
every deliberate removal and what it cost, so a decision's price is recorded
where the decision is, not in the order of work.

The rule for a step that drops power: it is recorded here, with why, in the same
change that drops it. "What is lost" means lost capability, not churn: if nothing
a user could do stopped working, there is nothing to record.

## The fork cut

Roughly **241,000 lines across 1,079 files**, in one commit
(`feat!: remove mermaid, diagrams, inline images, latex and replay`):

- mermaid/diagram rendering, inline images, LaTeX, session replay
- agent memory and ambient mode
- Gmail and Google login, dictation, the productivity dashboard
- the macOS computer-use tool, the menubar app, the client installer
- the iOS app, the telemetry worker, the TypeScript SDK

## The account cut

The jcode.sh account, subscription and hosted-model surface:

- `kcode account login/status/manage/logout` and its device-auth flow
- the `jcode` provider ("Jcode Subscription") - **this removes the only route to
  Jcode's hosted models**
- `/subscription`, `/subscribe`, `/hosted`, `/support`, and the hosted-model
  nudge
- the `subscription_api`, `subscription_catalog`, `account_login`,
  `provider/kcode`, `cli/account`, `kcode_device`, `subscribe_nudge` and
  `support` modules
- the "Jcode subscription" pill in the login-import summary

## Ambient leftovers, removed later

The fork cut ambient mode but left its CLI and transcript types behind. Now
gone as well:

- `kcode permissions` and the `kcode-tui-permissions` crate. With ambient mode
  removed, nothing ever enqueued a permission request, so the review TUI was
  unreachable.
- The dangling `#[command(subcommand)]` and ambient doc comment on the
  `Permissions` variant, which had no subcommands.
- `AmbientTranscript` and `SafetySystem::save_transcript` in `kcode-base` (zero
  callers).

## The model cut

The 0.x work (`plans/task-flow.md`) removed the machinery around the work list. Each
entry is a decision that dropped power, with what the drop cost.

**One mode: spawning is the root's, and the `/effort` swarm rungs go** (0.4b,
2026-10-01). Only the root session starts agents; a member's deeper work is rows the
run dispatches, and the rule lives at `spawn_swarm_agent`, the single choke point, so
the assign path obeys it too (it previously bypassed it). What is lost: a member can
no longer start a worker on its own initiative, and a user can no longer select
orchestration with `/effort` (`swarm`, `swarm-deep`). The rungs' only other effect
was a system-prompt nudge to fan out, and that nudge is deleted rather than
relocated: "use the swarm for non-trivial tasks" is the removed orchestration mode
in prose. Until F1 turns the swarm tool's orchestration actions into the run's own
behavior, nothing prompts the model to fan out, so orchestration quiets down between
0.4b and F1; the guidance returns with the row verbs, not as a paragraph.

**The plan's growth report and the seed/expand tag go** (0.4d, 2026-10-01).
`origin`, `GrowthStats`/`growth_stats`, the `seeded_count`/`grown_count` snapshot
fields, their renders, and the engine's `NodeOrigin`/`TaskNode.origin` are deleted.
What is lost: `plan_status` and the `run_plan` summary no longer print
`seeded -> grown (+expansion)`. Growth is a run fact, so a run-scoped counter is the
home to add if the scoreboard is wanted.

**What `parent` means, and where the plan's copy goes** (0.4f, 2026-10-01).
`row.parent` is the file's one hierarchy: `anchor_from_rows` adopts root rows and
`expand_row_on_disk` adds children, so anchor grouping and decomposition are the same
relation, not two. What is lost: nothing a user could do; this one moved a
representation. Recorded because it decided where a later cut (the plan's item cache)
would land.

**The plan object goes, and per-row run state lives in memory** (0.4g g1-g3,
2026-10-02). The rows are the file and membership is a count, so `VersionedPlan` was a
cache of the list plus four things the file cannot hold: a version, a participant
list, a per-row lifecycle and a per-row reclaim count. What is lost: restart recovery
no longer restores a plan, and the per-row reclaim bound is gone with the counter, so
a row whose workers keep dying stays open and unclaimed instead of failing after
three tries. The bound now belongs to the loop that repeats the work (the run's own
`worked` set) and to `run_plan`'s loop limits until F1 folds hand-outs into that
loop. Also gone: the `swarm:plan_version` debug read and the plan page's `done`
segment, which nothing can set once a closed row leaves the list. Two smaller signals
moved with it: the `run_plan` progress card has no completed count, and the
credential-failure wave guard now treats a worker that finished a turn as "the route
works" instead of a completed node, so it can fire while a long run is going well.

**`KCODE_SWARM_ID` goes with the stored swarm id** (0.4g g4, 2026-10-02). A swarm is a
count, not a mode, so membership is derived rather than declared. What is lost: two
working directories can no longer declare one shared swarm by environment variable,
and two sessions in one working directory no longer message each other unless one
spawned the other. Refined when g4 landed: membership is the **spawn edge**
(`report_back_to_session_id`), not who holds rows under the anchor, because a worker
holds no row between `spawn` and its first assignment, and because answering it per
query is a file read per membership question (the item cache again). Rows under the
anchor stay the run's scope.

**The spawn edge is the membership, and the coordinator slot goes** (0.4g g4,
2026-10-02). Deleted: `SwarmMember.swarm_id`/`swarm_enabled`/`role`, the coordinator
map, `swarms_by_id`, the `KCODE_SWARM_ID`/`KCODE_SWARM_ENABLED` env vars, the
per-session toggle, `assign_role`, the debug `swarm:id:` and `swarm:clear_coordinator`
ops, and the hook env `KCODE_SPAWN_SWARM_ID` (`KCODE_SPAWN_COORDINATOR_SESSION_ID`
stays). What is lost, three behaviours: a root that leaves no longer elects a
coordinator, so its workers become roots of their own runs and their completion
reports have nowhere to go; `/swarm on|off` is gone (`/swarm [status]` reports the
subtree), because a session is a worker when it holds and works a row, not a state to
be in; and a role can no longer be assigned by hand, since it is derived.

**A claim is a lock for a run and not for the user's session, so the verb family
collapses** (0.4g g5, 2026-10-02). Start, resume, retry, reassign, replace and salvage
were `assign_task` with different words, and each carried a status table and its own
refusal. What is lost: the status gate that refused a retry on a row that was not
failed (the check is a rule, not a field now), the busy-agent refusals that told the
caller to wake or stop instead, the plan-summary tail on a task-control response, and
the notice a displaced holder used to get: it is no longer told to stand down, it
finds out on its next write. What remains and covers the ground: `assign_task` as the
takeover, `retry`/`wake` as the same dispatch with a fixed sentence, the reclaim sweep
for a dead holder, the picker's hand-back, and the row's own `note`, where `salvage`'s
prior tool-call summaries now belong. S1b/S1d then deleted `assign_task`,
`retry`, `wake` and the rest of that family; see the control-surface entry below.

**Fold the Python live drivers into Rust** (tests). What is lost: the
streaming-timing cases (a wall-clock race; queueing while idle covers the same
placement), the real-provider cases, the urgent-skip case, and the API-error case.
The ported case is `tests/e2e/soft_interrupt.rs`'s live placement, which the unit
tests (`messages_end_with_tool_result_*`, `queue_tests.rs`) do not reach.

**The overnight run is gone, and two of its parts are still unplaced** (0.1,
2026-10-01). The overnight subsystem was a second run with its own crate, supervisor,
durable store, UI and preflight; its two parts that were really the run's permission
went back onto that permission. What is lost, and only if wanted: the run's bound
("work until 07:00") was the run property the model names, and its duration parser and
target-wake label are in git history (`fb2b7737` deleted `kcode-overnight-core`); cut
because a run already stops when its scope has no ready row, so a wall-clock stop was
a second stop with no caller. Add when "keep going for two hours" is wanted more than
the list draining. The quota-projection warning would read the existing provider usage
reports and print where the usage snapshot already renders (`info_widget_usage.rs`,
`usage_overlay.rs`); cut because it changed no behavior, so add when a run can act on
it, by throttling or stopping. A resource snapshot (battery, memory, disk, load: ~200
lines of /proc and sysfs readers that were only ever displayed) is worth keeping only
if something acts, so throttle or stop on battery and refuse a run with no disk. A
run-end summary is placed, not lost: the run's scope row closes last and its result is
the summary. Deliberately not carried over: the manifest, the events file, the task
cards and the review HTML (a second durable store of the same work), the second
supervisor and coordinator session, and the stored `validated` field, which a close's
result is now.

**The critique/verify pass and strict artifact validation go with 0.4** (2026-10-01).
Deep mode inserted a gate per node and refused a close whose artifact did not account
for its dependencies; the task flow has no room for a node that is not a row, so gates
and the deep/light axis are deleted. What is lost: the automatic insertion and the
refusal; what stays is the record every close leaves on the row that owns the work.
Add the pass back as work, not as machinery: a row typed `critique` whose close needs
the same record, which anyone (the user, the model, a coordinator) can add like any
other row.

**A one-shot `kcode run` no longer drains a list** (0.3, 2026-10-01). The
command-line poke was the last client-side continuation, so `kcode run` is one turn
again: the permission is read in the session's loop, and a plan-driven member holds it
inherently. What is lost: a scripted unattended run from the CLI. Add a drain back to
the CLI if that is wanted, which means giving that path the row loop the session
already has rather than a second poke.

**A quietly held claim can no longer be taken by a plain `assign_task`** (2026-10-01).
The double-assignment guard used to age out: an assignment older than
`KCODE_SWARM_TASK_STALE_AFTER_SECS` whose assignee showed no recent activity was
reassignable by naming its task. That needed a per-task assignment time, and the model
keeps none: a claim is assumed to be worked while its holder lives. What is lost:
ageing out. 0.4g then replaced the verb family this named: `assign_task` is the
takeover and the status tables are deleted, so the age-out is moot unless a per-task
clock is added back with it.

**An idle worker that still holds a row is reusable** (0.4f s10, 2026-10-01).
Auto-pick used to call a member busy when the plan held any non-terminal assignment
for it, a per-task count read beside the member's own status. Busy is the member's
in-flight work, and that is the member's status: `queued`/`running` already says it.
What is lost: a per-member cap on stacked rows. Add one only if stacking matters
again.

**A decomposition's finish step goes back to the agent that split it** (0.4f s11,
2026-10-01). The row's `assigned_to` is the record of who owes the work, so a ready row
claimed by a live, idle holder is handed back to that holder, and no fresh agent is
spawned for it. Two consequences, both accepted: `run_plan`'s default fresh-agent
preference stops applying to a held row, so the finish step waits for the agent that
planned the decomposition instead of running on a new agent; and a ready row held by a
free session is dispatchable by an untargeted `assign_next`. What is lost: the ability
to force a fresh agent onto a held row by preference alone.

**The plan's items are the file's open rows** (0.4f s12, 2026-10-02). A row write goes
through the store and the plan follows it, so a holder, a close and an artifact live
where the list is. What is lost: a row blocked by another no longer receives that
row's artifact, because the list keeps no edge to a closed row, and the debug
`swarm:graph` op is gone, since it existed to drive the engine against plan-owned
items. What is gained: a split row's join turn reads its children's artifacts, and a
run's top row reads the run.

**Effort is the session's level, and config carries none** (0.5, 2026-10-02). Four
spellings of a level go, each a way to *say* it at a different scope while pretending
to be the same thing: the swarm tool's `effort` argument with its wire fields and the
three-step spawn precedence (an explicit value, then `agents.swarm_effort`, then the
provider's own), the `agents.swarm_effort` pin with `KCODE_SWARM_EFFORT`,
`provider.openai_reasoning_effort` and `provider.anthropic_reasoning_effort` with
their env overrides, a named profile's per-model `reasoning_effort`, and
`/account openai effort`. A spawned session inherits its creator's level instead, and
the model table is the only place a provider's own default lives.

What is lost: a machine-wide default for new sessions, since config pinned every
session's level and `/effort` is per session; a coordinator's level per worker, per
call; and a repo's ability to pin its workers at one level. What stays: `/effort`, the
model picker's effort entries, ACP's `session/set_reasoning_effort`, and the effort
keys, so a human still sets one level in one action and the session keeps it. Add a
machine default back if it is wanted, as the non-interactive surface's own flag
(`kcode run --effort`), which is the same concept as `/effort` rather than a sixth
place to store one.

**The initiative/goal store goes** (A2, 2026-10-03). The goals read path had no
producer: `create_goal` was called only from tests, `update_goal`,
`attach_goal_to_session`, `write_goal_page` and `refresh_goals_overview_for_session`
had no caller at all, and no tool, CLI command or debug op wrote a goal file, so
`/initiatives` and `/goals` could only render JSON nothing in the tree ever created.
Deleted: `kcode-base/src/goal.rs` (620 lines) with `goal_tests.rs`, the `Goal*` types
in `kcode-task-types` (`Goal`, `GoalScope`, `GoalStatus`, `GoalStep`,
`GoalMilestone`, `GoalUpdate`, 185 lines), the `/initiatives` and `/goals` commands
with their registry, suggestion, help and status-notice entries, and the README row.
What is lost: someone who hand-wrote `~/.kcode/goals/**/*.json`, or kept files from
an older build, can no longer read them through kcode, and a tracked initiative is
no longer a thing the app knows about. It was a second work representation beside
the row list, with its own statuses, milestones, steps and progress; the row list is
the one representation the model keeps. Bring it back as rows if it is wanted.

**The mission store goes** (A3, 2026-10-03). The mission module had no writer:
`mission::set`, `checkpoint`, `clear`, `update_status`, `render_status` and
`render_mission_continuation_prompt` had zero callers, `/mission` and `/goal` were
refused as "disabled in this build", and the only live path was the TUI's per-turn
reminder reading a file nothing in the tree writes. Deleted `mission.rs`, the
`mission_continuation.md` template with `MISSION_CONTINUATION_TEMPLATE`, the reminder
hook, the disabled-command shim, and the TUI's `current_turn_system_reminder` field,
of which that hook was the only writer (every send already passed `None`). What is
lost: a mission file hand-written or left by an older build no longer injects a
continuation reminder, and `/mission` and `/goal` are no longer recognized, so like
any other unknown slash text they are sent to the model as a prompt rather than
refused with a notice. The concept itself is rule 11's permission, which stays.

**The tool's control surface is stop, message, spawn and list_models** (S1b/S1d,
2026-10-03). The verbs that drove a run by hand go: `assign_task`, `assign_next` and
`fill_slots` (hand a row to a session), `run_plan` (the driver with its concurrency
policy, stall retries, credential breaker and cap recovery), `retry` and `wake` (the
same assign with a fixed sentence), and `cleanup` (end finished workers by hand).
`stop` loses `force`: it ends the target's subtree on the spawn edge, deepest first,
and cannot reach a session outside it. What is lost: an agent can no longer hand a row
to a peer, wake a stopped member, or retry a failed turn by naming it, and a
coordinator can no longer stop a session outside its own run. What covers the ground:
a write that makes a row ready wakes its headless holder (the loop's wake, landed with
S1a), the run ends itself when none of its rows is ready (`end_spent_runs`), `message`
carries the handoff to one addressed session, and `stop` on the run's root ends the run
with every member under it.

**The swarm tool's read views and the report action go** (S1c, 2026-10-03). An agent
could ask the tool for a member's status snapshot, its recent tool calls, the run's
plan status, another session's transcript, and the member roster. Those five go, and
so does `report`: the structured completion report a worker pushed to its
coordinator (status, validation, follow-ups, tldr). What is lost: an agent can no
longer ask the tool about a peer at all. It reads the rows it may work, and the
holder's own state is the client's to render (S2/S3), so the roster's per-member
detail (files touched, live activity, provider and model, token churn, turns) is
client-only from here. A worker's words still reach its coordinator, but as the
close's `result` on the row that owns the work, not as a pushed report; the status
notification keeps the lifecycle fact alone. The instruction protocol that fed those reads goes with them:
the spawn reminder that told every worker to call `action="report"` before finishing
is deleted rather than replaced, because the discipline it stood for is already
enforced where the words are written (`close_row` refuses an empty result).

**The task-DAG verbs and the engine behind them go** (S1c, 2026-10-03).
`task_graph`/`seed_graph`, `expand_node` and `complete_node` were the `todo` tool's
add and close in a second vocabulary, and the only door to `kcode-plan`'s DAG engine:
the model, its ops, its scheduler, its deterministic simulator, and the typed
`HandoffArtifact`. The row list is the one graph, and the same writes went through it
while both existed, so nothing a user could do stopped working. What is lost is one
guard: the engine refused a `blocked_by` cycle, and nothing refuses one now. A cycle
written through the tool or by hand shows as a row that never becomes ready, with its
`blocked_by` visible in the list, which is why it was not carried over; the row's
`parent` edges and the loop's readiness walk are both cycle-safe.

**The await_members waiter goes** (S1e, 2026-10-03). A tool action let an agent park a
turn, or hand off to a detached watcher, until named swarm members reached a target
status, with a timeout, an any/all mode, and a durable pending state that survived a
server reload and resumed. All of it goes: the action and its inputs, the request pair
on the wire, the transcript read it used to pull each member's last assistant message
as a report, `latest_completion_report` on the member, its durable record and the
channel-member copy, and the background-await notification card. What is lost: an agent
can no longer block a turn on another session's status, and a run has no barrier
primitive. Readiness is the wait: a run ends when none of its rows is ready, and a
write that makes a row ready wakes its holder (the loop's wake, landed with S1), so
waiting for a worker is holding the row that will tell you. The blocking half never
resumed after a reload anyway, since its socket waiter dies with the process, so the
durable state bought only the background case, which the loop's own dispatch makes
unnecessary.

**The channel index and the shared-context store go** (S1f, 2026-10-03). Swarm
agents could publish named key/value entries every peer could read, and subscribe to
named channels so a message reached only that channel's members. Both go, with the
verbs that fed them (`share`, `share_append`, `read`, `subscribe_channel`,
`unsubscribe_channel`, `list_channels`, `channel_members`), the channel and delivery
shapes of `message`, and the one-line `tldr` that let a long message arrive collapsed.
What is lost: agents can no longer share a scratch key-value space or address a named
group, and the group verbs (`broadcast`, `dm`, `channel`) collapse into the one
`message` verb, which names a single session. A run keeps its shared state in the rows
it works and hands work over one message at a time, which is what the model already
asks for. The `tldr` collapse goes with its rule, so a long message now renders in
full rather than behind a one-line summary.

## Deliberately kept from upstream

- `/account` and `/accounts` - the multi-account picker for Claude and OpenAI.
- `grok-build` - independent of the account surface.
- The `jcode` provider id string, `jcode.sh` URLs, and the `_jcode` ACP
  capability: these name the *service*, not this binary, and removing them would
  break wire compatibility. The full list of names that stay is in
  `docs/dev/post-change.md` under Names.

## Why a removed feature has no doc of its own

A feature's own doc is deleted with the feature. `git log` is the archive, and a doc
set that describes features the code does not have teaches readers to distrust all of
it. This file is the exception, and this is why: it holds one entry per cut, saying
what the cut cost, because a price is not a feature description.
