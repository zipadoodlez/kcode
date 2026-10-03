# One flow

The row model (`plans/row-model.md`) is the first half of the destination: one file, one
item type, one close, and a run that is a session with a permission and an anchor. This
lane finishes it, so the tree has one flow of work and the machinery the old
orchestration needed is gone. Everything here is a row, a session, or something derived
from those two, so the whole design fits in one head.

## The model

Two nouns, two verbs.

- **A row** carries: words, kind, parent, blocked_by, holder, note, records. Seven facts.
- **A session** is an actor; it may or may not have a human attached.
- **A write** is the only mutation, in three shapes: add a row, update a row, close a row
  (delete it and leave its words on the row that owns it).
- **Ready** is the only question, computed on read: not blocked, no open child, held by
  me or nobody, in my scope, not already worked in this run.

Derived, never stored: ready, a run (the sessions under one anchor), membership (the
spawn edge), the coordinator (the anchor's holder), progress (open children plus
records), liveness (the holder's own status).

The loop decides the next turn in one place: is a human attached? then the human owns
the turn; do I hold a ready row I have not worked in this run? then take it; else sleep.
Nothing else starts work, and a write that makes a row ready is what wakes the loop.

The test for anything new: it is a write of a fact the model has, or a view computed on
read. A third noun, a second store, or a verb that is not "write" or "what is ready" is
the second system growing back.

## What exists today that this removes

Measured 2026-10-03, all bodies read.

- **Four schedulers** decide what runs next: the loop (`server/live_turn.rs`); `run_plan`'s
  driver (`tool/communicate.rs:431-1460`, ~1,030 lines with its own concurrency policy,
  200-loop cap, stall retries, credential breaker, cap recovery, utilization report and
  background card); `fill_slots`/`assign_next` (mini loops over the same assign request);
  and the turn-end auto-close (`server/comm_control.rs`).
- **Two dispatchers** pick the worker: the run status map and a 15 s TTL claim map
  (`auto_assign_claims`), because the plan write lands several awaits after the pick.
- **Two liveness predicates** disagree: `salvage_dead_assignees` waits out the reload
  grace, `next_dispatch` does not. The code carries a `braid:` note saying so.
- **Two waiters**: in-memory socket waiters, and a durable one
  (`server/await_members_state.rs`, 278 lines, persisted pending state and a startup
  resume), plus a notify/wake bus event.
- **Three artifact producers** write one form; **two report channels** carry one fact.
- **A KV store and topic channels** (in-memory, swarm-scoped), **four member shapes**,
  **a third event log** (the bounded swarm history), **five view handlers**
  (`comm_sync`), 607 lines of debug views, and a 3,100-line gallery rendering the same
  swarm state.

## Stages

### S1. The loop owns dispatch (row-model F1, refined)

Delete: the tool's driver with all of its policies, `fill_slots`, `assign_next`,
`assign_task`, `await_members`, `retry`, `wake`, `cleanup`, `stop`, the read views
(`status`, `summary`, `report`, `plan_status`, `read_context`, `resync_plan`), the
`report` action, and the instruction protocol around it (the spawn reminder, the
assignment suffix, the tldr rule, the notification advice text). Keep: `spawn` (root
only) and one message verb, whose address is the owner of a named row.

What replaces them: the loop's own dispatch (a holder write, plus the same wake a turn
end already uses when the holder is headless), readiness as the wait, the close as the
report, and the row's words as the handoff.

New code, one piece: a write hook that wakes a headless holder when a row becomes ready.
Everything else in this stage is deletion.

surface: −28 actions, −1 driver, −1 durable waiter store, −2 report channels, −1 KV,
−1 channel index, −3 artifact producers, −4 member shapes. lines ~−6,500 with tests.
risk: high; this stage is the lane's test.

Gate: one fan-out run in a scratch repo, end to end, on the loop alone. A root grants,
spawns two workers, hands them rows, the workers close, the root integrates and closes
the anchor. By hand, on the user's own socket, with the run's words in the transcript.
If it fails, stop and report; do not patch around it.

### S2. One view (row-model C5, A5)

The server sends each session the rows it may work (C5's answer, local sessions
included). The client renders one surface: the rows, with the holder named per row and
the holder's state (model, status, tokens) beside it. That surface absorbs the inline
card, the side-panel page, the pinned band, the info pips, and the gallery's
at-a-glance role. It is the one place this lane allows new code, because it absorbs the
others.

Delete: four renderers and their caches, 15 file reads, the gallery's separate member
rendering, the compacted member row cache. surface: −4 renderers, −4 caches, −3 protocol
member types, −1 item cache. lines ~−1,300. risk: med.

Gate: the four old surfaces gone; a live TUI check on a run with two workers, showing
progress, holder and status; the client suite.

### S3. One status (row-model A4)

Liveness derives from the holder's own session status plus the rule that a holder who
can never return releases its rows, through one `assignee_is_dead` used by both the
sweep and dispatch. The member projection and the runtime extras go.

Delete: `SwarmMemberRuntime.todo_items`, `SwarmTodoItem`, the two `update_active_todo_*`
helpers, `member_runtime_extras`' per-member list read, the four member shapes, the
third event log. The gallery's tool-intent display moves onto the holder's status.
surface: −3 protocol types, −4 shapes, −1 event log, −2 liveness predicates.
lines ~−1,000. risk: med.

### S4. The record is the words (row-model A7)

The artifact form goes; a record is `{id, result}`. The discipline lives in the close
instruction: state what proves it, what it showed, and what you did not check. Kinds
stay the run's word for the work, not six result schemas. surface: −1 type, −7 fields,
−2 tool vocabularies. lines ~−300. risk: med.

### S5. The file's fields

`status`, `priority`, `group`, `subsystem` and `file_scope` go (B3, C4), with the four
status vocabularies they keep alive. surface: −5 fields, −4 vocabularies. lines ~−500.
risk: med.

## The bound (decide)

Today two limits protect the machine: the hard constant `MAX_SWARM_MEMBERS = 1000` live
members per swarm, and a soft live-worker budget (`agents.swarm_max_concurrent_agents`,
32 by default) that the spawn path and `run_plan` both consult. `run_plan` adds its own
recovery dance when the cap is hit (free finished workers, retry, reuse-only, give up).

The proposal: exactly one software limit, an admission cap read where a spawn or a wake
is admitted (constant as the hard stop, config for the soft number), nothing on a row,
nothing in a grant. A run's work bound stays what it already is: no ready row ends it.
The recovery dance disappears, because the loop does not spawn past the cap. The
credential-wave breaker becomes a provider-health signal that fails a turn with a clear
error, not a dispatcher rule.

## What to verify before calling a stage done

- S1's scratch-repo fan-out, by hand, on the loop alone.
- A resume: a run interrupted between turns continues on the loop, with no stored run
  state anywhere.
- Two runs in one repo: each sees its own rows, and a row moved mid-turn is refused on
  close, as today.
- A dead holder: its rows return to the list, and one predicate decides it.
- A thin report: a close whose words say nothing is visible to the integrator.
- The gate: `check_guardrails.sh`, the touched crates' suites, `scripts/test.sh full`.

## Unread at the time of writing

`comm_session`'s spawn body (~150 of 1,264 lines read), `comm_sync`'s five handlers,
`comm_graph`'s seed and expand write paths (the complete path was read), the
`Comm*` arms in `client_lifecycle`, the channel handlers, the member-shape definitions
and the gallery's data adapter, `client_comm_message`, `todo.rs:330-470`, and the TUI's
render internals. The stages rest on the schedulers, waiters, dispatchers, artifact
producers and liveness predicates, all of which were read in full.
