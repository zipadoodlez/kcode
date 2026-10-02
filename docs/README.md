# kcode docs

Documentation for kcode, a lean fork of [jcode](https://github.com/1jehuang/jcode).

## Layout

- `user/` - how to use kcode: install, commands, configuration, providers.
- `internals/` - how kcode works: architecture, subsystems, protocols.
- `dev/` - contributor process: testing, benchmarking, dependency hygiene.
- `plans/` - designs for work not yet built. Not part of the bundled corpus.
- `todo.md` - outstanding work: the checklist of open items, with each item's design
  inline.
- `what-was-removed.md` - the ledger of cuts: every deliberate removal and what it
  cost.

## Conventions

- **kebab-case filenames.** Case-sensitive checkouts make `SCREAMING_CASE.md`
  links a recurring source of 404s.
- **A doc may only say what the code cannot tell you.** If a reader can get it
  from the tree with a search, it does not belong here: state the why, the trap,
  the contract, the measurement, or the recipe, and nothing else. A fact that is
  absent cannot go stale. If a doc and the code disagree, the doc is wrong.
- **Two homes, split by tense.** `todo.md` holds what is still to do: open work and
  open calls, in the order that unblocks the most. `what-was-removed.md` holds what a
  cut cost: a decision that landed is a record, and a record is not work. A deferred
  internal note is neither: it lives in the code as a `# braid: <ceiling>, <what
  surpasses it>` comment, next to what it is about.
- **The design lives with its item.** Inline when it is short, in the doc the item
  links to when it is not: a design too large to inline - a full protocol or
  interface spec for something not yet built - goes in `plans/`, which `build.rs`
  keeps out of the bundled `kcode_docs` corpus so the model never reads it as a
  description of the tree. Superseded items are deleted, not archived: git has the
  history. An item marked `(decision)` waits on a call from the maintainer; the rest
  are actionable.
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
| `todo.md` | the order of work | open items with their design, and open calls; a landed record is not here |
| `what-was-removed.md` | every deliberate removal and what it cost | the fork cut, the account cut, the model cut, and what was deliberately kept |
| `plans/row-model.md` | the destination for the work list | one file of rows, one item type, one writer, one run; a swarm is a count, not a mode; not served to the model |
| `plans/app-shape.md` | the app's shape | the `App` re-core's groups, the command surface, the two calls |
| `plans/server-shape.md` | the request path's shape | the `handle_client` split (H1-H5) and the `communicate.rs` condense |
| `plans/test-tree.md` | the test tree | its reshape items, and the tests red on a clean tree |
| `plans/browser-provider-protocol.md` | a design for work not yet built | the protocol; not served to the model |
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
