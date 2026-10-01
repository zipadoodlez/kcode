# Todo

Outstanding work, in the order that unblocks the most. See `README.md` for the
conventions. The current project's destination, model and open steps are
`plans/work-list.md`.

## 1. Shared shapes

- [ ] **Condense swarm/comm** (`plans/work-list.md` steps 0.3 and 0.4): the state
  gets one owner instead of being rebuilt at 31 sites, and membership is derived
  from the file rather than stored, so the swarm id and the coordinator map go.
  Design and measurements: `plans/work-list.md`; engine: `internals/swarm.md`. Do
  it before splitting `handle_client`, whose request context is designed to hold
  that state.
  - [ ] **Member appearance follows the typed status** (decided 2026-09-29).
    `kcode-tui-render` takes `SwarmLifecycleStatus` (a new dependency on the
    data-only `kcode-session-types`), so the string matches in `swarm_gallery.rs`
    and `is_active_status` become enum matches. One module owns appearance:
    accent, glyph, label, sort rank, and an `is_working` predicate kept distinct
    from lifecycle `is_active`, because a stalled node must not spin. Delete the
    duplicate map `info_widget_swarm_background::swarm_status_style`. The
    swarm-path `Color::Rgb` literals become `kcode_tui_style` role accessors so
    `/colors` can recolor them; the small default shift is accepted.
  - [ ] **A stalled plan node is visible** (planned 2026-09-29). `running_stale` is
    only ever a plan-item status (`recover_member_status` maps `Running` to
    `Crashed` and `Ready` to `Stopped`), so the member enum's `RunningStale` is
    unreachable. `info_widget_todos::normalize_plan_status_for_todo` folds it into
    `in_progress`, so a stalled node renders as running (`▶` amber, `[doing]`) and
    `tui_state.rs` counts it in the running bucket. Give it its own glyph and
    label, `warning_color()`, sorted with `in_progress`; leave the progress count
    alone so only the per-node marker changes.

## 2. God modules

Staged; each lands whole.

- [ ] **Re-core `App`** (`crates/kcode-tui/src/tui/app.rs`), the largest single
  cost in the tree. State (2026-10-01): `app_fields=182` (310 when this began),
  `impl_app_blocks=52`, `super_glob_imports=116`, 2,091 lines. Landed: the runtime
  axis, `CopySelection`, `Redraw`, `Viewport`, the side-panel page and decoration,
  `Swarm`, `HotkeyFeedback`, `ReloadState`, `TodosView`, `SplitView`, `Observe`,
  `Catchup`, `RemoteServerInfo`, `RemoteStartup`, `HistoryRecovery`,
  `PendingSplit`, `Transcript`, `BackgroundTaskBand`, `OvernightCard`, and the
  input slices. Target: `App` as a coordinator of named sub-structs, each owning
  the methods that touch only it (the pattern already in tree: `impl RemoteLogin`).
  Renaming a field without moving its methods is churn, not progress. Group names
  come from the `TuiState` trait's section headers; isolation first, coupling last;
  re-measure cohesion before each extraction, since the first cut found no group
  cleanly isolated and this order is provisional.
  - `Panels` (12 fields, 89 sites): side panel and split view. The mirror pages
    (split view, todos, observe, catchup) are out; `side_pane_*`/`diff_pane_*`
    remain.
  - Overlay/picker state: only the login picker remains.
  - Stream/status: not a mechanical sweep. `is_processing`, `status`, `streaming`,
    `status_detail`, `subagent_status`, `batch_progress`, `processing_started`,
    `stream_message_ended`, `context_limit`, `context_info`, `context_revision`
    collide with other structs or method names, and the remaining turn-lifecycle
    fields (`pending_turn`, `cancel_requested`, `visible_turn_started`,
    `last_stream_activity`, `last_user_interaction`, `deferred_stream_done_id`,
    `last_stream_error`, `attempt_committed_assistant_messages`) serve different
    consumers. Split per concept with the receiver type checked.
  - Provider and model context: 15 fields but ~500 sites across 30+ files, mixing
    provider connection (name/model/transport/credential/reasoning/service tier)
    with model selection (picker cache, pending switch/route/reasoning). Split
    along that seam first; do not create a second registry (coordinate with §1).
    Two names also live on other structs (`upstream_provider`,
    `provider_session_id`).
  - `Input`: 21 fields, ~1940 direct sites, 69 files, and hyper-common names
    (`input`, `cursor_pos` are also parameters, locals, and fields on
    `backend`/`debug`/`session_picker`/`ui_input`). Remaining slices use the
    receiver-aware rename plus a call-site guard (`input()` and
    `command_suggestion_selected()` are `TuiState` accessors).
  - Session/server leftovers: `server_spawning` vs `auto_server_reload` (the
    latter is cached `display` config, not connection state), `remote_session_id`
    (kept: active session identity, 86 sites), `pending_local_transfer`,
    `route_next_prompt_to_new_session` (single fields with their own lifecycles,
    not worth a struct).
  - Revisit `TuiState` last: a 122-method trait with two impls, and `TestState`
    (39 fields, 83 sites, 13 files) exists so render tests avoid constructing an
    `App`. Deleting it is a trade, decided once `App` is cheap to construct.
  - Out of scope here: `handle_client`, provider identity, the crate spine.
  - Done when: field count and `impl App` count fall monotonically
    (`check_app_shape.py`), `use super::*` falls from 117, and `app.rs` leaves
    `code_size_budget.json`.
  - [ ] **Behavior-check the landed extractions.** They were cut by cohesion
    (which methods touch which fields), so tests prove the moves preserved
    behavior, not that the boundaries are right. Trace the write sequences at
    turn/reconnect/reset boundaries and confirm no transition spans two structs.
    Highest risk: `ReloadState` (ten fields grouped as "session maintenance", never
    behaviorally traced; may be two things), then `Swarm`, then the
    `Redraw`/`Viewport` split. The outbound-input split was wrong for exactly this
    reason: `queue_recovery.rs` converts soft interrupts, interleave messages, and
    in-flight sends into `queued_messages`, so those are one pipeline.
  - [ ] **Re-core the outbound user-input pipeline** rather than condense it.
    `queued_messages`, `hidden_queued_system_messages`, `pending_soft_interrupts`,
    `pending_soft_interrupt_requests`, `interleave_message`, `interleave_images`,
    `queued_followup_starved_since`, `pending_queued_dispatch`,
    `rate_limit_pending_message`, `rate_limit_reset` are parallel representations
    of one thing, stitched by hand-written conversions in
    `remote/queue_recovery.rs`. One representation (a queue of outbound items with
    status and ack id) deletes the conversions; a struct keeps the duplication.
    Needs fresh context.
