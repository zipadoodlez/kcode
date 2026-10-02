# Testing

## One entry point

`scripts/test.sh` is the single way to run tests; it picks the feature profile,
the thread count, and per-suite timeouts for you.

```sh
scripts/test.sh                  # library + primary binary, minimal features
scripts/test.sh crate kcode-tui  # one crate while iterating
scripts/test.sh full             # lib-bins, provider-matrix, e2e; serial + timed
scripts/test.sh full --parallel  # same, Cargo's default parallelism
scripts/test.sh --last           # last recorded run for this tree, no rerun
```

Bare `cargo test` still works when you need a filter the script does not cover,
but prefer the script so a run matches how the suites are meant to be exercised.

`--last` never skips anything. It reports the last recorded cargo `test` action
for this repository (from the same log the wrapper writes) and whether this tree
still matches the one it ran on, so you can decide whether a rerun is worth it.

## Profiles: build each one once

`cargo check`, `cargo clippy` and `cargo test` build different profiles, so
alternating them rebuilds the tree. The commands that cover a change are:

- `cargo clippy -p <crate> --all-targets --all-features -- -D warnings` while
  iterating, because clippy compiles every target (a separate `cargo check` adds
  a second, mostly redundant compile);
- `scripts/test.sh` to run tests, which uses one profile for the whole run.

Avoid `cargo check` before the gate for this reason; the gate's clippy already
covers the compile errors it would find.

## Smoke, by hand, once per build

The suites cover the wiring they can reach; these are the ends they cannot, so a build
meant to be lived in gets them looked at once. Keep the list short: an item not worth
five seconds of looking is an item that will be skipped.

- Open a TUI on the build, no install needed (`./target/selfdev/kcode --socket <path>`
  serves its own daemon), and read `/info` against what the session holds: model,
  effort level, provider.
- Set a level (`/effort high`), quit, `--resume`, and read `/info` again. This is the
  hop no test crosses: a request that reaches the provider but never persists looks
  identical to one that worked.
- Spawn one worker and read its `/info`: it should show what the creator's session
  held, not the worker model's own default.
- Run one real turn against a provider, so the request path is exercised by something
  that is not a fixture.
- A `config.toml` carrying keys removed since the last build starts clean and sets
  nothing. A line that is silently ignored is worth noticing once per removal.

## Known flakiness: `kcode-tui` lib tests under parallel execution

`scripts/test.sh crate kcode-tui -- --test-threads=1` fails a handful of tests
per run at the default thread count, with a set that changes between runs. It is
a race on process-global state, not a logic bug: each failure passes in
isolation, and serial execution passes the whole suite. This is the last known
logic flake class; `AGENTS.md` carries the one-line version for a session that
just saw a failure.

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
reader. App-core had the same shape (a global bus, and env/config read at
dispatch) and is partly fixed. Making readers guard the same lock serializes the suite
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

## A test that fails on a clean tree

`kcode-tui`'s
`test_remote_fallback_provider_suggestions_normalize_bare_openai_openrouter_routes`
(`app/tests/state_model_poke_02/part_02.rs`) fails on a clean checkout, both
single-threaded and parallel, so a red run is not necessarily your change. It
asserts that provider suggestions include `/model openai/gpt-5.4@OpenAI`. The cause
is not diagnosed; it is tracked in `plans/test-tree.md`.

## A rare fork hang

`terminal_setup_command.rs`'s `decode_key_event_via_pty` forks the multithreaded
test process and runs crossterm in the child. Seen twice: the child wedges and the
parent blocks on its result pipe forever, so the run neither passes nor fails. It
needs a concurrent lock held at fork time and does not reproduce on demand (20+
clean runs). Spawning a fresh process instead of forking removes the class.

## Baselines

Single-threaded, every suite is at zero (2026-09-29). `kcode-tui --lib`
`test_remote_fallback_provider_suggestions_normalize_bare_openai_openrouter_routes`
was recorded here as a pre-existing failure on 2026-09-30; it is the ambient
`KCODE_*` read above, not drift, and passes with those unset. This section is the
list; keep it current here. For `kcode-tui`:

```sh
scripts/test.sh crate kcode-tui -- --test-threads=1
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
