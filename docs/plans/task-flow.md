# Task flow

This is the one plan for the task flow: the model, the rules the code cites, the
duplication left to remove, and the stages that remove it. Subjects with another owner
keep their own doc — the
app's shape in `plans/app-shape.md`, the request path in `plans/server-shape.md`, the
test tree in `plans/test-tree.md`, the loose ends in `plans/hygiene.md` — and losses are
named in `docs/what-was-removed.md`.

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
  committed; the full gate runs once, when the step is done. Tick a step when it lands
  and delete it: the plan holds open work, the commit holds the history, and a step
  that dropped power is recorded in `docs/what-was-removed.md`. The doc line rides the
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

Two nouns and two verbs. A write is the only mutation, in three shapes: add a row,
update a row, close a row. Ready is the only question, asked on read: not blocked, no
open child, held by me or nobody, in my scope, not already worked in this run.

A **run** is a session working the rows it holds. A swarm is a count, not a mode:
one member and eight are the same code. Membership, the coordinator, liveness, the
ready set and progress (open children plus records) are derived, never stored.

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

A member's session outlives its turn on purpose (decided 2026-10-04): a transcript, about
9 KB in the first fan-out run, buys the next row without a spawn plus the member's own
memory of the last one. Ending a member as soon as it holds no open row was declined,
since it would pay a fresh spawn on every second row for a saving that small, and forking
the summoner's history into a child was declined too, since a fresh member already shares
the cached prefix and a fork would carry the summoner's wrong turns for the child's whole
life.

Having a human is derived from live state that already exists, a client attached to
the session, and it is read in exactly one place, the loop's decision about who
supplies the next turn. Stored nowhere, and read anywhere else, it becomes the mode
boundary this model deleted.

Controlling a session you are not sitting in is attaching to it, and the server
already does the flip: a client attaching sets that member's `is_headless` to false,
so the run stops supplying turns and starts offering them, and detaching puts it
back. A run has two levers over itself and no more: **message it**, which hands a row to its
holder, and **stop it**, which ends it. `wake`, `retry`, `reassign` and `replace` were the
loop's job done by hand, and reading a run is reading the rows.

A person pulls those levers by sitting down: attaching to a member makes it a session with a
human, the client's interrupt takes the turn it was running, and that session then writes rows
or stops the run. So the verb is the run's, called by its root, and the person's door is the
attach they already have: no new verb, and no second client request.

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

The loop decides the next turn in one place: is a human attached? then the human owns
the turn; do I hold a ready row I have not worked in this run? then take it; else sleep.
Nothing else starts work, and a write that makes a row ready is what wakes the loop.

The test for anything new: it is a write of a fact the model has, or a view computed on
read. A third noun, a second store, or a verb that is not "write" or "what is ready" is
the second system growing back.

## Rules

These are the model the steps drive the tree to, not the tree. Where a rule and
the type disagree today, the type is behind: `TaskItem` still carries `status`,
`priority` and `group`, and S5 removes them.

1. **One file per repo**, `tasks.jsonl` at the root, found from git so a session in
   `crates/foo` reads the same list; outside a repo, a session scratch location.
2. **One writer: the store's read-modify-write.** `kcode-base`'s `todo` store owns the
   write protocol and re-reads the file before every write, so a human edit is an input,
   not a conflict; the `todo` tool and the loop's anchor adoption are its callers. A
   claim rides the work's commit.
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
   `group`, and line order promises nothing. Position
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

## The duplication this removes

Measured 2026-10-03, all bodies read, and re-read after S1 landed. S1 removed the
dispatch duplication (four schedulers, two dispatchers, two waiters, two disagreeing
liveness predicates), the three artifact producers with the report action, and the
channel index with the shared-context store; those cuts are recorded in
`docs/what-was-removed.md`. What remains:

- **Four renderers over one list**: the inline card, the side-panel page, the pinned
  band and the info widget's pips each render the rows, over four caches (three
  hashes in `todos_view.rs`, a 1s TTL cache with a refresh thread, and the
  transcript's previous-list parse). `commands_improve.rs` repeats the same load six
  times, and `turn_notify.rs`, `state_ui.rs` and `remote/key_handling.rs` read the
  file again.
- **A third copy of a session's rows** (`SwarmMemberRuntime.todo_items`, folded from
  `TodoEvent`s), **four member shapes**, **a third event log** (the bounded swarm
  history), 607 lines of debug views, and a 3,100-line gallery rendering the same
  swarm state.
- **Four status vocabularies**, and the three fields the file does not need
  (`status`, `priority`, `group`).
- **The `SwarmState` pair**, rebuilt as a literal at each request arm and threaded to
  ~40 functions.

