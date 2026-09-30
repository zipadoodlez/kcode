# Work list

Status: destination agreed 2026-10-01, work on branch `work-list`. Tick a step
when it lands and delete it; git has the history. `todo.md` points here.

## Destination

One list, one task type, one writer. A file in the repo holds the open work,
every session works from it starting at its first turn, and a swarm is the name
for the case where more than one session holds entries in one file.

Three lists exist today: `docs/todo.md`, the per-session todo JSON, and the swarm
plan, which the TUI already draws as todos (`info_widget_todos.rs`). They now share
one type, `TaskItem`; what is left is that the list itself lives in three places.

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
   a decision or an exploration, after the result is written somewhere durable.
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
9. **Every session works from a file from turn one.** There is one mode axis:
   the work. "Regular" is not a mode, it is a swarm with one member. Rigor stays
   the existing deep/light preset, so a solo session pays no gate or artifact
   cost.
10. **Hierarchy is one optional `parent` field on a flat list**, while blocking
    is order, so they are two fields. A parent's row stays after its children are
    gone, because the integration is still owed. `group` is deleted, because the
    parent chain is the grouping.
11. **Membership and the coordinator are derived, never stored.** The holder of a
    parent coordinates the holders of its children, which deletes the
    `coordinators` map and any swarm id.
12. **The durable/live seam.** Who holds an entry is durable and lives in the
    file. Whether that session is alive and busy is live server state that dies
    with the process. Today one member record welds the two.
13. **The file is JSON Lines, one task per line**, flat: `id`, `content`,
    optional `parent`, `blocked_by`, `assigned_to`, `note`. A row is a task and
    the only row kind, so a note that is not a task is a doc. Nothing else is
    stored: not `status`, `priority`, `group`, `subsystem` or `file_scope`, and
    line order promises nothing. A row reads as open, blocked, or claimed from
    its own fields.
14. **The check is a rule, not a field.** The close action requires a nonempty
    result, and the tool description names the check and asks for its actual
    result. Nothing for the harness to judge. A skipped check shows only in the
    commit.
15. **Refer to a task by its words when talking to the user.** The file keys on
    `id`; the conversation does not.

Why the tier must not come back: a self-rubric cannot catch the failure it is
named after, a weak self-assessment must not drive the model onward, the poke is
opt-in (`features.auto_poke` defaults false), and only enforcement was removed
(the tool, `/poke`, Ctrl+P, the flag and the guardrail breaker were kept).

## Checklist

Widest shared shape first, so no step sweeps call sites a later step reshapes.
Every step lands whole, proven by the gate and, where behavior moves, one
`kcode run` probe against its own socket.

### B. The file is the list

- [ ] **B1.** The `todo` tool's action set becomes add, claim, close: close
  requires a nonempty result and removes the row (rules 3 to 5), so `status`
  loses its writer and the poke counts open rows.
- [ ] **B2.** This repo's own list migrates: `docs/todo.md` becomes the first
  content of `tasks.jsonl`, its standing decisions move to
  `docs/what-was-removed.md`, and the twelve references follow.
- [ ] **B3.** Drop `group` and `status` from the type, once the file's `parent`
  field and the close action exist: `parent` is what `group` was grouping by, and
  the close action is what makes a completed row unrepresentable rather than
  stored.

### C. The swarm reads and writes the file

- [ ] **(decide)** what a running swarm shows in the file while a node is in
  flight, and what a stalled node shows.
- [ ] **(decide)** where a running swarm's finished-but-not-yet-integrated nodes
  live. The file deletes a row when its result is durable, but a deep gate must
  name every done node in its scope by id, and a swarm that resumes after a
  restart needs them too. Either the live graph is the gate's input and the
  persisted plan carries those nodes until the swarm closes, which is two durable
  stores while a swarm runs, or the gate reads something else.
- [ ] **C1.** The plan is derived from the file's entries and their `parent` and
  `blocked_by` fields, membership and the coordinator are computed from it, and
  the parent guard (rule 6) becomes the tool's check.
- [ ] **C2.** Delete `NodeKind`, the `coordinators` map, any stored swarm id, and
  the 31 `SwarmState { .. }` rebuilds, because the state gets one owner.
- [ ] **C3.** The two sub-items currently in `todo.md` §1: member appearance
  follows the typed status, and a stalled plan node is visible.
- [ ] **C4.** Move `subsystem` and `file_scope` off the shared type into the
  plan's own per-item state. They are the scheduler's inputs (assignment
  affinity matches them against a worker's metadata), not list fields, and
  `SwarmPlanItemSpec` already carries them on the plan side.

- [ ] **C5.** The client renders the list from server events instead of reading
  the file itself. Today it resolves the repo from its own working directory,
  which is the same thing for a local session and the wrong repo for a remote
  attach.

Gated by B.

### D. Delete what the file makes redundant

- [ ] **D1.** The topic channels and the shared-context key-value store, the
  removal `internals/swarm.md` recorded as pending and never did, plus any member
  projection the derived membership replaces.

Gated by C.

### E. The server shape

- [ ] **E1.** `ClientContext` for `handle_client`'s 28 arguments.
- [ ] **E2.** Fold swarm ownership in, which is gated on C.
- [ ] **E3.** Name the prologue, move the largest arms out, and settle the
  turn-lifecycle locals. `todo.md` §2's H1 to H5 in order.

### F. `tool/communicate.rs`

- [ ] **F1.** Split the 3369 lines along the seams it already has: swarm
  coordination, the run-plan driver, capacity cleanup, and the formatters around
  `execute`. Cheaper once C has given the state one owner.

### G. Close out

- [ ] **G1.** Restore the two size ratchets in `scripts/check_guardrails.sh` and
  re-baseline both with `--update`. They are paused, with the reason at the call
  site.

## Evidence (measured 2026-10-01)

- One type, `TaskItem` in `kcode-task-types`, behind the `todo` tool and the
  plan. It still carries four fields the list will not store: `status`, `group`,
  `subsystem`, and `file_scope`, dropped in B2 and C4 once their replacements
  exist.
- `SwarmState`: 67 references, 31 `SwarmState { .. }` rebuild sites. Then
  `communicate.rs` 3369, `swarm.rs` 3146, `comm_control.rs` 2640,
  `client_lifecycle.rs` 3584, `state.rs` 762 lines.
- `SwarmMember` is the sixth most connected node in the tree (180 edges), and the
  member projection is hand-written four times (`AgentInfo`, `SwarmMemberStatus`,
  `MemberStatic`, `SwarmMember`).
- Topic channels and the shared-context key-value store are documented as
  deprecated in `internals/swarm.md` and were never removed.
