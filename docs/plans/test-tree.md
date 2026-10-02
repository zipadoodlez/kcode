# The test tree

The tree is coupled through `create_test_app`, so shape changes are paid for in test
churn: it comes after the shape work in the tree proper. This doc also
holds the tests that are red on a clean tree, because until those are explained a red
run cannot be trusted.

The checkboxes are in `todo.md`. Each item names the measurement it is judged by, and
the counts are re-taken at the step, not read from here.

## Real modules instead of `include!`

`app/tests.rs` `include!`s 55 files into one module (116 `include!` sites repo-wide),
which is why helper collisions and `use super::*` are everywhere.

## Condense the near-duplicate tables

`state_model_poke_03.rs`, `session_tests/cases.rs`, `remote_events_reload_04.rs`. The
~40-51% figures predate both the 2026-09-28 census (13 cross-file duplicated test
names) and the todo step 1 deletion, so re-measure before treating them as targets.

## Move subsystem code out of test files

`smoothness_benchmark.rs` (313 lines) sits under `app/tests/`. Separately,
`kcode-base/src/live_tests.rs` (3,080 lines) is a misnamed *production* module
(`pub mod live_tests`, consumed by the TUI's `/live` report); rename it to what it is.

## One home per duplicated test helper

13 names are defined in more than one file (`lock_env` 8, `test_agent` 7,
`create_test_app` 3, then `tracked_env_vars`,
`clear_openai_compatible_runtime_env`, `ensure_test_kcode_home_if_unset`,
`empty_swarm_status_state`, `available_models_display_seeds_from_persisted_catalog`,
and the rest at 2). Re-measure before ranking.

## The known red tests

Two failures are unexplained on a clean tree. Until they are, a red run cannot be read.

**`kcode-tui`'s provider-suggestion test.**
`test_remote_fallback_provider_suggestions_normalize_bare_openai_openrouter_routes`
fails at the default thread count and single-threaded. Cause not diagnosed; it asserts
that provider suggestions include `/model openai/gpt-5.4@OpenAI`. `dev/testing.md`
carries the note for a session that just saw it.

**Three `session_flow` e2e tests** (2026-10-02, seen while landing 0.4f):
`test_debug_create_session_marks_debug`, `test_debug_create_selfdev_session_marks_canary`
and `test_clear_preserves_debug_for_resumed_debug_session`, each dying with a bare
`No such file or directory (os error 2)` after `create_session` over the debug socket.
Verified pre-existing: all three fail identically with the s12 change stashed, at the
commit before it. The suite's other session tests pass, including one that calls the
same `Session::load`, so the failure is specific to a session the *debug command*
created. The next reader should start at what `create_session` persists versus what
`Session::load` reads. Re-checked after 0.4g's g1-g3 (2026-10-02): the same three,
unchanged, and the plan's own suites pass around them.
