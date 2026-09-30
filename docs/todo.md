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
- **Search tool: `kgrep`, one name** (2026-09-30). The grep prior is met by the
  inbound alias in `kcode-tool-core::resolve_tool_name` (which stays: it also
  carries OAuth's `file_grep`/`Grep` and the `functions.*` namespace), not by a
  second advertised name. A canonical-vs-advertised map was considered and
  rejected: it adds a dual name that must stay in sync and re-creates the
  spelling divergence flagged below. Discovery is addressed by the one prompt
  line that rules out shell `rg`/`grep`/`find` (a trap guard: `rg` is absent, so
  a shell search returns empty and reads as no matches) and by the `kgrep`
  description stating capability. Revisit only if a live probe shows models
  still reaching for `bash`: one `kcode run` that forces a search, read the first
  tool call. A dirty result (bash wins) is real evidence; a clean one is weak,
  since deepseek and xiaomi-mimo carry less of the grep prior than the
  Claude/OpenAI families.

## Test baseline

Single-threaded, every suite is at zero (2026-09-29): `kcode-base`, root
`kcode`, `kcode-app-core`, the math crates, and `kcode-tui`. The front-line pass
is done; what remains is the one parallel-execution flake class below.

- **The one recorded failure was the ambient-env read, not drift.** `kcode-tui
  --lib`
  `test_remote_fallback_provider_suggestions_normalize_bare_openai_openrouter_routes`
  was recorded here on 2026-09-30 (found landing the todo step 1, failing at
  `4ef8c3e1` both with and without that change). Re-checked 2026-09-30 with every
  `KCODE_*` unset: it passes, and so does the rest of the suite. `kcode` exports
  its active provider profile as `KCODE_*` at startup, so a run from inside a
  kcode shell inherits it and the suite reads it as configuration. Keep the
  numbers below as the pre-correction measurement: `kcode-base` 1102/0, root
  `kcode` 180/0, `kcode-app-core` 998/0, `kcode-tui` 1830/1.
- **Parallel-only flakes.** `kcode-tui --lib` at the default thread count fails
  a changing set of about a dozen tests that pass single-threaded (slash
  pickers, `restore_session_*`, account settings, the model picker,
  `smoothness_benchmark_*`, `*_server_history_*`, `terminal_setup_*`). Root
  cause identified 2026-09-30: tests read configuration from process-global
  sources that other tests mutate concurrently, and the readers never take the
  `lock_test_env()` that the mutators hold. The live channels are `KCODE_HOME`
  (session save/restore and the config cache resolve through whatever home is
  set when they run), the `KCODE_SSH_REMOTE` / `KCODE_SSH_*` / `KCODE_MODEL` /
  `KCODE_PROVIDER` / `KCODE_RUNTIME_PROVIDER` family read at dispatch time by
  `tui::is_ssh_remote()` and friends, and ambient `KCODE_*` in the shell.
  Proven by `KCODE_SSH_REMOTE=test-remote <test> --exact` flipping a passing
  test to failing. App-core had the same shape (global bus, env/config) and is
  partly fixed. `--test-threads=1` is the workaround; serializing the readers on
  the same lock deadlocks or makes the suite >10 minutes, so the durable fix is
  to stop routing test configuration through the process environment.
- **A rare fork/PTY hang.** `terminal_setup_command.rs`'s
  `decode_key_event_via_pty` forks the multithreaded test process and runs
  crossterm in the child. Seen twice: the child wedges and the parent blocks on
  its result pipe forever (killing a CI job). Not reproduced on demand in 20+
  runs; it needs a concurrent lock held at fork time. Spawning a fresh process
  instead of forking would remove it. `docs/dev/testing.md` covers the class.
- **`docs/dev/testing.md` updated** on the same subject: it now describes the
  env/config-cache race above and the current zero single-threaded baseline,
  instead of the deleted render-lock race and the old per-suite counts.

## Next up: the dead-weight pass tail

The deletion pass landed 2026-09-29 (`516de13d`, `d0723556`, `7b2e3b7e`); what
remains is small and independent.

- [ ] Optional tail: §5 hygiene nits (config warnings, the
  `auth_remote/onboarding.rs` rename).

Not this batch: the `include!` test-tree replacement (§4) is paid for in test
churn, so it follows the shape work, and the God-module re-cores (§2) are their
own passes.

## Todo refactor (phase 1 landed 2026-09-30; phase 2 remains)

Delete the todo enforcement tier; keep the tool, its fields, Ctrl+P, and
`/poke`. A weak self-assessment must not drive the model onward, and the ladder
did exactly that; the wanted behavior is that the model may stop and ask. Phase 1
is done: the tier is gone, the poke is identified by shape, and the `todo` schema
is a five-field display (`id`, `content`, `status`, `priority`, optional `group`)
with no assessments. Intent, resolved decisions, and the final state:
`internals/todo-enforcement-removal.md`.

- [ ] Live check owed from step 2: the poke now reaches the model as a system
  reminder rather than as user content. One isolated `kcode run` with auto-poke
  armed, on a task that leaves items open, must show the model continuing rather
  than only replying. The fallback if it chats instead is in the internals doc
  (Step 2).
- [ ] Later, recorded as the desired final state in the internals doc: the list
  becomes a repository artifact (one markdown file in the working tree, written
  by the tool and readable by the maintainer, shared across sessions) and the
  automation that advances it writes each item's named check and its observed
  result into that file. Not planned yet; it has open design questions (which
  repository, two sessions in one repository, mid-session edits).

Gate: ratcheted files shrink, so each landing ends with
`scripts/check_guardrails.sh --fix`. The A/B probe recipe for comparing
verification and poke count before and after: `dev/todo-rework-ab-probe.md`.

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
| Dead local turn path | landed 2026-09-29 (~3.8k, not 810) | `App::run` (`run_shell.rs:198`) had zero callers (live entry is `run_remote`, `tui_launch.rs:160`); deleting it reached `turn.rs` (1,390), `turn_memory.rs`, `local.rs`, `process_queued_messages`, the model-context retry quartet, the local transfer path, and the helper cascade | the ~810 estimate counted only `local.rs` + `event_wrappers.rs` + `overnight_card.rs`; the whole dead engine was larger | deleted in `516de13d` / `d0723556` / `7b2e3b7e` |
| Second markdown renderer | landed 2026-09-29 (was ~4,700) | `render_markdown_via_core` had only test callers; the adapter's own doc said the legacy path "remains authoritative" | the one live symbol was `reasoning_line_markup`; the rest was a parallel parser/model/wrap | resolved: deleted the parser/model/wrap (1,193) + adapter (795), moved reasoning to `kcode-message-types`, and wired the engine into the renderer. `kcode-render-core` is now only the LaTeX engine + normalization (2,220). A follow-up (`e27d287a`) deleted the unwired `render_markdown_lazy` copy (983), which the streaming path never called |
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

- [ ] Move `smoothness_benchmark.rs` (313) out of `app/tests/`; the two
  provider-doctor halves (`live_provider_probes.rs`, `provider_e2e.rs`) already
  live in `kcode-provider-doctor/src/`. §4 owns the test-tree item.

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

  - **Member status is typed end to end (landed 2026-09-29).**
    `SwarmLifecycleStatus`, `SwarmRole`, and `SwarmMemberRecord` moved to
    `kcode-session-types`; `SwarmMember`, `SwarmMemberStatus`, `AgentInfo`,
    `AgentStatusSnapshot`, and `AwaitedMemberStatus` now carry the enum, and
    the "active/dead/terminal/in-flight" sets are the enum's methods. The
    durable `String` round-trip and the two `member_status_is_*` helpers are
    gone. What remains of the member projection: the TUI still synthesizes
    display words (`thinking`, `streaming`) into its own card's status; the
    presentation predicate `kcode_tui_render::swarm_gallery::is_active_status`
    still keys on those strings. Splitting lifecycle from display activity in
    the TUI is the open behavior decision.

  - [ ] **Member appearance follows the typed status** (planned 2026-09-29,
    decisions confirmed). `kcode-tui-render` takes `SwarmLifecycleStatus` for
    its member input (a new dependency on `kcode-session-types`, a data-only
    crate), so the six string matches in `swarm_gallery.rs` (41 status literals
    in production code, `is_active_status` at 12 call sites) become enum
    matches. One module owns member appearance: accent, glyph, label, sort
    rank, and an `is_working` predicate kept distinct from lifecycle
    `is_active`, because a stalled node must not spin. The duplicate appearance
    map `info_widget_swarm_background::swarm_status_style` is deleted. The
    swarm-path `Color::Rgb` literals become `kcode_tui_style` role accessors so
    `/colors` can recolor them; the small default shift for shades that are not
    exact role defaults is accepted (decision). Confirmed color decisions:
    adopt roles, accept the shift.
  - [ ] **A stalled plan node is visible** (planned 2026-09-29). `running_stale`
    is only ever a plan-item status. No member becomes it:
    `recover_member_status` maps `Running` to `Crashed` and `Ready` to
    `Stopped`, so the member enum's `RunningStale` is unreachable. Today
    `info_widget_todos::normalize_plan_status_for_todo` folds `running_stale`
    into `in_progress`, so a stalled node renders as a running one (`▶` amber,
    `[doing]`), and `tui_state.rs` counts it in the progress bar's running
    bucket. Give it its own display value and marker in the todo and plan
    widgets: a distinct glyph and label, `warning_color()`, sorted with
    `in_progress`. Decisions: reuse `warning_color()` and distinguish by glyph
    and label, no new palette role; leave the progress number's count as is, so
    only the per-node marker changes and stalled stays in the running bucket.

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

  Out of scope: `handle_client`, provider identity, and the
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
tree first would just move that churn around. The failure rows below are the
sampled causes; the Front line section owns driving them to zero.

- [ ] Replace the `include!`-wired test tree with real modules: `app/tests.rs`
  `include!`s 55 files into one module (120 `include!` sites repo-wide), which is
  why helper collisions and `use super::*` are everywhere.
- [ ] Condense near-duplicate tables: `state_model_poke_03.rs` (~40% near-dup
  pairs), `session_tests/cases.rs` (~51%), `remote_events_reload_04.rs` (~43%,
  header-phase table at 1038-1180). The
  percentages are per file and were not re-derived by the §0 census, which found
  only 13 cross-file duplicated test names; re-measure before treating them as
  targets.
  The todo step 1 (2026-09-30) shrank the first two of those files by deleting
  the tests for the removed tier, so both need re-measuring before they are
  ranked here again.
- [ ] Move subsystem code out of test files: `smoothness_benchmark.rs` (313)
  remains under `app/tests/`. The old `live_tests.rs` / `provider_e2e.rs` rows
  are stale: both already live under `kcode-provider-doctor/src/`.
- [ ] One home per duplicated test helper `[census]`; 13 names are defined in
  more than one file, with `lock_env` (7 files) and `test_agent`, `ctx` (4 each)
  the worst: `configured_swarm_root_effort_reads_real_config` (3),
  `available_models_display_seeds_from_persisted_catalog` (3), then
  `tracked_env_vars`, `clear_openai_compatible_runtime_env`, `create_test_app`,
  `ensure_test_kcode_home_if_unset`, `empty_swarm_status_state`,
  `load_auth_file_renames_existing_labels_to_animal_scheme`,
  `test_mask_email_censors_local_part`, `routes` (2 each).
- [ ] Pre-existing failures on this tree: `kcode-tui --lib` 38 failed / 1824
  passed at `06ab4a0f` (measured 2026-09-29; earlier 37 at `cd795377` and 27
  before that had drifted). After the local-turn-path deletion it is 41/1809,
  and the difference is flakiness, not regression: the four failures unique to
  the new tree (`slash_provider_test_coverage_*`, `slash_session(s)_alias_*`)
  pass in isolation, and `smoothness_benchmark_mid_transcript_growth_settles_quickly`
  only failed on the base run. `kcode-base --lib` 4 (was 15), root `kcode --lib`
  10 of 195 (last measured 2026-09-28). Sampled root causes are stale
  expectations for removed or renamed surface. Also math/LaTeX 15 and
  `test_lock_order` 1. Environmental, not regressions. Treat as the baseline; the
  suite still covers removed features and brittle pixel/color assertions, so
  collapse or delete rather than maintain.
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

- [ ] **(decision)** Where does work land: `main` directly, or a branch with a
  merge-back step? Two days and 85 commits went onto
  `batch-ab-provider-onboarding` (created 2026-09-29 05:16 from `cd795377`,
  merged by fast-forward 2026-10-01) while `main` did not move, and nothing
  noticed. Nothing was lost, since `main` held zero commits the branch lacked,
  but "is `main` green?" could not be answered from `main`, the pushed branch
  was the first visibility GitHub had, and the tree's uncommitted `pkgver` bump
  targeted a two-day-old HEAD. `AGENTS.md` says work on your own branch and
  stops there; this fork has no CI and no PR gate, and the 233 merge commits in
  `main`'s history show merge-back was the practice until this stretch. Pick
  one, then make it visible: if branches stay, a post-commit or session-start
  line when `git rev-list --count main..HEAD` is non-zero. The two hooks in
  `.githooks/` are graphify's and stay out of it.
- [ ] Not every color derives from a role. `configured_native_color`
  (`kcode-tui-style/src/palette.rs`) attributes a shade to a role only when it
  equals that role's default, so hardcoded `Color::Rgb(...)` shades pass through
  and `/colors` cannot recolor them. `info_widget_swarm_gallery.rs` and the rest
  of the swarm path are covered by the member-appearance item in §1; what
  remains here is `login_picker.rs` `PANEL_BG`/`PANEL_BORDER` and any other
  orphans. Give each shade a role, or mark it intentionally fixed.
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
- [ ] Reduce the always-on per-request tool cost. Two suite tests that capped
  tool and parameter descriptions (20 / 25 tokens) were removed rather than
  fixed, so nothing guards this today; the numbers below are the last measured.
  Worst offenders: `batch` (129), `swarm.model` (84),
  `browser.candidates` (73); 12 parameter descriptions are over 25 tokens. The
  `todo` schema's two worst offenders (`todo.feedback_loop_relevance` 132,
  `todo.feedback_loop_traceability` 94) went with the assessment fields (todo
  step 3), leaving that tool near 120 tokens total. The
  essays are paid on every request. Direction to consider: keep only the call
  contract always-on (name, one-line description, terse parameter shape plus
  enums/required) and give the prose a single home in the bundled,
  version-matched docs (`kcode_docs`), echoed in the tool's own error when a
  call is wrong. The gate messages that had their own token budget are gone with
  the enforcement tier (todo step 1). The same
  shape applies to the remaining essays. Control shape to consider: one
  aggregate schema-token budget instead of two per-item caps plus the `swarm`
  exemption.

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
