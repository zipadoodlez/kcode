# Todo

The one list of outstanding work. Tick an item when it lands and delete it; git
has the history. Longer designs live in the doc an item links to; this file is
the checklist, not the reasoning. A count quoted here is a measurement as of the
date given, not a fact to trust: re-derive it.

An item carries a compact evidence line so it can be picked up without
re-deriving why it exists. An item marked `(decision)` needs a call from the
maintainer before work starts; everything else is actionable.

Ordered by what unblocks what, not by payoff alone: a few shared representations
sit under many call sites, so settling a shape before sweeping sites avoids
redoing the sweep. Which shapes those are is a `graphify god_nodes` query; run
it rather than trusting a pasted list.

## Standing decisions

- **Fork policy: diverged.** No rebase lane and no upstream to track; kcode is
  its own tree, and the ratchets measure this tree's drift.
- **No provider is deleted** (maintainer decision 2026-09-27): every cut
  candidate backs a user-selectable provider. Cleanup is identity unification
  (§1), not removal.
- **Gate is wired and green.** `scripts/check_guardrails.sh` is the fork's only
  gate; there is no CI. The script is the list of gates, so read it rather than
  a copy. The ratchets fail when a tracked number improves without the baseline
  being updated, so they can only tighten; a landing that shrinks one ends with
  `scripts/check_guardrails.sh --fix`.