- [ ] **Split `handle_client`** (`crates/kcode-app-core/src/server/client_lifecycle.rs`,
  3,584 lines; the function is 434-3037, ~2600). Researched 2026-09-28: the arms
  are already thin and mostly delegate, so the god-ness is not the arms. It is:
  - **744 lines of setup prologue** (434-1178): the read loop until `Subscribe`
    (lightweight control requests answered inline and dropped), working-dir
    resolution, provider fork, `Registry::new`,
    `Agent::new_with_initial_working_dir`, prewarm, `SessionControlHandle`
    registration, four `write().await` map inserts, and the event-forwarder spawn.
  - **77 `Request::` arms, 1742 lines**: largest `Subscribe` 195,
    `SoftInterrupt` 81, `ResumeSession` 74, `Message` 64, `Rewind` 61,
    `RewindUndo` 60, `Clear` 47; the rest 15-40. The `Comm*` arms only unpack and
    forward.
  - **~117 lines of teardown** (2920-3037) that already calls
    `client_disconnect_cleanup` helpers.
  - **28 args** under `#[expect(clippy::too_many_arguments)]`, with one production
    caller (`server/runtime.rs:263`) plus tests, so a context struct is mechanical.
    Ten args are swarm state: four already modelled by `SwarmState`
    (`swarm_members`/`swarms_by_id`/`swarm_plans`/`swarm_coordinators` -> `members`,
    `swarms_by_id`, `plans`, `coordinators`) and six loose Arcs beside it
    (`shared_context`, `event_history`, `event_counter`, `swarm_event_tx`,
    `await_members_runtime`, `swarm_mutation_runtime`). Every `Comm*` arm re-wraps
    them into a `SwarmState { .. }` literal: that literal is §1's duplication, and
    a request context is where it dies.
  - **The prologue is four unnamed state machines** sharing ~15 locals by name
    (accept-until-Subscribe, session creation, registration, forwarder spawn).
  - **Per-client mutable locals** every arm mutates: turn lifecycle
    (`client_is_processing`, `processing_task`, `processing_message_id`,
    `processing_session_id`), subscribe stage (`client_subscribed`,
    `provisional_session`, `pending_request`), connection flags
    (`continue_on_disconnect`, `model_usage_updates_enabled`, `supports_pdf_panels`,
    `client_selfdev`, `swarm_enabled`, `last_available_models_snapshot`,
    `current_client_instance_id`).
  - Order; H1-H3 are pure moves and can share a change. H1 **`ClientContext`**:
    one struct for the 28 args (Arcs cloned once at the caller), deleting the
    `#[expect]`. H2 **Fold swarm ownership in**: pass `SwarmState` plus one
    `SwarmRuntimeHandles` for the six Arcs and delete the per-arm literals (gated
    on §1, not H1). H3 **Name the prologue**: `accept_initial_request`,
    `start_client_session`, `spawn_client_event_forwarder`, target under ~100 lines
    of named calls before the `match`. H4 **Move the inline arms** into sibling
    modules, largest first, so the file leaves `code_size_budget.json` (gated on
    H1-H3). H5 **The turn-lifecycle locals**: one owner for the in-flight turn,
    reusing the `App` result rather than re-deriving it.
  - Done when: `handle_client` is under ~600 lines, the file is out of the size
    budget, and no `SwarmState { .. }` literal is built inside a request arm.
