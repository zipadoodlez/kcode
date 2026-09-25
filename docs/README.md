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

- Install and first run: `user/install.md`
- Command surface: `user/cli.md`, `user/tui.md`
- Architecture: `internals/architecture.md`
- Swarm: `internals/swarm.md`
- Providers and auth: `user/providers.md`, `user/auth.md`
- What the fork removed: `what-was-removed.md`
