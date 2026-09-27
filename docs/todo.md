# Todo

The one list of outstanding work. Tick an item when it lands and delete it; git
has the history. Longer designs live in the doc an item links to; this file is
the checklist, not the reasoning.

**Order to work these in: `plans/roadmap.md`.** It sequences the items by shared
shape and dependency; this file stays the unordered checklist.

Ordered roughly by payoff. An item carries a compact evidence line so it can be
picked up without re-deriving why it exists. An item marked `(decision)` needs a
call from the maintainer before work starts; everything else is actionable.

## Now

- [ ] (decision) Settle the two quality ratchets: drop the swallowed-error one
  (it counts `let _ =`, `.ok()`, `.unwrap_or_default()`: 3,129 hits across 423
  files, mostly idiomatic), and either re-baseline or drop the panic one, whose
  remaining flags are guarded invariants or build/test code.
- [ ] `scripts/` classification: ~80 inherited files, no README, several
  jcode-specific. Keep / delete / broken triage.
- [ ] (decision) `packaging/arch/PKGBUILD`: README advertises it, it does not
  exist. Write it or drop the README promise. License fields were deliberately
  not added to the manifests (nothing here is published); revisit with
  packaging.

## Code structure (largest payoff first)

- [ ] **Re-core `App`** (`crates/jcode-tui/src/tui/app.rs`): 309 declared fields
  (~287 loose), 57 `impl App` blocks across 53 files held together by 124
  `use super::*` globs, ~3,092 direct `self.<field>` sites. Group the fields
  into sub-structs that also own their methods, collapse the runtime-mode
  triplication (`AppRuntimeMode` + `is_remote` + `is_replay` +
  `is_replay_runtime()`), then narrow each impl's imports. End state: `TuiState`
  (122 methods, 77 `&dyn` sites) has no reason to exist. Staged plan with
  per-group cost: `plans/app-recoring.md`.
- [ ] **Split `handle_client`** (`crates/jcode-app-core/src/server/client_lifecycle.rs:434`):
  28 args, body to :3041 (~2600 lines), 85 `Request::` arms, 15 sibling handler
  modules already exist for the arms. Introduce a request-context struct and move
  arm groups into their modules. Design the context to hold `SwarmState` (below).
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
- [ ] **Unify the command surface**: slash-command identity is a string matched
  in four places: registry `REGISTERED_COMMANDS` (113 literals), `commands.rs`
  (59), `commands_dispatch.rs` (85), `remote/key_handling.rs` (54).
  `commands_dispatch.rs:1-13` records the last drift bug. One table
  (name, aliases, help, handler, remote-safe); fixes the `/help` gap and the
  dead SSH-block commands for free.