- **Search tool: `kgrep`, one name** (2026-09-30). The grep prior is met by the
  inbound alias in `kcode-tool-core::resolve_tool_name` (which stays: it also
  carries OAuth's `file_grep`/`Grep` and the `functions.*` namespace), not by a
  second advertised name. A canonical-vs-advertised map was considered and
  rejected: it adds a dual name that must stay in sync. Discovery is addressed
  by the one prompt line that rules out shell `rg`/`grep`/`find` (a trap guard:
  `rg` is absent, so a shell search returns empty and reads as no matches) and
  by the `kgrep` description stating capability. Revisit only if a live probe
  shows models still reaching for `bash`: one `kcode run` that forces a search,
  read the first tool call. A dirty result (bash wins) is real evidence; a clean
  one is weak, since deepseek and xiaomi-mimo carry less of the grep prior than
  the Claude/OpenAI families.
- **The 200k line goal is off the table** (measured 2026-09-28 by four
  read-only census passes). Production is ~353k lines, so deleting every test
  line still leaves 353k; a total under 200k needs the majority of the product
  gone, and the measured deletable pool was ~15-25k. Do not reopen the ledger.
  Checked and not duplicates, so they are not re-litigated: the provider
  wire/runtime split and the persisted-state runtime. The floor could still be
  settled by three measurements nobody has run: `pub` reachability across
  crates (the compiler cannot flag it), a byte-level pairwise diff of provider
  impls, and per-file near-duplication inside the test tree. Until then "the
  floor is ~450-500k" is an estimate, not a measurement.

## Test baseline

Single-threaded, every suite is at zero (2026-09-29). Two failure classes remain,
both environmental, and `docs/dev/testing.md` owns the detail:

- **Parallel-only flakes.** `kcode-tui --lib` at the default thread count fails a
  changing set of about a dozen tests that pass single-threaded. Root cause
  identified 2026-09-30: tests read configuration from process-global sources
  that other tests mutate concurrently, and the readers never take the
  `lock_test_env()` the mutators hold. The live channels are `KCODE_HOME` (session
  save/restore and the config cache resolve through whatever home is set when
  they run), the `KCODE_SSH_REMOTE` / `KCODE_SSH_*` / `KCODE_MODEL` /
  `KCODE_PROVIDER` / `KCODE_RUNTIME_PROVIDER` family read at dispatch time by
  `tui::is_ssh_remote()` and friends, and ambient `KCODE_*` in the shell.
  Proven by `KCODE_SSH_REMOTE=test-remote <test> --exact` flipping a passing
  test to failing. App-core had the same shape (global bus, env/config) and is
  partly fixed. `--test-threads=1` is the workaround; serializing the readers on
  the same lock deadlocks or makes the suite >10 minutes, so the durable fix is
  to stop routing test configuration through the process environment.

  Note for anyone reading a number from this file: a run started from inside a
  kcode shell inherits the active provider profile as `KCODE_*`, so the suite can
  read it as configuration. Check with every `KCODE_*` unset before believing a
  failure.
- **A rare fork/PTY hang.** `terminal_setup_command.rs`'s
  `decode_key_event_via_pty` forks the multithreaded test process and runs
  crossterm in the child. Seen twice: the child wedges and the parent blocks on
  its result pipe forever. Not reproduced on demand in 20+ runs; it needs a
  concurrent lock held at fork time. Spawning a fresh process instead of forking
  would remove it.

## 1. Shared shapes

Widest representation first, while the tree is still quiet. Sweeping call sites
before the shape is settled is churn.

- [ ] **Condense swarm/comm**: `SwarmState` (`server/state.rs`) is a real owner,
  so this is condensation. Member projection hand-written 4x (`AgentInfo`,
  `SwarmMemberStatus`, `MemberStatic`); status vocabulary diverged
  (`SwarmLifecycleStatus` lacks `streaming`/`thinking`; the "active" set is
  re-spelled in 9 places with 3 different sets; 134 string-match sites);
  coordination state unowned (`SwarmMutationRuntime`, `AwaitMembersRuntime`, two
  global claim maps); channel subs across two locks; `SwarmState` rebuilt at ~31
  sites instead of passed. References: `internals/swarm.md`. This is the trunk of
  the work-list project (`plans/work-list.md`): the swarm becomes one executor of
  the repo's list of open work, so coordination state needs one owner before the
  list can be written from it. Do this before
  splitting `handle_client`, whose request-context struct is designed to hold
  `SwarmState`.

  Member status is typed end to end (`SwarmLifecycleStatus`, `SwarmRole`, and
  `SwarmMemberRecord` moved to `kcode-session-types`, landed 2026-09-29). What
  remains of the projection is the display split below: the TUI synthesizes
  display words (`thinking`, `streaming`) distinct from lifecycle, which is why
  activity and lifecycle stay two concepts rather than one enum.

  - [ ] **Member appearance follows the typed status** (decisions confirmed
    2026-09-29). `kcode-tui-render` takes `SwarmLifecycleStatus` for its member
    input (a new dependency on `kcode-session-types`, a data-only crate), so the
    string matches in `swarm_gallery.rs` and `is_active_status` become enum
    matches. One module owns member appearance: accent, glyph, label, sort rank,
    and an `is_working` predicate kept distinct from lifecycle `is_active`,
    because a stalled node must not spin. The duplicate appearance map
    `info_widget_swarm_background::swarm_status_style` is deleted. The swarm-path
    `Color::Rgb` literals become `kcode_tui_style` role accessors so `/colors`
    can recolor them; the small default shift for shades that are not exact role
    defaults is accepted.
  - [ ] **A stalled plan node is visible** (planned 2026-09-29). `running_stale`
    is only ever a plan-item status: `recover_member_status` maps `Running` to
    `Crashed` and `Ready` to `Stopped`, so the member enum's `RunningStale` is
    unreachable. Today `info_widget_todos::normalize_plan_status_for_todo` folds
    `running_stale` into `in_progress`, so a stalled node renders as a running
    one (`▶` amber, `[doing]`), and `tui_state.rs` counts it in the progress
    bar's running bucket. Give it its own display value and marker in the todo
    and plan widgets: a distinct glyph and label, `warning_color()`, sorted with
    `in_progress`. Decisions: reuse `warning_color()` and distinguish by glyph
    and label, no new palette role; leave the progress number's count as is, so
    only the per-node marker changes and stalled stays in the running bucket.

## 2. God modules

Staged, each lands whole.

- [ ] **Re-core `App`** (`crates/kcode-tui/src/tui/app.rs`): the largest single
  cost in the tree.

  State (measured `scripts/check_app_shape.py`, 2026-10-01): `app_fields=188`
  (310 when this work began), `impl_app_blocks=53`, `super_glob_imports=117`.
  `app.rs` is 2,147 lines, still over the size ratchet. Landed: the runtime axis
  (`runtime_mode`), `CopySelection`, `Redraw`, `Viewport`, the shared
  side-panel page and decoration, `Swarm`, `HotkeyFeedback`, `ReloadState`,
  `TodosView`, `SplitView`, `Observe`, `Catchup`, the session/server state
  (`RemoteServerInfo`, `RemoteStartup`, `HistoryRecovery`), `PendingSplit`, the
  transcript (`Transcript`), `BackgroundTaskBand`, `OvernightCard`, and the
  input slices (`CommandSuggestions`, `Composer`, `PromptHistoryState`). Each
  moved fields plus the methods that touch only them into an owning struct;
  every landing verified against the `kcode-tui --lib` baseline.

  Target: `App` becomes a coordinator holding named sub-structs, and each
  sub-struct owns the methods that touch only it. The pattern is already in tree
  (`impl RemoteLogin`). Renaming `self.scroll_offset` to
  `self.viewport.offset` is churn unless the methods move with the fields; that
  move is the point. Group names come from the `TuiState` trait's own section
  headers. Order is isolation first, coupling last. Re-measure cohesion before
  each extraction: the first cut found no group cleanly isolated, and the order
  below is provisional.

  Remaining stages:
  - `Panels` (12 fields, 89 sites): side panel and split view. The mirror pages
    (split view, todos, observe, catchup) are out; the `side_pane_*`/`diff_pane_*`
    fields remain.
  - Overlay/picker state: the login picker remains (session, account, usage, and
    model pickers have landed).
  - Stream/status. Not a mechanical sweep: `is_processing`, `status`,
    `streaming`, `status_detail`, `subagent_status`, `batch_progress`,
    `processing_started`, `stream_message_ended`, `context_limit`,
    `context_info`, and `context_revision` collide with other structs or method
    names, and the remaining turn-lifecycle fields (`pending_turn`,
    `cancel_requested`, `visible_turn_started`, `last_stream_activity`,
    `last_user_interaction`, `deferred_stream_done_id`, `last_stream_error`,
    `attempt_committed_assistant_messages`) serve different consumers, so they
    are not one struct. Split per concept with the receiver type checked.
  - Provider and model context (15 fields, but ~500 `self.`/`app.` sites across
    30+ files). It mixes two concepts: provider connection (name/model/
    transport/credential/reasoning/service tier) and model selection (picker
    cache, pending switch/route/reasoning). Split along that seam first; do not
    create a second registry (coordinate with §1). Two names also live on other
    structs (`upstream_provider`, `provider_session_id`).
  - `Input`: 21 fields, ~1940 direct sites, 69 files, and hyper-common names
    (`input`, `cursor_pos` also parameters, locals, and fields on
    `backend`/`debug`/`session_picker`/`ui_input`). Remaining slices beyond the
    landed ones use the receiver-aware rename plus a call-site guard (`input()`
    and `command_suggestion_selected()` are `TuiState` accessors).
  - Session/server leftovers: `server_spawning` vs `auto_server_reload`
    (`auto_server_reload` is cached `display` config, not connection state),
    `remote_session_id` (kept: active session identity, 86 sites),
    `pending_local_transfer`, and `route_next_prompt_to_new_session` (single
    fields with their own lifecycles, not worth a struct).
  - Revisit `TuiState`: it is a 122-method trait with two impls, and `TestState`
    (39 fields, 83 sites, 13 files) exists so render tests avoid constructing an
    `App`. Deleting it is a trade, decided last, once `App` is cheap to
    construct.

  Out of scope: `handle_client`, provider identity, and the crate spine are
  separate items.

  Done when: the field count and `impl App` count fall monotonically
  (`check_app_shape.py`), `use super::*` falls from 117, and `app.rs` drops out
  of `code_size_budget.json`.

  - [ ] **Behavior-check the landed extractions.** They were cut by *cohesion*
    (which methods touch which fields), not by tracing behavior; tests prove the
    moves preserved behavior, they do not prove the boundaries are right. Verify
    per landed struct by tracing the write sequences at turn/reconnect/reset
    boundaries and confirming no transition spans two structs. Highest risk:
    `ReloadState` (ten fields grouped under "session maintenance" and never
    behaviorally traced; may be the background-client-maintenance half and the
    server reload handshake, not one thing). Then `Swarm`, then the
    `Redraw`/`Viewport` split. Evidence over looks: the outbound-input region's
    field split was wrong for exactly this reason (`queue_recovery.rs` converts
    soft interrupts, interleave messages, and in-flight sends into
    `queued_messages`, so those are one pipeline, not three groups).
  - [ ] **Re-core the outbound user-input pipeline** rather than condense it.
    `queued_messages`, `hidden_queued_system_messages`, `pending_soft_interrupts`,
    `pending_soft_interrupt_requests`, `interleave_message`, `interleave_images`,
    `queued_followup_starved_since`, `pending_queued_dispatch`,
    `rate_limit_pending_message`, and `rate_limit_reset` are parallel
    representations of one thing, stitched by hand-written conversions in
    `remote/queue_recovery.rs`. Packing them into a struct keeps the duplication;
    the fix is one representation (a queue of outbound items carrying status and
    ack id) that deletes the conversions. Needs a fresh context; not a field-move.
- [ ] **Split `handle_client`** (`crates/kcode-app-core/src/server/client_lifecycle.rs`,
  3,584 lines). Researched 2026-09-28 by reading the function and its call sites.
  The line count is real; the god-ness is not where the old one-liner put it,
  because the arms are already thin and mostly delegate.

  Measured. The function is lines 434-3037 (~2600), and it decomposes as:

  - **744 lines of setup prologue** (434-1178): the read loop that accepts
    requests until `Subscribe` (lightweight control requests are answered inline
    and dropped), working-dir resolution, provider fork, `Registry::new`,
    `Agent::new_with_initial_working_dir`, prewarm, `SessionControlHandle`
    registration, four separate `write().await` map inserts, and the per-client
    event-forwarder task.
  - **77 `Request::` arms, 1742 lines total**: largest `Subscribe` 195,
    `SoftInterrupt` 81, `ResumeSession` 74, `Message` 64, `Rewind` 61,
    `RewindUndo` 60, `Clear` 47; the rest are 15-40 lines and the `Comm*` arms
    carry no logic, they unpack and forward.
  - **~117 lines of teardown** (2920-3037) that already calls
    `client_disconnect_cleanup` helpers.

  The three real costs, in order:

  1. **28 args** under `#[expect(clippy::too_many_arguments)]`, with exactly one
     production caller (`server/runtime.rs:263`) plus tests, so a context struct
     is mechanical. Ten of the args are swarm state: four are already modelled by
     `SwarmState` (`swarm_members` -> `members`, `swarms_by_id`,
     `swarm_plans` -> `plans`, `swarm_coordinators` -> `coordinators`), and six
     more loose Arcs travel beside it (`shared_context`, `event_history`,
     `event_counter`, `swarm_event_tx`, `await_members_runtime`,
     `swarm_mutation_runtime`). Every `Comm*` arm re-wraps those into a
     `SwarmState { .. }` literal inline. That literal is the "`SwarmState`
     rebuilt at ~31 sites" duplication in §1, and a request context is where it
     dies.
  2. **The prologue is four unnamed state machines sharing ~15 locals by name**
     (accept-until-Subscribe, session creation, registration, forwarder spawn).
  3. **Per-client mutable locals** that every arm mutates: turn lifecycle
     (`client_is_processing`, `processing_task`, `processing_message_id`,
     `processing_session_id`), subscribe stage (`client_subscribed`,
     `provisional_session`, `pending_request`), and connection flags
     (`continue_on_disconnect`, `model_usage_updates_enabled`,
     `supports_pdf_panels`, `client_selfdev`, `swarm_enabled`,
     `last_available_models_snapshot`, `current_client_instance_id`).

  Order, each stage lands whole; H1-H3 are pure moves and can share one change:

  - H1: **`ClientContext`.** One struct holding the 28 args (Arcs cloned once at
    the caller). Deletes the `#[expect]` and the 28-arg list. No behavior.
  - H2: **Fold swarm ownership in.** Pass `SwarmState` (from the swarm/comm item
    above, which is why that is ordered first) plus one `SwarmRuntimeHandles` for
    the six loose Arcs, and delete the per-arm `SwarmState { .. }` literals.
    Gated on §1's condensation, not on H1.
  - H3: **Name the prologue.** Extract `accept_initial_request` (the
    lightweight-control loop), `start_client_session` (provider / registry /
    agent / prewarm / registration), and `spawn_client_event_forwarder`. Target:
    under ~100 lines of named calls between the signature and the `match`.
  - H4: **Move the remaining inline arms** into their sibling modules, largest
    first (`Subscribe` -> `client_session`, `SoftInterrupt`, `ResumeSession`,
    `Message`, `Rewind`, `RewindUndo`, `Clear`), so `client_lifecycle.rs` drops
    out of `code_size_budget.json` (tracked 3584, threshold 1200). Gated on
    H1-H3; each move is mechanical once the context exists.
  - H5: **The turn-lifecycle locals.** Same concept as the `App` stream/status
    turn cluster: one owner for the in-flight turn, with the ops that keep
    `processing_message_id`, the task handle, and `client_is_processing`
    consistent. Do it after H3 has made the prologue readable, and reuse the
    `App` result rather than re-deriving it.

  Done when: `handle_client` is under ~600 lines, `client_lifecycle.rs` is out of
  the size budget, and no `SwarmState { .. }` literal is constructed inside a
  request arm.
- [ ] **Condense `tool/communicate.rs`** (3,369 lines) `[census]`: the 2026-09-28
  census read it and found four concepts welded together — swarm coordination,
  capacity cleanup (`cleanup_swarm_workers`, `stop_swarm_sessions`), the run-plan
  driver (`run_swarm_plan_loop`, the driver-claim helpers), and a large block of
  `format_*`/`fetch_*` output formatters around a ~1,100-line `execute`. Same
  census explicitly found these are *not* splits: `server/swarm.rs` (one cohesive
  membership concept), `server/comm_control.rs` (one concept, covered by §1),
  `agent/turn_streaming_mpsc.rs` (one concept with labeled injection points).
  `tool/session_search.rs` has two real seams (native index vs external-source
  ingestion).
- [ ] **Unify the command surface**: slash-command identity is a string matched
  in four tables: the registry `REGISTERED_COMMANDS`
  (`app/state_ui_input_helpers.rs`, 108 entries), `app/commands.rs` (57),
  `app/commands_dispatch.rs` (83), `app/remote/key_handling.rs` (53).
  `commands_dispatch.rs` already merged the two local/remote-disconnected
  copies and records the drift bug it fixed in its header. One table (name,
  aliases, help, handler, remote-safe) fixes the `/help` gap and the dead
  SSH-block commands for free. Shares `commands_dispatch.rs` with the `App`
  re-core, so keep them in separate changes.
- [ ] (decision) **Collapse `AppRuntimeMode::TestHarness`**: the local turn
  path is deleted (`516de13d`), so the axis is only a marker now. Decide whether
  it survives.
- [ ] (decision) **Re-core the SSH-login state** (`crates/kcode-tui/src/tui/app/auth_remote.rs`):
  one flow tracked by five correlated fields (`phase`, `task`, `operation`,
  `input_kind`, `input`) with 12 guarded `.unwrap()`s. Target is two enums,
  `Stage` and `Activity` (two, because a background `Operation::Status` poll runs
  while the picker is open, so stage and running-work are separate axes). Removes
  the illegal combinations and the stringly-typed `input_kind`. In rep change
  only: keep the no-`Debug`/no-`Clone` secrecy property and the `Drop`
  cleanup semantics; a non-1:1 state is a stop-and-report, not a guess. Verify
  with `cargo test -p kcode-tui auth_remote` (21 tests, security-focused).
  Confirm before touching credential code.

## 3. Spine

One direction, four items. Each depends on the previous being settled, and all
are easier once §2 has shrunk the cross-crate surface.

- [ ] **Crate spine** (`kcode-base` -> `kcode-app-core` -> `kcode-tui` -> root):
  `pub use kcode_*::*` makes every module path global, so no call site names its
  owning crate. `scripts/check_wildcard_reexport_budget.py` ratchets this
  (baseline 13, goal 0). Drive it to zero, or declare the layout cosmetic and
  collapse it.

## 4. Tests

After the shape work, not before: the tree is coupled through `create_test_app`,
so shape changes are paid for in test churn. Replacing the include tree first
would just move that churn around.

- [ ] Replace the `include!`-wired test tree with real modules: `app/tests.rs`
  `include!`s 55 files into one module (116 `include!` sites repo-wide), which is
  why helper collisions and `use super::*` are everywhere.
- [ ] Condense near-duplicate tables: `state_model_poke_03.rs`,
  `session_tests/cases.rs`, `remote_events_reload_04.rs` (header-phase table).
  The ~40-51% figures are per file and predate both the 2026-09-28 census
  (which found only 13 cross-file duplicated test names) and the todo step 1
  deletion, which shrank the first two files. Re-measure before treating them as
  targets.
- [ ] Move subsystem code out of test files: `smoothness_benchmark.rs` (313)
  remains under `app/tests/`. Separately, `kcode-base/src/live_tests.rs` (3,080)
  is a misnamed *production* module (`pub mod live_tests`, consumed by the TUI's
  `/live` coverage report); rename it to match what it is.
- [ ] One home per duplicated test helper: 13 names are defined in more than one
  file. Re-measure before ranking; the last count was `lock_env` 8 definitions,
  `test_agent` 7, `create_test_app` 3, then
  `tracked_env_vars`, `clear_openai_compatible_runtime_env`,
  `ensure_test_kcode_home_if_unset`, `empty_swarm_status_state`,
  `available_models_display_seeds_from_persisted_catalog` and the rest at 2 each.

## 5. Hygiene

- [ ] Unknown config sections are silently ignored (`toml::from_str` with no
  `deny_unknown_fields`), so older configs keep dead keys with no warning.
- [ ] A stale explicit key in `config.toml` silently beats a changed compiled
  default, with no migration or warning, so the user keeps the old behavior and
  cannot tell why.

## Todo refactor (phase 2)

Phase 1 landed 2026-09-30: the enforcement tier is gone, the poke is identified
by shape, and the `todo` schema is a five-field display (`id`, `content`,
`status`, `priority`, optional `group`). Intent, resolved decisions, and the
landed record are in `internals/todo-enforcement-removal.md`; the A/B probe
recipe is `dev/todo-rework-ab-probe.md`. Phase 2 is the work list, and it has
outgrown this section: destination and settled decisions are in
`plans/work-list.md`, and its roadmap is not written yet.

- [ ] Live check owed from step 2: the poke now reaches the model as a system
  reminder rather than as user content. One isolated `kcode run` with auto-poke
  armed, on a task that leaves items open, must show the model continuing rather
  than only replying. The fallback if it chats instead is in the internals doc
  (Step 2).

## Anytime

Independent, no dependency on the phases above.

- [ ] **(decision)** Where does work land: `main` directly, or a branch with a
  merge-back step? Two days and 85 commits went onto
  `batch-ab-provider-onboarding` (created 2026-09-29 05:16 from `cd795377`,
  merged by fast-forward 2026-10-01) while `main` did not move, and nothing
  noticed. Nothing was lost, since `main` held zero commits the branch lacked,
  but "is `main` green?" could not be answered from `main`, the pushed branch
  was the first visibility GitHub had, and the tree's uncommitted `pkgver` bump
  targeted a two-day-old HEAD. `AGENTS.md` says work on your own branch and
  stops there; this fork has no CI and no PR gate. Pick one, then make it
  visible: if branches stay, a post-commit or session-start line when
  `git rev-list --count main..HEAD` is non-zero. The two hooks in `.githooks/`
  are graphify's and stay out of it.
- [ ] Not every color derives from a role. `configured_native_color`
  (`kcode-tui-style/src/palette.rs`) attributes a shade to a role only when it
  equals that role's default, so hardcoded `Color::Rgb(...)` shades pass through
  and `/colors` cannot recolor them. `info_widget_swarm_gallery.rs` and the rest
  of the swarm path are covered by the member-appearance item in §1; what
  remains here is `login_picker.rs` `PANEL_BG`/`PANEL_BORDER` and any other
  orphans. Give each shade a role, or mark it intentionally fixed.
- [ ] `now_ms` is defined 4x: `app/observe.rs:212`, `app/split_view.rs:295`,
  `app/todos_view.rs:500`, and `kcode-base/src/side_panel.rs:557`. Deliberately
  left so far: the existing clock home is `tui::test_harness::now_ms()`, and
  routing through it changes behavior under the test clock, so that is a fix
  needing its own verification, not a move.
- [ ] Hooks are unobservable: no `/hooks`, no listing, no dry-run. A typo looks
  identical to a hook that does nothing.
- [ ] Blocked calls are invisible: `pre_tool` stderr goes to the model, nothing
  tells the user; hook failures are logged and dropped.
- [ ] Reduce the always-on per-request tool cost. Two suite tests that capped
  tool and parameter descriptions (20 / 25 tokens) were removed rather than
  fixed, so nothing guards this today; re-measure before acting. Direction to
  consider: keep only the call contract always-on (name, one-line description,
  terse parameter shape plus enums/required) and give the prose a single home in
  the bundled, version-matched docs (`kcode_docs`), echoed in the tool's own
  error when a call is wrong. Control shape to consider: one aggregate
  schema-token budget instead of per-item caps plus the `swarm` exemption.
