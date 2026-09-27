# Codebase reduction plan

Status: proposal, nothing implemented. Claims re-verified against the code on
2026-09-27; the corrections are folded in below and the open judgment calls are
listed at the end.

The goal is a codebase where the next maintainer can be lazy: fewer lines,
fewer concepts, fewer places to look, and a test suite that fails only for real
reasons. This plan is ordered by that. Every change is ranked by how much it
removes, never by how elegant the result reads. A re-core that deletes a
duplicate representation removes as much as a deletion does, so it belongs here
too.

This absorbs [tuistate-decomposition.md](tuistate-decomposition.md) and the
withdrawn `tui-render-ownership.md` (git history). It also folds the 2026-09
structural audit of `jcode-tui`, `jcode-app-core`, and `jcode-base`; the audit's
deletion-shaped items are tiers 0-2, and its unification items are tiers 3-6.

## Governing rules

- **Deletion over design.** A design that removes nothing is a cost, not a win.
  A re-core qualifies only when it deletes a concept (a duplicate
  representation, a god module, a parallel path), not when it rearranges one.
- **Every tier ships alone and leaves the tree consistent.** No tier may leave a
  subsystem half-migrated. The withdrawn render-ownership plan failed this rule:
  its intermediate state was worse than its start, which made finishing it
  mandatory, which is why it was unsafe to begin. Each tier below states its
  ship-alone status.
- **No sweep before the core.** Call-site migration happens after the shared
  representation is settled, never before. Tier 5 settles the provider
  registry first; the 153 hardcoded provider strings migrate later, on change,
  not as a prelude.
- **A test that cannot fail for a real reason is negative value.** It taxes
  every future change and buys nothing.

## Measured state

Every number below was counted on this tree on 2026-09-27, not carried over.

| measurement | value |
|---|---|
| lines of Rust | ~538k across 945 files |
| workspace crates | 61 members (60 under `crates/` plus the root package) |
| packages in `Cargo.lock` | 660 |
| three largest crates | `jcode-tui` 181k, `jcode-app-core` 110k, `jcode-base` 94k = 72% |
| provider crates | 20 crates, 61.1k lines |
| `App` declared fields | 307 (279 loose primitives); 57 `impl App` blocks across 158 files under `tui/app/` |
| `use super::*` globs in `jcode-tui` | 124 |
| `TuiState` | 122 methods, 2 implementors, 77 `dyn TuiState` sites |
| largest single function | `handle_client` (`server/client_lifecycle.rs:434`): 28 args, 85 `Request::` arms, body 434-3041 (~2600 lines); its header imports 15 sibling modules on 32 `use` lines |
| swarm/comm surface | audit-named files 31.3k lines (~36.4k production in the wider neighborhood, ~37k in the narrow surface); not the 55k the first draft claimed |
| test-named Rust files | 162.5k lines, but two of the largest are mostly production code: `live_tests.rs` 21% tests, `provider_e2e.rs` 11% tests |
| test brittleness | 0 of the large suites assert colors or grid coordinates; ~1% touch glyph text or style enums |
| pre-existing test failures on `main` | `jcode-tui --lib` 31, math/LaTeX 15, `test_lock_order` 1 |

Each tier below is backed by the budget scripts in `scripts/`; when a tier moves
a tracked number, refresh the baseline in the same commit or the budget blocks
unrelated work.

## Tier 0 - delete

No design, no migration, nothing to half-finish. Highest return per hour.
Ships alone trivially.

- **Provider cut - withdrawn (maintainer decision, 2026-09-27).** The first
  draft proposed deleting ~19k lines across `cursor-runtime`, `copilot*`,
  `antigravity*`, `grok-build-runtime`, `claude-cli-runtime`, `bedrock`,
  `provider-doctor`, `provider-metadata`. That is off the table: **every
  provider stays accessible to users.** The evidence agrees - the candidates
  are all user-selectable in `ProviderChoice` (`src/cli/provider_init.rs`:
  `Cursor`, `Copilot`, `Antigravity`, `GrokBuild`, `Bedrock`), `provider-metadata`
  has 5 dependents, and `jcode-base` depends on `jcode-provider-copilot`. None
  is safe to remove. Provider cleanup moves to tier 5 (unify the
  representation, keep every provider).
