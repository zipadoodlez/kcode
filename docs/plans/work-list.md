# Work list

Status: destination agreed 2026-10-01, work on branch `work-list`. Tick a step
when it lands and delete it; git has the history. `todo.md` points here.

## Destination

One list, one task type, one writer. A file in the repo holds the open work and
every session works from it starting at its first turn. An entry may record which
session holds it, and a swarm is the name for the case where more than one
session holds entries in one file, so there is no second container and no state
where a session is not working from a file. A person reads it and asks the model
to change it. The tool is the only writer.

Three lists exist today: `docs/todo.md`, the per-session todo JSON, and the swarm
plan, which the TUI already draws as todos (`info_widget_todos.rs`). The plan and
the todo tool now share one type, `TaskItem`; what is left is that the list itself
lives in three places.

## Rules

Settled 2026-10-01. Not steps; what every step has to satisfy.

1. **One file per repo, `tasks.jsonl` at the repo root, and it supersedes
   `docs/todo.md`.** The home is found from git, not from the working directory,
   so a session started in `crates/foo` reads the same list. Work with no repo
   uses the same basename in a scratch location tied to the session.
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
   one writer.
8. **One item type**, in `kcode-task-types`, the crate whose name is the concept.
9. **Every session works from a file from turn one.** There is one mode axis:
   the work. "Regular" is not a mode, it is a swarm with one member. Rigor stays
   the existing deep/light preset, so a solo session pays no gate or artifact
   cost.
10. **Hierarchy is one optional `parent` field on a flat list**, while blocking
    is order, so they are two fields. A parent is work: it owns the integration
    of its children, and its row is the integration reminder that keeps it in the
    file until that integration is done. `group` is deleted, because the parent
    chain is the grouping.
11. **Membership and the coordinator are derived, never stored.** The holder of a
    parent coordinates the holders of its children, which deletes the
    `coordinators` map and any swarm id.
12. **The durable/live seam.** Who holds an entry is durable and lives in the
    file. Whether that session is alive and busy is live server state that dies
    with the process. Today one member record welds the two.
13. **The file is JSON Lines, one task per line**, flat. Each entry is `id`,
    `content`, optional `parent`, optional `blocked_by`, optional `assigned_to`,
    optional `note`. `status` is derived, and `priority`, `group`, `subsystem`
    and `file_scope` are not stored. There is one row kind: a task whose job is
    to search is a row like any other, while a fog or out-of-scope note is
    doc-shaped, so it earns no row and no section. An out-of-scope finding is a
    result ("searched, and this is out of scope because ..."), which rule 5
    already records in the commit.
14. **The check is a rule, not a field.** The close action requires a nonempty
    result, and the tool description names the check and asks for its actual
    result. Nothing for the harness to judge. A skipped check shows only in the
    commit.
15. **Refer to a task by its words when talking to the user**, while the file
    keys on `id`, and resolve one decision per planning session.

Why the tier must not come back: a self-rubric cannot catch the failure it is
named after, a weak self-assessment must not drive the model onward, the poke is
opt-in (`features.auto_poke` defaults false), and only enforcement was removed
(the tool, `/poke`, Ctrl+P, the flag and the guardrail breaker were kept).

## Checklist

Widest shared shape first, so no step sweeps call sites a later step reshapes.
Every step lands whole, proven by the gate and, where behavior moves, one
`kcode run` probe against its own socket.

### B. The file is the list

- [ ] **(decide)** whether line order means anything, such as priority.
- [ ] **(decide)** where the standing decisions at the top of `docs/todo.md` go.
  The file is not prose any more, and those are boundaries rather than tasks, so
  they are not rows.
- [ ] **(decide)** who writes when the working directory is remote, since the
  server is the only writer and the file lives in the working tree.
- [ ] **(decide)** whether a claim is committed. The file is tracked, so claiming
  a row edits the working tree and leaves it dirty until the work lands; either
  the claim rides the work's commit, or the tree sits dirty mid-task.
- [ ] **(decide)** what happens to the per-session todo JSON's existing content
  when B1 stops reading it: imported once into the repo file, or dropped.
- [ ] **B1.** The `todo` tool reads and writes `tasks.jsonl` and its action set
  becomes add, claim, close: close requires a nonempty result and removes the row
  (rules 3 to 5), so `status` loses its writer and the poke counts open rows
  instead of incomplete statuses. This repo's `docs/todo.md` becomes the file's
  first content, `/todos` and the widget render from it, the per-session todo
  JSON is deleted, and the twelve references to `docs/todo.md` move to the new
  path (`README.md`, `docs/README.md` four times, `hooks.md`, `dev/testing.md`,
  `dev/benchmarking.md`, `dev/post-change.md`, the browser plan, one comment in
  `kcode-tui/src/tui/mod.rs`).
- [ ] **B2.** Drop `group` and `status` from the type. B1 gives `parent`, which
  is what `group` was grouping by, and the close action is what makes a
  completed row unrepresentable rather than stored.

Gated by the five decisions above.

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

- [ ] **G1.** Restore the two size ratchets in `scripts/check_guardrails.sh`,
  paused for this project, and re-baseline both with `--update`. Paused because
  a type merge and a file rewrite move lines between files faster than a
  per-commit baseline can follow, and the ratchet only tightens, so raising a
  baseline mid-project would leave a looser cap behind.

## Evidence (measured 2026-10-01)

- One type, `TaskItem` in `kcode-task-types`, behind the `todo` tool and the
  plan. It still carries four fields the list will not store: `status`, `group`,
  `subsystem`, and `file_scope`, dropped in B2 and C4 once their replacements
  exist.
- `SwarmState`: 67 references, 31 `SwarmState { .. }` rebuild sites.
- `tool/communicate.rs` 3369, `server/swarm.rs` 3146, `server/comm_control.rs`
  2640, `client_lifecycle.rs` 3584, `server/state.rs` 762 lines.
- `SwarmMember` is the sixth most connected node in the tree (180 edges), and the
  member projection is hand-written four times (`AgentInfo`, `SwarmMemberStatus`,
  `MemberStatic`, `SwarmMember`).
- Topic channels and the shared-context key-value store are documented as
  deprecated in `internals/swarm.md` and were never removed.