- [ ] **Unify provider identity**: seven enums (`ActiveProvider`,
  `RuntimeProviderId`, `ConfigProviderSelection`, `ProviderChoice`,
  `LoginProviderTarget`, `WidgetProviderKind`, `ModelRouteApiMethod`) plus
  string vocabularies (`provider_key`, `RuntimeKey`, `LoginProviderDescriptor`,
  `ProviderAvailability`'s bools) name the same thing; the code comment on
  `cli_provider_arg_for_session_key` admits they "overlap but are NOT identical".
  153 files hardcode provider strings, 30 match arms. One `ProviderId` registry;
  derive the rest. Do not sweep the 153 sites first; migrate on change.
- [ ] **Condense swarm/comm**: `SwarmState` (`server/state.rs:108`) is a real
  owner, so this is condensation. Member projection hand-written 4x
  (`AgentInfo`, `SwarmMemberStatus`, `MemberStatic`); status vocabulary diverged
  (`SwarmLifecycleStatus` lacks `streaming`/`thinking`; the "active" set is
  re-spelled in 9 places with 3 different sets; 134 string-match sites);
  coordination state unowned (`SwarmMutationRuntime`, `AwaitMembersRuntime`, two
  global claim maps); channel subs across two locks; `SwarmState` rebuilt at ~28
  sites instead of passed.
- [ ] **Crate spine** (`jcode-base` -> `jcode-app-core` -> `jcode-tui` -> root):
  `pub use jcode_*::*` makes every module path global, so no call site names its
  owning crate. `scripts/check_wildcard_reexport_budget.py` ratchets this
  (baseline 13, goal 0). Drive it to zero, or declare the layout cosmetic and
  collapse it.
- [ ] **Dependency diet**: 660 lock packages for a terminal agent.

## Tests

- [ ] Condense near-duplicate tables: `onboarding_eval.rs` (nine unrolled
  `meta_tierN` tests, liveness logic triplicated at 2672-2736 and 3174-3293),
  `state_model_poke_03.rs` (~40% near-dup pairs), `session_tests/cases.rs` (~51%),
  `remote_events_reload_04.rs` (~43%, header-phase table at 1038-1180).
- [ ] Move subsystem code out of test files: `live_tests.rs` is a live-provider
  coverage ledger (21% tests), `provider_e2e.rs` (11% tests).
- [ ] Replace the `include!`-wired test tree with real modules: `app/tests.rs`
  `include!`s 62 files into one module (126 `include!` sites repo-wide), which is
  why helper collisions and `use super::*` are everywhere.
- [ ] Pre-existing failures on this tree: `jcode-tui --lib` 30, `jcode-base --lib`
  15, math/LaTeX 15, `test_lock_order` 1. Environmental, not regressions. Treat
  as the baseline; the suite still covers removed features and brittle
  pixel/color assertions, so collapse or delete rather than maintain.
- [ ] Optional: the render-state globals (`ui.rs` has 18 production `static`s
  with `#[cfg(test)]` mirrors, the pattern repeated across 17 `ui_*.rs`). Only
  worth doing if a snapshot layer needs the inputs explicit.

## Provider layer

- [ ] Provider cleanup keeps every provider reachable. No provider is deleted
  (maintainer decision 2026-09-27): every cut candidate backs a user-selectable
  provider (`ProviderChoice` exposes Cursor, Copilot, Antigravity, GrokBuild,
  Bedrock; `provider-metadata` has 5 dependents). The 20 `jcode-provider-*`
  crates (61.1k lines) stay. Cleanup is the identity unification above.
- [ ] Optional: per-provider crate shape is non-uniform (6 providers ship a wire
  crate plus a `-runtime` sibling; 3 are runtime-only; `bedrock` is wire-only).
  Uniformity win at a possible compile-time cost; removes no provider.

## Swarm layer

Covered by "Condense swarm/comm" above. References: `internals/swarm.md`.

## Hooks

- [ ] Hooks are unobservable: no `/hooks`, no listing, no dry-run. A typo looks
  identical to a hook that does nothing.
- [ ] Blocked calls are invisible: `pre_tool` stderr goes to the model, nothing
  tells the user; hook failures are logged and dropped.

## Repo hygiene

- [ ] `JCODE_*` env vars: state dir is `~/.kcode` but the prefix was never
  renamed, and the runtime dir is `<runtime_dir>/jcode/`. Rename with a
  `JCODE_*` fallback, or document as-is.
- [ ] `-p` accepts 52 provider choices, `provider list` prints 26.
- [ ] Unknown config sections are silently ignored, so older configs keep dead
  keys with no warning.
- [ ] Ambient-mode vocabulary outlived the ambient cut in user-facing places:
  `info_widget_tips.rs:20-21` advertises background cycles and emailed
  summaries, `input_help.rs:41` and `state_ui_input_helpers.rs:538` offer
  `/agents ambient`, plus stale comments in `agent.rs`, `turn_execution.rs`,
  `app.rs`. Check whether `/agents ambient` still routes before deleting it.
- [ ] Self-dev tooling names the wrong package. `selfdev build` and
  `build-reload` run `-p jcode --bin jcode`, but this fork's package is `kcode`
  (root `[lib] name = "jcode"`, `[[bin]] name = "kcode"`), so the build fails
  with "package ID specification `jcode` did not match any packages". Either
  teach the tooling the fork's names, or keep
  `cargo build --profile selfdev -p kcode --bin kcode` as the documented path
  (see `docs/dev/post-change.md`).
- [ ] (decision) Fork policy: rebase lane vs hard divergence is undecided and
  blocks crate names and the env prefix. `README.md` says "does not track
  upstream"; nothing follows from it.

## Committed ideas (no plan yet)

- [ ] Compile-time isolation: the workspace recompiles far more than it should.
  Same spine as the crate-spine item above.
- [ ] The axis kcode owns: as a subtraction fork on MIT code, only what kcode
  *adds* is ownable. Leading candidate: verifiability plus package-manager
  ownership (reproducible builds, no self-modification, no telemetry,
  permission-gated by default, every performance claim shipped with a runnable
  script and raw artifacts in-repo).

## Spec (not a checklist)

`plans/browser-provider-protocol.md`: draft spec, no implementation. Tighten the
core method set and the normalized `page.snapshot` format before building an
adapter.
