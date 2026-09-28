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

## 0. Decisions and baseline

Cheap, and they unblock the rest.

**Fork policy decided (2026-09-28): diverged.** There is no rebase lane and no
upstream to track; kcode is its own tree. What that decision unblocks, now
actionable:

- [x] **`KCODE_*` is the public env prefix (2026-09-28).** The process copies
  every `KCODE_*` variable onto its `JCODE_*` twin at startup
  (`crates/jcode-core/src/env.rs::adopt_kcode_env_prefix`, called first in
  `src/main.rs`), so both spellings configure kcode and the public one wins when
  both are set. State already lived in `~/.kcode`; the stale
  `<runtime_dir>/jcode/` socket directory is now `<runtime_dir>/kcode/`. The
  copy is direction-flippable, which is what keeps it alive across upgrades of
  the installed package.
- [ ] **Env literals: sweep to `KCODE_*` or declare them cosmetic.** Measured
  2026-09-28: 397 distinct names, 3012 string literals (1101 in production code,
  1911 in tests) across 280 Rust files, plus 58 non-Rust files (docs, scripts,
  packaging). Nothing user-visible depends on the literals now that
  `adopt_kcode_env_prefix` accepts the public spelling, so this is the same
  shape as the crate-name call above: a mechanical rename with no functional
  payoff. Needs a maintainer decision, and if it is a sweep, the startup copy
  flips to `JCODE_*` → `KCODE_*` for one release.
- [ ] **Crate names: pick (a) or (b), do not churn.** The workspace has 61
  members and 60 of them are `jcode-*`; the root crate sets `[lib] name =
  "jcode"` while its package is `kcode`. Either (a) rename the root `[lib]` to
  `kcode` and leave the crate names, or (b) declare the crate names cosmetic and
  change nothing. A mechanical rename of 60 crates has no functional payoff.
- **Ratchets: keep kcode's re-baselined numbers.** Decided. They measure this
  tree's drift, which is the point of a ratchet. Two things the decision
  surfaced were measured and are fixed below: the two size ratchets were never
  in the gate, and the binary rename left broken references behind.
- **`scripts/` triage: done** (classification in §5). The gate, the dev
  wrappers, and the documented harnesses stay; the one-off investigation
  scripts are deleted.
- **`packaging/arch/PKGBUILD`: done.** A `-git` source package, because kcode
  has no tags to build a release from. README documents `makepkg -si` and
  `aur build` with aurutils. Licence fields stay out of the Cargo manifests:
  nothing is published to a registry, and the PKGBUILD declares `MIT` next to
  `LICENSE`.

### The gate, measured (2026-09-27, HEAD `6dff3825`, rustc 1.98.1)

Every gate was run rather than trusted, and three things the old text here said
were wrong:

- **Clippy was undercounted.** The old text said "3 lints in `jcode-base`, 3 in
  `jcode-tui-workspace`, 1 in `jcode-compaction-core`" (7). The real run is
  **40 lints in 28 files across 12 crates**, because a `-D warnings` run stops
  at the first failing crate and only that crate's list had been read. Several
  are not hygiene but real smells: `src/cli/login.rs:862,892,903` "statement
  with no effect", `client_lifecycle.rs:1696,1725` and `turn.rs:908` "this if
  branch is empty", `comm_session.rs:550` "this lint expectation is unfulfilled"
  (a stale `#[expect]`), `browser_fast.rs:117,120` "`.fold` can be `any`".
- **Two red ratchets were unrecorded.** `code_size_budget.json` fails on 37
  files and `test_size_budget.json` on 19. Their baselines are jcode's numbers:
  `client_lifecycle.rs` reads `3282 -> 3610`, and 3610 is what the file has been
  since before the fork cut, so those checks can never pass. A gate that cannot
  go green is a gate nobody reads, which is how they went unrecorded.
- **The panic ratchet's classification is wrong**, not just its baseline.
  `build.rs`, `**/examples/**` and `**/src/bin/**` count as production, so
  `harness_repl.rs` alone is 15 of the 49 tracked hits and
  `provider-grok-build-runtime/src/bin/fake_acp.rs` is a test fixture. The
  baseline was also internally inconsistent: `total: 77` against a
  `tracked_files` sum of 49.

Decided and landed 2026-09-27:

- The swallowed-error ratchet is **deleted**. It counts `let _ =`, `.ok()`,
  `.unwrap_or_default()`: 3,129 hits across 423 files, which is idiomatic Rust,
  so its signal is dominated by code that is not a defect.
- The panic ratchet **excludes non-production paths** and is re-baselined to
  this tree: 51 hits in 24 files, down from a baseline whose `total: 77` did not
  match its own 49 tracked hits.
- Both size ratchets are **re-baselined** to this tree (79 oversized files, 34
  oversized test files), so they ratchet kcode's drift from here.
- Clippy is **fixed**, so `-D warnings` means something again. Three of the
  fixes were not hygiene: two `if tool_result.is_err()` bodies had been gutted,
  `login.rs` carried three `"api_key";`-style statements whose
  `telemetry::record_auth_success` consumer the fork cut removed, and clippy's
  suggested `any()` for `browser_fast.rs`'s credential redaction would have
  skipped the remaining credentials in a payload. The full gate is green.

### Gate follow-ups (2026-09-28)

Found while acting on the ratchet decision; both are fixed, and both were
invisible because nothing ran them.

- **The two size ratchets were not in the gate.** `check_guardrails.sh` ran the
  panic, dependency-boundary, wildcard, and `App`-shape ratchets, but neither
  `check_code_size_budget.py` nor `check_test_size_budget.py`: the only caller of
  the first was the (now deleted) `refactor_phase1_verify.sh`, and nothing at all
  called the second. They passed (`tracked=79`, `tracked=34`), so they were
  wired into `check_guardrails.sh` rather than deleted. A re-baselined ratchet
  that never runs still guards nothing.
- **The binary rename left broken references.** The fork renamed the root package
  and bin to `kcode`, but scripts still invoked `--bin jcode`, `-p jcode`,
  `target/{debug,release,selfdev}/jcode`, and a hardcoded
  `~/.local/bin/jcode`. None of those resolve: the declared bins are `kcode`,
  `test_api`, and `jcode-harness`. Worst case was `scripts/build_linux_compat.sh`,
  which built `-p kcode --bin kcode` and then copied `.../release/jcode` into the
  benchmark artifact, so the Harbor path could not have been producing the
  binary it named. Fixed in `build_linux_compat.sh`, `quick-release.sh`,
  `test_fast.sh`, `test_auth_e2e.sh`, `auth_fixture.sh`, `onboarding_sandbox.sh`,
  `check_startup_budget.sh`, `real_provider_smoke.sh`, `agent_trace.sh`,
  `antigravity_live_coverage.sh`, `memory_probe.sh`, `bench_startup.py`,
  `profile_spawn.py`, plus the selfdev prompt (`selfdev_mode.txt` and its test)
  and a rendered-command fixture in `ui_messages/tests.rs`. The `jcode-harness`
  bin name and the `JCODE_*` env names are deliberately unchanged (the latter is
  the env-prefix item above).

