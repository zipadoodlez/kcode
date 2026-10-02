# The app's shape

`App` (`crates/kcode-tui/src/tui/app.rs`) is the largest single cost in the tree, and
the slash-command surface is read out of it. The shape is ratcheted
(`scripts/check_app_shape.py`), so the counts are read where they are enforced rather
than repeated here. Every open item of this subject is one checkbox below, with its
design in the item; the tasks are independent except where a line names a gate.

## Tasks

- [ ] **Re-core `App`** into a coordinator of named sub-structs, each owning the
 methods that touch only it (the pattern already in tree: `impl RemoteLogin`).
 Renaming a field without moving its methods is churn, not progress. Group names come
 from the `TuiState` trait's section headers; isolation first, coupling last;
 re-measure cohesion before each extraction, since the first cut found no group
 cleanly isolated and this order is provisional.
 Landed already: the runtime axis, `CopySelection`, `Redraw`, `Viewport`, the
 side-panel page and decoration, `Swarm`, `HotkeyFeedback`, `ReloadState`,
 `TodosView`, `SplitView`, `Observe`, `Catchup`, `RemoteServerInfo`,
 `RemoteStartup`, `HistoryRecovery`, `PendingSplit`, `Transcript`,
 `BackgroundTaskBand`, `OvernightCard`, and the input slices.
 The remaining groups, with their size when measured:
  - [ ] `Panels` (12 fields, 89 sites): side panel and split view. The mirror pages
    (split view, todos, observe, catchup) are out; `side_pane_*`/`diff_pane_*` remain.
  - [ ] Overlay/picker state: only the login picker remains.
  - [ ] Stream/status: not a mechanical sweep. `is_processing`, `status`, `streaming`,
    `status_detail`, `subagent_status`, `batch_progress`, `processing_started`,
    `stream_message_ended`, `context_limit`, `context_info`, `context_revision`
    collide with other structs or method names, and the remaining turn-lifecycle
    fields (`pending_turn`, `cancel_requested`, `visible_turn_started`,
    `last_stream_activity`, `last_user_interaction`, `deferred_stream_done_id`,
    `last_stream_error`, `attempt_committed_assistant_messages`) serve different
    consumers. Split per concept with the receiver type checked.
  - [ ] Provider and model context: 15 fields but ~500 sites across 30+ files, mixing
    provider connection (name/model/transport/credential/reasoning/service tier) with
    model selection (picker cache, pending switch/route/reasoning). Split along that
    seam first; do not create a second registry (coordinate with the swarm/comm
    condense, `plans/row-model.md` D2). Two names also live on other structs
    (`upstream_provider`, `provider_session_id`).
  - [ ] `Input`: 21 fields, ~1940 direct sites, 69 files, and hyper-common names
    (`input`, `cursor_pos` are also parameters, locals, and fields on
    `backend`/`debug`/`session_picker`/`ui_input`). The remaining slices use the
    receiver-aware rename plus a call-site guard (`input()` and
    `command_suggestion_selected()` are `TuiState` accessors).
  - [ ] Session/server leftovers: `server_spawning` vs `auto_server_reload` (the latter
    is cached `display` config, not connection state), `remote_session_id` (kept:
    active session identity, 86 sites), `pending_local_transfer`,
    `route_next_prompt_to_new_session` (single fields with their own lifecycles, not
    worth a struct).
  - [ ] Revisit `TuiState` last: a 122-method trait with two impls, and `TestState` (39
    fields, 83 sites, 13 files) exists so render tests avoid constructing an `App`.
    Deleting it is a trade, decided once `App` is cheap to construct.
 Done when: field count and `impl App` count fall monotonically
 (`check_app_shape.py`), `use super::*` falls from 116, and `app.rs` leaves
 `code_size_budget.json`. Out of scope here: `handle_client`, provider identity, the
 crate spine (`plans/hygiene.md`).
- [ ] **Behavior-check the landed extractions.** They were cut by cohesion (which
 methods touch which fields), so tests prove the moves preserved behavior, not that
 the boundaries are right. Trace the write sequences at turn/reconnect/reset
 boundaries and confirm no transition spans two structs. Highest risk: `ReloadState`
 (ten fields grouped as "session maintenance", never behaviorally traced; may be two
 things), then `Swarm`, then the `Redraw`/`Viewport` split. The outbound-input split
 was wrong for exactly this reason: `queue_recovery.rs` converts soft interrupts,
 interleave messages, and in-flight sends into `queued_messages`, so those are one
 pipeline.
- [ ] **Re-core the outbound user-input pipeline** rather than condense it.
 `queued_messages`, `hidden_queued_system_messages`, `pending_soft_interrupts`,
 `pending_soft_interrupt_requests`, `interleave_message`, `interleave_images`,
 `queued_followup_starved_since`, `pending_queued_dispatch`,
 `rate_limit_pending_message`, `rate_limit_reset` are parallel representations of one
 thing, stitched by hand-written conversions in `remote/queue_recovery.rs`. One
 representation (a queue of outbound items with status and ack id) deletes the
 conversions; a struct keeps the duplication. Needs fresh context.
- [ ] **Unify the command surface.** Slash-command identity is a string matched in
 four tables: the registry `REGISTERED_COMMANDS` (`app/state_ui_input_helpers.rs`, 108
 entries), `app/commands.rs` (57), `app/commands_dispatch.rs` (83),
 `app/remote/key_handling.rs` (53), plus `app/input_help.rs` for the help text, which
 the list did not name: landing `/auto` meant an entry in three of them. One table
 (name, aliases, help, handler, remote-safe) fixes the `/help` gap and the dead
 SSH-block commands. Shares `commands_dispatch.rs` with the `App` re-core, so keep
 them in separate changes.
- [ ] **(decision) Collapse `AppRuntimeMode::TestHarness`.** The local turn path is
 deleted (`516de13d`), so the axis is only a marker. Decide whether it survives.
- [ ] **(decision) Re-core the SSH-login state**
 (`crates/kcode-tui/src/tui/app/auth_remote.rs`). One flow tracked by five correlated
 fields (`phase`, `task`, `operation`, `input_kind`, `input`) with 12 guarded
 `.unwrap()`s. Target two enums, `Stage` and `Activity` (two, because a background
 `Operation::Status` poll runs while the picker is open), which removes the illegal
 combinations and the stringly-typed `input_kind`. Keep the no-`Debug`/no-`Clone`
 secrecy and the `Drop` cleanup; a non-1:1 state is a stop-and-report. Verify with
 `cargo test -p kcode-tui auth_remote` (21 tests). Confirm before touching credential
 code.
