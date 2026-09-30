# Work list

Status: destination agreed 2026-10-01. The roadmap is not written yet. This
absorbs the todo rework that landed 2026-09-30; the reason the enforcement tier
was removed is kept below, and git has the rest of that history.

## Destination

One list, one task type, one writer. A file in the repo holds the open work, the
`todo` tool shows the slice in play, and every session works from its repo's file
starting with its first turn. An entry may record which session holds it. A swarm
is the name for the case where more than one session holds entries in one file,
so there is no second container and no state where a session is not working from
a file.

The list is the contract the swarm and the solo session both serve. It is not
hand-authored: the human reads it and asks the model to change it.

## What it replaces

Three lists exist today.

- `docs/todo.md`: the real one, edited by the model with its text tool.
- The per-session todo file (JSON, one per session), written by the `todo` tool.
- The swarm plan (`kcode-plan`'s `PlanItem`), server-owned, drawn as todos by
  the TUI (`info_widget_todos.rs`).

The plan and the todo item are the same five fields (`content`, `status`, `id`,
`priority`, `blocked_by` where present, `assigned_to`). `kcode-plan/src/lib.rs`
carries a comment claiming they are "intentionally separate: plan data is shared
at the server/swarm level, while todos remain session-local". That premise is
what this project deletes.

## Why the tier must not come back

The `todo` tool used to enforce. It stored a plan with goals, a per-item
confidence score, and a per-item check, scored them at turn end, and queued a
hidden continuation telling the model to keep going and not to ask the user.
That was deleted 2026-09-30, and the reasons are not rediscoverable from the
code:

- **A self-rubric cannot catch the failure it is named after.** The model scores
  its own understanding of the request, and the same misunderstanding that drops
  a requirement rates the understanding as clear.
- **A weak self-assessment must not drive the model onward.** The wanted
  behavior is that the model may stop and ask, and that the weakness is visible
  to the user rather than acted on by the harness.
- **The poke is opt-in.** `features.auto_poke` defaults to false, and that does
  not change here.
- **Capability was fixed, only enforcement went.** The tool, `/poke`, Ctrl+P,
  the flag, and the guardrail breaker were all kept on purpose.

The one idea that survived is now a line in the tool description: name the check
that proves the item is done, and report its actual result. That is the seed of
the open question above about whether a row carries its check.

## Settled

1. **The file supersedes `docs/todo.md`** and is the single home for open work,
   one per repo. Work with no repo gets the same format in a scratch location
   tied to the session.
2. **The list is never hand-authored.** The tool owns the write protocol, and it
   re-reads the file before each write, so a human edit is an input rather than a
   conflict.
3. **Completed rows are deleted.** There is no completed state, so a done item
   cannot drift in the file. The durable record of what was done is the commit.
4. **A row is removed when its result is durable.** For code that is the commit
   that lands the work, and the delete rides in the same commit. For a decision
   or an exploration, the result is written somewhere durable first.
5. **The server is the only writer.** Agents claim a row by setting
   `assigned_to` before any work, so an unclaimed row is free. Many claimers,
   one writer.
6. **One item type.** `PlanItem` and `TodoItem` collapse; the surviving type
   lives in the shared data crate, beside `SwarmMemberRecord`.
7. **A swarm is the only kind of executor, one member by default.** More members
   when the work warrants; rigor stays the existing deep/light preset, so a solo
   session pays no gate or artifact cost.
8. **The file must be readable and diff-stable.** It does not have to be pleasant
   to hand-edit, because it is never hand-authored.
9. **Every session works from a file from turn one.** The file has entries, not
   members. An entry may record which session holds it, and that claim is the
   only thing that makes a session a member. There is no solo path and no swarm
   path, because a swarm is just the case where more than one session holds
   entries in the same file. Today's solo path re-derives what the swarm path
   owns, and that duplication is most of the state work in `todo.md` §1.

The seam that falls out: who holds an entry is durable and lives in the file,
while whether that session is alive and busy is live server state that dies with
the process. Today one member record welds the two, and most of the four
duplicated member shapes come from that weld.
10. **Hierarchy is one optional `parent` field on a flat list**, one entry per
    line. Parenting means containment and blocking means order, so they are two
    different fields. A parent is work: it owns the integration of its children,
    which is what makes "two sessions on two subtasks" a swarm, and its row is
    the integration reminder that keeps it in the file. `group` is deleted,
    because the parent chain is the grouping.
11. **Membership and the coordinator are derived from the parent chain, never
    stored.** The holder of a parent coordinates the holders of its children.
    That deletes the `coordinators` map and any swarm id, since a swarm exists
    only while two sessions hold entries under one parent.
12. **No new language.** The file reuses the JSON the tree already reads and
    writes. Nesting is stored flat and rendered as a tree by the tool.
13. **Closing is one operation, for every entry.** The holder produces the result
    the entry owes and the tool removes the row. A parent's result is its
    children's results integrated, a leaf's is its own work, and nothing else
    differs. There is no composite/atomic kind: having children is read from the
    `parent` field, so the plan's `NodeKind` is a second copy of that fact and
    goes. The single enforcement is that a parent's row cannot be removed while a
    row still names it as parent, which needs no status tracking.
14. **A drop is a completion whose result says so.** One close action, one
    precondition: a nonempty result naming what happened and why. Neither case
    shows in the file, because both remove the row. The distinction survives
    where results live: the commit for durable work, and the running plan's
    artifacts for a live gate. Consequence: `cancelled` stops being a state, so
    the four-word status vocabulary and its helpers (`canonical_todo_status`,
    `todo_status_is_completed`, `todo_status_is_cancelled`) go with it. Cost,
    deliberate: a cold session sees a drop only in the commit.

## Open

- The exact fields. Proposed minimum: `content`, `id`, `blocked_by`,
  `assigned_to`, and one note line ("where I got to"). `status` is derivable
  (open, blocked, or claimed), and `priority`, `group`, `subsystem`, and
  `file_scope` have no reader yet.
- Whether a row carries the check that proves it done, or only the commit does.
- Where the file lives and what it is called. `docs/todo.md` is the incumbent.
- "Where does work land" (the `(decision)` item in `todo.md`: `main` directly, or
  a branch) becomes a prerequisite: a session on a stale branch reads a stale
  list.
- Whether a supertask's row stays when its last child is removed. The rule
  above says it stays, because the integration is still owed.
- Whether line order in the file means anything, such as priority.
- What a running swarm shows in the file while a node is in flight, and what a
  stalled node shows.

## Harvest from wayfinder (Matt Pocock)

Worth taking:

- **Claim by assignment.** Set `assigned_to` before work starts; the assignee is
  the claim. Concurrent sessions skip claimed rows with no lock.
- **A "not yet specified" section.** Work you can see coming but cannot yet
  phrase does not become a row. `docs/todo.md` already half-has this.
- **An "out of scope" section.** Scope decisions stop living as rows.
- **Refer to a task by its name, never a bare id.**
- **One decision per session.** A session that is planning resolves exactly one
  open question rather than half-resolving several.

## Surface (measured 2026-10-01)

- `PlanItem` and `TodoItem`: identical five fields, plus `subsystem` and
  `file_scope` on one, `group` on the other.
- `SwarmState`: 67 references, 31 `SwarmState { .. }` rebuild sites.
- `tool/communicate.rs` 3369, `server/swarm.rs` 3146, `server/comm_control.rs`
  2640, `client_lifecycle.rs` 3584, `server/state.rs` 762 lines.
- `SwarmMember` is the sixth most connected node in the tree (180 edges).
- Member projection is hand-written four times (`AgentInfo`,
  `SwarmMemberStatus`, `MemberStatic`, `SwarmMember`).
- Topic channels and the shared-context key-value store are documented as
  deprecated in `internals/swarm.md` and were never removed.

## Roadmap

Not written yet. Next.