- [ ] **Condense `tool/communicate.rs`** (3,369 lines) `[census]`: four concepts
  welded together, swarm coordination, capacity cleanup (`cleanup_swarm_workers`,
  `stop_swarm_sessions`), the run-plan driver (`run_swarm_plan_loop`, the
  driver-claim helpers), and the `format_*`/`fetch_*` formatters around a
  ~1,100-line `execute`. The same census found these are *not* splits:
  `server/swarm.rs`, `server/comm_control.rs` (covered by §1), and
  `agent/turn_streaming_mpsc.rs`. `tool/session_search.rs` has two real seams
  (native index vs external-source ingestion).
- [ ] **Unify the command surface**: slash-command identity is a string matched in
  four tables, the registry `REGISTERED_COMMANDS`
  (`app/state_ui_input_helpers.rs`, 108 entries), `app/commands.rs` (57),
  `app/commands_dispatch.rs` (83), `app/remote/key_handling.rs` (53), plus
  `app/input_help.rs` for the help text, which the list did not name: landing
  `/auto` meant an entry in three of them. One table
  (name, aliases, help, handler, remote-safe) fixes the `/help` gap and the dead
  SSH-block commands. Shares `commands_dispatch.rs` with the `App` re-core, so
  keep them in separate changes.
- [ ] (decision) **Collapse `AppRuntimeMode::TestHarness`**: the local turn path is
  deleted (`516de13d`), so the axis is only a marker. Decide whether it survives.
- [ ] (decision) **Re-core the SSH-login state**
  (`crates/kcode-tui/src/tui/app/auth_remote.rs`): one flow tracked by five
  correlated fields (`phase`, `task`, `operation`, `input_kind`, `input`) with 12
  guarded `.unwrap()`s. Target two enums, `Stage` and `Activity` (two, because a
  background `Operation::Status` poll runs while the picker is open), which removes
  the illegal combinations and the stringly-typed `input_kind`. Keep the
  no-`Debug`/no-`Clone` secrecy and the `Drop` cleanup; a non-1:1 state is a
  stop-and-report. Verify with `cargo test -p kcode-tui auth_remote` (21 tests).
  Confirm before touching credential code.

## 3. Spine

- [ ] **Crate spine** (`kcode-base` -> `kcode-app-core` -> `kcode-tui` -> root):
  `pub use kcode_*::*` makes every module path global, so no call site names its
  owning crate. `scripts/check_wildcard_reexport_budget.py` ratchets it (baseline
  13, goal 0). Drive it to zero, or declare the layout cosmetic and collapse it.
  All of §3 is easier once §2 has shrunk the cross-crate surface.

## 4. Tests

After the shape work: the tree is coupled through `create_test_app`, so shape
changes are paid for in test churn.

- [ ] Replace the `include!`-wired test tree with real modules: `app/tests.rs`
  `include!`s 55 files into one module (116 `include!` sites repo-wide), which is
  why helper collisions and `use super::*` are everywhere.
- [ ] Condense near-duplicate tables: `state_model_poke_03.rs`,
  `session_tests/cases.rs`, `remote_events_reload_04.rs`. The ~40-51% figures
  predate both the 2026-09-28 census (13 cross-file duplicated test names) and the
  todo step 1 deletion, so re-measure before treating them as targets.
- [ ] Move subsystem code out of test files: `smoothness_benchmark.rs` (313) sits
  under `app/tests/`. Separately, `kcode-base/src/live_tests.rs` (3,080) is a
  misnamed *production* module (`pub mod live_tests`, consumed by the TUI's
  `/live` report); rename it to what it is.
