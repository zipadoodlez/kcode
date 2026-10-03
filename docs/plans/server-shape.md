# The server's shape

The request path and the tool that drives it. `handle_client` is first because its
request context is where the per-arm `SwarmState` literals die. Every open item of this
subject is one checkbox below, with its design in the item.

## Tasks

- [ ] **Split `handle_client`**
 (`crates/kcode-app-core/src/server/client_lifecycle.rs`, 3,620 lines; the function is
 435-3029, ~2,595). Researched 2026-09-28: the arms are already thin and mostly
 delegate, so the god-ness is not the arms. It is:
  - **~750 lines of setup prologue** (435-1184): the read loop until `Subscribe`
    (lightweight control requests answered inline and dropped), working-dir resolution,
    provider fork, `Registry::new`, `Agent::new_with_initial_working_dir`, prewarm,
    `SessionControlHandle` registration, four `write().await` map inserts, and the
    event-forwarder spawn.
  - **75 `Request::` arms, ~1,730 lines** (1185-2913): largest `Subscribe` 195, `Message`
    95, `SoftInterrupt` 81, `ResumeSession` 74, `Rewind` 61, `RewindUndo` 60, `Clear`
    47; the rest 15-40. The `Comm*` arms only unpack and forward.
  - **~116 lines of teardown** (2914-3029) that already calls
    `client_disconnect_cleanup` helpers.
  - **28 args** under `#[expect(clippy::too_many_arguments)]`, with one production
    caller (`server/runtime.rs:261`) plus tests, so a context struct is mechanical. Six
    args are swarm state: one `SwarmState` pair (`swarm_members`/`swarm_runs` ->
    `members`, `runs`) and five loose Arcs beside it (`shared_context`, `event_history`,
    `event_counter`, `swarm_event_tx`, `await_members_runtime`, `swarm_mutation_runtime`).
    Every `Comm*` arm re-wraps them into a `SwarmState { .. }` literal: that literal is
    the swarm-state condense's duplication, and a request context is where it dies.
  - **The prologue is four unnamed state machines** sharing ~15 locals by name
    (accept-until-Subscribe, session creation, registration, forwarder spawn).
  - **Per-client mutable locals** every arm mutates: turn lifecycle
    (`client_is_processing`, `processing_task`, `processing_message_id`,
    `processing_session_id`), subscribe stage (`client_subscribed`, `provisional_session`,
    `pending_request`), connection flags (`continue_on_disconnect`,
    `model_usage_updates_enabled`, `supports_pdf_panels`, `client_selfdev`,
    `last_available_models_snapshot`, `current_client_instance_id`).
 H1-H3 are pure moves and can share a change.
  - [ ] H1 **`ClientContext`**: one struct for the 28 args (Arcs cloned once at the
    caller), deleting the `#[expect]`.
  - [ ] H2 **Fold swarm ownership in**: pass `SwarmState` plus one
    `SwarmRuntimeHandles` for the six Arcs and delete the per-arm literals (gated on
    the swarm-state condense, `plans/task-flow.md` D2, not on H1).
  - [ ] H3 **Name the prologue**: `accept_initial_request`, `start_client_session`,
    `spawn_client_event_forwarder`, target under ~100 lines of named calls before the
    `match`.
  - [ ] H4 **Move the inline arms** into sibling modules, largest first, so the file
    leaves `code_size_budget.json` (gated on H1-H3).
  - [ ] H5 **The turn-lifecycle locals**: one owner for the in-flight turn, reusing the
    `App` result rather than re-deriving it.
 Done when: `handle_client` is under ~600 lines, the file is out of the size budget,
 and no `SwarmState { .. }` literal is built inside a request arm.
- [ ] **Condense `tool/communicate.rs`.** It lands after the verb cut
 (`plans/task-flow.md` S1), which removes two of the four concepts first. Four concepts
 are welded together: swarm
 coordination, capacity cleanup (`cleanup_swarm_workers`, `stop_swarm_sessions`), the
 run-plan driver (`run_swarm_plan_loop`, the driver-claim helpers), and the
 `format_*`/`fetch_*` formatters around a large `execute`. A census found these are
 *not* splits: `server/swarm.rs`, `server/comm_control.rs` (covered by the swarm-state
 condense, `plans/task-flow.md` D2), and `agent/turn_streaming_mpsc.rs`.
 `tool/session_search.rs` has two real seams (native index vs external-source
 ingestion). Measure the size at the step; it moves.
