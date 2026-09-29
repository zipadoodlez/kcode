# Todo

The one list of outstanding work. Tick an item when it lands and delete it; git
has the history. Longer designs live in the doc an item links to; this file is
the checklist, not the reasoning.

Ordered by what unblocks what, not by payoff alone: a few shared representations
sit under many call sites, so settling a shape before sweeping sites avoids
redoing the sweep. The fan-in, from `graphify god_nodes` at `6dff3825`, says
which shapes those are: `create_test_app` 771, `Provider` 312, `Message` 227,
`ServerEvent` 208, `TuiState` 205, `SwarmMember` 182, `App` 137, `ToolContext`
135, `LoginProviderDescriptor` 125.

An item carries a compact evidence line so it can be picked up without
re-deriving why it exists. An item marked `(decision)` needs a call from the
maintainer before work starts; everything else is actionable.

## Standing decisions

- **Fork policy: diverged.** No rebase lane and no upstream to track; kcode is
  its own tree, and the ratchets measure this tree's drift.
- **No provider is deleted** (maintainer decision 2026-09-27): every cut
  candidate backs a user-selectable provider. Cleanup is identity unification
  (below), not removal.
- **Gate is wired and green.** `scripts/check_guardrails.sh` runs eight steps:
  `cargo fmt --check`, `cargo clippy -- -D warnings` (which also compiles every
  target), `Cargo.lock` freshness, the code-size, test-size, wildcard-reexport,
  and `App`-shape ratchets, and crate dependency boundaries. Audited
  2026-09-28: it had fourteen steps; five were deleted as redundant or as counts
  that could not make the distinction their rule needed (see
  `docs/dev/post-change.md`). The ratchets fail when a tracked number improves
  without the baseline being updated, so they can only tighten.

## 0. Deletion ledger (the line-count question, measured)

Written because "over 500k lines, want under 200k" was a goal with no data
behind it. Measured 2026-09-28 by four read-only census passes (one per big
region) plus a dead-code probe on `App`. Every count is `wc -l` or a run
`grep`; rows sourced from the passes are marked `[census]` where I did not
re-verify them myself. The probe edited one file and reverted it.

### Size

- **536,649 Rust lines**, 954 tracked `.rs` files (`git ls-files '*.rs' | xargs
  wc -l`). Non-Rust tracked text adds ~28k.
- **Production ~353k, tests ~184k**: 132,323 lines in test-named files plus
  51,352 inside inline `#[cfg(test)]` blocks. A broader test-file match counts
  ~194k `[census]`; the ~10k gap is definition, not disagreement.
- Three crates are 72% of the tree: `kcode-tui` 181,595, `kcode-app-core`
  109,167, `kcode-base` 93,624. 60 crates total, 8 under 400 lines.
- `kcode-tui/src/tui/app` alone is ~117k (72k prod, 45k test), and the
  `app/tests` include! tree is 40k of that.

### Verdict on the 200k hope

**Not reachable by deletion without removing capability.** Production alone is
353k, so deleting *every* test line still leaves 353k. A total under 200k needs
~340k production lines gone, i.e. the majority of the product. The measured
deletable pool below is ~15-25k. The shape work in §1-§3 is the real lever, and
it moves comprehension, not the count: its own ratchet is field counts, not
lines.

### The ledger

