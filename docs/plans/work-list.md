# Work list

Status: destination agreed 2026-10-01 on branch `work-list`; the roadmap is
written and stage A is next. This absorbs the todo rework that landed
2026-09-30, and git has that history.

## Destination

One list, one task type, one writer. A file in the repo holds the open work and
every session works from it starting at its first turn. An entry may record which
session holds it, and a swarm is the name for the case where more than one
session holds entries in one file, so there is no second container and no state
where a session is not working from a file.

A person reads the file and asks the model to change it. The tool is the only
writer.

## What it replaces

Three lists exist today.

- `docs/todo.md`: the real one, edited by the model with its text tool.
- The per-session todo file (JSON, one per session), written by the `todo` tool.
- The swarm plan (`kcode-plan`'s `PlanItem`), server-owned, drawn as todos by the
  TUI (`info_widget_todos.rs`).

`kcode-plan/src/lib.rs` says the plan and the todo are "intentionally separate:
plan data is shared at the server/swarm level, while todos remain session-local".
That premise is what this project deletes.

## Why the tier must not come back

The `todo` tool used to enforce: it stored goals, a per-item confidence score and
a per-item check, scored them at turn end, and queued a hidden continuation
telling the model to keep going. That was deleted 2026-09-30, for reasons the
code cannot tell you:

- A self-rubric cannot catch the failure it is named after. The same
  misunderstanding that drops a requirement rates the understanding as clear.
- A weak self-assessment must not drive the model onward. The model may stop and
  ask, and the weakness belongs in its own message, not in a harness that acts on
  it.
- The poke is opt-in (`features.auto_poke` defaults to false) and stays opt-in.
- Capability was fixed, only enforcement went: the tool, `/poke`, Ctrl+P, the
  flag, and the guardrail breaker were all kept on purpose.

## Settled

1. **One file per repo, and it supersedes `docs/todo.md`.** Work with no repo
   uses the same format in a scratch location tied to the session.
2. **Never hand-authored.** The tool owns the write protocol and re-reads the
   file before each write, so a human edit is an input, not a conflict.
3. **One close action, for every entry.** The holder produces the result the
   entry owes and the row is removed. A parent's result is its children's
   results integrated, a leaf's is its own work, and nothing else differs.
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
   `PlanItem` and `TodoItem` collapse into it, which deletes the plan's copy and
   the comment that claims the two are separate.
9. **Every session works from a file from turn one.** There is one mode axis:
   the work. "Regular" is not a mode, it is a swarm with one member, and a swarm
   is only the case where more than one session holds entries in one file. Rigor
   stays the existing deep/light preset, so a solo session pays no gate or
   artifact cost.
10. **Hierarchy is one optional `parent` field on a flat list**, while blocking
    is order, so they are two fields. A parent is work: it owns the integration
    of its children, which is what makes two sessions on two subtasks a swarm,
    and its row is the integration reminder. `group` is deleted, because the
    parent chain is the grouping.
11. **Membership and the coordinator are derived, never stored.** The holder of a
    parent coordinates the holders of its children, which deletes the
    `coordinators` map and any swarm id.
12. **The durable/live seam.** Who holds an entry is durable and lives in the
    file. Whether that session is alive and busy is live server state that dies
    with the process. Today one member record welds the two, and most of the four
    duplicated member shapes come from that weld.
13. **The file is JSON Lines, one task per line**, flat. Each entry is `id`,
    `content`, optional `parent`, optional `blocked_by`, optional `assigned_to`,
    optional `note`. `status` is derived, and `priority`, `group`, `subsystem`
    and `file_scope` are not stored.

## Open

- Whether a row carries the check that proves it done, or only the commit does.
  This is the one surviving idea of the deleted rubric: name the check, report
  its result.
- Whether the file grows a "not yet specified" section and an "out of scope"
  section, both from wayfinder. `docs/todo.md` already half-has the first.
- Where the file lives and what it is called. `docs/todo.md` is the incumbent.
- "Where does work land" (`main` directly, or a branch: the `(decision)` item in
  `todo.md`) is now a prerequisite, because a session on a stale branch reads a
  stale list.
- Who writes when the working directory is remote, since the server is the only
  writer and the file lives in the working tree.
- Whether a supertask's row stays when its last child is removed. Decision 10
  says it stays, because the integration is still owed.
- Whether line order means anything, such as priority.
- What a running swarm shows in the file while a node is in flight, and what a
  stalled node shows.
- Two wayfinder habits worth adopting: refer to a task by its name, and resolve
  one decision per planning session.

## Surface (measured 2026-10-01)

- `PlanItem` and `TodoItem`: the same five fields, plus `subsystem` and
  `file_scope` on one, `group` on the other.
- `SwarmState`: 67 references, 31 `SwarmState { .. }` rebuild sites.
- `tool/communicate.rs` 3369, `server/swarm.rs` 3146, `server/comm_control.rs`
  2640, `client_lifecycle.rs` 3584, `server/state.rs` 762 lines.
- `SwarmMember` is the sixth most connected node in the tree (180 edges), and the
  member projection is hand-written four times (`AgentInfo`, `SwarmMemberStatus`,
  `MemberStatic`, `SwarmMember`).
- Topic channels and the shared-context key-value store are documented as
  deprecated in `internals/swarm.md` and were never removed.

## Roadmap

Widest shared shape first, so no stage sweeps call sites a later stage reshapes.
Each stage lands whole, proven by the gate and, where behavior moves, one
`kcode run` probe against its own socket.

**Prerequisite, decided 2026-10-01: work lands on the `work-list` branch**, and
a stage merges back when it lands. The file lives in the repo, so a session on a
stale branch reads a stale list; the `todo.md` Anytime item's session-start line
when `main..HEAD` is non-zero is what keeps that visible.

**A. One durable item type.** `PlanItem` and `TodoItem` merge into
`kcode-task-types`, and live execution status leaves the type for runtime state
keyed by entry id, which is the durable/live seam (12). Two landings: the merge
with no behavior change, then the field drops (`group`, `subsystem`,
`file_scope`, and the unused status vocabulary). Deletes the plan's item copy and
three status helpers. The fields are settled (13), so this is unblocked.

**B. The file is the list.** The `todo` tool reads and writes the repo file,
`docs/todo.md` becomes its first user, and `/todos` and the widget render from it.
Deletes the per-session todo JSON. Gated by the file's name and home, and by
whether a row carries its check.

**C. The swarm reads and writes the file.** The plan is derived from the file's
entries and their `parent` and `blocked_by` fields, membership and the coordinator
are computed from it, and the parent guard becomes the tool's check. Deletes
`NodeKind`, the `coordinators` map, any stored swarm id, and the 31
`SwarmState { .. }` rebuilds, because the state gets one owner. This is `todo.md`
§1, and it absorbs that item's two sub-items: member appearance follows the typed
status, and a stalled plan node is visible. Gated by A and B.

**D. Delete what the file makes redundant.** The topic channels and the
shared-context key-value store, the removal `internals/swarm.md` recorded as
pending and never did, plus any member projection the derived membership replaces.

**E. The server shape.** `ClientContext` for `handle_client`'s 28 arguments,
folding swarm ownership in, naming the prologue, moving the largest arms out, and
the turn-lifecycle locals. This is `todo.md` §2's H1 to H5 in order, and H2 is
gated on C.

**F. `tool/communicate.rs`.** Split the 3369 lines along the seams it already has
(swarm coordination, the run-plan driver, capacity cleanup, and the formatters
around `execute`), which is cheaper once C has given the state one owner.

Order is not payoff order. A is invisible and unblocks B and C; C is the largest
and the only one that needs the swarm running to prove; F is a mechanical split
that gets cheaper the longer it waits.
