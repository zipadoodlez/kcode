# Codebase reduction plan

Status: proposal, nothing implemented (tiers 0-2 unchanged; tiers 3-6 added from the
2026-09 structural audit)

The goal is a codebase where the next maintainer can be lazy: fewer lines,
fewer concepts, fewer places to look, and a test suite that fails only for real
reasons. This plan is ordered by that. Every change is ranked by how much it
removes, never by how elegant the result reads. A re-core that deletes a
duplicate representation removes as much as a deletion does, so it belongs here
too.

This absorbs [tuistate-decomposition.md](tuistate-decomposition.md) and the
withdrawn `tui-render-ownership.md` (git history). It also folds the 2026-09
structural audit of `jcode-tui`, `jcode-app-core`, and `jcode-base`; the audit's
deletion-shaped items are tiers 0-2, and its re-core items are tiers 3-6.

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

| measurement | value |
|---|---|
| lines of Rust | ~538k across ~946 files |
| workspace crates | 61 (91 members) |
| packages in `Cargo.lock` | 660 |
| three largest crates | `jcode-tui` 181k, `jcode-app-core` 110k, `jcode-base` 94k = 72% |
| provider crates | 20 crates, 61k lines |
| `App` fields | 307-317 (279 loose primitives); ~60 `impl App` blocks across 158 files under `tui/app/` |
| `use super::*` globs in `jcode-tui` | 124 |
| largest single functions | `handle_client` (`server/client_lifecycle.rs`) ~2600 lines, 28 args, 85 `Request::` arms |
| largest files in the repo | all tests: `onboarding_eval` 3294, `openrouter_tests` 3685, `state_model_poke_03` 3020, `live_tests` 3080 |
| test-named Rust files | 162.5k lines (~30% of the tree) |
| pre-existing test failures on `main` | `jcode-tui --lib` 31, math/LaTeX 15, `test_lock_order` 1 |

The last rows are the most informative. The repo's biggest files are
generated-looking test suites, the suite is already failing on `main`, and the
largest function is a protocol dispatcher with 28 arguments. Those are the
largest ongoing costs in the tree.

Each tier below is backed by the budget scripts in `scripts/`; when a tier moves
a tracked number, refresh the baseline in the same commit or the budget blocks
unrelated work.

## Tier 0 - delete

No design, no migration, nothing to half-finish. Highest return per hour.
Ships alone trivially.

- **Provider cut.** ~19k lines already scoped in `wip.md` (`cursor-runtime`,
  `copilot*`, `antigravity*`, `grok-build-runtime`, `claude-cli-runtime`,
  `bedrock`, `provider-doctor`, `provider-metadata`). Delete as each is proven
  unused. Not a rewrite: the surviving lines are accumulated production
  bugfixes. Note these crates are *not* thin wrappers (openai 2379, openrouter
  2681, anthropic 1641, bedrock 1981 lines), so each deletion needs evidence.
- **Repo hygiene** already listed in `wip.md`: upstream CI workflows, dead
  config, `scripts/`, budget baselines, missing `packaging/arch/PKGBUILD`.
- **Dead TUI files**: the three one-helper `ui_pinned_*` files, and any module
  with no `#[path]`/`mod` declaration. (`ui_transitions.rs` is already deleted.)
- **Dead local TUI event loop.** `App::run` (`tui/app/run_shell.rs:197`, ~112
  lines) has no caller anywhere in the repo: production launches
  `app.run_remote` (`src/cli/tui_launch.rs:161`), and no test calls it. It is
  `pub`, so rustc never warns. Delete `run` and `local::handle_terminal_event`
  (called only from `run`, verified), plus the `EventStream`/`Bus` plumbing only
  they use. Keep `local::handle_tick` and `local::finish_turn`: tests and the
  remote loop still call them.

## Tier 1 - rewrite the test strategy

The one place a rewrite is justified, because it is the tax on every future
change and it cannot be fixed by deletion alone. Ships alone behind the pure
view layer it introduces.

- Replace `TestBackend` pixel assertions and the derived suites
  (`state_model_poke_01..03`, `scroll_copy_01..03`, `onboarding_eval`) with
  snapshot assertions on a **pure view layer**: a `view(...) -> FrameDescription`
  that is computed without a `Terminal` and asserted as text.
- Keep a small number of real end-to-end harness tests for the paths that must
  go through the terminal.
- Acceptance: the suite passes on a clean tree, and a layout change produces one
  readable snapshot diff instead of a screen of coordinate assertions.
