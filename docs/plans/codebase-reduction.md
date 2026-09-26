# Codebase reduction plan

Status: proposal, nothing implemented

The goal is a codebase where the next maintainer can be lazy: fewer lines,
fewer concepts, fewer places to look, and a test suite that fails only for real
reasons. This plan is ordered by that. Every change is ranked by how much it
removes, never by how elegant the result reads.

This replaces and absorbs
[tuistate-decomposition.md](tuistate-decomposition.md) and the withdrawn
`tui-render-ownership.md` (git history): their end states survive here as
consequences of other tiers, not as standalone projects.

## Governing rules

- **Deletion over design.** A design that removes nothing is a cost, not a win.
- **Every tier ships alone and leaves the tree consistent.** No tier may leave a
  subsystem half-migrated. The withdrawn render-ownership plan failed this rule:
  its intermediate state was worse than its start, which made finishing it
  mandatory, which is why it was unsafe to begin.
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
| `App` fields | 317 (279 loose primitives) |
| largest files in the repo | all tests: `onboarding_eval` 3294, `openrouter_tests` 3685, `state_model_poke_03` 3020, `live_tests` 3080 |
| pre-existing test failures on `main` | `jcode-tui --lib` 31, math/LaTeX 15, `test_lock_order` 1 |

The last two rows are the most informative. The repo's biggest files are
generated-looking test suites, and the suite is already failing on `main`, so it
is not trusted. That is the largest ongoing cost in the tree.

## Tier 0 - delete

No design, no migration, nothing to half-finish. Highest return per hour.

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

## Tier 1 - rewrite the test strategy

The one place a rewrite is justified, because it is the tax on every future
change and it cannot be fixed by deletion alone.

- Replace `TestBackend` pixel assertions and the derived suites
  (`state_model_poke_01..03`, `scroll_copy_01..03`, `onboarding_eval`) with
  snapshot assertions on a **pure view layer**: a `view(...) -> FrameDescription`
  that is computed without a `Terminal` and asserted as text.
- Keep a small number of real end-to-end harness tests for the paths that must
  go through the terminal.
- Acceptance: the suite passes on a clean tree, and a layout change produces one
  readable snapshot diff instead of a screen of coordinate assertions.

**This tier is what pulls in render-state ownership.** A pure view needs its
inputs explicit, which means the render results currently in
`OnceLock<Mutex<..>>` globals (`tui/ui.rs`, 14 accessors with a
`#[cfg(test)]`/`#[cfg(not(test))]` pair each) have to be passed in. Scope that
to what the snapshot layer actually needs. Do not turn it into a standalone
migration of all 14 fields; that is the withdrawn plan and it can end
half-done.

## Tier 2 - group `App`'s fields

Mechanical, always compiles, never half-broken. `self.foo` becomes
`self.group.foo`.

- 279 of `App`'s 317 fields are loose primitives. Group them into the named
  structs the withdrawn decomposition doc catalogued (Transcript, Input, Scroll,
  Provider, Stream/status, Session/server, Workspace, Overlay, Onboarding,
  RenderState). Several groups already exist (`streaming`, `token_accounting`,
  `kv_cache`, `cost`), so the pattern is proven.
- Start with the 9 loose keybinding fields: pure data, no render coupling, no
  concurrency. Then continue group by group.
- Effect: a maintainer reads ~15 named groups instead of 279 fields, and
  `TuiState` (122 methods, 2 implementors, 49 `&dyn` sites) loses its reason to
  exist and can be deleted last by removing code rather than by designing
  sub-traits.

## Tier 3 - dependency diet

660 lock packages for a terminal agent. Each removal is compile time and
supply-chain surface. Build time tracks crate and dependency count, so Tier 0's
provider deletions pay here too. This is the concrete answer to "faster".

## Tier 4 - file layout

Last, and only where the boundaries are now real. Pure churn until then.

## Do not rewrite

- `tui/app/` (114k lines). No clean seam, highest risk.
- The websocket/protocol layer. Mostly correct and hard-won.
- Session persistence.
- Provider runtime internals (accumulated bugfixes; delete only with evidence).

## Verification

- Every tier: `scripts/dev_cargo.sh check` (`TMPDIR` on real disk, not tmpfs, or
  `ring`/`aws-lc-sys` build scripts fail with "Disk quota exceeded").
- Tier 1 and 2: `cargo test -p jcode-tui --lib`. The suite has pre-existing
  parallel-order failures on `main`; verify a suspect with `--test-threads=1`
  before blaming the change.
- Tier 0: a deletion is only done when `cargo check` and the crate's tests pass
  without it.
