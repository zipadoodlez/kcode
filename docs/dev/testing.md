# Testing

## Running the suites

`cargo test` runs the workspace. Target individual crates while iterating:
`cargo test -p kcode-tui --lib`, `cargo test -p kcode-app-core --lib`, and so
on. The onboarding state-space invariants are also a guardrail gate; see
[../internals/onboarding.md](../internals/onboarding.md).

## Known flakiness: `kcode-tui` lib tests under parallel execution

`cargo test -p kcode-tui --lib` fails 1-4 tests per run at the default thread
count, with a set that changes between runs. It is a parallelism race on
process-global render state, not a logic bug: each test passes in isolation, and
`--test-threads=1` passes the whole suite.

Root cause: `create_test_app()` (and `create_named_provider_test_app`) in
`crates/kcode-tui/src/tui/app/tests/support_failover/part_01.rs` calls
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

## Baseline failures on this tree (2026-09-27)

Recorded so a red run is not mistaken for a regression. Single-threaded to take
the parallelism race above out of the picture:

```sh
cargo test -p kcode-tui --test-threads=1
```

- `--lib`: 1966 passed, 27 failed, 17 ignored (2010 total, ~83s).
- Integration targets, run by name (cargo stops at the first failing target when
  run as one command): `glyph_safe_wire` 2/2, `no_new_raw_rgb_literals` 1/1,
  `width_stable_glyphs` 1/1, `test_lock_order` 2/3
  (`inline_images_persistence_locks_env_before_render`, "regression target must
  exist").

The 27 `--lib` failures:

```text
tui::app::helpers::helpers_tests::build_resume_command_uses_imported_kcode_session_for_claude_code
tui::app::helpers::helpers_tests::build_resume_command_uses_imported_kcode_session_for_codex
tui::app::tests::ancient_server_history_is_deferred_via_client_side_release_check
tui::app::tests::recent_project_review_falls_back_cleanly_when_no_repo_is_known
tui::app::tests::stale_server_history_is_deferred_before_remote_state_is_applied
tui::app::tests::test_account_switch_shorthand_switches_openai_account_by_label
tui::app::tests::test_changelog_overlay_mouse_drag_release_copies_text
tui::app::tests::test_gate_digest_is_delivered_at_turn_end_and_rearms_next_cycle
tui::app::tests::test_handle_server_event_compaction_mode_changed_updates_remote_mode
tui::app::tests::test_handle_server_event_compaction_shows_completion_message_in_remote_mode
tui::app::tests::test_improve_mode_persists_in_session_file
tui::app::tests::test_info_widget_local_direct_api_runtime_shows_cost_based_usage
tui::app::tests::test_input_composer_drag_selects_and_copies_typed_text
tui::app::tests::test_input_composer_drag_then_release_copies_via_full_mouse_path
tui::app::tests::test_logout_clear_anthropic_accounts_removes_all_accounts_once
tui::app::tests::test_prepare_review_spawned_session_uses_visible_transcript_for_judge_sessions
tui::app::tests::test_registered_command_suggestions_include_aliases_and_hide_secret_commands
tui::app::tests::test_selfdev_command_spawns_session_in_test_mode
tui::app::tests::test_startup_update_checking_stays_quiet_until_update_work_starts
tui::app::tests::test_startup_update_error_replaces_checking_card
tui::app::tests::test_tool_side_panel_uses_shared_right_pane_keyboard_focus
tui::session_picker::loading::tests::load_sessions_includes_saved_sessions_beyond_scan_limit
tui::session_picker::tests::onboarding_banner_renders_prompt_and_both_action_rows
tui::session_picker::tests::test_loading_preview_refreshes_search_index_for_picker_filtering
tui::ui::messages::tests::render_tool_message_memory_recall_centered_mode_left_aligns_with_padding
tui::ui::messages::tests::visually_appealing_prompt_batched_retry_renders_complete_todo_card
tui::ui::tests::swarm_buffer::right_fact_stack_uses_transcript_status_notification_and_input_rows_in_order
```

The `App` re-core changes shape, not behavior, so this set should only shrink.
`scripts/check_app_shape.py` ratchets the shape itself; see `../todo.md`.

The root crate has its own pre-existing set: `cargo test -p kcode --lib` fails
10 of 195 (185 passed), measured 2026-09-28. It was 12 of 193 at `6dff3825`;
B1/B2 fixed the two provider round-trip failures and added two tests. Sampled
causes: stale expectations for removed or renamed surface, e.g.
`login::next_step::tests::extracted_hints_match_the_strings_login_printed_before_extraction`
wants "run kcode" where the code now prints "run kcode", and
`cli::args::tests::login_scriptable_flags_parse` parses a `--google-access-tier`
flag that no longer exists after the Google login was cut. The `../todo.md`
baseline bullet is the list to keep current.

## Testing onboarding locally

Onboarding is easiest to iterate with an isolated sandbox, so repeated runs never
touch real auth state. `scripts/onboarding_sandbox.sh` roots state under
`KCODE_HOME` and `KCODE_RUNTIME_DIR`, so no real config, sockets, or trusted
external-auth imports are reused, and one `reset` throws it all away.

```sh
scripts/onboarding_sandbox.sh fresh                  # clean isolated launch
scripts/onboarding_sandbox.sh reset                  # blank onboarding state
scripts/onboarding_sandbox.sh seed-real-logins       # copy real external logins in
scripts/onboarding_sandbox.sh login openai           # log in without touching normal config
scripts/onboarding_sandbox.sh kcode auth status      # run any kcode command in the sandbox
```

Because a fresh sandbox has nothing to import, `seed-real-logins` copies your
real external credential files into `$KCODE_HOME/external/<same relative path>`
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
`KCODE_ONBOARDING_SANDBOX`, `KCODE_ONBOARDING_DIR`, `KCODE_AUTH_FIXTURE_DIR`.

### Headless screenshots

`scripts/capture_onboarding.sh` renders the same `OnboardingFlow` phases and
ratatui widget tree into an offscreen `TestBackend`, writing SVG (and PNG when
`rsvg-convert` is present) for every resting state in `onboarding_graph.rs`. It
never launches a terminal or reads real credentials.