- **Repo hygiene** already listed in `wip.md`: upstream CI workflows, dead
  config, `scripts/`, budget baselines, missing `packaging/arch/PKGBUILD`.
- **Dead TUI files.** No verified dead-file set exists yet. The inherited
  claim in the first draft ("the three one-helper `ui_pinned_*` files, and any
  module with no `#[path]`/`mod` declaration") did not survive reading: all four
  `ui_pinned_*` files are declared (`ui.rs:64` loads `ui_pinned.rs`, 1102 lines;
  its submodules at `ui_pinned.rs:3,6,40`), and the "no declaration" heuristic
  is unreliable because the test tree is wired with `include!` (126 sites), not
  the module system. Find real dead files with the compiler, not a file-scan.
  (`ui_transitions.rs` is already deleted.)
- **Dead local TUI event loop.** `App::run` (`tui/app/run_shell.rs:197`, ~112
  lines) has no caller anywhere in the repo: production launches
  `app.run_remote` (`src/cli/tui_launch.rs:161`), and no test calls it. It is
  `pub`, so rustc never warns. Delete `run` and `local::handle_terminal_event`
  (called only from `run`, verified), plus the `EventStream`/`Bus` plumbing only
  they use. Keep `local::handle_tick` and `local::finish_turn`: tests and the
  remote loop still call them.

## Tier 1 - condense the test suite

The first draft of this tier said "replace `TestBackend` pixel assertions".
That premise is refuted: the large suites are behavioral. 0 of them assert
colors or grid coordinates, and only ~1% touch glyph text or style enums. So
brittleness is not the problem, and the pure-view rewrite is not justified by
it. What the reading actually found:

- **Near-duplicate unrolled tables.** `onboarding_eval.rs` repeats one
  monotonic check across nine hand-unrolled `meta_tierN` tests and carries the
  same liveness logic three times (2672-2736, 3174-3293). `state_model_poke_03.rs`
  is ~40% near-duplicate pairs, `session_tests/cases.rs` ~51%,
  `remote_events_reload_04.rs` ~43% (header-phase table at 1038-1180). These
  collapse to table-driven tests with one assertion body.
- **Subsystem code parked in test files.** `live_tests.rs` is a live-provider
  coverage ledger (schema, checkpoint taxonomy, coverage merge, ranking) with
  only 11 tests; `provider_e2e.rs` is 11% tests. The line counts overstate test
  weight. Move the infrastructure to a named module and leave the tests.
- **The test tree is `include!`-wired, so it is one giant module.**
  `tui/app/tests.rs` (1607 lines) `include!`s 62 files into a single module; the
  repo has 126 `include!` sites. Every test file therefore shares one namespace
  and its imports, which is why helper name collisions and `use super::*` are
  everywhere, and why the test files are invisible to the module system
  (and to grep-driven dead-code checks). Converting `include!` to real `mod`
  files is mechanical, compiles independently per file, and is the enabling
  cleanup for collapsing the near-duplicate tables.
- **The snapshot pattern already exists.** `onboarding_golden.rs:render_onboarding_text`
  already renders to an offscreen buffer and compares visible text lines. The
  move is to generalize that, not to invent a pure view layer.

Ships one file/cluster at a time; each collapsed cluster is consistent alone.

- Acceptance: the duplicated tables are one table; test infrastructure lives in
  a module named for it; the suite still passes.

**Open question (see the end):** this reframe demotes the "pure view layer",
which was what pulled in render-state ownership (below). Whether that work is
still wanted is a judgment call, not a measurement.

**Render-state ownership, only if the snapshot layer needs it.** Render results
live in globals: `tui/ui.rs` has 18 production `static` values (atomics and
`OnceLock<Mutex<..>>`) each with a `#[cfg(test)]` mirror, and the same
`#[cfg(test)]` pattern is repeated across 17 `ui_*.rs` files. A text snapshot
layer needs those inputs explicit. Scope this to what the snapshot tests
actually need; do not migrate all of them for their own sake.

## Tier 2 - group `App`'s fields (the App re-core)

Mechanical, always compiles, never half-broken. `self.foo` becomes
`self.group.foo`. Ships group by group; each group is consistent on its own.

- 279 of `App`'s 307 declared fields are loose primitives. Group them into the
  named structs the withdrawn decomposition doc catalogued (Transcript, Input,
  Scroll, Provider, Stream/status, Session/server, Workspace, Overlay,
  Onboarding, RenderState). Several groups already exist (`streaming`,
  `token_accounting`, `kv_cache`, `cost`), so the pattern is proven.
- Start with the 9 loose keybinding fields: pure data, no render coupling, no
  concurrency. Then continue group by group.
- **Collapse the runtime-mode triplication as its own group.** Runtime mode is
  currently four things that can disagree: `AppRuntimeMode` (`app.rs:43`),
  `is_remote: bool` (`app.rs:281`), `is_replay: bool` (`app.rs:1218`),
  `is_replay_runtime()` (`tui_lifecycle_runtime.rs:171`), read together as
  `runtime_mode()==RemoteClient && !self.is_replay` (`turn_notify.rs:82`) and
  `self.is_remote || self.is_replay_runtime()` (`tui_state.rs:206`). Keep the
  enum, delete the bools.
- **Then narrow the `impl App` imports.** 57 `impl App` blocks are spread across
  158 files under `tui/app/`, held together by 124 `use super::*` globs, so no
  impl file declares what state it actually touches. Once the field groups
  exist, each impl file imports its group explicitly instead of `super::*`.
- Effect: a maintainer reads ~15 named groups instead of 279 fields, and
  `TuiState` (122 methods, 77 `&dyn` sites) loses its reason to exist and can be
  deleted last by removing code rather than by designing sub-traits.

Reconcile with "Do not rewrite `tui/app/`" below: this is the mechanical move
that rule carves out, not a rewrite.

## Tier 3 - protocol dispatch re-core

`handle_client` (`jcode-app-core/src/server/client_lifecycle.rs:434`) is the
entire client protocol in one function: 28 parameters (with an
`#[expect(clippy::too_many_arguments)]` at line 430), a body running to line
3041 (~2600 lines), and 85 `Request::` arms. Its 32 import lines already pull
from 15 sibling handler modules, so the split is half-done and the dispatcher
kept the god role. Ships in arm-group increments; each group moves to the
sibling that owns it and leaves the dispatcher smaller but working.

- Introduce a request context struct (session, swarm, file, channel, debug
  refs) so the arms stop threading 28 positional arguments. Note the swarm
  refs are themselves 7 loose `Arc<RwLock<HashMap<..>>>` maps; Tier 6 wants
  `SwarmState` passed instead, so design the context to hold it.
- Move arm groups into the modules that already exist for them
  (`client_actions`, `client_comm`, `client_session`, `client_state`,
  `provider_control`, `comm_*`).
- Acceptance: `handle_client` takes the context plus the stream, its body fits
  on a screen, and `scripts/code_size_budget.json`'s
  `server/client_lifecycle.rs` entry comes down instead of up.

## Tier 4 - command surface unification

Slash-command identity is a string literal matched, independently, in four
hand-maintained places: the registry `REGISTERED_COMMANDS`
(`tui/app/state_ui_input_helpers.rs:39`, 113 distinct command literals), the
local dispatch (`tui/app/commands.rs`, 59), the shared in-process dispatch
(`tui/app/commands_dispatch.rs`, 85), and the remote reconnected path
(`tui/app/remote/key_handling.rs`, 54, e.g. line 964 `trimmed == "/help"`, 1147
`starts_with("/subagent")`). `commands_dispatch.rs:1-13` documents the last
drift: the two copies diverged and `/cancel`, `/ssh`, `/model-status` silently
did nothing remotely. This is also the root of the `wip.md` rows "dead SSH-block
commands" and `/help <item>` coverage, which are symptoms of the same string
table.

Ships one command-family at a time; a family that moves to the table is
consistent on its own.

- Give each command one entry (name, aliases, help text, handler, whether it is
  remote-safe) and make every entry point resolve through it.
- Acceptance: adding a command touches one table; a new name cannot be
  reachable on one path and dead on another. Fixes the `/help` coverage gap for
  free.

## Tier 5 - provider identity unification

Provider identity is encoded in seven enums and several string vocabularies, and
the code says so. The enums: `ActiveProvider` (`provider-core/src/selection.rs:5`),
`RuntimeProviderId` (`base/provider/activation.rs:10`), `ConfigProviderSelection`
(`base/provider/selection.rs:6`), `ProviderChoice` (`src/cli/provider_init.rs:25`),
`LoginProviderTarget` (`provider-metadata/src/lib.rs:25`), `WidgetProviderKind`
(`tui/app/tui_state.rs:17`), `ModelRouteApiMethod`
(`provider-core/src/lib.rs:885`). The strings: `provider_key`
(`provider-core/src/selection.rs:95`), `RuntimeKey`
(`provider-core/src/lib.rs:704`), `LoginProviderDescriptor`
(`provider-metadata/src/lib.rs:104`), and `ProviderAvailability`'s 9 bools
(`selection.rs:17`). The doc comment on `cli_provider_arg_for_session_key` states
the vocabularies "overlap but are NOT identical". `grep '"openai"|"anthropic"|"openrouter"'`
matches 153 files; `=> "openai"|"anthropic"|"openrouter"` appears 30 times.

This is a **unify, not a sweep**: do not start by replacing the 153 literal
sites. Ships when the registry exists and one vocabulary is derived from it;
the literals migrate later, on change.

- Pick one `ProviderId` registry owning key, label, CLI arg, route method,
  login target, and availability.
- Derive the existing vocabularies and enums from it, deleting the ones that
  are pure projections.
- Acceptance: adding a provider is one registry row plus its runtime; the count
  of hardcoded provider strings stops growing (add a budget for it if useful).
- Optional crate-shape consolidation, evaluate separately: the per-provider
  layout is non-uniform. Six providers ship a wire crate plus a `-runtime`
  sibling (`anthropic`, `antigravity`, `copilot`, `gemini`, `openai`,
  `openrouter`); three are runtime-only (`claude-cli-runtime`, `cursor-runtime`,
  `grok-build-runtime`); `bedrock` is wire-only; `provider-core`, `env`,
  `metadata`, `doctor` are shared or tooling. Making every provider the same
  shape is a uniformity win at a possible compile-time cost. It removes no
  provider; treat it as a candidate, not a mandate, and only if the pairs
  genuinely change together.

Tier 0's provider cut is withdrawn, so this is the **only** provider cleanup:
keep every provider reachable, collapse the representation. If a change here
would remove a provider rather than a duplicate *of* one, stop. Provider
capability is fixed; only the way it is named and routed is in scope.

## Tier 6 - swarm/comm condensation

The first draft called this a re-core with "no shared representation". Reading
the region refutes that. There **is** an owner: `SwarmState`
(`server/state.rs:108`) owns members, plans, coordinators and swarms_by_id, with
`SwarmRuntime` (`state.rs:117`) as its per-swarm snapshot and `load_runtime()`
(`state.rs:150`). `jcode-swarm-core` is upstream, not a duplicate:
`SwarmMemberRecord` (`swarm-core/src/lib.rs:215`) is the durable record,
`SwarmMember` (`state.rs:188`) the runtime one. `VersionedPlan` -> its persisted
form -> `PlanGraphStatus` is legal DTO layering. `tool/communicate.rs` holds no
parallel model: it fetches `AgentInfo`/`PlanGraphStatus` over the socket and
applies run-scoped policy. The task DAG has one model (`dag/mod.rs`).

So this is a **condense**, not a re-core. What the reading did confirm:

- **The member projection is written four times by hand.** `SwarmMember` is
  mapped field-by-field into `AgentInfo` (`protocol/src/lib.rs:219`) at
  `client_comm_context.rs:298` and `client_comm_channels.rs:84`, into
  `SwarmMemberStatus` (`protocol/src/lib.rs:456`) at `swarm.rs:701`, and into a
  local `MemberStatic` (`client_comm_context.rs:239`). `AgentInfo` and
  `SwarmMemberStatus` share ~10 hand-copied fields with no conversion between
  them.
- **The status vocabulary diverged.** `SwarmLifecycleStatus`
  (`swarm-core/src/lib.rs:136`) is typed, but it is missing the `streaming` and
  `thinking` values that the code actually uses as member statuses
  (`turn_loops.rs:207` writes `status: "streaming"`), so it would file them
  under `Other(String)`. On top of that, `SwarmMember.status` is `String`,
  `AgentInfo.status` is `Option<String>`, and the "active" set is re-spelled by
  hand in at least nine places with three different sets:
  `running|running_stale` (`plan/src/lib.rs:286`, `swarm.rs:568`,
  `comm_control.rs:284`), `running|running_stale|queued` (`swarm.rs:635`,
  `swarm.rs:1481`), and `running|streaming|thinking`
  (`tui-render/src/swarm_gallery.rs:55`, `swarm.rs:722`, `swarm.rs:1382`,
  `server_events.rs:2535`). Terminal-ness is likewise defined three ways
  (`plan/src/lib.rs:279`, `swarm.rs:227`, inline at `server.rs:341`). Note the
  two same-named `is_active_status` functions serve different domains (plan item
  vs swarm member), so this is a name collision inside one vocabulary, not two
  copies of one predicate. 134 non-test sites string-match status literals.
- **Coordination state has no owner.** `SwarmMutationRuntime`
  (`swarm_mutation_state.rs:95`), `AwaitMembersRuntime` (`await_members_state.rs:71`),
  and two process-global claim maps (`comm_control.rs:101`,
  `communicate.rs:726`).
- **Channel subscriptions straddle two locks** with no atomic owner
  (`comm_session.rs:28`, stitched with `std::mem::take` in
  `swarm_channels.rs:13-21`).
- **Textual repeats:** `type SessionAgents` x19, `type ChannelSubscriptions`
  x13, `swarm_id_for_session` copy-pasted 4 times
  (`client_comm_channels.rs:13`, `client_comm_context.rs:12`,
  `client_comm_message.rs:17`, `comm_graph.rs:45`).
- **`SwarmState` is rebuilt, not passed.** `handle_client` threads 7 loose
  `Arc<RwLock<HashMap<..>>>` maps, and `SwarmState { .. }` is reconstructed at
  ~28 non-persisted sites instead of being the value passed across the boundary.

Ships one duplication at a time: the status predicates are independent of the
member projection, which is independent of the channel locks.

- Unify the member projection behind one conversion so `SwarmMember` is the
  single source and the wire types derive from it.
- Make status one typed value end to end (`SwarmLifecycleStatus`), and collapse
  the two `is_active_status` and the three terminal-ness definitions to one.
- Give coordination state one owner; pass `SwarmState`, not loose maps (shared
  with Tier 3's context struct).
- One lock for channel subscriptions.
- Layering smell to fix in passing: policy (not formatting) lives in
  `jcode-protocol/src/comm_format.rs` (`comm_cleanup_candidate_session_ids:53`,
  default status lists `:23,:33,:43`).
- If a unification does not delete a representation, stop; do not rearrange for
  taste.

## Tier 7 - dependency diet

660 lock packages for a terminal agent. Each removal is compile time and
supply-chain surface. Build time tracks crate and dependency count, so any
crate-count reduction pays here too (this is not the withdrawn provider cut; it
trims genuinely unused dependencies, not providers). This is the concrete
answer to "faster".

## Tier 8 - crate spine and file layout

Last, and only where the boundaries are now real. Pure churn until then.

- **The 61-member layout is invisible at every call site.** `jcode-base/src/lib.rs`
  says it exists to halve the largest compilation unit; `jcode-app-core/src/lib.rs:19`
  does `pub use jcode_base::*` "so every existing `crate::<module>` path keeps
  resolving"; `src/lib.rs:22` does `pub use jcode_tui::*`. So `crate::provider`,
  `crate::config`, `crate::tool`, `crate::server` all resolve globally, and no
  call site tells you which physical crate owns a symbol. `jcode-base` is a
  grab bag (55 top-level modules: browser, mcp, storage, side_panel,
  terminal_launch, github, hooks, ...) that 10 crates depend on wholesale.
- `scripts/check_wildcard_reexport_budget.py` already ratchets this spine
  (baseline total 15) with a stated goal of zero. That script is the measure for
  this tier: drive the budget to zero and let the crate layout become real, or
  decide the layout is cosmetic and collapse it honestly.
- Only then move files. `CRATE_OWNERSHIP_BOUNDARIES.md` is referenced by the
  budget script but does not exist; either create it under this tier or fix the
  reference (`wip.md`).

## Do not rewrite

- `tui/app/` (114k lines). No clean seam, highest risk. Tier 2's mechanical
  field grouping and impl-block import narrowing are the carved-out exception;
  do not "restructure" the TUI on top of it.
- The websocket/protocol layer. Mostly correct and hard-won. Tier 3 moves arms
  into existing modules; it does not redesign the protocol.
- Session persistence.
- Provider runtime internals (accumulated bugfixes; delete only with evidence).
  Tier 5 unifies names above them; it does not touch their bodies.
- The task DAG (`jcode-plan`) and `jcode-swarm-core`'s durable records: one
  model each, verified.

## Verification

- Every tier: `scripts/dev_cargo.sh check` (`TMPDIR` on real disk, not tmpfs, or
  `ring`/`aws-lc-sys` build scripts fail with "Disk quota exceeded").
- Tiers 1 and 2: `cargo test -p jcode-tui --lib`. The suite has pre-existing
  parallel-order failures on `main`; verify a suspect with `--test-threads=1`
  before blaming the change.
- Tiers 3, 5, 6: `cargo test -p jcode-app-core --lib` and the touched crate's
  tests. Tier 3 also has `client_lifecycle_tests.rs`; Tier 6 has
  `swarm_persistence_tests.rs` and `comm_control_tests/dag_e2e.rs`.
- Tier 4: the command-suggestion cache test and the remote-path input tests
  (`tui/app/tests/command_suggestions_cache.rs`, `issue_496_input_routing.rs`).
- Tier 0: a deletion is only done when `cargo check` and the crate's tests pass
  without it.
- Any tier that moves a budget file: re-run the matching `scripts/check_*` and
  refresh only the entries the tier intentionally changed.

## Open judgment calls

These are decisions, not measurements. Recorded so the plan does not silently
pick for the maintainer.

1. **Is Tier 1 still a rewrite?** Its brittleness premise did not survive
   reading. The remaining work is collapsing near-duplicate tables and moving
   test infrastructure out of `tests/`. That is editing, not rewriting. If we
   accept that, Tier 1's "one justified rewrite" status and its output (the
   pure view layer) should both be dropped, and render-state ownership with
   them.
2. **Does render-state ownership survive?** It was pulled in by Tier 1's pure
   view. If Tier 1 is demoted, this either disappears or attaches to Tier 2 as
   "narrow the `ui.rs` globals while grouping `App`".
3. **Tier 3 and Tier 6 coupling.** `handle_client`'s context struct is where
   `SwarmState` should replace the 7 loose maps. That is one change serving two
   tiers. Do them together, or keep Tier 6 strictly after Tier 3?
4. **Scope of this pass.** The plan is docs-only so far. Dead `App::run` is a
   safe standalone cut and could land now. The duplicate `is_active_status` is
   *not* a safe standalone cut: the two functions serve different domains and
   the member one relies on status values the typed enum lacks, so collapsing
   them requires the Tier 6 enum work first. Corrected from the first draft.