The tool's action surface is now four actions (spawn, stop, list_models, message).

## Steps

Every step lands whole, proven by the gate. The one `kcode run` probe against its own
socket waits for the end of the list, where the user runs it, so no step is gated on it.
Widest shared shape first, so no step sweeps call sites a later step reshapes: S2 opens
the client's path and S3-S5 then delete what it leaves behind. The numbers label the
stages, they are not an order: S3 waits on S2, which delivers the rows it renders, and S5
waits on S3 too (measured 2026-10-04): removing `TaskItem.status` strands
`dispatch_swarm_todo_progress`/`compact_todo_items` and `SwarmMemberRuntime.todo_items`,
which is S3's deletion and is gated by S2, so S5 cannot land first. D2 waits on nothing. The `app.rs` re-core (`plans/app-shape.md`) is the tail and it is
droppable: nothing here depends on it.

### S2. One view

The server sends each session the rows it may work, local sessions included. The client
renders one surface: the rows, with the holder named per row and the holder's state
(model, status, tokens) beside it. That surface absorbs the inline card, the side-panel
page, the pinned band, the info pips, and the gallery's at-a-glance role. It is the one
place this lane allows new code, because it absorbs the others.

A run's rows already cross as `ServerEvent::SwarmPlan`
(`wire.rs:837`, produced at `server/swarm.rs:836` and `:894`, applied at
`remote/server_events.rs:1995`), and the full rows already cross the bus on every write
(`BusEvent::TodoUpdated { session_id, todos }`, `tool/todo.rs:307`), which the server
only folds into the compacted member cache (`server.rs:2065`). What is missing is the arm
that forwards that bus event to the owning session's clients, and the client rendering
its surfaces from it. The view is the rows this session may work, computed once on the
server: a session in a run gets the run's rows (`swarm_rows`, `server/swarm.rs:12`); a
session outside a run gets the whole list. `Action::List` takes the same rule, which is
what ends its whole-file read (`tool/todo.rs:64` over `load_tasks` at `:303`).

The render is A5. The inline card, the side-panel page (its own comment says "legacy"),
the pinned band and the info widget's pips each render the list, and the client keeps four
caches over one file: three hashes in `todos_view.rs`, a 1s TTL cache with a refresh
thread (`helpers.rs:969-...`), and the transcript's previous-list parse in
`ui_todo_changes.rs`. `commands_improve.rs` repeats the same load six times, and
`turn_notify.rs:220`, `state_ui.rs:1811` and `remote/key_handling.rs` (four sites) read it
again. Keep one read and one model with the renderers as pure functions.

Delete: four renderers and their caches, 15 file reads, the gallery's separate member
rendering, the compacted member row cache. surface: −4 renderers, −4 caches, −3 protocol
member types, −1 item cache. lines ~−1,300. risk: med.

**Landed 2026-10-04 (first half).** The server is the one reader: on every write and on
subscribe it sends each session the rows it may work, and the pinned band renders them
with its own vocabulary (id, indentation by `parent`, a glyph and color from the row plus
its holder's live status, the holder's name). Deleted: the pinned band's file read, its
1-second cache and its refresh thread, the plan broadcast, `rows_with_run_status`,
`failed_reasons_for`, `member_details` and the plan's derived summary. Kept for the second
half: the card, the side-panel page, the info widget's pips and the separate roster.

**Smoke test (the second half is not landed, so check this on what is).** Build
`scripts/dev_cargo.sh build --profile selfdev -p kcode --bin kcode`, then in a scratch repo
with a `tasks.jsonl`:

1. Add rows with the `todo` tool: two root rows, one child naming a root as `parent`, and
   one row blocked by another. The pinned list at the top should show `<id> <words>` per
   row, the child indented under its parent, and the blocked row marked. Fixing the list
   should need no command: it is always on.
2. Run `/auto <words>` (or spawn a worker by hand) so a member takes a row. That row's
   glyph should animate and its holder's name (`@name`) should appear beside it.
3. Stop the holder mid-row (`swarm stop`). Its row should go red.
4. Close a blocker with the tool; the blocked row should lose its mark on that write.

A hand edit of `tasks.jsonl` is read on the next write, not pushed: the list shows the
last write, and the next write re-reads the file first (rule 2).

Gate: the four old surfaces gone; a live TUI check on a run with two workers, showing
progress, holder and status; the client suite.

### S3. One status

Liveness derives from the holder's own session status plus the rule that a holder who can
never return releases its rows, through one `assignee_is_dead` used by both the sweep and
dispatch. The member projection and the runtime extras go.

