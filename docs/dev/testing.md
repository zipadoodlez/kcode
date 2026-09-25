# Testing

## Running the suites

`cargo test` runs the workspace. Target individual crates while iterating:
`cargo test -p jcode-tui --lib`, `cargo test -p jcode-app-core --lib`, and so
on. The onboarding state-space invariants are also a guardrail gate; see
[../internals/onboarding.md](../internals/onboarding.md).

## Known flakiness: `jcode-tui` lib tests under parallel execution

`cargo test -p jcode-tui --lib` fails 1-4 tests per run at the default thread
count, with a set that changes between runs. It is a parallelism race on
process-global render state, not a logic bug: each test passes in isolation, and
`--test-threads=1` passes the whole suite.

Root cause: `create_test_app()` (and `create_named_provider_test_app`) in
`crates/jcode-tui/src/tui/app/tests/support_failover/part_01.rs` calls
`clear_test_render_state_for_tests`, which wipes process-global flicker history,
layout snapshots, status-area snapshots, copy targets, and scroll positions.
Rendering tests guard that state with `render_state_test_lock()`, but
`create_test_app` clears it *without* the lock, so any of its ~810 call sites can
reset a concurrently running render test mid-assertion.

Taking the lock inside `create_test_app` fixes it but serializes all ~810 call
sites (suite runtime ~12s to >10 minutes), so it was measured and reverted. The
fix is to stop sharing the state: make render state thread-local (production has
one render thread, so behavior is unchanged), or have `create_test_app` skip the
clear entirely after auditing which tests rely on it. A `--test-threads=1` run is
the workaround until then.

If a run fails after a `cargo` SIGTERM under memory pressure, that is a different
failure (the compiler was killed), not this race.

## Testing onboarding locally

Onboarding is easiest to iterate with an isolated sandbox, so repeated runs never
touch real auth state. `scripts/onboarding_sandbox.sh` roots state under
`JCODE_HOME` and `JCODE_RUNTIME_DIR`, so no real config, sockets, or trusted
external-auth imports are reused, and one `reset` throws it all away.

```sh
scripts/onboarding_sandbox.sh fresh                  # clean isolated launch
scripts/onboarding_sandbox.sh reset                  # blank onboarding state
scripts/onboarding_sandbox.sh seed-real-logins       # copy real external logins in
scripts/onboarding_sandbox.sh login openai           # log in without touching normal config
scripts/onboarding_sandbox.sh kcode auth status      # run any kcode command in the sandbox
```

Because a fresh sandbox has nothing to import, `seed-real-logins` copies your
real external credential files into `$JCODE_HOME/external/<same relative path>`
(and with `--with-transcripts`, your Codex/Claude transcripts), so detection and
import behave as on a first-run machine that already has those tools. The copies
are real tokens, so the sandbox stays local-only; your original `$HOME` files are
never moved, rewritten, or deleted.

### Auth fixtures

Repeated login testing can skip the browser: put a sandbox into an interesting
state (logged in, expired token, approved import) and save it.

```sh
scripts/onboarding_sandbox.sh fixture-save normal-openai
scripts/onboarding_sandbox.sh fixture-load normal-openai
scripts/auth_fixture.sh list          # lower-level helper
```

The fixture store defaults to `.tmp/auth-fixtures` (local developer state) and may
hold real tokens, so do not commit or share it. Overrides:
`JCODE_ONBOARDING_SANDBOX`, `JCODE_ONBOARDING_DIR`, `JCODE_AUTH_FIXTURE_DIR`.

### Headless screenshots

`scripts/capture_onboarding.sh` renders the same `OnboardingFlow` phases and
ratatui widget tree into an offscreen `TestBackend`, writing SVG (and PNG when
`rsvg-convert` is present) for every resting state in `onboarding_graph.rs`. It
never launches a terminal or reads real credentials.