| cluster | lines | evidence | what holds its place | verdict |
|---|---|---|---|---|
| Dead local turn path | ~810 whole files (`local.rs` 590, `event_wrappers.rs` 38, `overnight_card.rs` 183) + ~30 partial items | `App::run` (`run_shell.rs:198`) has zero callers (live entry is `run_remote`, `tui_launch.rs:160`); cfg-ing it out yields exactly 48 `never used` items | tests drive `local::` from 8 files / 33 sites | delete after migrating those tests; ~1.5k with the partials |
| Second markdown renderer | ~4,700 (`kcode-render-core` 4,542 + adapter 795) | `render_markdown_via_core` has only test callers; the TUI uses the legacy path (`ui_messages.rs:95`); the adapter's own doc says the legacy path "remains authoritative; this adapter is validated against it before any switchover" | `render-core` also supplies `reasoning_line_markup` to `kcode-base/src/session/render.rs` | a switchover, not a deletion: land it and delete the legacy path, or abandon it and delete the core |
| Harnesses living in test files | ~3.7k | `[census]`: `live_tests.rs` 3,080 is `pub mod` production code consumed by `kcode-provider-doctor`; `smoothness_benchmark.rs` 332; `browser_fast_live_tests.rs` 250 | `live_tests` is genuinely production | move the non-test halves into modules; `live_tests` is a misnamed production module |
| Duplicated tiny helpers | ~230, not the 500 first guessed | every family in the item list below was re-read and its bodies hash-compared; the census's `detect_*`/`generate_diff_*` rows were dropped as not-duplicates | 3-90 lines each, all with a home that already exists | one home each; see the item list |
| Parallel persisted-state runtime | ~20, not 150 | read both files: the mechanism already lives in `server/durable_state.rs` (`load_json_state`, `save_json_state`, `hashed_request_key`, `state_dir`); only the 3-line `load_state`/`save_state` wrappers rhyme, and the payload, TTLs, and `is_stale` bodies differ | payload-specific | leave duplicated; a generic would weld two unrelated payloads together |
| Provider wire/runtime split, provider catalog | 0 deletable | `[census]`: runtime crates re-export the wire crates rather than reimplementing them; the catalog is single-source in `kcode-provider-metadata`, re-exported by `kcode-base` | real protocol/transport separation | keep; §1's B3-B5 is identity, not data |
| Provider trait test doubles | 0 deletable, a per-file tax | 45 files `impl …Provider for`, many of them `_tests.rs`; the trait has ~50 methods | test doubles need the shape | same trade as `TestState` in §2 |
| Test near-duplicates | small, unverified | `[census]` found only 13 cross-file duplicated test function names | — | §4's "~40% near-dup" figures were *per file* and were **not** re-derived this pass |

Not duplicated, checked so it is not re-litigated: the `kcode-base` one-line
`console.rs`/`env.rs`/`id.rs`/`plan.rs`/`protocol.rs`/`stdin_detect.rs` files are
the 13 wildcard re-export shims tracked in §3, not dead code.

### Actionable, cheapest first

Landed 2026-09-28 (gate green): `ui_prefs` + `provider/fingerprint.rs` deleted
(~31 lines); the hint trio (`hotkey_feedback`, `shortcut_hints`, `swarm_hint`)
shares `app/hint_state.rs` for load/save/unix-seconds; `fenced_block` moved to
`kcode_base::side_panel`; `now_unix_ms` x3 folded onto `durable_state`. Net
about -150 lines. The rest, each re-read and verified before it was written
down; rows I did not personally re-check say `[census]`.

Dead weight. The census listed five things here; verification found most of them
live. Landed 2026-09-28: `set_swarm_panel_focus` (10 lines, hidden by
`#[allow(dead_code)]`).

The `#[allow(dead_code)]` sweep is done (2026-09-28). Each unconditional site was
stripped and clippy asked what it hid:

- `kcode-base/src/session.rs:18` held an RAII power assertion for its lifetime
  only, so the field is renamed `_sleep_assertion` (matching the `_marker`
  beside it) and the attribute is gone, not the field.
- `session_search_index.rs:216` hid `len`/`is_empty` with no caller; deleted.
- `info_widget_swarm_gallery.rs:454` hid items only tests use; it now says
  `#[cfg(test)]`, and its now-test-only import moved with it.
- `tests/smoothness_benchmark.rs` hid an unused debug helper; deleted. That
  attribute was *not* redundant, contrary to the first reading.
- `examples/swarm_agent_count.rs` was redundant: both annotated items are read or
  used. Attributes deleted.
- `tool/open.rs:478` was not redundant either: the struct is used only by a
  `cfg(not(target_os = "macos"))` function, so the attribute is now
  `cfg_attr(target_os = "macos", allow(dead_code))`.