## 1. Shared shapes

Widest representation first, while the tree is still quiet. Sweeping call sites
before the shape is settled is churn.

- [ ] **One provider catalog, stated once.** The previous text here said "seven
  enums name the same thing; one `ProviderId` registry, derive the rest". That
  framing was wrong, and the correction matters, because merging what it named
  would build the god module this file exists to avoid.

  **They are three axes, and only one is duplicated.** `ActiveProvider` (8,
  `provider-core/src/selection.rs:5`) is the *execution slot* the process runs
  on, and `ConfigProviderSelection::active_provider` already proves it is
  many-to-one (`OpenAiCompatibleProfile(_) => OpenRouter`).
  `ModelRouteApiMethod` (15, `provider-core/src/lib.rs:885`) is the *per-model
  wire method*, parsed from a persisted string at module boundaries.
  `ProviderAvailability` (9 bools) is a *set of slots*. None of those should
  merge with each other or with identity.

  **What is duplicated is the catalog, five times over, plus one identity enum
  stated twice.** Measured by reading each table:

  - `ProviderChoice` and its `#[value(alias = ..)]` attributes: 53 variants,
    `src/cli/provider_init.rs`.
  - `ProviderChoice::as_arg_value()`: 53 arms.
  - `PROVIDER_CHOICE_LOGIN_PROVIDERS`: **deleted** (B2, landed 2026-09-28). 51
    hand-written pairs that restated what the descriptor ids already say.
  - `LOGIN_PROVIDERS: [LoginProviderDescriptor; 52]` plus 52 constants:
    `provider-metadata/src/catalog.rs`, 1,284 lines.
  - `list_cli_providers()`: **was** a hand-typed 26-element array; now every
    `ProviderChoice` resolved through the registry by `as_arg_value()`, plus
    `Auto` (B1 + B2, landed 2026-09-28).
  - `LoginProviderTarget` (14) and `RuntimeProviderId` (14) have identical
    membership, and `crates/jcode-base/src/auth/integration.rs:54-69` is a
    14-arm 1:1 conversion between them (`Azure -> AzureOpenAi`,
    `OpenAiCompatible(_) -> OpenAiCompatible`). A 1:1 conversion between two
    enums is a proof that they are one concept written twice.
  - `as_arg_value()` and `descriptor.id` agree on 51 of 52. The exception is
    the deprecated `claude-subprocess`, which deliberately points at the
    `claude` descriptor. So the pairing table is *derivable*, i.e. it is pure
    duplication rather than a distinct vocabulary.
  - **The `-p` value was a fourth spelling, and it disagreed for four
    providers.** clap derives each value from the variant name, and implicit
    kebab-case is not `as_arg_value()`: `Ai302` parsed as `ai302` not `302ai`,
    `HuggingFace` as `hugging-face` not `huggingface`, `MoonshotAi` as
    `moonshot-ai` not `moonshotai`, `TogetherAi` as `together-ai` not
    `togetherai`. The `alias` attributes are the tell: `HuggingFace` carried
    `alias = "hugging-face"` and `TogetherAi` `alias = "together-ai"`, i.e. the
    author believed the kebab spelling was the alias while clap silently made
    it the primary. So `provider list` printed `togetherai` while only
    `together-ai` parsed. Fixed in B1 by pinning `#[value(name = ..)]` to
    `as_arg_value()` and keeping the old spellings as aliases. Flagged here
    because B5 is where the fourth spelling stops existing at all.

  **The duplication already shipped a bug, now fixed.** 26 of the 52 values `-p`
  accepted were not printed by `kcode provider list`: `anthropic-api`,
  `openai-api`, `bedrock`, `gemini-api`, `celeris`, `huggingface`, `minimax`,
  `nvidia-nim`, `ollama`, `lmstudio` and 16 more, while the unknown-provider
  error told the user to consult that list. The class was live, not historical:
  `9d9d2259` ("Add Celeris provider support", 7 files, 59 lines) wrote one
  provider in five places and updated all of them except `list_cli_providers`.
  B1 made the list a view of the mapping, so all 26 gaps closed in one edit.

  The drift ran the other way too, and it was **red on this tree** until B2:
  `cli::provider_init::tests::login_provider_choice_table_round_trips_catalog_providers`
  (`provider_init_tests.rs:513`) asserted that every catalog id appears in the
  CLI choice table, and failed because `orcarouter` sat in `LOGIN_PROVIDERS` with
  `order` set on all five surfaces (so it was offered in the login menus) but had
  no `-p` value at all. The other unpaired descriptor, `auto-import`, is
  correctly exempt: its target is `AutoImport`, not a provider you select. Both
  this test and `auth_integration_registry_matches_cli_choice_runtime_wiring`
  are green as of B2.

  **Target.** One registry entry per provider, in `provider-metadata` where the
  descriptors already live and already have the right fields (`id`,
  `display_name`, `aliases`, `auth_kind`, `auth_state_key`, `target`,
  per-surface `order`). The CLI arg value *is* the registry id. "Profile-backed"
  is a property of the entry, not a list of 38 variants. Adding a provider is
  one file plus a registry line.

  Order, each lands whole; the first three delete code rather than move it.
  `--` marks the verification.

  - B1: **Landed 2026-09-28. `provider list` derives from the registry.** The
    26-entry `ProviderChoice` array is gone, so the list printed 52 ids instead
    of 26. Verifying the invariant surfaced the fourth-spelling bug above, so
    the four divergent variants also got `#[value(name = ..)]` pinned to
    `as_arg_value()`, with the old spellings kept as aliases so nothing that
    parsed before stopped parsing. `--` new
    `provider_list_ids_match_the_accepted_cli_values` compares the printed id set
    against `ProviderChoice::value_variants()` / `to_possible_value()`; it fails
    without the name fix (exactly the four ids) and passes with it. A companion
    alias test pins both spellings for all four.
  - B2: **Landed 2026-09-28. The pairing table is deleted.** 205 lines gone (the
    const, its accessor, and the two scans). `login_provider_for_choice(c)` is
    now `resolve_login_provider(c.as_arg_value())` with one explicit branch for
    the deprecated `claude-subprocess`, and `choice_for_login_provider(p)` finds
    the choice whose `as_arg_value()` equals `p.id` over `value_variants()`. The
    `!matches!(choice, ClaudeSubprocess)` filter disappeared with it, because
    `claude-subprocess` is not a descriptor id. Precondition verified before
    deleting, not assumed: for 50 of the 51 rows the registry lookup returned
    exactly the paired descriptor, and the 51st was the documented
    `claude-subprocess` case, so there were no alias collisions. The round-trip
    test now iterates `value_variants()` and asserts each choice resolves to a
    descriptor with the same id.

    B2 also fixed the defect it exposed: **`ProviderChoice::OrcaRouter` added**
    (arg value `orcarouter`, alias `orca-router`), because `orcarouter` was a
    registry provider offered on every login surface with no way to select it.
    `provider list` prints 53 ids, all 53 parse, and both red round-trip tests
    are green. Adding a provider is now enum variant + `as_arg_value()` arm +
    the profile-backed arm in `init_provider_with_options` + descriptor (four
    places, three of them in one file); B3 removes the profile-backed list and
    B5 the enum.
  - B3: **Collapse the 38-variant arm** in `init_provider_with_options` (the
    "is profile-backed" list) onto the predicate that already exists one screen
    away, `profile_for_choice` (`provider_init.rs:207`). B2 had to append
    `OrcaRouter` to that list to compile; B3 deletes the list so the next
    provider does not need the arm at all.
  - B4: **One identity enum.** Delete `RuntimeProviderId`'s 14-arm bridge.
    Preferred shape: keep `ProviderActivation` free of the profile payload and
    give the descriptor a `runtime_key()` accessor. `--` read
    `activation.rs`'s consumers before committing.
  - B5: **The registry drives the clap values.** Delete the 53 variants, their
    aliases, and `as_arg_value()`; build `PossibleValuesParser` from the
    registry with `PossibleValue::new(id).help(display_name).alias(..)` and
    `.hide(true)` for the deprecated one, which keeps `--help` and completions.
    Then `-p` accepts exactly what `provider list` prints, by construction.
    About 250 `ProviderChoice::` sites in 16 files, 49 of them in
    `provider_init_tests.rs`. Last and alone, gated on the provider-doctor
    suite. B1 already added names and aliases additively; B5 is the stage that
    *removes* the variant set and rebuilds the parser, so it is the one that can
    drop an accepted value by accident.

  Not doing, with reasons: not merging `ActiveProvider` (execution slot, so
  merging gives one enum doing three jobs), not merging `ModelRouteApiMethod`
  (routing is not identity), not touching `ProviderAvailability`'s 9 bools
  (~30 sites, readable, the win does not pay for the churn). The old "153 files
  hardcode provider strings" figure was not re-derived here; what was measured
  is the table structure above.

  Open question, not a claim: `fallback_sequence`
  (`provider-core/src/selection.rs:315`) is a hand-written 8x8 failover table,
  8 arms each restating one priority order with the active provider moved
  first. The Claude and OpenAI arms omit `Antigravity`, the other six include
  it. Deliberate frontier-pair policy or an omission? Unread: the failover call
  path. If it is "active first, then a canonical order", the ~80-line table
  collapses to two lines.

  Done when: adding a provider is 1 file plus a registry line (today it is enum
  variant + `as_arg_value()` arm + descriptor, down from 6 files and ~55 lines),
  the `-p` accepted set equals the `provider list` printed set (true since B1),
  and no pairing table exists (true since B2).
