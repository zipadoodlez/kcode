# Testing

## Running the suites

`cargo test` runs the workspace. Target individual crates while iterating:
`cargo test -p kcode-tui --lib`, `cargo test -p kcode-app-core --lib`, and so
on.

## Known flakiness: `kcode-tui` lib tests under parallel execution

`cargo test -p kcode-tui --lib` fails a handful of tests per run at the default
thread count, with a set that changes between runs. It is a race on
process-global state, not a logic bug: each failure passes in isolation, and
`--test-threads=1` passes the whole suite. This is the last known flake class;
`../todo.md` tracks it.

Root cause: tests read configuration from process-global sources that other
tests mutate concurrently. The live channels are:

- **`KCODE_HOME`.** `create_test_app()` installs a per-process test home, and
  `with_temp_kcode_home()` swaps in a tempdir for one test's duration. Session
  save/restore and the config cache resolve through whichever value is in the
  environment at the moment they run, so a test that saves a session can restore
  from a home another test just switched to.
- **`KCODE_SSH_REMOTE` and the other `KCODE_SSH_*` / `KCODE_MODEL` /
  `KCODE_PROVIDER` / `KCODE_RUNTIME_PROVIDER` variables.** `tui::is_ssh_remote()`
  is read at dispatch time, so a concurrent test that set `KCODE_SSH_REMOTE`
  flips `/resume`, `/model`, and `/terminal-setup` into remote mode for
  everyone. Reproduce with
  `KCODE_SSH_REMOTE=test-remote <test-binary> <test> --exact`.
- **The ambient environment.** `KCODE_OPENROUTER_*` and other `KCODE_*`
  variables in the shell are read as configuration too, so a run started from a
  kcode shell can fail tests that pass under a clean environment.

Env-mutating tests hold `storage::lock_test_env()` for their whole body, but the
readers never take it, so the exclusion buys nothing against a concurrent
reader. Making readers guard the same lock serializes the suite
(`create_test_app` alone is ~810 call sites; measured at >10 minutes) and can
deadlock with worker threads that read the same accessors, so it is not a
drop-in fix. The workarounds are `--test-threads=1` for a full run and a clean
environment; the durable fix is to stop routing test configuration through the
process environment.

The old render-lock story no longer applies: `clear_test_render_state_for_tests`
takes `render_state_test_lock` reentrantly (via `with_render_state_lock`), and
the frame-metrics singletons were ruled out as the cause. `smoothness_benchmark_*`
is timing-sensitive and can still report a stray blink under load.

If a run fails after a `cargo` SIGTERM under memory pressure, that is a different
failure (the compiler was killed), not this race.

## Baselines

Single-threaded, every suite is at zero (2026-09-29) except one pre-existing
failure, `kcode-tui --lib`
`test_remote_fallback_provider_suggestions_normalize_bare_openai_openrouter_routes`
(found 2026-09-30, fails at `4ef8c3e1` too). `../todo.md` holds the list to keep
current. For `kcode-tui`:

```sh
cargo test -p kcode-tui --lib -- --test-threads=1
```

## Auth fixtures

Repeated login testing can skip the browser: put an isolated sandbox into an
interesting state (logged in, expired token, approved import) and save it.

```sh
scripts/auth_fixture.sh save normal-openai
scripts/auth_fixture.sh load normal-openai
scripts/auth_fixture.sh list
scripts/auth_fixture.sh run normal-openai -- auth status
```

`scripts/auth_fixture.sh` roots state under `KCODE_HOME` and `KCODE_RUNTIME_DIR`
inside its own directory, so no real config, sockets, or credentials are reused.
The store defaults to `.tmp/auth-fixtures` (local developer state) and may hold
real tokens, so do not commit or share it. Overrides:
`KCODE_AUTH_FIXTURE_SANDBOX`, `KCODE_AUTH_FIXTURE_SANDBOX_DIR`,
`KCODE_AUTH_FIXTURE_DIR`.
