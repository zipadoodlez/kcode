# kcode docs

Documentation for kcode, a lean fork of [jcode](https://github.com/1jehuang/jcode).

## Layout

- `user/` - how to use kcode: install, commands, configuration, providers.
- `internals/` - how kcode works: architecture, subsystems, protocols.
- `dev/` - contributor process: testing, benchmarking, dependency hygiene.
- `plans/` - designs for work not yet built. Not part of the bundled corpus.
- `todo.md` - outstanding work: the checklist, with each item's design inline.

## Conventions

- **kebab-case filenames.** Case-sensitive checkouts make `SCREAMING_CASE.md`
  links a recurring source of 404s.
- **A doc may only say what the code cannot tell you.** If a reader can get it
  from the tree with a search, it does not belong here: state the why, the trap,
  the contract, the measurement, or the recipe, and nothing else. A fact that is
  absent cannot go stale. If a doc and the code disagree, the doc is wrong.
- **`todo.md` is the one home for outstanding work.** The checklist item and its
  design live in the same entry, so finishing something is one edit in one file.
  Superseded items are deleted, not archived: git has the history. A design too
  large to inline there - a full protocol or interface spec for something not
  yet built - goes in `plans/`, which `build.rs` keeps out of the bundled
  `kcode_docs` corpus so the model never reads it as a description of the tree.
- **Prefer one good doc over three thin ones.** If two docs would share a
  header structure, they are one doc.
- **No marketing.** State what a thing does and what it costs.

## Start here

- Install and usage: [`../README.md`](../README.md)
- Order of work: [`todo.md`](todo.md)

## The map

Every doc, what it is for, and what belongs in it. This is the only place that
lists docs: adding or removing one means editing this table, and a doc that does
not fit its row is a doc in the wrong place.

| doc | purpose | holds |
|---|---|---|
| `todo.md` | the order of work | open items with their design; landed items deleted |
| `what-was-removed.md` | why this fork looks nothing like upstream | the two cuts and what was deliberately kept |
| `plans/work-list.md` | the destination for the work list | one list, one task type, one writer; the swarm as one executor; not served to the model |
| `plans/browser-provider-protocol.md` | a design for work not yet built | the protocol; not served to the model |
| `user/cli.md` | the non-interactive surface | flags a wrapper needs and their traps |
| `user/tui.md` | terminal input, picker, panels | input mechanics and their terminal quirks |
| `user/config.md` | settings, env vars, the system prompt | layer order and precedence rules |
| `user/hooks.md` | lifecycle hooks | the contract and runnable examples |
| `user/providers.md` | picking and wiring a provider | per-backend setup and gotchas |
| `user/auth.md` | where credentials live | the two-path model, traps, the import sources |
| `user/ssh.md` | remote attach | the security and disconnect contract, verification |
| `internals/architecture.md` | the runtime shape | server/client/session model, paths, lifecycle |
| `internals/swarm.md` | coordinated agents | the DAG, gates, member semantics; one executor of the work list |
| `internals/browser.md` | the browser tool | what it does and what is not live evidence |
| `internals/websocket.md` | OpenAI WebSocket transport | the wire contract with the peer |
| `internals/soft-interrupt.md` | interrupting a turn | the stop semantics |
| `internals/rendering.md` | colors and markdown | roles, and terminal compatibility |
| `internals/memory.md` | the memory subsystem | budget and the incident runbook |
| `internals/usage.md` | usage and statistics | what the ledgers mean |
| `internals/todo-enforcement-removal.md` | why the tier went | decisions, rejected alternatives, the landed record; the final state moved to `plans/work-list.md` |
| `dev/post-change.md` | what to do after a change | the gate, the graph, the naming rules |
| `dev/testing.md` | running the suites | the failure classes and their causes |
| `dev/benchmarking.md` | measuring | recipes and the measured budgets |
| `dev/dependencies.md` | advisories | why each ignore exists and what unblocks it |
| `dev/message-voice.md` | user-facing strings | the voice rules |
| `dev/todo-rework-ab-probe.md` | A/B recipe | how to compare two shas |
