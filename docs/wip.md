# Work in progress

The index of what is not finished, so it does not get lost between passes. This
file is expected to shrink.

When an item lands, delete its row. When a plan completes, delete the plan doc -
git has the history.

## Plans in flight

| plan | state | what's left |
|---|---|---|
| [plans/codebase-reduction.md](plans/codebase-reduction.md) | proposal, nothing implemented; claims re-verified 2026-09-27; provider cut withdrawn by maintainer decision | tier 0 (delete: hygiene, dead local TUI loop) is the entry point - the inherited "dead TUI files" list did not survive reading, so no dead-file set is verified, and the provider cut is off the table (all providers stay accessible); tier 1 is now "condense the test suite" (its pixel-brittleness premise was refuted; the test tree is `include!`-wired into one module); tiers 3-6 are the unifications (protocol dispatch, command surface, provider identity, swarm/comm). Four open judgment calls are listed at the bottom of the plan |
| [plans/browser-provider-protocol.md](plans/browser-provider-protocol.md) | draft spec, no implementation | tighten the core method set and the normalized `page.snapshot` format before building any adapter |

Withdrawn: `plans/tui-render-ownership.md` (git history at `ce83f61a`). Its goal
still lands, as a consequence of tier 1 rather than as a standalone project: it
had no symptom behind it on its own, and its intermediate state was worse than
its start, which made finishing mandatory and therefore starting unsafe. Re-read
2026-09-27: tier 1's brittleness premise was refuted, so whether this goal still
lands at all is an open call in the plan.

