# kcode docs

Documentation for kcode, a lean fork of [jcode](https://github.com/1jehuang/jcode).

## Layout

- `user/` - how to use kcode: install, commands, configuration, providers.
- `internals/` - how kcode works: architecture, subsystems, protocols.
- `plans/` - work we intend to do. Ideas we have committed to, not history.
- `dev/` - contributor process: testing, benchmarking, dependency hygiene.

## Conventions

- **kebab-case filenames.** Case-sensitive checkouts make `SCREAMING_CASE.md`
  links a recurring source of 404s.
- **`user/` and `internals/` describe what the code does today.** If a doc and
  the code disagree, the doc is wrong.
- **`plans/` describes intent.** Anything not yet built lives there, with the
  problem it solves and how we would know it worked. Superseded plans are
  deleted, not archived: git has the history.
- **Prefer one good doc over three thin ones.** If two docs would share a
  header structure, they are one doc.
- **No marketing.** State what a thing does and what it costs.

## Entry points

- Install: [`../README.md`](../README.md) (`## Install`; packaging is not written yet)
- Command surface: `user/cli.md`, `user/tui.md`
- Configuration and system prompt: `user/config.md`
- Hooks and terminal routing: `user/hooks.md`
- Providers and credentials: `user/providers.md`, `user/auth.md`
- Remote attach over SSH: `user/ssh.md`
- Architecture: `internals/architecture.md`
- Swarm: `internals/swarm.md`
- OpenAI WebSocket transport: `internals/websocket.md`
- Soft interrupt: `internals/soft-interrupt.md`
- Rendering: `internals/rendering.md`
- Server memory: `internals/memory.md`
- Usage accounting: `internals/usage.md`
- What the fork removed: `what-was-removed.md`
- What is still in flight: `wip.md`