- [ ] Provider cleanup keeps every provider reachable. No provider is deleted
  (maintainer decision 2026-09-27): every cut candidate backs a user-selectable
  provider (`ProviderChoice` exposes Cursor, Copilot, Antigravity, GrokBuild,
  Bedrock; `provider-metadata` has 5 dependents). The 20 `jcode-provider-*`
  crates (61.1k lines) stay. Cleanup is the identity unification above.
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

- [ ] **Re-core `App`** (`crates/jcode-tui/src/tui/app.rs`): the largest single
  cost in the tree, so the plan lives here in full now that `plans/` is gone.

  Evidence: `struct App` spans `app.rs:821-1501`, 285 fields (measured by
  `scripts/check_app_shape.py`; 310 at the start of this work), 57 `impl App`
  blocks across 53 files, 124 `use super::*` globs. `app.rs` is 2,425 lines,
  still over the size ratchet. The runtime axis is now a single representation
  (`runtime_mode`, Stage 1). `create_test_app()` is the graph's #1 hub (771
  edges), so every shape change ripples through test construction first.

  Target: `App` becomes a coordinator holding named sub-structs, and each
  sub-struct owns the methods that touch only it. The pattern is already in tree
  (`impl OnboardingFlow`, `impl RemoteLogin`). Renaming `self.scroll_offset` to
  `self.viewport.offset` is churn unless the methods move with the fields; that
  move is the point. Group names come from the `TuiState` trait's own section
  headers.

  Order is isolation first, coupling last; each stage lands whole.

  - Stage 0: **Landed.** `scripts/check_app_shape.py` (with
    `scripts/app_shape_budget.json`) measures `app_fields`, `impl_app_blocks`,
    and `super_glob_imports` and refuses growth; wired into
    `scripts/check_guardrails.sh`. Baseline after Stage 1: `app_fields=308`,
    `impl_app_blocks=57`, `super_glob_imports=124`. `cargo test -p jcode-tui`
    baseline in `dev/testing.md`.

  Cohesion (first cut, 2026-09-27): direct `self.<field>` reads per `impl App`
  method, fields assigned to a group by name (approximate; no transitive calls,
  and a "pure" method may still call a coupled one). Methods touching fields of
  exactly one group and no other group's fields: panels 20/43, input 22/67,
  viewport 12/35, copy_selection 6/18, todos_view 4/14. No group is cleanly
  isolated. `copy_selection`, the stage-2 template, scored low on that cut; it
  landed anyway as a state + self-contained-method extraction once the mirror
  pages had set the pattern. The order below is provisional; re-measure before
  each extraction.
  - Stage 1: one runtime axis. **Landed.** `is_remote` and `is_replay` are gone;
    `runtime_mode` is the single representation, written only through
    `App::set_runtime_mode` and read through `is_remote_client()` /
    `is_replay_runtime()`. `cargo test -p jcode-tui --lib` is unchanged from the
    baseline (1966 passed, same 27 failed), and `app_fields` fell 310 -> 308.
  - Stage 2: `CopySelection`. **Landed.** Eight `copy_selection_*` fields and
    their self-contained methods (`exit_mode`, `current_pane`, `normalized`,
    `current_text`) live in `copy_selection.rs` as `CopySelection`; `App` holds
    one `copy_selection` field. The drag/anchor methods that also touch
    `diff_pane_*`, auto-scroll, and status notices stay on `App`. `app_fields`
    fell 292 to 285, tests unchanged from baseline. `copy_badge_ui` is a
    separate concept and stayed.
  - Stage 3: `Viewport` (13 fields, 145 sites): scroll, bookmark, redraw flags.
    Measured: it is two concepts, not one, and each half is cohesive.
    **Redraw half landed:** `force_full_redraw`, `force_full_repaint`,
    `last_resize_redraw`, `resize_redraw_pending` and their five methods
    (`request_full_redraw`, `request_full_repaint`, `commit_resize_redraw`,
    `should_redraw_after_resize`, `flush_pending_resize_redraw`) move into
    `redraw.rs` as `Redraw` (out of `input.rs`); `app_fields` fell 248 to 245.
    The rename was receiver-aware because `force_full_redraw`/`force_full_repaint`
    also live on `run_shell`/`ui_frame_metrics`. **Scroll half landed:** the five
    scroll fields (`scroll_offset`, `auto_scroll_paused`, `pending_history_anchor`,
    `scroll_bookmark`, `typing_scroll_lock`) move into `viewport.rs` as
    `Viewport`, along with the six methods that touch only them
    (`follow_chat_bottom`, `debug_scroll_top`, `follow_chat_bottom_for_typing`,
    `capture_history_anchor`, `reconcile_history_anchor`, `scroll_to_next_prompt`);
    `app_fields` fell 245 to 241. The `TuiState` getters stay on `App` (the trait
    is implemented for `App`), and the keys/commands that also touch
    `set_status_notice`, `scroll_max_estimate`, or the compacted-history loader
    stay on `App` and read through the struct. `last_wheel` and
    `overscroll_status_mode` are not scroll state either and remain on `App`.
  - Shared ephemeral side-panel page. **Landed.** The four mirror-page builders
    now call `SidePanelPage::ephemeral_markdown`, so the page shape lives with
    the type instead of four times over.
  - `Swarm`. **Landed** (App-side state only). Seven `swarm_*` fields (member
    snapshots, plan, panel selection) move into `swarm.rs` as `Swarm`; the
    panel navigation and subtree filtering stay on `App` because they read
    config, session identity, and the transcript. `swarm_enabled`,
    `debug_force_inline_gallery`, and `swarm_hint_shown_this_session` stay as
    flags on `App`. `app_fields` fell 285 to 279. The cross-crate swarm/comm
    condensation (phase 1) is separate and untouched.
  - `HotkeyFeedback`. **Landed.** The inline note slot, the persisted per-action
    usage counters, and the unknown-chord tracking move into
    `hotkey_feedback.rs` as `HotkeyFeedbackState`; `app_fields` fell 271 to 268.
  - `ReloadState`. **Landed.** Ten session-maintenance fields (the requested
    reload/rebuild/update/restart actions, background client maintenance, the
    server reload handshake, and the reload reconnect status) move into
    `state_ui_maintenance.rs` as `ReloadState`; `app_fields` fell 268 to 259.
  - Stage 4: `TodosView`. **Landed as the first extraction** (moved up from
    here). Nine `todos_view_*`/`pinned_todos_*`/`todo_card_rendered_hash` fields
    plus their sub-struct-only methods live in `todos_view.rs` as
    `TodosView`; `App` holds one `todos_view` field. `app_fields` fell 308 to
    300, tests unchanged from baseline. The gate/poke todo fields (e.g.
    `auto_poke_*`, `todo_completion_gate_attempts`) are a different concept and
    stayed on `App`.
  - `SplitView`. **Landed.** Five `split_view_*` fields plus their
    sub-struct-only methods moved into `split_view.rs` as `SplitView`;
    `app_fields` fell 300 to 296, tests unchanged from baseline.
  - `Observe`. **Landed.** Three `observe_*` fields plus `enabled`/`page` moved
    into `observe.rs` as `Observe`; `app_fields` fell 296 to 294, tests
    unchanged from baseline. The reload-state fields on `RestoredReloadInput`
    are a different struct and stay put.
  - `Catchup`. **Landed.** Three `catchup_*` fields plus the resume
    bookkeeping methods (queue, take, begin/clear in-flight, pop return
    target) moved into `catchup.rs` as `Catchup`; `app_fields` fell 294 to 292,
    tests unchanged from baseline. `maybe_show_catchup_after_history` stays on
    `App` (it loads a session and pushes a message).
  - Stage 5: `Panels` (12 fields, 89 sites): side panel and split view.
    The mirror pages (split view, todos, observe, catchup) are out; the
    `side_pane_*`/`diff_pane_*` fields remain.
  - Shared side-panel decoration. **Landed.** The three mirror pages (todos,
    observe, split view) plus catchup each carried a private copy of the same
    two algorithms. They are now `App::decorate_side_panel_with_page`,
    `App::snapshot_without_page`, and `App::apply_mirror_page` (the
    enable/disable dance the three setters shared) in `state_ui.rs`; the
    per-page cache refresh and content builders stay local. No per-caller
    flags.
  - Stage 6: overlay/picker state (14 fields, 174 sites): session, model,
    account, and login pickers, one sub-struct each. **Session picker landed:**
    `session_picker_overlay`/`session_picker_mode`/`pending_session_picker_load`
    are now `App::session_picker: SessionPickerState` in `session_picker.rs`;
    `app_fields` fell 279 to 277. **Account picker landed:** the overlay, pending
    action, and pending follow-up input are `App::account_picker:
    AccountPickerState` in `auth_account_picker.rs`; `app_fields` fell 277 to
    275. **Usage overlay landed:** the overlay and its refresh flag are
    `App::usage: UsageOverlayState` in `usage_overlay.rs`; `app_fields` fell 275
    to 274. **Model picker landed:** the cache, catalog revision, pending load,
    and load request id are `App::model_picker: ModelPickerLoadState` in
    `inline_interactive.rs`; `app_fields` fell 274 to 271. The login picker
    remains.
  - Stage 7: stream/status (26 fields, 414 sites). **Reasoning slice landed:**
    the live thinking/reasoning stream (9 fields: the thinking buffer/line
    state, the reasoning tail offsets, and the per-turn traces) moves into
    `reasoning.rs` as `ReasoningState`; `app_fields` fell 259 to 251. **Last-API
    slice landed:** the cache-TTL inputs (`last_api_completed[_provider/_model]`,
    `last_turn_input_tokens`) move into `state_ui_runtime.rs` as
    `LastApiResponse`; `app_fields` fell 251 to 248. The rest of the region is
    not a mechanical sweep: `is_processing`, `status`, `streaming`,
    `status_detail`, `subagent_status`, `batch_progress`, `processing_started`,
    `stream_message_ended`, `context_limit`, `context_info`, and
    `context_revision` collide with other structs or method names, and the
    remaining turn-lifecycle fields (`pending_turn`, `cancel_requested`,
    `visible_turn_started`, `last_stream_activity`, `last_user_interaction`,
    `deferred_stream_done_id`, `last_stream_error`,
    `attempt_committed_assistant_messages`) serve different consumers, so they
    are not one struct. Split per concept with the receiver type checked.
  - Stage 8: provider and model context (15 fields, 281 sites); coordinate with
    the provider-identity item above, do not create a second registry.
    **Measured larger than the estimate**: the candidate fields total ~500
    `self.`/`app.` sites across 30+ files, and they mix two concepts
    (provider connection: name/model/transport/credential/reasoning/service
    tier; model selection: picker cache, pending switch/route/reasoning). Two
    names also live on other structs (`upstream_provider`,
    `provider_session_id`). Split along that seam before moving anything; do
    not lump the two into one sub-struct.
  - Stage 9: `Input`. Measured: 21 fields, ~1940 direct sites, 69 files, and
    the names are hyper-common (`input`, `cursor_pos` also parameters, locals,
    and fields on `backend`/`debug`/`session_picker`/`ui_input`). Split into
    slices with the receiver-aware rename plus a call-site guard (`input()` and
    `command_suggestion_selected()` are `TuiState` accessors).
    - **Command suggestions landed:** `command_candidates_cache`,
      `command_suggestions_cache`, `command_suggestions_epoch`,
      `command_suggestion_selected` move into `state_ui_input_helpers.rs` as
      `CommandSuggestions`, with the two methods that touch only them
      (`advance_epoch`, `invalidate_candidates_cache`); `app_fields` fell 241 to
      238.
    - **Composer landed:** `input`, `cursor_pos`, `stashed_input`,
      `input_undo_stack`, `tab_completion_state` move into `composer.rs` as
      `Composer`, with the four methods that touch only them
      (`clear_input_undo_history`, `remember_input_undo_state`,
      `find_word_boundary_back`, `find_word_boundary_forward`) and the undo
      limit; `app_fields` fell 238 to 234. `set_input_for_test` (a 50-call-site
      test helper) and the inherent `input()`/`cursor_pos()` getters stay on
      `App`. This begins draining `input.rs`.
    - **Prompt history landed:** `prompt_history_search` and
      `persisted_prompt_history` move into `prompt_history.rs` as
      `PromptHistoryState`, with the loader it owns (`ensure_loaded`);
      `app_fields` fell 234 to 233.
    - **Composer payload landed (behaviorally chosen):** `pasted_contents` and
      `pending_images` move into `Composer` too. The seam came from behavior, not
      cohesion: `take_prepared_input` (`input.rs:1495`) reads `composer.input`,
      `pasted_contents`, `pending_images`, and `composer.cursor_pos` together,
      clears the pastes, and takes the images to build the message; `input.rs:1587`
      asks "is there a draft?" as `input` + `pending_images`. `interleave_message`/
      `interleave_images` were **not** included: they are a soft-interrupt payload
      that gets merged into `pending_images` at send time, i.e. part of the
      outbound pipeline. `app_fields` fell 233 to 231.
  - Stage 10: transcript, then session/server (18 fields, 390 sites); sweep the
    ~121 remaining loose fields into their owners.
    - **Stage 10a behavior trace (done, no code change).** The transcript is
      four fields, not thirteen: `display_messages`, `display_messages_version`,
      `display_user_message_count`, `display_edit_tool_message_count`. Their home
      is already `state_ui_messages.rs`; the trace says why they belong together
      and where the seam leaks.
      Verified invariant: the two counters are `O(1)` caches of the vec (the
      comment records that appending M messages was once `O(M^2)`), and every
      mutation must bump the version, which is the *change notification* the
      render caches key on: `split_view.rendered_display_version`
      (split_view.rs:92), the navigation wrapped-line cache (navigation.rs:312),
      `ui_prepare` (:558, :958), frame-equality in `ui_frame_metrics`
      (:720, :738), pinned diffs (ui.rs:2303). `bump_display_messages_version_
      no_stats` (state_ui.rs:182) also bumps `context_revision` and calls
      `refresh_split_view_if_needed`, so the version is a fan-out point, not a
      private counter.
      Checked, not assumed: `context_revision` is *not* a duplicate of the
      version. It is bumped from `conversation_state.rs:414` too (provider
      context) and its readers cache provider render info
      (tui_state.rs:1018-1029), so the two stay separate.
      **The leak, and the reason to extract:** the mutation surface is not
      contained. Six files outside the module reach into the vec directly and
      have to remember to bump: `input.rs:3461`
      (`remove_reasoning_trace_messages`), `state_ui_maintenance.rs:85`
      (in-place title/content) and `:117` (remove), `update_sim.rs:34` (retain),
      `turn.rs:814` (streaming tool output writes `dm.content`),
      `navigation.rs:1013` (drops the Ctrl+L spacer on scroll-up), plus whole-vec
      assignment in `debug.rs:349` / `debug_bench.rs:97`. Two of those also
      hand-call `refresh_split_view_if_needed`. The "mutate, remember to bump,
      maybe refresh" ritual recurs at ~15 sites.
      **The module is also three concepts.** `background_task_rows` with its
      `retain_latest_background_tasks` / `upsert_*` / `finish_*` / `prune_*`
      (the pinned task band) and `last_overnight_card_refresh` with
      `maybe_refresh_overnight_display_card` / `upsert_overnight_display_card` /
      `maybe_tail_overnight_current_session_transcript` (the overnight card
      writer) share the file with the transcript and touch it only through
      `push_display_message` / `replace_*`. Separate concerns, not transcript
      parts; the old "13 fields" estimate would have lumped them.
      **Move:** extract `Transcript` (`app/transcript.rs`) owning `messages`,
      `version`, `user_message_count`, `edit_tool_message_count`, with the stats
      machinery (`recompute_display_message_stats`,
      `adjust_display_message_stats`) moved next to it from `state_ui.rs`. `App`
      keeps the fan-out (`bump_display_messages_version` = version bump +
      `bump_context_revision` + `refresh_split_view_if_needed`) and delegates.
      Churn measured: 357 read sites already use the `TuiState::
      display_messages()` accessor; ~220 direct field sites remain (54 whole-vec
      assignments, 14 push, the rest len/iter/last), 146 of them in `app/tests*`.
      Reads go through `messages()`; whole-vec writes through `set_all()` so tests
      pass the same invariant; the six out-of-module direct mutations above
      become the module's own methods.
      **Landed.** `app/transcript.rs` holds `messages`, `version`,
      `user_message_count`, `edit_tool_message_count`, and `messages` is private,
      so every mutation goes through an op that keeps the counters right. Two
      concepts disappeared: the `bump_display_messages_version` /
      `_no_stats` pair (it existed only to avoid an `O(M)` rescan, and the rescan
      is gone) and the whole `state_ui.rs` stats block
      (`recompute_display_message_stats`, `adjust_display_message_stats`).
      `bump_display_messages_version` remains as the one cross-cutting fan-out
      (version + `context_revision` + split view). `app_fields` 231 -> 228 (four
      fields out, one `transcript` field in); `super_glob_imports` stayed at 124
      by importing explicitly instead of globbing. Verified: check, clippy, and
      `cargo test -p jcode-tui --lib -- --test-threads=1` identical to the
      baseline (1966 passed, the same 27 pre-existing failures); the gate is red
      only on the three recorded items.
    - **Stage 10b/10c, separate slices:** give the pinned background-task band
      and the overnight card writer their own homes out of
      `state_ui_messages.rs`. Same shape as 10a: fields private, every mutation
      through an op that keeps any derived state right, reads through one
      accessor with no `len()`/`last()` shortcuts beside it, `pub(super)` and
      `#[cfg(test)]` only where a caller actually needs it. Both touch the
      transcript only through its public ops, so they can move without it.
      **10b landed (pinned background-task band).** `app/background_tasks.rs`
      holds `BackgroundTaskBand`: `rows` is private and the band owns the trim
      (`retain_latest`, at most two rows, newest last) plus `upsert_running` /
      `upsert_progress` / `upsert_started` / `finish` / `prune_irrelevant`.
      The seven `background_task_rows_*` methods are gone from `impl App`;
      callers read through `background_tasks.rows()` and mutate through the
      ops (`app.background_tasks.upsert_started(..)`), with `rows_mut()` under
      `#[cfg(test)]` for the one test that ages a completed row. Net-zero on
      the ratchet, as expected for a field-move: `app_fields` stays 228 (one
      `background_tasks` field in, one `background_task_rows` field out),
      `impl_app_blocks` 57 and `super_glob_imports` 124 unchanged, so no
      re-baseline. `state_ui_messages.rs` 813 -> 690 lines. Verified:
      `cargo test -p jcode-tui --lib -- --test-threads=1` identical to the
      baseline (1966 passed, the same 27 pre-existing failures), check, clippy,
      fmt, and `check_app_shape.py` all green.
      **10c landed (overnight card writer).** `app/overnight_card.rs` holds
      `OvernightCard` (the poll throttle: `last_refresh` private, one
      `begin_refresh` op) plus the card operations. The writer touches the
      transcript and the session, not just its own field, so the operations are
      free functions over `&mut App` (`refresh_if_due`, `upsert_card`) instead
      of a new `impl App` block; a new `impl App` file would have bumped
      `impl_app_blocks` 57 -> 58, which the ratchet forbids. The free-function
      shape also matches the sibling `commands_overnight.rs`. The
      session-to-display conversion (`display_message_from_stored_message`,
      `stored_message_visible_text`) moved with the tail that was its only
      caller; `is_background_task_lifecycle_message` stayed in
      `state_ui_messages.rs` as `pub(super)` because the transcript ops and the
      card writer both need it. `state_ui_messages.rs` 690 -> 540 lines.
      Ratchet flat on the field swap: `app_fields` 228 (one `overnight_card`
      field in, one `last_overnight_card_refresh` out), `impl_app_blocks` 57,
      `super_glob_imports` 124 (imports explicit, not `use super::*`). Verified
      as 10b: identical lib suite, check, clippy, fmt, shape green; the three
      red gate items are the pre-existing recorded ones (local.rs was already
      at 6 at HEAD, and neither new file appears in the panic/swallowed
      reports).
      **Session/server recon (seam analysis, kept for reference; the slices
      below have since landed).** The region is not one concept, and the
      trait's "Session / server" section (`tui/mod.rs:489`) is a presentation
      grab-bag, not a field contract. The live connection object is loop-local
      (`RemoteRunState`, `remote/reconnect.rs:17`); the `remote_*` fields on
      `App` are the render-facing snapshot. Seams found: (1) server hello
      snapshot, 7 fields, single writer in the History handler
      (`server_events.rs:1501-1503`, `1647-1653`), never reset on session
      switch; (2) startup phase, 2 fields, ops `set/clear_remote_startup_phase`
      already hold the invariant; (3) history-recovery watchdog, 3 fields, the
      whole state machine is `recover_stuck_remote_history`, ops
      `begin/clear_remote_history_wait` exist; (4) `server_spawning` (op exists)
      vs `auto_server_reload` (cached `display` config, not connection state);
      (5) `current_message_id` and `remote_session_id` are turn/session
      identity, not server info. The session-switch reset
      (`server_events.rs:1545-1602`) and History handler are App-level
      orchestration across ~12 clusters and must stay so (sub-structs expose
      ops; App keeps the sequence). **Dead-weight removal:**
      `last_client_focus_session_id`, `last_client_focus_recorded_at`,
      `CLIENT_FOCUS_RECORD_DEBOUNCE`, and `note_client_focus` were residue from
      `c4a95aa5` ("feat!: remove dictation"), which deleted the only body that
      wrote the fields, leaving a no-op recorder with six call sites. Deleted;
      `app_fields` 228 -> 226, baseline refreshed. Verified: lib suite identical
      (1966/27), check, clippy, fmt, shape green.
      **Server/client state extraction landed.** `app/remote_state.rs` now
      holds `RemoteServerInfo` (7 -> 1; flat snapshot, fields `pub(super)` since
      it has no derived state), `RemoteStartup` (2 -> 1; `set`/`clear` moved off
      `App` with their reset-started-on-change invariant), and `HistoryRecovery`
      (3 -> 1; the first-delay / rate-limit / give-up transition moved into
      `HistoryRecovery::step`, returning `Wait`/`GiveUp`/`Retry`, so `remote.rs`
      only supplies the environment and does the I/O). `App` keeps thin
      `set/clear_remote_startup_phase` wrappers because `ui_header.rs` is outside
      `app`; the `begin/clear_remote_history_wait` wrappers were deleted (all
      callers are `app` descendants and call the struct). `app_fields` 226 ->
      217, baseline refreshed; `impl_app_blocks` 57, `super_glob_imports` 124.
      Verified: lib suite identical (1966/27), check, clippy, fmt, shape green.
      Follow-up: `RemoteStartup` exposes `phase`/`started` as `pub(super)` with
      only `set`/`clear`; its four single-caller accessors were deleted, matching
      `RemoteServerInfo`.
      **Active-session-id resolution condensed.** The
      `if is_remote_client() { remote_session_id } else { session.id }` policy was
      reimplemented in nine places: `state_ui.rs` (`active_client_session_id`),
      `app.rs` (kv-cache id), `tui_state.rs` (current_session_id, context key,
      info widget, workspace rows + map, swarm subtree), `tui_lifecycle_runtime.rs`
      (terminal title), and `run_shell.rs` (run result). All now call the existing
      `active_client_session_id()`; the two sites that deliberately fall back to
      the local stub id keep an explicit `.unwrap_or(&self.session.id)`. No fields
      moved, no ratchet change. Verified: lib suite identical (1966/27).
      **Resume-target resolution condensed.** The
      `remote_session_id.or(resume_session_id)` policy was retyped in nine
      expressions: `update_terminal_title` (ssh), `reconnect_target_session_id`,
      `session_display_name`, workspace setup, session persistence,
      `SessionRenamed`, and both reconnect status messages. One accessor,
      `App::resume_target_session_id()`, now holds it; `reload_handoff_session_id`
      keeps its extra `pending_reload_session_id` middle step. No fields moved,
      no ratchet change. Verified: lib suite identical (1966/27).
      **Deferred: `current_message_id` belongs to the remote turn lifecycle.**
      It is written with `is_processing`/`status`/`processing_started`/
      `visible_turn_started`/`last_stream_activity` at every remote send
      (`input_dispatch.rs:27,474`) and cleared at Done/disconnect/cancel/stall
      (`server_events.rs:1011,1128,1182,1221`, `remote.rs:930,1621`,
      `input_dispatch.rs:342,360`), and compared against the Done id. It is the
      in-flight remote turn id, not session or server-snapshot state. Put it with
      the turn-lifecycle cluster (`deferred_stream_done_id`, `pending_turn`,
      `remote_resume_activity`) when that cluster is extracted.
      **`PendingSplit` extracted.** `app/pending_split.rs` holds the nine fields
      describing a pending launch of the current work into a new sibling session
      (split, transfer, review, workspace add: the server models all of them as
      one `split()`, distinguished by `label`). The nine-field clear retyped in
      five places is now `clear_payload()`, and the `SplitResponse` handler
      consumes the payload with `take_payload()`; `PendingSplitPrompt` moved with
      it. app_fields 217 -> 209, baseline refreshed; impl_app_blocks 57,
      super_glob_imports 124. Verified: lib suite identical (1966/27), check,
      clippy, fmt, shape green.
      **Still open from this region:** `server_spawning`/`auto_server_reload`
      (seam 4; `auto_server_reload` is cached `display` config, not connection
      state), `remote_session_id` (kept: active session identity, 86 sites),
      `pending_local_transfer` (the local transfer prepare channel), and
      `route_next_prompt_to_new_session` (the next-prompt arm toggle). The last
      two are single fields with their own lifecycles and are not worth a
      struct.
  - Stage 11: revisit `TuiState`. It is a 122-method trait with two impls, and
    `TestState` (39 fields, 83 sites, 13 files) exists so render tests avoid
    constructing an `App`. Deleting it is a trade, decided last, once `App` is
    cheap to construct.

  Out of scope: `handle_client`, the local turn path, provider identity, and the
  crate spine are separate items.

  Done when: the field count and `impl App` count fall monotonically
  (`check_app_shape.py`), `use super::*` falls from 124, and `app.rs` drops out
  of `code_size_budget.json`.

  - [ ] **Behavior-check the landed extractions.** They were cut by *cohesion*
    (which methods touch which fields), not by tracing behavior; tests prove the
    moves preserved behavior, they do not prove the boundaries are right. A
    cohesion cut can slice a state machine in half where two fields are written
    in sequence by different methods, or joined where they merely co-occur.
    Verify per landed struct by tracing the write sequences at turn/reconnect/
    reset boundaries and confirming no transition spans two structs. Highest
    risk: `ReloadState` (ten fields grouped under "session maintenance" and
    never behaviorally traced; may be the background-client-maintenance half and
    the server reload handshake, not one thing). Then `Swarm`, then the
    `Redraw`/`Viewport` split. Lower risk (single render path, methods moved with
    their fields): the four mirror pages, `CopySelection`, `Composer`,
    `CommandSuggestions`, `PromptHistoryState`, and the picker/overlay states.
    Evidence over looks: the outbound-input region's field split was wrong for
    exactly this reason (`queue_recovery.rs` converts soft interrupts, interleave
    messages, and in-flight sends into `queued_messages`, so those are one
    pipeline, not three groups).
    **Verified 2026-09-27 (this session's structs).** Traced writers and
    boundaries for `BackgroundTaskBand`, `OvernightCard`, `RemoteServerInfo`,
    `RemoteStartup`, `HistoryRecovery`, and `PendingSplit`: no module leaks
    writes around its ops (only tests, plus `RemoteServerInfo`'s intentional
    `pub(super)` fields), and the session-switch reset touches none of them
    except `clear_remote_startup_phase` (correct: a switch ends "loading
    session"). One defect found and fixed: the resume-target condense dropped
    the per-source name fallback in the two reconnect status messages, so a
    remote id carrying no parseable name no longer fell through to the resume
    id. Not yet traced: `ReloadState`, `Swarm`, the `Redraw`/`Viewport` split,
    and the lower-risk list above.
  - [ ] **Re-core the outbound user-input pipeline** rather than condense it.
    `queued_messages`, `hidden_queued_system_messages`, `pending_soft_interrupts`,
    `pending_soft_interrupt_requests`, `interleave_message`, `interleave_images`,
    `queued_followup_starved_since`, `pending_queued_dispatch`,
    `rate_limit_pending_message`, and `rate_limit_reset` are parallel
    representations of one thing, stitched by hand-written conversions in
    `remote/queue_recovery.rs`. Packing them into a struct keeps the duplication;
    the fix is one representation (a queue of outbound items carrying status and
    ack id) that deletes the conversions. Needs a fresh context; not a field-move.
- [ ] **Split `handle_client`** (`crates/jcode-app-core/src/server/client_lifecycle.rs`).
  Researched 2026-09-28 by reading the function and its call sites, not
  estimated. The line count is real; the god-ness is not where the old one-liner
  put it, because the arms are already thin and mostly delegate.

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
  - H5: **The turn-lifecycle locals.** Same concept as the `App` stage-7 turn
    cluster: one owner for the in-flight turn, with the ops that keep
    `processing_message_id`, the task handle, and `client_is_processing`
    consistent. Do it after H3 has made the prologue readable, and reuse the
    `App` result rather than re-deriving it.

  Done when: `handle_client` is under ~600 lines, `client_lifecycle.rs` is out of
  the size budget, and no `SwarmState { .. }` literal is constructed inside a
  request arm.
- [ ] **Unify the command surface**: slash-command identity is a string matched
  in four places: registry `REGISTERED_COMMANDS` (113 literals), `commands.rs`
  (59), `commands_dispatch.rs` (85), `remote/key_handling.rs` (54).
  `commands_dispatch.rs:1-13` records the last drift bug. One table
  (name, aliases, help, handler, remote-safe); fixes the `/help` gap and the
  dead SSH-block commands for free. Shares `commands_dispatch.rs` with the
  `App` re-core, so keep them in separate changes.
- [ ] **The local in-process turn path** (`crates/jcode-tui/src/tui/app/local.rs`
  and the orphaned `App::run`): `run` has zero callers; it was `pub`, so the
  compiler kept the whole subtree live. Deleting it alone surfaces ~41 items as
  dead and regresses the 0-warning baseline. It is the documented
  `AppRuntimeMode::TestHarness` path (tests call `local::handle_tick`,
  `handle_bus_event`, `finish_turn`). Either delete it and drive tests through
  the remote path, or `#[cfg(test)]`-gate the transitive closure. Two turn
  implementations is the real smell.
- [ ] (decision) **Re-core the SSH-login state** (`crates/jcode-tui/src/tui/app/auth_remote.rs`):
  one flow tracked by five correlated fields (`phase`, `task`, `operation`,
  `input_kind`, `input`) with 12 guarded `.unwrap()`s. Target is two enums,
  `Stage` and `Activity` (two, because a background `Operation::Status` poll runs
  while the picker is open, so stage and running-work are separate axes). Removes
  the illegal combinations and the stringly-typed `input_kind`. In rep change
  only: keep the no-`Debug`/no-`Clone` secrecy property and the `Drop`
  cleanup semantics; a non-1:1 state is a stop-and-report, not a guess. Verify
  with `cargo test -p jcode-tui auth_remote` (21 tests, security-focused).
  Confirm before touching credential code.

## 3. Spine

One direction, four items. Each depends on the previous being settled, and all
are easier once phase 2 has shrunk the cross-crate surface.

- [ ] **Crate spine** (`jcode-base` -> `jcode-app-core` -> `jcode-tui` -> root):
  `pub use jcode_*::*` makes every module path global, so no call site names its
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
  `include!`s 62 files into one module (126 `include!` sites repo-wide), which is
  why helper collisions and `use super::*` are everywhere.
- [ ] Condense near-duplicate tables: `onboarding_eval.rs` (nine unrolled
  `meta_tierN` tests, liveness logic triplicated at 2672-2736 and 3174-3293),
  `state_model_poke_03.rs` (~40% near-dup pairs), `session_tests/cases.rs` (~51%),
  `remote_events_reload_04.rs` (~43%, header-phase table at 1038-1180).
- [ ] Move subsystem code out of test files: `live_tests.rs` is a live-provider
  coverage ledger (21% tests), `provider_e2e.rs` (11% tests).
- [ ] Pre-existing failures on this tree: `jcode-tui --lib` 27, `jcode-base --lib`
  15, root `kcode --lib` 10 of 195 (measured 2026-09-28; it was 12 of 193 at
  `6dff3825`, and B1/B2 cleared the two provider round-trip failures). Sampled
  root causes are stale expectations for removed or renamed surface:
  `login::next_step::tests::extracted_hints_match_the_strings_login_printed_before_extraction`
  wants "run jcode" where the code now prints "run kcode", and
  `cli::args::tests::login_scriptable_flags_parse` parses a `--google-access-tier`
  flag that went with the Google login cut. Also math/LaTeX 15 and
  `test_lock_order` 1. Environmental, not regressions. Treat as the baseline; the
  suite still covers removed features and brittle pixel/color assertions, so
  collapse or delete rather than maintain.

## 5. Hygiene, then packaging

- [x] `JCODE_*` env vars: state dir is `~/.kcode`, the prefix is now accepted in
  its `KCODE_*` spelling, and `<runtime_dir>/jcode/` is renamed (2026-09-28).
  The remaining question is the internal literal sweep, tracked in §0.
- [ ] Unknown config sections are silently ignored, so older configs keep dead
  keys with no warning.
- [x] **Self-dev tooling names the wrong package.** The prompt and
  `docs/dev/post-change.md` now name `-p kcode --bin kcode`, and the prompt no
  longer instructs the agent to call a tool this fork does not ship (fixed
  2026-09-28, `crates/jcode-base/src/prompt/selfdev_mode.txt`). The `selfdev
  build` / `build-reload` tool still lives outside this repo and still targets
  upstream's `jcode`, so it remains unusable if something else supplies it. Not
  fixable here; the documented path is
  `cargo build --profile selfdev -p kcode --bin kcode`.
- [x] **`scripts/` classification** (triaged 2026-09-28; 82 tracked -> 63). The
  rule: keep it if it is wired, documented, or a reusable harness with a
  dependent; delete it if it was a one-off tied to a past bug, refactor, or
  experiment that nothing references. `git show <sha>:scripts/<name>` recovers
  anything deleted.
  - **Gate (keep, wired):** `check_guardrails.sh`, `check_module_files.py`,
    `check_panic_budget.py`, `check_code_size_budget.py`,
    `check_test_size_budget.py`, `check_dependency_boundaries.py`,
    `check_wildcard_reexport_budget.py`, `check_app_shape.py`,
    `check_warning_budget.sh`, plus their `*_budget.json`/`warning_budget.txt`
    data files. The two size ratchets were re-wired into the gate in this pass
    (§0).
  - **Dev wrapper / build (keep):** `dev_cargo.sh`, `cargo_exec.sh`,
    `remote_build.sh`, `remote_config.sh`, `build_linux_compat.sh`,
    `quick-release.sh`, `generate_release_notes.sh`, `security_preflight.sh`,
    `setup_git_hooks.sh`, `test_dev_cargo_cwd.py`, `test_dev_cargo_jobs.sh`.
  - **Documented harness (keep):** `onboarding_sandbox.sh`, `auth_fixture.sh`,
    `capture_onboarding.sh`, `bench_startup.py`, `check_startup_budget.sh`,
    `compile_time_probe.sh`, `compile_isolation_report.py`,
    `jcode_harbor_agent.py`, `run_terminal_bench_harbor.sh`,
    `run_terminal_bench_campaign.py`, `run_terminal_bench_claude.sh`,
    `test_ci_suites.py`, `test_fast.sh`, `test_e2e.sh`, `test_auth_e2e.sh`,
    `real_provider_smoke.sh`, `analyze_runtime_memory_log.py`,
    `test_analyze_runtime_memory_log.py`, `screenshot_watcher.sh`,
    `webfetch_corpus.sh`, `browser_handoff_fixture.py`,
    `test_browser_handoff_live.py`.
  - **Live / perf harness (keep, but not verified since the fork cut):**
    `test_swarm.py`, `test_swarm_debug.py`, `test_dag_live.py`,
    `test_soft_interrupt.py`, `antigravity_live_coverage.sh`,
    `profile_spawn.py`, `profile_real_spawn.py`, `repro_input_lag.py`,
    `repro_input_flicker.py`, `repro_real_spawn_lag.py`, `memory_probe.sh`,
    `memory_regression_gate.sh`, `find_unlocked_env_tests.py`, `agent_trace.sh`,
    `clean_target.sh`. These are capability, not clutter, so they were not
    deleted; retest or drop them next pass. `memory_probe.sh` in particular still
    hardcodes `$HOME/.jcode/sessions/`, which is the state-path half of the
    env-prefix item above.
  - **Deleted (19):** `analyze_root_crate.py` (planned a split that landed),
    `refactor_shadow.sh` and `refactor_phase1_verify.sh` (verifier pair for the
    same landed refactor), `debug_socket_test.sh` (superseded by the
    `debug_socket` tool), `compare_token_usage.py`, `count_idle_draws.py` and
    `dump_fresh_spawn_screen.py` (one animation-bug probe),
    `measure_key_echo.py`, `sweep_animation_fps.py`,
    `bench_startup_visible_ready.py`, `profile_single_spawn.py`,
    `profile_remote_resume_burst.py`, `repro_expand_edit_shortcut.py`,
    `stress_test.py`, `stress_test_40.sh`, `verify_light_theme.py`,
    `bench_compile.sh` (duplicated `compile_time_probe.sh` and invoked the
    removed `-p jcode --bin jcode`), `lib/configure_path.sh` (dead installer
    helper; the fork does not self-install), and
    `repro/tls-bad-record-mac/.gitignore` (empty fixture directory).
  - No `scripts/README.md`: the gate script is self-documenting and this list is
    the map, so a README would only be a second copy of it.

## Anytime

Independent, no dependency on the phases above.

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
