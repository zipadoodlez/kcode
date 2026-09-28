# Repository Guidelines

## Development Workflow

- **Use the user's Git identity** - Create commits with the configured
  `user.name` and `user.email`. Do not override them with `Kcode`, `Kcode agent`,
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
- **Do not "fix" the remaining `jcode` names** - some of them deliberately refer
  to the upstream project or to the wire service, and the list is in
  `docs/dev/post-change.md` under Names. Renaming those breaks compatibility.
- **Run the local gate before committing** - this fork has no CI.
  `scripts/check_guardrails.sh` (format, compile, clippy, lock, guardrail checks)
  must pass. See `docs/dev/post-change.md`.
- **Keep the knowledge graph current** - `graphify-out/` is a local cache and is
  not tracked: it is derived, and its content depends on the environment, so a
  committed copy would be neither reproducible nor current. Build it once per
  clone with `graphify update .`; the committed `.githooks/` hook then rebuilds
  it after each commit (incremental, no API key; no-op when graphify is absent).
  `scripts/setup_git_hooks.sh` points git at the hooks once per clone. For
  codebase questions prefer `graphify query "<question>"` over grepping raw
  files. See `docs/dev/post-change.md`.

## Install Notes
- kcode does not install, update, or repoint itself. The OS package manager owns
  the installed binary, and there are no `~/.kcode/builds` version channels,
  launcher symlinks, or self-dev build machinery any more. Older installs may
  still have a `~/.kcode/builds` tree; it is inert.
- `~/.local/bin/kcode` is simply whatever binary you or your package manager put
  on `PATH`.

## Verifying a change at runtime

`cargo build` alone proves nothing about behavior. `kcode run` and interactive
sessions are served by a long-lived daemon that is the installed binary, so a
freshly built binary is inert until you install it and restart the daemon.

To test a change without disturbing the shared daemon or the caller's session,
run your build against its own socket:

```bash
cargo build --profile selfdev
./target/selfdev/kcode run --socket /run/user/1000/kcode-mytest.sock '<prompt>'
```

Two things that waste time otherwise:

- `crate::logging::info` writes to a log file, not stderr, so instrumenting a
  code path with it produces no visible output under `--trace`. Use `eprintln!`
  for throwaway diagnostics and delete it before committing.
- Confirm which binary you are actually inspecting. If a path is a symlink,
  resolve it with `readlink -f` before running `strings` on it.
