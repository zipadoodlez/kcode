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
- **`user/` and `internals/` describe what the code does today.** If a doc and
  the code disagree, the doc is wrong.
- **`todo.md` is the one home for outstanding work.** The checklist item and its
  design live in the same entry, so finishing something is one edit in one file.
  Superseded items are deleted, not archived: git has the history. A design too
  large to inline there - a full protocol or interface spec for something not
  yet built - goes in `plans/`, which `build.rs` keeps out of the bundled
  `kcode_docs` corpus so the model never reads it as a description of the tree.
- **Prefer one good doc over three thin ones.** If two docs would share a
  header structure, they are one doc.
- **No marketing.** State what a thing does and what it costs.

## Entry points

- Order of work: [`todo.md`](todo.md)
- Install: [`../README.md`](../README.md) (`## Install`; packaging is not written yet)
- Command surface: `user/cli.md`, `user/tui.md`
- Configuration and system prompt: `user/config.md`
- Hooks and terminal routing: `user/hooks.md`
- Providers and credentials: `user/providers.md`, `user/auth.md`
- Remote attach over SSH: `user/ssh.md`
- Architecture: `internals/architecture.md`
- Swarm: `internals/swarm.md`
- Browser: `internals/browser.md`
- OpenAI WebSocket transport: `internals/websocket.md`
- Soft interrupt: `internals/soft-interrupt.md`
- Rendering: `internals/rendering.md`
- Server memory: `internals/memory.md`
- Usage and statistics: `internals/usage.md`
- Contributor process: `dev/post-change.md`, `dev/testing.md`, `dev/benchmarking.md`, `dev/dependencies.md`
- Knowledge graph: `graphify-out/` (local untracked cache; build once with `graphify update .`, the committed git hook refreshes it after each commit; see `dev/post-change.md`)
- Message voice (user-facing strings): `dev/message-voice.md`
- What the fork removed: `what-was-removed.md`
- What is still in flight: `todo.md`