No unconditional `#[allow(dead_code)]` remains. The `cfg_attr` sites are
conditional on platform, feature, or `cfg(test)` and stay as they are.

- [ ] The dead local turn path (~810-1,500 lines); §2 owns it.

One home per duplicated helper. Body hashes were compared for each of these:

Landed 2026-09-29: `build_file_touch_preview` + its two consts moved to
`kcode-base/src/bus.rs` beside `FileTouch` (three copies -> one);
`SessionAgents`/`ChannelSubscriptions` now imported from `server.rs` in 22 files
(30 restatements deleted, no unused-import fallout once clippy was asked); the
async member lookups became `util::member_swarm_id` / `member_friendly_name`
(5 copies -> 2, ending the name clash with the sync `swarm_id_for_session`);
`sanitize_session_id` is one `pub(crate)` fn in `server/util.rs` (**4** copies
existed, not 3: the ledger missed `session_recovery.rs`'s associated fn), with
the reload_recovery test moved onto the shared fn.

The label precedence collapsed to one rule (see the landed note below).

Landed 2026-09-29: **a session is labelled by its name and nothing else.** The
precedence rule (rename -> task/goal -> generated title) was stated three times
(`process_title::terminal_display_title_for_id`, the TUI `update_terminal_title`,
and `recent_session_index::RecentSession::display_title`) plus an omission in
`Session::display_title`; it is gone. Every label surface (window title,
`/resume`, picker rows, recent list) now calls `process_title::session_name(id)`:
the memorable word, else the raw id. Deleted: `Session.custom_title` and
`/rename` end to end (TUI command + help, `Request::RenameSession`,
`ServerEvent::SessionRenamed`, `handle_rename_session`,
`provider.rename_session_title`, the CLI `kcode session rename`, the
`SessionRenamed` ACP arm), the task-title machinery
(`todo::derive_session_title` / `load_session_title`, the recent-index
`todo_title` column + `refresh_todo_title` + its callers, the TUI todo lookup),
`Session::display_title` / `display_title_or_name`, `terminal_session_label`,
`terminal_display_title_for_id`, `terminal_session_label_for_id`, and the
duplicate `resumed_window_title` (the tui copy now delegates to app-core's).

The review marker that had been smuggled through `title == "review" | "judge"`
is now `Session.kind: SessionKind { Normal, Review, Judge }`, set where the tag
was set and read in `commands_review`; `Session.title` survives as a plain
description (import provenance, search detail) that no label consults.

Also fixed the pre-existing breakage that had made `kcode-app-core` library
tests uncompilable since `0472526c`: `session_search_index`'s test still called
the deleted `TokenHashIndex::len`. That is why the crate's own test run is now
possible at all.
- [ ] `parse_meminfo_kb` 3x identical (`overnight.rs:595`, `perf.rs:282`, and
  `kcode-tui/src/tui/ui_frame_metrics.rs:916`, so this one is cross-crate).
- [ ] `truncated_stream_payload_context` identical in both wire crates
  (`provider-openai/src/stream.rs:19`, `provider-openrouter/src/stream.rs:14`);
  `kcode-provider-core` is the shared hub.
- [ ] Two const pairs: `RELOAD_MARKER_MAX_AGE` = 30s twice (`app/remote.rs:67`,
  `app/remote/reconnect.rs:15`); `RELOAD_RESTORE_MARKER_MAX_AGE` = 60s twice
  (`client_state.rs:26`, `client_session.rs:31`).
- [ ] `now_ms` 3x in `app/` (`observe.rs:242`, `split_view.rs:295`,
  `todos_view.rs:566`) plus a fourth in `kcode-base/src/side_panel.rs:557`.
  Deliberately left: the existing clock home is
  `tui::test_harness::now_ms()`, and routing through it changes behavior under
  the test clock, so that is a fix needing its own verification, not a move.

Checked and deliberately not welded: `detect_memory` returns a 4-tuple in
`overnight.rs` and a 2-tuple in `perf.rs`; `detect_load` differs in how it
counts CPUs; `generate_diff_summary` (3x) and `generate_diff` (2x) have
different bodies despite identical signatures. The rest of the census's
same-name hits are coincidental (`shell_single_quote`, `record`, `default_action`,
`DEFAULT_LIMIT`, `execute_debug_command`, `SearchInput`/`SearchResult`, ...),
not one concept.

Bigger, each its own pass:

- [ ] Move the non-test halves out of the test tree (`live_tests.rs`,
  `smoothness_benchmark.rs`); §4 owns the test-tree item.
- [ ] Delete the dead local turn path and migrate the 33 test call sites onto
  the remote path (~810-1,500 lines); §2 owns the local-turn-path item.
- [ ] Decide the markdown renderer: finish the switchover or delete
  `kcode-render-core` (~4,000 lines either way).

### What would settle the floor

The pool above is what four passes could *prove* is removable. The remaining
uncertainty is not measured: reachability of the `pub` surface across crates
(the compiler cannot flag it), a byte-level pairwise diff of provider impls
(name-level only), and per-file near-duplication inside the test tree. Until
those run, "the floor is ~450-500k" is an estimate, not a measurement, and
200k is not on the table.

## 1. Shared shapes

Widest representation first, while the tree is still quiet. Sweeping call sites
before the shape is settled is churn.

- [ ] Provider cleanup keeps every provider reachable. No provider is deleted
  (see Standing decisions): the catalog exposes Cursor, Copilot, Antigravity,
  GrokBuild, Bedrock; `provider-metadata` has 5 dependents, so the 20
  `kcode-provider-*` crates (61.1k lines) stay.
- [ ] **Condense swarm/comm**: `SwarmState` (`server/state.rs:108`) is a real
  owner, so this is condensation. Member projection hand-written 4x
  (`AgentInfo`, `SwarmMemberStatus`, `MemberStatic`); status vocabulary diverged
  (`SwarmLifecycleStatus` lacks `streaming`/`thinking`; the "active" set is
  re-spelled in 9 places with 3 different sets; 134 string-match sites);
  coordination state unowned (`SwarmMutationRuntime`, `AwaitMembersRuntime`, two
  global claim maps); channel subs across two locks; `SwarmState` rebuilt at ~28
  sites instead of passed. References: `internals/swarm.md`. Do this before
  splitting `handle_client`, whose request-context struct is designed to hold
  `SwarmState`.

## 2. God modules

Staged, each lands whole.

- [ ] **Re-core `App`** (`crates/kcode-tui/src/tui/app.rs`): the largest single
  cost in the tree.

  State (measured `scripts/check_app_shape.py`, 2026-09-28): `app_fields=209`
  (310 when this work began), `impl_app_blocks=57`, `super_glob_imports=124`.
  `app.rs` is 2,425 lines, still over the size ratchet. Landed: the runtime axis
  (`runtime_mode`), `CopySelection`, `Redraw`, `Viewport`, the shared
  side-panel page and decoration, `Swarm`, `HotkeyFeedback`, `ReloadState`,
  `TodosView`, `SplitView`, `Observe`, `Catchup`, the session/server state
  (`RemoteServerInfo`, `RemoteStartup`, `HistoryRecovery`), `PendingSplit`, the
  transcript (`Transcript`), `BackgroundTaskBand`, `OvernightCard`, and the
  input slices (`CommandSuggestions`, `Composer`, `PromptHistoryState`). Each
  moved fields plus the methods that touch only them into an owning struct;
  every landing verified against the `kcode-tui --lib` baseline (1966 passed,
  the same 27 pre-existing failures).

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

  Out of scope: `handle_client`, the local turn path, provider identity, and the
  crate spine are separate items.

  Done when: the field count and `impl App` count fall monotonically
  (`check_app_shape.py`), `use super::*` falls from 124, and `app.rs` drops out
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
- [ ] **Split `handle_client`** (`crates/kcode-app-core/src/server/client_lifecycle.rs`).
  Researched 2026-09-28 by reading the function and its call sites. The line
  count is real; the god-ness is not where the old one-liner put it, because the
  arms are already thin and mostly delegate.

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
     rebuilt at ~28 sites" duplication in §1, and a request context is where it
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
    out of `code_size_budget.json` (tracked 3606, threshold 1200). Gated on
    H1-H3; each move is mechanical once the context exists.
  - H5: **The turn-lifecycle locals.** Same concept as the `App` stream/status
    turn cluster: one owner for the in-flight turn, with the ops that keep
    `processing_message_id`, the task handle, and `client_is_processing`
    consistent. Do it after H3 has made the prologue readable, and reuse the
    `App` result rather than re-deriving it.

  Done when: `handle_client` is under ~600 lines, `client_lifecycle.rs` is out of
  the size budget, and no `SwarmState { .. }` literal is constructed inside a
  request arm.
- [ ] **Condense `tool/communicate.rs`** (3,364 lines) `[census]`: the 2026-09-28
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
  in four places: registry `REGISTERED_COMMANDS` (113 literals), `commands.rs`
  (59), `commands_dispatch.rs` (85), `remote/key_handling.rs` (54).
  `commands_dispatch.rs:1-13` records the last drift bug. One table
  (name, aliases, help, handler, remote-safe); fixes the `/help` gap and the
  dead SSH-block commands for free. Shares `commands_dispatch.rs` with the
  `App` re-core, so keep them in separate changes.
- [ ] **The local in-process turn path** (`crates/kcode-tui/src/tui/app/local.rs`
  and the orphaned `App::run`): `run` has zero callers; it was `pub`, so the
  compiler kept the whole subtree live. Deleting it alone surfaces ~41 items as
  dead and regresses the 0-warning baseline. It is the documented
  `AppRuntimeMode::TestHarness` path (tests call `local::handle_tick`,
  `handle_bus_event`, `finish_turn`). Either delete it and drive tests through
  the remote path, or `#[cfg(test)]`-gate the transitive closure. Two turn
  implementations is the real smell.
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
are easier once phase 2 has shrunk the cross-crate surface.

- [ ] **Crate spine** (`kcode-base` -> `kcode-app-core` -> `kcode-tui` -> root):
  `pub use kcode_*::*` makes every module path global, so no call site names its
  owning crate. `scripts/check_wildcard_reexport_budget.py` ratchets this
  (baseline 13, goal 0). Drive it to zero, or declare the layout cosmetic and
  collapse it.
- [ ] **Dependency diet**: 660 lock packages for a terminal agent.
- [ ] Compile-time isolation: the workspace recompiles far more than it should.
  Same spine as the crate-spine item above.

## 4. Tests

After the shape work, not before: the tree is coupled through `create_test_app`
(771 edges), so shape changes are paid for in test churn. Replacing the include
tree first would just move that churn around.

- [ ] Replace the `include!`-wired test tree with real modules: `app/tests.rs`
  `include!`s 55 files into one module (120 `include!` sites repo-wide), which is
  why helper collisions and `use super::*` are everywhere.
- [ ] Condense near-duplicate tables: `state_model_poke_03.rs` (~40% near-dup
  pairs), `session_tests/cases.rs` (~51%), `remote_events_reload_04.rs` (~43%,
  header-phase table at 1038-1180). The
  percentages are per file and were not re-derived by the §0 census, which found
  only 13 cross-file duplicated test names; re-measure before treating them as
  targets.
- [ ] Move subsystem code out of test files: `live_tests.rs` is a live-provider
  coverage ledger (21% tests), `provider_e2e.rs` (11% tests).
- [ ] One home per duplicated test helper `[census]`; 13 names are defined in
  more than one file, with `lock_env` (7 files) and `test_agent`, `ctx` (4 each)
  the worst: `configured_swarm_root_effort_reads_real_config` (3),
  `available_models_display_seeds_from_persisted_catalog` (3), then
  `tracked_env_vars`, `clear_openai_compatible_runtime_env`, `create_test_app`,
  `ensure_test_kcode_home_if_unset`, `empty_swarm_status_state`,
  `load_auth_file_renames_existing_labels_to_animal_scheme`,
  `test_mask_email_censors_local_part`, `routes` (2 each).
- [ ] Pre-existing failures on this tree: `kcode-tui --lib` 37 (measured
  2026-09-29 at `cd795377`; the earlier 27 had drifted), `kcode-base --lib` 4 (was
  15), root `kcode --lib` 10 of 195 (last measured 2026-09-28). Sampled root causes are stale expectations for removed or renamed
  surface. Also math/LaTeX 15 and `test_lock_order` 1. Environmental, not
  regressions. Treat as the baseline; the suite still covers removed features and
  brittle pixel/color assertions, so collapse or delete rather than maintain.
- [ ] `kcode-app-core --lib` also has two failures with one root cause,
  diagnosed 2026-09-28: `client_actions_tests.rs:441` still asserts the member's
  `swarm_id` equals its working dir, and `communicate_tests/end_to_end.rs:587`
  still expects two independent root clients in one repo to share a swarm.
  `a830fe18` ("fix(swarm): isolate plans by root session", 2026-08-09) moved
  swarm identity to `session:<id>` across 5 files and updated neither test;
  swarm identity is defined at `server/util.rs:113`. Fix is a test update
  (expect `session:<id>`; make the peer a spawned child, since children inherit
  the parent's swarm via `parent_swarm_id`) unless the cwd-sharing model that
  `d4eb3250` (#481) introduced is actually wanted, in which case `a830fe18` is
  the regression and the fix is in production code.

## 5. Hygiene, then packaging

- [ ] Unknown config sections are silently ignored, so older configs keep dead
  keys with no warning.
- [ ] A stale explicit key in `config.toml` silently beats a changed compiled
  default, with no migration or warning, so the user keeps the old behavior and
  cannot tell why.
- [ ] Rename `crates/kcode-tui/src/tui/app/auth_remote/onboarding.rs`: it is the
  remote-login first-attach import, not the welcome screen the name suggests.

## Anytime

Independent, no dependency on the phases above.

- [ ] Not every color derives from a role. `configured_native_color`
  (`kcode-tui-style/src/palette.rs`) attributes a shade to a role only when it
  equals that role's default, so hardcoded `Color::Rgb(...)` shades pass through
  and `/colors` cannot recolor them. Examples: `login_picker.rs`
  `PANEL_BG`/`PANEL_BORDER`, `info_widget_swarm_gallery.rs`. Give each orphaned
  shade a role, or mark it intentionally fixed.
- [ ] Hooks are unobservable: no `/hooks`, no listing, no dry-run. A typo looks
  identical to a hook that does nothing.
- [ ] Blocked calls are invisible: `pre_tool` stderr goes to the model, nothing
  tells the user; hook failures are logged and dropped.
- [ ] Optional: per-provider crate shape is non-uniform (6 providers ship a wire
  crate plus a `-runtime` sibling; 3 are runtime-only; `bedrock` is wire-only).
  Uniformity win at a possible compile-time cost; removes no provider.
- [ ] Optional: the render-state globals (`ui.rs` has 18 production `static`s
  with `#[cfg(test)]` mirrors, the pattern repeated across 17 `ui_*.rs`). Only
  worth doing if a snapshot layer needs the inputs explicit.

## Committed ideas (no plan yet)

- [ ] The axis kcode owns: as a subtraction fork on MIT code, only what kcode
  *adds* is ownable. Leading candidate: verifiability plus package-manager
  ownership (reproducible builds, no self-modification, no telemetry,
  permission-gated by default, every performance claim shipped with a runnable
  script and raw artifacts in-repo).

## Spec (not a checklist)

`internals/browser-provider-protocol.md`: draft spec, no implementation. Tighten
the core method set and the normalized `page.snapshot` format before building an
adapter.