- The audit's test finding reinforces this: 162.5k test lines (~30%) with
  suites that are themselves god modules (`openrouter_tests` 3685,
  `onboarding_eval` 3294). Confirm whether the largest suites are duplicated
  assertions or legitimately broad before deciding to collapse them; that read
  is not done.

**This tier is what pulls in render-state ownership.** A pure view needs its
inputs explicit, which means the render results currently in
`OnceLock<Mutex<..>>` globals (`tui/ui.rs`, 14 accessors with a
`#[cfg(test)]`/`#[cfg(not(test))]` pair each) have to be passed in. Scope that
to what the snapshot layer actually needs. Do not turn it into a standalone
migration of all 14 fields; that is the withdrawn plan and it can end
half-done.

## Tier 2 - group `App`'s fields (the App re-core)

Mechanical, always compiles, never half-broken. `self.foo` becomes
`self.group.foo`. Ships group by group; each group is consistent on its own.

- 279 of `App`'s fields are loose primitives. Group them into the named structs
  the withdrawn decomposition doc catalogued (Transcript, Input, Scroll,
  Provider, Stream/status, Session/server, Workspace, Overlay, Onboarding,
  RenderState). Several groups already exist (`streaming`, `token_accounting`,
  `kv_cache`, `cost`), so the pattern is proven.
- Start with the 9 loose keybinding fields: pure data, no render coupling, no
  concurrency. Then continue group by group.
- **Collapse the runtime-mode triplication as its own group.** Runtime mode is
  currently four things that can disagree: `AppRuntimeMode` (`app.rs:43`),
  `is_remote: bool` (`app.rs:281`), `is_replay: bool` (`app.rs:1218`),
  `is_replay_runtime()` (`tui_lifecycle_runtime.rs:171`), read together as
  `runtime_mode()==RemoteClient && !self.is_replay` (`turn_notify.rs:82`) and
  `self.is_remote || self.is_replay_runtime()` (`tui_state.rs:206`). Keep the
  enum, delete the bools.
- **Then split the `impl App` blocks.** ~60 `impl App` blocks are spread across
  158 files under `tui/app/`, held together by 124 `use super::*` globs, so no
  impl file declares what state it actually touches. Once the field groups
  exist, each impl file can import its group explicitly instead of `super::*`.
  This is the step that turns the globs off.
- Effect: a maintainer reads ~15 named groups instead of 279 fields, and
  `TuiState` (156 methods, 2 implementors, ~50 `&dyn` sites) loses its reason to
  exist and can be deleted last by removing code rather than by designing
  sub-traits.

Reconcile with "Do not rewrite `tui/app/`" below: this is the mechanical move
that rule carves out, not a rewrite.

## Tier 3 - protocol dispatch re-core

`handle_client` (`jcode-app-core/src/server/client_lifecycle.rs:434`) is the
entire client protocol in one function: 28 parameters (with an
`#[expect(clippy::too_many_arguments)]` at line 430), a body running to line
3043 (~2600 lines), and 85 `Request::` arms. Its 30 import lines already pull
from 20 sibling handler modules, so the split is half-done and the dispatcher
kept the god role. Ships in arm-group increments; each group moves to the
sibling that owns it and leaves the dispatcher smaller but working.

- Introduce a request context struct (session, swarm, file, channel, debug
  refs) so the arms stop threading 28 positional arguments.
- Move arm groups into the modules that already exist for them
  (`client_actions`, `client_comm`, `client_session`, `client_state`,
  `provider_control`, `comm_*`).
- Acceptance: `handle_client` takes the context plus the stream, its body fits
  on a screen, and `scripts/code_size_budget.json`'s
  `server/client_lifecycle.rs` entry comes down instead of up.

## Tier 4 - command surface unification

Slash-command identity is a string literal matched, independently, in four
hand-maintained places: the registry `REGISTERED_COMMANDS`
(`tui/app/state_ui_input_helpers.rs:39`), the local dispatch (`tui/app/commands.rs`,
67 literals), the shared in-process dispatch (`tui/app/commands_dispatch.rs`, 101
literals), and the remote reconnected path (`tui/app/remote/key_handling.rs`, 65
literals, e.g. line 964 `trimmed == "/help"`, 1147 `starts_with("/subagent")`).
`commands_dispatch.rs:1-13` documents the last drift: the two copies diverged and
`/cancel`, `/ssh`, `/model-status` silently did nothing remotely. This is also
the root of the `wip.md` rows "dead SSH-block commands" and `/help <item>`
coverage, which are symptoms of the same string table.

Ships one command-family at a time; a family that moves to the table is
consistent on its own.