Delete (A4): `SwarmMemberRuntime.todo_items` (`protocol/src/lib.rs:450`, the third copy
of a session's rows after the file and the run's sparse status, folded from `TodoEvent`s
by `compact_todo_items` and the two `update_active_todo_*` helpers
(`server/background_tasks.rs:362-505`) and mapped again into `GalleryTodo` for the
gallery), `SwarmTodoItem`, `SwarmToolIntent`, the two `update_active_todo_*` helpers, the
TUI mapping (`info_widget_swarm_gallery.rs:98`), `member_runtime_extras`' per-member list
read (`comm_sync.rs:133-140`, a `completed/total` counter the forwarded bus event already
carries), the four member shapes, and the third event log. `tool_intents` is not a row
fact: it is the gallery's "which tool is this worker running" display, nested under the
compacted item by `update_active_todo_tool`, so it moves onto `SwarmMemberStatus` rather
than dying with the cache. Gated by S2; the gallery is the only consumer.

surface: −3 protocol types, −4 shapes, −1 event log, −2 liveness predicates.
lines ~−1,000. risk: med.

The projection becomes one type, so member appearance follows the typed status:
`kcode-tui-render` takes `SwarmLifecycleStatus` (a dependency on the data-only
`kcode-session-types`), so the string matches in `swarm_gallery.rs` and
`is_active_status` become enum matches; one module owns accent, glyph, label and sort
rank, with an `is_working` predicate kept distinct from lifecycle `is_active` so a
stalled node does not spin; the duplicate map
`info_widget_swarm_background::swarm_status_style` goes; and the swarm-path `Color::Rgb`
literals become `kcode_tui_style` role accessors so `/colors` can recolor them (the small
default shift is accepted). The projection's `RunningStale` goes with it: 0.4f removed
its only producer (a plan item's status), so the variant is unreachable.

### S5. The file's fields

**`status`, `priority` and `group` leave the type** (B3), with the four status
vocabularies they keep alive: five status helpers in `kcode-plan/src/lib.rs`,
`canonical_todo_status` plus its two wrappers in `kcode-base/src/todo.rs`, and
`normalize_plan_status_for_todo`, `status_badge` and a second `priority_rank` in the TUI.

Dropping `status` is the larger half, because the plan classifies by it everywhere:
`summarize_plan_graph`, `completed_item_ids`, `is_active_status`, `newly_ready_item_ids`
(read by the swarm path), the task-control actions and `status_from_plan`. `status_to_plan`
is already gone: 0.4f's s12 stopped lowering the engine's statuses back into items, so the
engine's own statuses no longer reach a row. Afterwards a row is ready when `blocked_by`
is empty and liveness comes from the member, not the item.

surface: −3 fields, −4 vocabularies. lines ~−500. risk: med.

### D2. The swarm/comm condense (gates `plans/server-shape.md` H2)

The `SwarmState` handle pair is still threaded to ~40 functions and rebuilt as a literal
at each request arm, so the request context that ends it is the remaining half. Give the
swarm state its one owner and delete the pair. Engine: `internals/swarm.md`. This is the
the only work here that `plans/server-shape.md`'s H1-H5 carry: H2 (the swarm state in the
client context) waits on it. (A1 measured 19 `SwarmState { .. }` literals and 244
`swarm_runs` references on the settled tree.)

### Close out

- **G1.** Restore the two size ratchets in `scripts/check_guardrails.sh` and re-baseline
  both with `--update`. They are paused, with the reason at the call site.
- **G2.** The one live `kcode run` probe against its own socket, in a scratch repo, run by
  the user: the permission (a granted turn continues, a wake does not) and the poke's
  removal (no client continuation; every non-retryable error gets the short budget). It is
  the step's landing proof, so it runs when the list is done, not per step.

### What to verify when the list is done

Each stage's own gate is in its section. These are the properties that outlive a stage.

- A resume: a run interrupted between turns continues on the loop, with no stored run
  state anywhere.
- Two runs in one repo: each sees its own rows, and a row moved mid-turn is refused on
  close, as today.
- A dead holder: its rows return to the list, and one predicate decides it.
- A thin report: a close whose words say nothing is visible to the integrator.
- The gate: `check_guardrails.sh`, the touched crates' suites, `scripts/test.sh full`.

### Unread at the time of writing

Still un-read, and the open stages rest on them: the TUI's render internals and the
gallery's data adapter (S2, S3), the four member-shape definitions (S3),
`todo.rs:330-470` (S5), and the `Comm*` arms in `client_lifecycle` (D2). The
schedulers, waiters, dispatchers, artifact producers and liveness predicates were read in
full before the loop's stage landed.
