# The test tree

The tree is coupled through `create_test_app`, so shape changes are paid for in test
churn: it comes after the shape work in the tree proper. This doc also holds the tests
that are red on a clean tree, because until those are explained a red run cannot be
trusted. Every open item of this subject is one checkbox below, with its design in the
item; each names the measurement it is judged by, and the counts are re-taken at the
step, not read from here.

## Tasks

The reshape items come after the app and server lanes, because the tree is coupled
through `create_test_app`. The two diagnoses are independent of everything.

- [ ] **Real modules instead of `include!`.** `app/tests.rs` `include!`s 55 files into
 one module (116 `include!` sites repo-wide), which is why helper collisions and
 `use super::*` are everywhere.
- [ ] **Condense the near-duplicate tables.** `state_model_poke_03.rs`,
 `session_tests/cases.rs`, `remote_events_reload_04.rs`. The ~40-51% figures predate
 both the 2026-09-28 census (13 cross-file duplicated test names) and the work-list
 plan's landed deletions, so re-measure before treating them as targets.
- [ ] **Move subsystem code out of test files.** `smoothness_benchmark.rs` (313 lines)
 sits under `app/tests/`. Separately, `kcode-base/src/live_tests.rs` (3,080 lines) is a
 misnamed *production* module (`pub mod live_tests`, consumed by the TUI's `/live`
 report); rename it to what it is.
- [ ] **One home per duplicated test helper.** 13 names are defined in more than one
 file (`lock_env` 8, `test_agent` 7, `create_test_app` 3, then `tracked_env_vars`,
 `clear_openai_compatible_runtime_env`, `empty_swarm_status_state`,
 `available_models_display_seeds_from_persisted_catalog`, and the rest at 2).
 `ensure_test_kcode_home_if_unset` is gone: its three copies and 25 call sites
 were removed in favour of kcode-base's process-wide install (`test_env.rs`).
 Re-measure before ranking.
- [ ] **Diagnose the provider-suggestion failure.** `kcode-tui`'s
 `test_remote_fallback_provider_suggestions_normalize_bare_openai_openrouter_routes`
 fails at the default thread count and single-threaded. Cause not diagnosed; it asserts
 that provider suggestions include `/model openai/gpt-5.4@OpenAI`. `dev/testing.md`
 carries the note for a session that just saw it.
- [ ] **Diagnose the three `session_flow` e2e failures** (2026-10-02, seen while
 landing 0.4f): `test_debug_create_session_marks_debug`,
 `test_debug_create_selfdev_session_marks_canary` and
 `test_clear_preserves_debug_for_resumed_debug_session`, each dying with a bare
 `No such file or directory (os error 2)` after `create_session` over the debug socket.
 Verified pre-existing: all three fail identically with the s12 change stashed, at the
 commit before it. The suite's other session tests pass, including one that calls the
 same `Session::load`, so the failure is specific to a session the *debug command*
 created. Start at what `create_session` persists versus what `Session::load` reads.
 Re-checked after 0.4g's g1-g3 (2026-10-02): the same three, unchanged, and the plan's
 own suites pass around them.
- [ ] **Diagnose the four `kcode-provider-openai-runtime` failures** (2026-10-02, seen
 while landing 0.5): `persistent_terminal_public_stream_ends`,
 `persistent_terminal_public_next_call_not_stalled`,
 `persistent_terminal_public_missing_previous_full_replay`, and
 `websocket_v2_prewarm_is_adopted_by_complete_without_losing_request_state`. Verified
 pre-existing: the same four fail with the 0.5 change stashed, at the commit before it
 (129 passed, 4 failed either way). They bind a local socket and drive the prewarm and
 persistent-terminal paths, so start at whether they need a live account or an
 advertised model catalog. Re-run while gating 0.5 (2026-10-03, single-threaded, the
 same four, 129 passed either way): the panic is `set_model` rejecting `gpt-5.6-sol`
 (`openai_provider_impl.rs:753`), and that id **is** in `ALL_OPENAI_MODELS`, so the
 static fallback is not what is missing. `known_openai_model_ids()` takes
 `cached_openai_model_ids()` first, so a catalog read that returns an empty list
 shadows the static one; start at which test seeds that empty catalog for the account
 scope.
