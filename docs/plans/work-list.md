# Work list

Status: destination agreed 2026-10-01. The roadmap is not written yet. This
supersedes the "Desired final state" section of
`internals/todo-enforcement-removal.md`, which scoped the same idea to the todo
feature alone.

## Destination

One list, one task type, one writer. A file in the repo holds the open work, the
`todo` tool shows the slice in play, and a swarm is the only kind of executor,
with one member by default.

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
