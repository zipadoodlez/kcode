# kcode docs

Documentation for kcode, a lean fork of [jcode](https://github.com/1jehuang/jcode).

## Layout

- `user/` - how to use kcode: install, commands, configuration, providers.
- `internals/` - how kcode works: architecture, subsystems, protocols.
- `dev/` - contributor process: testing, benchmarking, dependency hygiene.
- `plans/` - the work lanes: one plan per subject, each holding its own task
  checklist with the design beside the item. Not part of the bundled corpus.
- `what-was-removed.md` - the ledger of cuts: every deliberate removal and what it
  cost.

## Conventions

- **kebab-case filenames.** Case-sensitive checkouts make `SCREAMING_CASE.md`
  links a recurring source of 404s.
- **A doc may only say what the code cannot tell you.** If a reader can get it
  from the tree with a search, it does not belong here: state the why, the trap,
  the contract, the measurement, or the recipe, and nothing else. A fact that is
  absent cannot go stale. If a doc and the code disagree, the doc is wrong.
- **One home per fact.** An open item is one checkbox in exactly one lane, with its
  design beside it, and a gate between two lanes is named once, in the item that
  waits. Nothing is summarized into a second list and nothing is stated twice, so no
  two places can disagree about what is left to do.
- **Two homes, split by tense.** `plans/` holds what is still to do; a landed decision
  is a record, and a record is not work, so it lives in `what-was-removed.md`. A
  deferred internal note is neither: it lives in the code as a
  `# braid: <ceiling>, <what surpasses it>` comment, next to what it is about.
- **One lane per subject, and its tasks do not touch each other.** A lane is a
  `plans/*.md` file, so several can be in flight at once; a task that must wait names
  its gate. A design too large to sit beside its item - a full protocol or interface
  spec for something not yet built - is a lane of its own, since `build.rs` keeps
  `plans/` out of the bundled `kcode_docs` corpus and the model never reads it as a
  description of the tree. Superseded items are deleted, not archived: git has the
  history. An item marked `(decision)` waits on a call from the maintainer; the rest
  are actionable.
- **Prefer one good doc over three thin ones.** If two docs would share a
  header structure, they are one doc.
- **No marketing.** State what a thing does and what it costs.

## Start here

- Install and usage: [`../README.md`](../README.md)
- The work: one lane per subject under [`plans/`](plans/), with
  [`plans/row-model.md`](plans/row-model.md) as the destination.

## The map

Every doc, what it is for, and what belongs in it. This is the only place that
lists docs: adding or removing one means editing this table, and a doc that does
not fit its row is a doc in the wrong place.

| doc | purpose | holds |
|---|---|---|
| `what-was-removed.md` | every deliberate removal and what it cost | the fork cut, the account cut, the model cut, and what was deliberately kept |
| `plans/row-model.md` | the destination lane: the row model | the model, its rules, its steps and where the work stands; not served to the model |
| `plans/one-flow.md` | the destination's second half: one flow of work | the one-page model, the duplication this lane removes, its stages, and the bound call; not served to the model |
| `plans/app-shape.md` | the app's shape | the `App` re-core's tasks, the command surface, the two calls |
| `plans/server-shape.md` | the request path's shape | the `handle_client` split (H1-H5) and the `communicate.rs` condense |
| `plans/test-tree.md` | the test tree | its reshape tasks, and the tests red on a clean tree |
| `plans/hygiene.md` | the loose ends | config warnings, hook observability, palette and clock consistency, tool-description cost, the crate spine, and the two process calls |
| `plans/browser-provider-protocol.md` | a design for work not yet built | the protocol, and the one call to build it; not served to the model |
| `user/cli.md` | the non-interactive surface | flags a wrapper needs and their traps |
| `user/tui.md` | terminal input, picker, panels | input mechanics and their terminal quirks |
| `user/config.md` | settings, env vars, the system prompt | layer order and precedence rules |
| `user/hooks.md` | lifecycle hooks | the contract and runnable examples |
| `user/providers.md` | picking and wiring a provider | per-backend setup and gotchas |
| `user/auth.md` | where credentials live | the two-path model, traps, the import sources |
| `user/ssh.md` | remote attach | the security and disconnect contract, verification |
| `internals/architecture.md` | the runtime shape | server/client/session model, paths, lifecycle |
| `internals/swarm.md` | coordinated agents | the DAG, decomposition, member semantics; one executor of the work list |
| `internals/browser.md` | the browser tool | what it does and what is not live evidence |
| `internals/websocket.md` | OpenAI WebSocket transport | the wire contract with the peer |
| `internals/soft-interrupt.md` | interrupting a turn | the stop semantics |
| `internals/rendering.md` | colors and markdown | roles, and terminal compatibility |
| `internals/memory.md` | the memory subsystem | budget and the incident runbook |
| `internals/usage.md` | usage and statistics | what the ledgers mean |
| `dev/post-change.md` | what to do after a change | the gate, the graph, the naming rules |
| `dev/testing.md` | running the suites | the failure classes and their causes |
| `dev/benchmarking.md` | measuring | recipes and the measured budgets |
| `dev/dependencies.md` | advisories | why each ignore exists and what unblocks it |
| `dev/message-voice.md` | user-facing strings | the voice rules |
| `dev/todo-rework-ab-probe.md` | A/B recipe | how to compare two shas |
