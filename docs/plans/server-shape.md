# The server's shape

The request path and the tool that drives it. `handle_client` is first because its
request context is where the per-arm `SwarmState` literals die. Every open item of this
subject is one checkbox below, with its design in the item.

## Tasks

- [ ] **Split `handle_client`**
 (`crates/kcode-app-core/src/server/client_lifecycle.rs`, 2,952 lines; the function is
 417-2380, ~1,964, re-measured after S1). Researched 2026-09-28: the arms are already
 thin and mostly
 delegate, so the god-ness is not the arms. It is:
  - **~734 lines of setup prologue** (417-1150): the read loop until `Subscribe`
    (lightweight control requests answered inline and dropped), working-dir resolution,
    provider fork, `Registry::new`, `Agent::new_with_initial_working_dir`, prewarm,
    `SessionControlHandle` registration, four `write().await` map inserts, and the
    event-forwarder spawn.
  - **51 `Request::` arms, ~1,120 lines** (1151-2270): largest `Subscribe` 195, `Message`
    95, `SoftInterrupt` 81, `ResumeSession` 74, `Rewind` 61, `RewindUndo` 60, `Clear`
    47; the rest 15-40. The `Comm*` arms only unpack and forward.
  - **~85 lines of teardown** (2296-2380) that already calls
    `client_disconnect_cleanup` helpers.
  - **24 args** under `#[expect(clippy::too_many_arguments)]`, with one production
    caller (`server/runtime.rs:261`) plus tests, so a context struct is mechanical. Six
    args are swarm state: one `SwarmState` pair (`swarm_members`/`swarm_runs` ->
    `members`, `runs`) and four loose Arcs beside it (`event_history`, `event_counter`,
    `swarm_event_tx`, `swarm_mutation_runtime`).
    The `Comm*` arms re-wrap them into a `SwarmState { .. }` literal: that literal is
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
- [ ] **Condense `tool/communicate.rs`.** The verb cut landed
 (`plans/task-flow.md` S1), removing the assign verbs, the read views, the report
 action and the channels, so the file is now 426 lines and two concepts remain: swarm
 coordination (spawn, stop, list_models, message) and the model-list formatter, with
 an `execute` match between them. Measure at the step: the formatter is the only
 seam left, and if it is a pure function the item is closable as done.
 `tool/session_search.rs` has two real seams (native index vs external-source
 ingestion).