- [ ] `kcode-tui`'s
  `test_remote_fallback_provider_suggestions_normalize_bare_openai_openrouter_routes`
  fails on a clean tree, at the default thread count and single-threaded, so the
  suite has one false positive to explain before a red run can be trusted. Cause not
  diagnosed; it asserts that provider suggestions include
  `/model openai/gpt-5.4@OpenAI`. `dev/testing.md` carries the note for a session that
  just saw it.
- [ ] One home per duplicated test helper: 13 names are defined in more than one
  file (`lock_env` 8, `test_agent` 7, `create_test_app` 3, then
  `tracked_env_vars`, `clear_openai_compatible_runtime_env`,
  `ensure_test_kcode_home_if_unset`, `empty_swarm_status_state`,
  `available_models_display_seeds_from_persisted_catalog`, and the rest at 2).
  Re-measure before ranking.

## 5. Hygiene

- [ ] The work list has no user doc. `tasks.jsonl` at the repo root, the `todo`
  tool's three actions, and the close's required result are described only in
  `plans/work-list.md`, which is a plan rather than a manual. It goes in
  `docs/user/` when the list settles, which is late enough to be worth writing
  once: after 0.3, since that is where the row gains its `kind`.
- [ ] Unknown config keys and sections are silently ignored (`toml::from_str` with no
  `deny_unknown_fields`), so a config keeps dead lines with no warning. First real
  instance: `auto_poke_toggle`, which died with the client poke in 0.2.
- [ ] A stale explicit key in `config.toml` silently beats a changed compiled
  default, with no migration or warning.

## Anytime

- [ ] **(decision)** Two of the four things the overnight run had are still unplaced;
  the other two went back onto the permission in `plans/work-list.md` 0.1.
  - **A resource snapshot.** Battery, memory, disk and load: ~200 lines of /proc and
    sysfs readers that were only ever displayed, never acted on. Worth keeping only
    if something acts, so throttle or stop on battery and refuse a run with no disk.
  - **A run-end summary.** The morning-report, handoff-ready and final-wrap-up
    prompts were three timers on one idea, the human-facing "what happened". Under
    the new model the record is the row closes and the commits, which is readable but
    not a summary. At most one prompt when the run ends, never three.
  Deliberately not carried over: the manifest, the events file, the task cards and
  the review HTML (a second durable store of the same work), the second supervisor
  and coordinator session, and the stored `validated` field, which a close's result
  is now.
- [ ] **(decision)** Where does work land: `main`, or a branch with a merge-back?
  Two days and 85 commits went onto `batch-ab-provider-onboarding` while `main` did
  not move and nothing noticed; nothing was lost, but "is `main` green?" could not
  be answered from `main`. Pick one and make it visible: if branches stay, a
  post-commit or session-start line when `git rev-list --count main..HEAD` is
  non-zero. The two hooks in `.githooks/` are graphify's and stay out of it.
- [ ] Not every color derives from a role: `configured_native_color`
  (`kcode-tui-style/src/palette.rs`) attributes a shade to a role only when it
  equals that role's default, so hardcoded `Color::Rgb(...)` shades pass through
  and `/colors` cannot recolor them. The swarm path is covered by §1; what remains
  is `login_picker.rs` `PANEL_BG`/`PANEL_BORDER` and other orphans. Give each shade
  a role, or mark it intentionally fixed.
- [ ] `now_ms` is defined 4x: `app/observe.rs:212`, `app/split_view.rs:295`,
  `app/todos_view.rs:500`, `kcode-base/src/side_panel.rs:557`. The existing clock
  home is `tui::test_harness::now_ms()`, and routing through it changes behavior
  under the test clock, so that is a fix needing its own verification.
- [ ] Hooks are unobservable: no `/hooks`, no listing, no dry-run. A typo looks
  identical to a hook that does nothing.
- [ ] Blocked calls are invisible: `pre_tool` stderr goes to the model, nothing
  tells the user; hook failures are logged and dropped.
- [ ] Reduce the always-on per-request tool cost. Two suite tests that capped tool
  and parameter descriptions (20 / 25 tokens) were removed rather than fixed, so
  nothing guards this; re-measure before acting. Direction: keep only the call
  contract always-on (name, one-line description, terse parameter shape plus
  enums/required) and put the prose in the bundled, version-matched docs
  (`kcode_docs`), echoed in the tool's own error when a call is wrong. Control
  shape: one aggregate schema-token budget instead of per-item caps plus the
  `swarm` exemption.
