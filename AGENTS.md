# Repository Guidelines

## Development Workflow

- **Start at `docs/todo.md`** - It is the order of work, and the current project's
  destination, model and open steps are `docs/plans/work-list.md`. Read the plan's
  Steps intro before changing anything there: it says where the work stands, and a
  step in flight is finished before a new one starts. `docs/README.md` lists every doc
  and what belongs in it.
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
- **This fork tracks no upstream** - there is no rebase lane and nothing to fetch
  from `jcode`; the ratchets measure this tree's drift.
- **No provider is deleted** - every cut candidate backs a user-selectable
  provider, so cleanup there is identity unification, not removal.
- **Line count is not a goal** - a smaller tree is a side effect of better
  placement, never a target; the census that closed the question is in git.
- **Do not "fix" the remaining `jcode` names** - some of them deliberately refer
  to the upstream project or to the wire service, and the list is in
  `docs/dev/post-change.md` under Names. Renaming those breaks compatibility.
- **Gate the change, not the commit** - this fork has no CI, so
  `scripts/check_guardrails.sh` is the only check. Run the whole script when the
  change can affect the build (Rust code, `Cargo.toml`/`Cargo.lock`, anything the
  build reads) and once before a push. A docs-, comment-, or markdown-only change
  needs no gate at all. For a small Rust change, check it narrowly first -
  `cargo fmt --all --check`, the ratchet covering what you touched, and
  `cargo clippy -p <crate> --all-targets --all-features -- -D warnings` - and
  save the full run for the push. Keep `--all-targets --all-features` on the
  narrow run: dropping them can pass where the full gate fails. Run tests
  through `scripts/test.sh` (`crate <name>` while iterating, `full` for the
  suites; see `docs/dev/testing.md`). Do not add test
  suites beyond what the change can affect. The suite is at zero single-threaded;
  at the default thread count about a dozen tests flake on process-global
  `KCODE_*` configuration, so re-run a failure with `--test-threads=1` before
  believing it. See `docs/dev/post-change.md` and `docs/dev/testing.md`.
- **Keep the knowledge graph current** - `graphify-out/` is a local cache and is
  not tracked: it is derived, and its content depends on the environment, so a
  committed copy would be neither reproducible nor current. Build it once per
  clone with `graphify update .`; the committed `.githooks/` hook then rebuilds
  it after each commit (incremental, no API key; no-op when graphify is absent).
  `scripts/setup_git_hooks.sh` points git at the hooks once per clone. For
  codebase questions prefer `graphify query "<question>"` over grepping raw
  files. Search with `kgrep`: shell `rg`/`grep`/`find` are ruled out on purpose,
  and `rg` is absent, so a shell search returns empty and reads as no matches.
  See `docs/dev/post-change.md`.

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