[plans/tuistate-decomposition.md](plans/tuistate-decomposition.md) is also
absorbed by tier 2 (group `App`'s fields). Its method categorization matches the
section comments already in `crates/jcode-tui/src/tui/mod.rs`, so it stays only
as the reference for those groups.

## Committed ideas, no plan doc yet

| idea | the problem |
|---|---|
| compile-time isolation | build and link time; the workspace recompiles far more than it should. The old `COMPILE_TIME_ISOLATION_REFACTOR.md` was a historical record naming removed machinery, so it was deleted - the idea stays here. It is the same spine as reduction tier 8: the `pub use jcode_*::*` chain is what makes every edit rebuild the whole workspace |
| the axis kcode owns | as a subtraction fork on MIT code, kcode has no moat on anything inherited; only what it *adds* is ownable, and today that is nothing. Leading candidate: verifiability plus package-manager ownership - reproducible builds, no self-modification, no telemetry, permission-gated by default, every performance claim shipped with a runnable script and raw artifacts in-repo |

## Open items in the code, no plan

| item | state |
|---|---|
| `JCODE_*` env vars | the state dir is `~/.kcode` but the env prefix was never renamed, and the runtime dir is still `<runtime_dir>/jcode/` (`registry.rs` joins `"jcode"` and `JCODE_RUNTIME_DIR` names it; `internals/architecture.md` documents it). Decide once: rename with a `JCODE_*` fallback, or document as-is |
| dead SSH-block commands | `/theme`, `/stats`, `/file`, `/open`, `/permission`, `/permissions`, `/new-terminal`, `/debug-fixture` are blocked over SSH but have no handler anywhere, so they do nothing locally either. Root cause is the command string table being matched in four places; reduction tier 4 fixes the class |
| `-p` vs `provider list` | `-p` accepts 52 provider choices; `provider list` prints 26. The extras are aliases and gateways with no catalog entry |
| `/help <item>` coverage | written detail for 70 of 114 registered commands; the other 44 answer `Unknown command`, and there is no `/help list`. Falls out of reduction tier 4: help text becomes a column of the command table |
| packaging | `packaging/arch/PKGBUILD` does not exist, and the README install section points at it |
| unknown config sections | the loader silently ignores unknown top-level sections, so older config files keep dead keys with no warning |
| pre-existing test failures | measured on this tree: `jcode-tui --lib` 30, `jcode-base --lib` 15, math/LaTeX 15, `test_lock_order` 1. They are environmental (clock-dependent strings, pixel/geometry assertions, and tests still expecting `jcode` paths now under `kcode`), not regressions. The `jcode-tui` count was 31 before the mermaid debug test was deleted with its command. Treat these as the baseline so a change is not diffed against a clean tree every time |
| irrelevant tests | the suite still covers removed features and carries many brittle pixel/color assertions. Collapse or delete rather than keep maintaining them |

## Repo hygiene, no plan

| item | state |
|---|---|
| CI is upstream's | 3 of 8 are already disabled on the fork (`ci.yml`, `update-star-history.yml`, `freebsd-smoke.yml` are `workflow_dispatch`-only); `discord-release.yml` posts to jcode's Discord and is dispatched by `release.yml`; `label-pr.yml` calls the Greptile service. Pruning to kcode-shaped workflows is a fork decision - the fork has so far chosen disable, not delete. `ci.yml`'s citation of a since-removed decision doc was dropped 2026-09-27 |
| dead config | verified 2026-09-27: `.gitignore` names no dead paths. `/.jcode/generated-images/` is live (`crates/jcode-provider-openai/src/stream.rs:769` writes `<cwd>/.jcode/generated-images`), and `Cargo.lock` is not ignored. The earlier claim here was stale |
| `scripts/` | ~90 inherited files, no README; several are explicitly jcode-specific. Classify keep/delete/broken or delete |
| budget baselines | the six budget files (`panic_budget.json`, `swallowed_error_budget.json`, `code_size_budget.json`, `warning_budget.txt`, `wildcard_reexport_budget.json`, `test_size_budget.json`) carry jcode's numbers; re-baseline or they are meaningless or block work |
| fork policy | rebase lane vs hard divergence is undecided, and it blocks crate names and the env prefix. `README.md` states "does not track upstream", but nothing follows from it |
| licensing | no `license` field on the root `Cargo.toml` or any of the 63 members; add `license = "MIT"`, ship the LICENSE inside the package (the PKGBUILD), and generate a `THIRD_PARTY_NOTICES` from `Cargo.lock` |

## Provider layer

| item | state |
|---|---|
| provider clean format (all providers kept) | maintainer decision 2026-09-27: **no provider is deleted.** The old ~19k provider cut is withdrawn - every candidate backs a user-selectable provider (`ProviderChoice` exposes Cursor, Copilot, Antigravity, GrokBuild, Bedrock) and `provider-metadata` has 5 dependents. The 20 `jcode-provider-*` crates (61.1k lines) stay. Cleanup is reduction tier 5: one `ProviderId` representation, plus an optional per-provider crate-shape unification |
| provider identity cohesion | 7 enums plus several string vocabularies name the same thing; the code comment on `cli_provider_arg_for_session_key` admits they "overlap but are NOT identical". Reduction tier 5 unifies around one `ProviderId` registry, keeping every provider reachable |

## Swarm layer

Verified while re-checking the plan's swarm claim (2026-09-27). `SwarmState` is a
real owner, so this is condensation, not a re-core.

| item | detail |
|---|---|
| member projection written 4x | `SwarmMember` is hand-mapped into `AgentInfo` (`protocol:219`), `SwarmMemberStatus` (`protocol:456`), and a local `MemberStatic` (`client_comm_context.rs:239`), by separate mappers (`swarm.rs:701`, `client_comm_context.rs:298`, `client_comm_channels.rs:84`) |
| status vocabulary diverged | `SwarmLifecycleStatus` (`swarm-core:136`) lacks `streaming`/`thinking` which the code uses (`turn_loops.rs:207`), so they fall to `Other`; the "active" set is re-spelled in 9 places with 3 different sets; `is_active_status` exists twice as a name collision across two domains; 134 non-test string-match sites |
| coordination state unowned | `SwarmMutationRuntime`, `AwaitMembersRuntime`, and two process-global claim maps (`comm_control.rs:101`, `communicate.rs:726`) |
| channel subs across two locks | stitched with `std::mem::take` (`swarm_channels.rs:13-21`) |
| `SwarmState` rebuilt, not passed | `handle_client` threads 7 loose `Arc<RwLock<HashMap>>` maps; `SwarmState { .. }` rebuilt at ~28 sites |

## Hook surface gaps

Found while writing `user/hooks.md`.

| gap | detail |
|---|---|
| `turn_start` `SOURCE` | only `chat` is ever emitted; the schema advertises `chat`/`resume`/`ambient`, and `ambient` belongs to a removed mode. Narrow the contract or wire the resume path |
| `session_start` schema comment | stale: says `create`/`resume`; the code emits `create`, `attach`, `resume` |
| hooks are unobservable | no `/hooks` command, no listing of configured hooks, no dry-run. A typo in a command string looks identical to a hook that runs and does nothing |
| blocked calls are invisible | `pre_tool` stderr goes to the model; nothing tells the user their policy blocked a call |
| hook failures | logged and dropped, with no user-visible signal |

## This doc set

The rebuild is complete: `user/` (7), `internals/` (9), `dev/` (4), `plans/` (3),
plus `README.md`, `what-was-removed.md`, and this file at the root. Notes that
outlive it:

- **Install** stays in the root `README.md`; there is no `user/install.md`.
- The repo-root `OAUTH.md` is folded into `user/providers.md` + `user/auth.md`.
- **Dropped as unshipped/speculative** (git has it): the multi-session
  protocol-multiplexing phases and open questions, and Herdr's upstream TODO list.