- [ ] **`kcode-provider-anthropic-runtime`'s
 `tests::anthropic_fallback_honors_server_recommendation` is red** (2026-10-02, seen
 while landing 0.5): 57 pass, this one fails, alone and in-crate, single-threaded too.
 It asserts that a 404 body's prose recommendation ("please use Opus 4.8") maps to
 `claude-opus-4-8`, and the mapping scores `known_anthropic_model_ids()`, so it depends
 on the process-global model catalog that the test itself resets; no effort path is
 involved, and the 0.5 change touches only Anthropic's constructor effort seed. Not
 proven pre-existing: reverting that one file alone does not compile (the config field
 it read is gone workspace-wide), so the clean check is a worktree at `d927b4ae`, the
 commit before it (`cargo test -p kcode-provider-anthropic-runtime --lib
 anthropic_fallback_honors_server_recommendation`). Settle it there before blaming the
 catalog.
- [ ] **Stop the session tests writing into the real store.** `client_session_tests`
 and its `resume/` and `reload` includes call `Session::save()` and `mark_active()`
 with only `KCODE_RUNTIME_DIR` redirected, so `kcode_dir()` resolves to the
 developer's real `~/.kcode/sessions`. Their orphaned `active_pids` markers are then
 swept as crashes on the next TUI start, and because `save()` re-stamps `updated_at`
 the picker sorts the leftovers to the top of `/resume`. The sibling
 `client_lifecycle_tests` already routes through `super::tests::configure_test_env`
 (`KCODE_HOME` + `KCODE_RUNTIME_DIR` + `KCODE_SOCKET`); these tests should too, or
 `setup_runtime_dir` should carry the home. Its durable fix is the same as "One home
 per duplicated test helper" above: a single app-core test-home guard that every
 session-saving test goes through. Measured by the count of `~/.kcode/sessions/*.json`
 whose `working_dir` sits under this repo after a suite run (2026-10-02: 2,027 of
 2,537 session files, 2,425 marked crashed, nearly all from these tests and swarm
 workers).
- [ ] **Stop routing test configuration through the process environment.** The
 full suite runs serial (`test.sh full` passes `--test-threads=1` to its
 lib-bins group, and `docs/dev/testing.md` tells a session that hits the
 kcode-tui flake to pass it too) because tests read `KCODE_HOME`,
 `KCODE_SSH_*`, `KCODE_MODEL`, `KCODE_PROVIDER` and the rest from the process
 env while other tests mutate them. The lock does not fix it: mutating tests
 hold `lock_test_env()` but readers never take it, so the exclusion buys
 nothing; guarding the readers serializes the suite (`create_test_app` alone is
 ~810 call sites, measured at >10 minutes) and can deadlock with worker threads
 that read the same accessors. `docs/dev/testing.md` carries the full flake
 section and the evidence. Same root as "Stop the session tests writing into
 the real store" above: one config root per test process, injected, not global.
 Measured by `scripts/test.sh full` passing green at the default thread count
 on a clean environment, and by its wall time (2026-10-04: 5,367 test functions
 across 519 files, run on one core today).
- [ ] **The loop's dispatch has no test.** `wake_ready_owners` (`live_turn.rs:374`),
 `next_held_ready_row` (`:249`) and `row_turn_message` (`:299`) are exercised only by the
 by-hand fan-out. S1 deleted 26 test files with the code they covered (the assign, await
 and dag suites), which is right for deleted behavior, and left the replacement untested.
 One `#[tokio::test]`: seed two headless members on the spawn edge, write one ready row
 each, assert each woken member runs its own row's turn, close both, and assert the run
 ends with the anchor the last row to close. Measured by the wake firing on the write with
 no verb, and by the run's member sessions being gone when the anchor closes.