- Give each command one entry (name, aliases, help text, handler, whether it is
  remote-safe) and make every entry point resolve through it.
- Acceptance: adding a command touches one table; a new name cannot be
  reachable on one path and dead on another. Fixes the `/help` coverage gap for
  free.

## Tier 5 - provider identity unification

Provider identity is encoded in at least six parallel vocabularies plus five
enums, and the code says so. `jcode_provider_core::provider_key`
(`selection.rs:95`, "claude"/"openai"/...), the `RuntimeKey` vocabulary
("anthropic-api-key"), the CLI `ProviderChoice` ("anthropic-api"),
`ModelRouteApiMethod` ("openai-compatible:<profile>"), `LoginProviderTarget`,
and `ActiveProvider` (8 variants), `RuntimeProviderId` (14 variants,
`base/provider/activation.rs`), `ConfigProviderSelection`, `WidgetProviderKind`
(9 variants, `tui/app/tui_state.rs:17`), `ProviderAvailability` (9 bools). The
doc comment on `cli_provider_arg_for_session_key` states the vocabularies
"overlap but are NOT identical". `grep '"openai"|"anthropic"|"openrouter"'`
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

Do not confuse this with Tier 0's provider cut. The cut deletes unused
providers; this unifies how the surviving ones are named. The cut makes this
smaller.

## Tier 6 - swarm/comm re-core

~55k lines model swarm/comm with no single owner. The same concept lives in
`app-core/server/swarm.rs` (3170), `swarm_persistence.rs`, `swarm_mutation_state.rs`,
`swarm_channels.rs`, `comm_control.rs` (2625), `comm_session.rs` (1431),
`comm_plan.rs`, `comm_await.rs`, `comm_sync.rs`, `comm_graph.rs`; in
`tool/communicate.rs` (3364), which re-fetches member/plan state over the wire
and re-derives in-flight/cleanup policy locally; in `protocol/comm_format.rs`;
in `jcode-swarm-core` + `jcode-plan`; and in `jcode-tui-render/swarm_gallery.rs` +
`tui/app/commands_*.rs`.

This is the highest-risk tier and the least read. It is gated on Tier 3 because
the server half of swarm flows through `handle_client`. Ships only when the
member/plan/channel state model moves whole; a half-moved swarm state model is
the exact "half-migrated" failure the rules forbid.

- Read the region end to end first, then name the invariants (member registry,
  plan DAG, channel subscriptions) and give each one home.
- Collapse the tool-side re-derivation onto the server state model rather than a
  second implementation.
- If the read does not produce a state model that shrinks, stop and leave it;
  do not rearrange it for taste.

## Tier 7 - dependency diet

660 lock packages for a terminal agent. Each removal is compile time and
supply-chain surface. Build time tracks crate and dependency count, so Tier 0's
provider deletions pay here too. This is the concrete answer to "faster".

## Tier 8 - crate spine and file layout

Last, and only where the boundaries are now real. Pure churn until then.

- **The 61-crate layout is invisible at every call site.** `jcode-base/src/lib.rs`
  says it exists to halve the largest compilation unit; `jcode-app-core/src/lib.rs:19`
  does `pub use jcode_base::*` "so every existing `crate::<module>` path keeps
  resolving"; `src/lib.rs:22` does `pub use jcode_tui::*`. So `crate::provider`,
  `crate::config`, `crate::tool`, `crate::server` all resolve globally, and no
  call site tells you which physical crate owns a symbol. `jcode-base` is a
  70-module grab bag (browser, mcp, storage, side_panel, terminal_launch, github,
  hooks, ...) that 10 crates depend on wholesale.
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

## Verification

- Every tier: `scripts/dev_cargo.sh check` (`TMPDIR` on real disk, not tmpfs, or
  `ring`/`aws-lc-sys` build scripts fail with "Disk quota exceeded").
- Tiers 1 and 2: `cargo test -p jcode-tui --lib`. The suite has pre-existing
  parallel-order failures on `main`; verify a suspect with `--test-threads=1`
  before blaming the change.
- Tiers 3, 5, 6: `cargo test -p jcode-app-core --lib` and the touched crate's
  tests. Tier 3 also has `client_lifecycle_tests.rs`.
- Tier 4: the command-suggestion cache test and the remote-path input tests
  (`tui/app/tests/command_suggestions_cache.rs`, `issue_496_input_routing.rs`).
- Tier 0: a deletion is only done when `cargo check` and the crate's tests pass
  without it.
- Any tier that moves a budget file: re-run the matching `scripts/check_*` and
  refresh only the entries the tier intentionally changed.
