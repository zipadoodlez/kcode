# Repository Guidelines

## Development Workflow

- **Use the user's Git identity** - Create commits with the configured
  `user.name` and `user.email`. Do not override them with `Jcode`, `Jcode agent`,
  or a fabricated agent email. Preserve existing contributor attribution when
  integrating work. If no identity is configured, ask rather than inventing one.
- **Welcome pull requests from everyone** - Review contributions on their merits,
  regardless of whether the author is a maintainer, an existing contributor, a
  first-time contributor, or an agent. Good PRs can be merged directly after review
  and validation. Do not require a maintainer-authored rewrite merely because of
  who submitted the change. See `CONTRIBUTING.md` for the contribution policy.
- **Keep work scoped** - Work on your own branch and preserve unrelated work. When
  the user asks you to review or integrate a PR or branch, you may inspect, test,
  and integrate that contribution regardless of author status. Do not pull in
  unrelated branches or merge a PR without user authorization.

## Install Notes
- jcode does not install, update, or repoint itself. The OS package manager owns
  the installed binary, and there are no `~/.jcode/builds` version channels,
  launcher symlinks, or self-dev build machinery any more. Older installs may
  still have a `~/.jcode/builds` tree; it is inert.
- `~/.local/bin/jcode` is simply whatever binary you or your package manager put
  on `PATH`.

## Verifying a change at runtime

`cargo build` alone proves nothing about behavior. `jcode run` and interactive
sessions are served by a long-lived daemon that is the installed binary, so a
freshly built binary is inert until you install it and restart the daemon.

To test a change without disturbing the shared daemon or the caller's session,
run your build against its own socket:

```bash
cargo build --profile selfdev
./target/selfdev/jcode run --socket /run/user/1000/jcode-mytest.sock '<prompt>'
```

Two things that waste time otherwise:

- `crate::logging::info` writes to a log file, not stderr, so instrumenting a
  code path with it produces no visible output under `--trace`. Use `eprintln!`
  for throwaway diagnostics and delete it before committing.
- Confirm which binary you are actually inspecting. If a path is a symlink,
  resolve it with `readlink -f` before running `strings` on it.
